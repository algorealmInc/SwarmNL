// Copyright 2024 Algorealm
// Apache 2.0 License

use libp2p::gossipsub::MessageId;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, time::Instant};
use thiserror::Error;

use self::ping_config::PingInfo;

use super::*;

/// The duration (in seconds) to wait for response from the network layer before timing
/// out.
pub const NETWORK_READ_TIMEOUT: Seconds = 30;

/// The time it takes for the task to sleep before it can recheck if an output has been placed in
/// the repsonse buffer;
pub const TASK_SLEEP_DURATION: Seconds = 3;

/// Type that represents the response of the network layer to the application layer's event handler.
type AppResponseResult = Result<AppResponse, NetworkError>;

/// The delimeter that separates the messages to gossip
pub(super) const GOSSIP_MESSAGE_SEPARATOR: &str = "~#~";

/// Time to wait (in seconds) for the node (network layer) to boot.
pub(super) const BOOT_WAIT_TIME: Seconds = 1;

/// The buffer capacity of an mpsc stream.
pub(super) const STREAM_BUFFER_CAPACITY: usize = 100;

/// Data exchanged over a stream between the application and network layer
#[derive(Debug, Clone)]
pub(super) enum StreamData {
	/// Application data sent over the stream.
	FromApplication(StreamId, AppData),
	/// Network response data sent over the stream to the application layer.
	ToApplication(StreamId, AppResponse),
}

/// Request sent from the application layer to the networking layer.
#[derive(Debug, Clone)]
pub enum AppData {
	/// A simple echo message.
	Echo(String),
	/// Dail peer.
	DailPeer(PeerId, MultiaddrString),
	/// Store a value associated with a given key in the Kademlia DHT.
	KademliaStoreRecord {
		key: Vec<u8>,
		value: Vec<u8>,
		// expiration time for local records
		expiration_time: Option<Instant>,
		// store on explicit peers
		explicit_peers: Option<Vec<PeerIdString>>,
	},
	/// Perform a lookup of a value associated with a given key in the Kademlia DHT.
	KademliaLookupRecord { key: Vec<u8> },
	/// Perform a lookup of peers that store a record.
	KademliaGetProviders { key: Vec<u8> },
	/// Stop providing a record on the network.
	KademliaStopProviding { key: Vec<u8> },
	/// Remove record from local store.
	KademliaDeleteRecord { key: Vec<u8> },
	/// Return important information about the local routing table.
	KademliaGetRoutingTableInfo,
	/// Fetch data(s) quickly from a peer over the network.
	FetchData { keys: Vec<Vec<u8>>, peer: PeerId },
	/// Get network information about the node.
	GetNetworkInfo,
	/// Send message to gossip peers in a mesh network.
	GossipsubBroadcastMessage {
		/// Topic to send messages to
		topic: String,
		message: Vec<String>,
	},
	/// Join a mesh network.
	GossipsubJoinNetwork(String),
	/// Get gossip information about node.
	GossipsubGetInfo,
	/// Leave a network we are a part of.
	GossipsubExitNetwork(String),
	/// Blacklist a peer explicitly.
	GossipsubBlacklistPeer(PeerId),
	/// Remove a peer from the blacklist.
	GossipsubFilterBlacklist(PeerId),
}

/// Response to requests sent from the application to the network layer.
#[derive(Debug, Clone, PartialEq)]
pub enum AppResponse {
	/// The value written to the network.
	Echo(String),
	/// The peer we dailed.
	DailPeerSuccess(String),
	/// Store record success.
	KademliaStoreRecordSuccess,
	/// DHT lookup result.
	KademliaLookupSuccess(Vec<u8>),
	/// Nodes storing a particular record in the DHT.
	KademliaGetProviders {
		key: Vec<u8>,
		providers: Vec<PeerIdString>,
	},
	/// No providers found.
	KademliaNoProvidersFound,
	/// Routing table information.
	KademliaGetRoutingTableInfo { protocol_id: String },
	/// Result of RPC operation.
	FetchData(Vec<Vec<u8>>),
	/// A network error occured while executing the request.
	Error(NetworkError),
	/// Important information about the node.
	GetNetworkInfo {
		peer_id: PeerId,
		connected_peers: Vec<PeerId>,
		external_addresses: Vec<MultiaddrString>,
	},
	/// Successfully broadcast to the network.
	GossipsubBroadcastSuccess,
	/// Successfully joined a mesh network.
	GossipsubJoinSuccess,
	/// Successfully exited a mesh network.
	GossipsubExitSuccess,
	/// Gossipsub information about node.
	GossipsubGetInfo {
		/// Topics that the node is currently subscribed to
		topics: Vec<String>,
		/// Peers we know about and their corresponding topics
		mesh_peers: Vec<(PeerId, Vec<String>)>,
		/// Peers we have blacklisted
		blacklist: HashSet<PeerId>,
	},
	/// A peer was successfully blacklisted.
	GossipsubBlacklistSuccess,
}

/// Network error type containing errors encountered during network operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum NetworkError {
	#[error("timeout occured waiting for data from network layer")]
	NetworkReadTimeout,
	#[error("internal request stream buffer is full")]
	StreamBufferOverflow,
	#[error("failed to store record in DHT")]
	KadStoreRecordError(Vec<u8>),
	#[error("failed to fetch data from peer")]
	RpcDataFetchError,
	#[error("failed to fetch record from the DHT")]
	KadFetchRecordError(Vec<u8>),
	#[error("task carrying app response panicked")]
	InternalTaskError,
	#[error("failed to dail peer")]
	DailPeerError,
	#[error("failed to broadcast message to peers in the topic")]
	GossipsubBroadcastMessageError,
	#[error("failed to join a mesh network")]
	GossipsubJoinNetworkError,
}

/// A simple struct used to track requests sent from the application layer to the network layer.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct StreamId(u32);

impl StreamId {
	/// Generate a new random stream id.
	/// Must only be called once.
	pub fn new() -> Self {
		StreamId(0)
	}

	/// Generate a new random stream id, using the current as reference.
	pub fn next(current_id: StreamId) -> Self {
		StreamId(current_id.0.wrapping_add(1))
	}
}

/// Type that contains the result of querying the network layer.
pub type NetworkResult = Result<AppResponse, NetworkError>;

/// Type that keeps track of the requests from the application layer.
/// This type has a maximum buffer size and will drop subsequent requests when full.
/// It is unlikely to be ever full as the default is usize::MAX except otherwise specified during
/// configuration. It is always good practice to read responses from the internal stream buffer
/// using `query_network()` or explicitly using `recv_from_network`.
#[derive(Clone, Debug)]
pub(super) struct StreamRequestBuffer {
	/// Max requests we can keep track of.
	size: usize,
	buffer: HashSet<StreamId>,
}

impl StreamRequestBuffer {
	/// Create a new request buffer.
	pub fn new(buffer_size: usize) -> Self {
		Self {
			size: buffer_size,
			buffer: HashSet::new(),
		}
	}

	/// Push [`StreamId`]s into buffer.
	/// Returns `false` if the buffer is full and request cannot be stored.
	pub fn insert(&mut self, id: StreamId) -> bool {
		if self.buffer.len() < self.size {
			self.buffer.insert(id);
			return true;
		}
		false
	}
}

/// Type that keeps track of the response to the requests from the application layer.
pub(super) struct StreamResponseBuffer {
	/// Max responses we can keep track of.
	size: usize,
	buffer: HashMap<StreamId, AppResponseResult>,
}

impl StreamResponseBuffer {
	/// Create a new request buffer.
	pub fn new(buffer_size: usize) -> Self {
		Self {
			size: buffer_size,
			buffer: HashMap::new(),
		}
	}

	/// Push a [`StreamId`] into buffer.
	/// Returns `false` if the buffer is full and request cannot be stored.
	pub fn insert(&mut self, id: StreamId, response: AppResponseResult) -> bool {
		if self.buffer.len() < self.size {
			self.buffer.insert(id, response);
			return true;
		}
		false
	}

	/// Remove a [`StreamId`] from the buffer.
	pub fn remove(&mut self, id: &StreamId) -> Option<AppResponseResult> {
		self.buffer.remove(&id)
	}
}

/// Type representing the RPC data structure sent between nodes in the network.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) enum Rpc {
	/// Using request-response.
	ReqResponse { data: Vec<Vec<u8>> },
}

/// The configuration for the RPC protocol.
pub struct RpcConfig {
	/// Timeout for inbound and outbound requests.
	pub timeout: Duration,
	/// Maximum number of concurrent inbound + outbound streams.
	pub max_concurrent_streams: usize,
}

/// The high level trait that provides an interface for the application layer to respond to network
/// events.
pub trait EventHandler {
	/// Event that informs the application that we have started listening on a new multiaddr.
	fn new_listen_addr(
		&mut self,

		_local_peer_id: PeerId,
		_listener_id: ListenerId,
		_addr: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that informs the application that a new peer (with its location details) has just
	/// been added to the routing table.
	fn routing_table_updated(&mut self, _peer_id: PeerId) {
		// Default implementation
	}

	/// Event that informs the application about a newly established connection to a peer.
	fn connection_established(
		&mut self,

		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_established_in: Duration,
	) {
		// Default implementation
	}

	/// Event that informs the application about a closed connection to a peer.
	fn connection_closed(
		&mut self,

		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: u32,
		_cause: Option<ConnectionError>,
	) {
		// Default implementation
	}

	/// Event that announces expired listen address.
	fn expired_listen_addr(&mut self, _listener_id: ListenerId, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces a closed listener.
	fn listener_closed(&mut self, _listener_id: ListenerId, _addresses: Vec<Multiaddr>) {
		// Default implementation
	}

	/// Event that announces a listener error.
	fn listener_error(&mut self, _listener_id: ListenerId) {
		// Default implementation
	}

	/// Event that announces a dialing attempt.
	fn dialing(&mut self, _peer_id: Option<PeerId>, _connection_id: ConnectionId) {
		// Default implementation
	}

	/// Event that announces a new external address candidate.
	fn new_external_addr_candidate(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces a confirmed external address.
	fn external_addr_confirmed(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces an expired external address.
	fn external_addr_expired(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces new connection arriving on a listener and in the process of
	/// protocol negotiation.
	fn incoming_connection(
		&mut self,

		_connection_id: ConnectionId,
		_local_addr: Multiaddr,
		_send_back_addr: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that announces an error happening on an inbound connection during its initial
	/// handshake.
	fn incoming_connection_error(
		&mut self,

		_connection_id: ConnectionId,
		_local_addr: Multiaddr,
		_send_back_addr: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that announces an error happening on an outbound connection during its initial
	/// handshake.
	fn outgoing_connection_error(
		&mut self,

		_connection_id: ConnectionId,
		_peer_id: Option<PeerId>,
	) {
		// Default implementation
	}

	/// Event that announces the arrival of a pong message from a peer.
	/// The duration it took for a round trip is also returned.
	fn outbound_ping_success(&mut self, _peer_id: PeerId, _duration: Duration) {
		// Default implementation
	}

	/// Event that announces a `Ping` error.
	fn outbound_ping_error(&mut self, _peer_id: PeerId, _err_type: Failure) {
		// Default implementation
	}

	/// Event that announces the arrival of a `PeerInfo` via the `Identify` protocol.
	fn identify_info_recieved(&mut self, _peer_id: PeerId, _info: Info) {
		// Default implementation
	}

	/// Event that announces the successful write of a record to the DHT.
	fn kademlia_put_record_success(&mut self, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to save a record.
	fn kademlia_put_record_error(&mut self) {
		// Default implementation
	}

	/// Event that announces a node as a provider of a record in the DHT.
	fn kademlia_start_providing_success(&mut self, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to become a provider of a record in the DHT.
	fn kademlia_start_providing_error(&mut self) {
		// Default implementation
	}

	/// Event that announces the arrival of an RPC message.
	fn rpc_incoming_message_handled(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>>;

	/// Event that announces that a peer has just left a network.
	fn gossipsub_unsubscribe_message_recieved(&mut self, _peer_id: PeerId, _topic: String) {
		// Default implementation
	}

	/// Event that announces that a peer has just joined a network.
	fn gossipsub_subscribe_message_recieved(&mut self, _peer_id: PeerId, _topic: String) {
		// Default implementation
	}

	/// Event that announces the arrival of a gossip message.
	fn gossipsub_incoming_message_handled(&mut self, _source: PeerId, _data: Vec<String>);

	/// Event that announces the beginning of the filtering and authentication of the incoming
	/// gossip message. It returns a boolean to specify whether the massage should be dropped or
	/// should reach the application. All incoming messages are allowed in by default.
	fn gossipsub_incoming_message_filtered(
		&mut self,
		_propagation_source: PeerId,
		_message_id: MessageId,
		_source: Option<PeerId>,
		_topic: String,
		_data: Vec<String>,
	) -> bool {
		true
	}
}

/// Default network event handler.
#[derive(Clone)]
pub struct DefaultHandler;
/// Implement [`EventHandler`] for [`DefaultHandler`]
impl EventHandler for DefaultHandler {
	/// Echo the message back to the sender.
	fn rpc_incoming_message_handled(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		data
	}

	/// Echo the incoming gossip message to the console.
	fn gossipsub_incoming_message_handled(&mut self, _source: PeerId, _data: Vec<String>) {
		// Default implementation
	}
}

/// Important information to obtain from the [`CoreBuilder`], to properly handle network
/// operations.
#[derive(Clone)]
pub(super) struct NetworkInfo {
	/// The name/id of the network.
	pub id: StreamProtocol,
	/// Important information to manage `Ping` operations.
	pub ping: PingInfo,
	/// Important information to manage `Gossipsub` operations.
	pub gossipsub: gossipsub_cfg::GossipsubInfo,
}

/// Module that contains important data structures to manage `Ping` operations on the network.
pub mod ping_config {
	use libp2p_identity::PeerId;
	use std::{collections::HashMap, time::Duration};

	/// Policies to handle a `Ping` error.
	/// All connections to peers are closed during a disconnect operation.
	#[derive(Debug, Clone)]
	pub enum PingErrorPolicy {
		/// Do not disconnect under any circumstances.
		NoDisconnect,
		/// Disconnect after a number of outbound errors.
		DisconnectAfterMaxErrors(u16),
		/// Disconnect after a certain number of concurrent timeouts.
		DisconnectAfterMaxTimeouts(u16),
	}

	/// Struct that stores critical information for the execution of the [`PingErrorPolicy`].
	#[derive(Debug, Clone)]
	pub struct PingManager {
		/// The number of timeout errors encountered from a peer.
		pub timeouts: HashMap<PeerId, u16>,
		/// The number of outbound errors encountered from a peer.
		pub outbound_errors: HashMap<PeerId, u16>,
	}

	/// The configuration for the `Ping` protocol.
	#[derive(Debug, Clone)]
	pub struct PingConfig {
		/// The interval between successive pings.
		/// Default is 15 seconds.
		pub interval: Duration,
		/// The duration before which the request is considered failure.
		/// Default is 20 seconds.
		pub timeout: Duration,
		/// Error policy.
		pub err_policy: PingErrorPolicy,
	}

	/// Critical information to manage `Ping` operations.
	#[derive(Debug, Clone)]
	pub struct PingInfo {
		pub policy: PingErrorPolicy,
		pub manager: PingManager,
	}
}

/// Module containing important state relating to the `Gossipsub` protocol.
pub(crate) mod gossipsub_cfg {
	use super::*;

	/// The struct containing the list of blacklisted peers.
	#[derive(Clone, Debug, Default)]
	pub struct Blacklist {
		// Blacklist
		pub list: HashSet<PeerId>,
	}

	impl Blacklist {
		/// Return the inner list we're keeping track of.
		pub fn into_inner(&self) -> HashSet<PeerId> {
			self.list.clone()
		}
	}

	/// Important information to manage `Gossipsub` operations.
	#[derive(Clone)]
	pub struct GossipsubInfo {
		pub blacklist: Blacklist,
	}
}

/// Network queue that tracks the execution of application requests in the network layer.
pub(super) struct ExecQueue {
	buffer: Mutex<VecDeque<StreamId>>,
}

impl ExecQueue {
	// Create new execution queue
	pub fn new() -> Self {
		Self {
			buffer: Mutex::new(VecDeque::new()),
		}
	}

	// Remove a [`StreamId`] from the top of the queue.
	pub async fn pop(&mut self) -> Option<StreamId> {
		self.buffer.lock().await.pop_front()
	}

	// Append a [`StreamId`] to the queue.
	pub async fn push(&mut self, stream_id: StreamId) {
		self.buffer.lock().await.push_back(stream_id);
	}
}

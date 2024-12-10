// Copyright 2024 Algorealm, Inc.
// Apache 2.0 License

//! The module that contains important data structures and logic for the functioning of swarmNl.

use self::ping_config::PingInfo;
use libp2p::gossipsub::MessageId;
use libp2p_identity::PublicKey;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::hash::Hash;
use std::{collections::VecDeque, time::Instant};
use thiserror::Error;

use super::*;

/// The duration (in seconds) to wait for response from the network layer before timing
/// out.
pub const NETWORK_READ_TIMEOUT: Seconds = 30;

/// The time it takes for the task to sleep before it can recheck if an output has been placed in
/// the response buffer.
pub const TASK_SLEEP_DURATION: Seconds = 3;

/// The height of the internal queue. This represents the maximum number of elements that a queue
/// can accommodate without losing its oldest elements.
const MAX_QUEUE_ELEMENTS: usize = 300;

/// Type that represents the response of the network layer to the application layer's event handler.
pub type AppResponseResult = Result<AppResponse, NetworkError>;

/// Type that represents the data exchanged during RPC operations.
pub type RpcData = ByteVector;

/// Type that represents a vector of vector of bytes.
pub type ByteVector = Vec<Vec<u8>>;

/// Type that represents the id of a shard
pub type ShardId = String;

/// Type that represents the result for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Type that represents a vector of string
pub type StringVector = Vec<String>;

/// Type that represents a nonce.
pub type Nonce = u64;

/// Time to wait (in seconds) for the node (network layer) to boot.
pub(super) const BOOT_WAIT_TIME: Seconds = 1;

/// The buffer capacity of an mpsc stream.
pub(super) const STREAM_BUFFER_CAPACITY: usize = 100;

/// Data exchanged over a stream between the application and network layer.
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
	FetchData { keys: RpcData, peer: PeerId },
	/// Get network information about the node.
	GetNetworkInfo,
	/// Send message to gossip peers in a mesh network.
	GossipsubBroadcastMessage {
		/// Topic to send messages to
		topic: String,
		message: ByteVector,
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
	FetchData(RpcData),
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
		topics: StringVector,
		/// Peers we know about and their corresponding topics
		mesh_peers: Vec<(PeerId, StringVector)>,
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
	#[error("failed to exit a mesh network")]
	GossipsubExitNetworkError,
	#[error("internal stream failed to transport data")]
	InternalStreamError,
	#[error("replica network not found")]
	MissingReplNetwork,
	#[error("network id for sharding has not been configured. See `CoreBuilder::with_shard()`")]
	MissingShardingNetworkIdError,
	#[error("threshold for data forwarding not met")]
	DataForwardingError,
	#[error("failed to shard data")]
	ShardingFailureError,
	#[error("failed to fetch sharded data")]
	ShardingFetchError,
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
	ReqResponse { data: RpcData },
}

/// The configuration for the RPC protocol.
pub enum RpcConfig {
	Default,
	Custom {
		/// Timeout for inbound and outbound requests.
		timeout: Duration,
		/// Maximum number of concurrent inbound + outbound streams.
		max_concurrent_streams: usize,
	},
}

/// Enum that represents the events generated in the network layer.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum NetworkEvent {
	/// Event that informs the application that we have started listening on a new multiaddr.
	///
	/// # Fields
	///
	/// - `local_peer_id`: The `PeerId` of the local peer.
	/// - `listener_id`: The ID of the listener.
	/// - `address`: The new `Multiaddr` where the local peer is listening.
	NewListenAddr {
		local_peer_id: PeerId,
		listener_id: ListenerId,
		address: Multiaddr,
	},
	/// Event that informs the application that a new peer (with its location details) has just
	/// been added to the routing table.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the new peer added to the routing table.
	RoutingTableUpdated { peer_id: PeerId },
	/// Event that informs the application about a newly established connection to a peer.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the connected peer.
	/// - `connection_id`: The ID of the connection.
	/// - `endpoint`: The `ConnectedPoint` information about the connection's endpoint.
	/// - `num_established`: The number of established connections with this peer.
	/// - `established_in`: The duration it took to establish the connection.
	ConnectionEstablished {
		peer_id: PeerId,
		connection_id: ConnectionId,
		endpoint: ConnectedPoint,
		num_established: NonZeroU32,
		established_in: Duration,
	},
	/// Event that informs the application about a closed connection to a peer.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer.
	/// - `connection_id`: The ID of the connection.
	/// - `endpoint`: The `ConnectedPoint` information about the connection's endpoint.
	/// - `num_established`: The number of remaining established connections with this peer.
	ConnectionClosed {
		peer_id: PeerId,
		connection_id: ConnectionId,
		endpoint: ConnectedPoint,
		num_established: u32,
	},
	/// Event that announces an expired listen address.
	///
	/// # Fields
	///
	/// - `listener_id`: The ID of the listener.
	/// - `address`: The expired `Multiaddr`.
	ExpiredListenAddr {
		listener_id: ListenerId,
		address: Multiaddr,
	},
	/// Event that announces a closed listener.
	///
	/// # Fields
	///
	/// - `listener_id`: The ID of the listener.
	/// - `addresses`: The list of `Multiaddr` where the listener was listening.
	ListenerClosed {
		listener_id: ListenerId,
		addresses: Vec<Multiaddr>,
	},
	/// Event that announces a listener error.
	///
	/// # Fields
	///
	/// - `listener_id`: The ID of the listener that encountered the error.
	ListenerError { listener_id: ListenerId },
	/// Event that announces a dialing attempt.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer being dialed, if known.
	/// - `connection_id`: The ID of the connection attempt.
	Dialing {
		peer_id: Option<PeerId>,
		connection_id: ConnectionId,
	},
	/// Event that announces a new external address candidate.
	///
	/// # Fields
	///
	/// - `address`: The new external address candidate.
	NewExternalAddrCandidate { address: Multiaddr },
	/// Event that announces a confirmed external address.
	///
	/// # Fields
	///
	/// - `address`: The confirmed external address.
	ExternalAddrConfirmed { address: Multiaddr },
	/// Event that announces an expired external address.
	///
	/// # Fields
	///
	/// - `address`: The expired external address.
	ExternalAddrExpired { address: Multiaddr },
	/// Event that announces a new connection arriving on a listener and in the process of
	/// protocol negotiation.
	///
	/// # Fields
	///
	/// - `connection_id`: The ID of the incoming connection.
	/// - `local_addr`: The local `Multiaddr` where the connection is received.
	/// - `send_back_addr`: The remote `Multiaddr` of the peer initiating the connection.
	IncomingConnection {
		connection_id: ConnectionId,
		local_addr: Multiaddr,
		send_back_addr: Multiaddr,
	},
	/// Event that announces an error happening on an inbound connection during its initial
	/// handshake.
	///
	/// # Fields
	///
	/// - `connection_id`: The ID of the incoming connection.
	/// - `local_addr`: The local `Multiaddr` where the connection was received.
	/// - `send_back_addr`: The remote `Multiaddr` of the peer initiating the connection.
	IncomingConnectionError {
		connection_id: ConnectionId,
		local_addr: Multiaddr,
		send_back_addr: Multiaddr,
	},
	/// Event that announces an error happening on an outbound connection during its initial
	/// handshake.
	///
	/// # Fields
	///
	/// - `connection_id`: The ID of the outbound connection.
	/// - `peer_id`: The `PeerId` of the peer being connected to, if known.
	OutgoingConnectionError {
		connection_id: ConnectionId,
		peer_id: Option<PeerId>,
	},
	/// Event that announces the arrival of a pong message from a peer.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer that sent the pong message.
	/// - `duration`: The duration it took for the round trip.
	OutboundPingSuccess { peer_id: PeerId, duration: Duration },
	/// Event that announces a `Ping` error.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer that encountered the ping error.
	OutboundPingError { peer_id: PeerId },
	/// Event that announces the arrival of a `PeerInfo` via the `Identify` protocol.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer that sent the identify info.
	/// - `info`: The `IdentifyInfo` received from the peer.
	IdentifyInfoReceived { peer_id: PeerId, info: IdentifyInfo },
	/// Event that announces the successful write of a record to the DHT.
	///
	/// # Fields
	///
	/// - `key`: The key of the record that was successfully written.
	KademliaPutRecordSuccess { key: Vec<u8> },
	/// Event that announces the failure of a node to save a record.
	KademliaPutRecordError,
	/// Event that announces a node as a provider of a record in the DHT.
	///
	/// # Fields
	///
	/// - `key`: The key of the record being provided.
	KademliaStartProvidingSuccess { key: Vec<u8> },
	/// Event that announces the failure of a node to become a provider of a record in the DHT.
	KademliaStartProvidingError,
	/// Event that announces the arrival of an RPC message.
	///
	/// # Fields
	///
	/// - `data`: The `RpcData` of the received message.
	RpcIncomingMessageHandled { data: RpcData },
	/// Event that announces that a peer has just left a network.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer that left.
	/// - `topic`: The topic the peer unsubscribed from.
	GossipsubUnsubscribeMessageReceived { peer_id: PeerId, topic: String },
	/// Event that announces that a peer has just joined a network.
	///
	/// # Fields
	///
	/// - `peer_id`: The `PeerId` of the peer that joined.
	/// - `topic`: The topic the peer subscribed to.
	GossipsubSubscribeMessageReceived { peer_id: PeerId, topic: String },
	/// Event that announces the arrival of a replicated data content
	///
	/// # Fields
	///
	/// - `data`: The data contained in the gossip message.
	/// - `outgoing_timestamp`: The time the message left the source
	/// - `outgoing_timestamp`: The time the message was recieved
	/// - `message_id`: The unique id of the message
	/// - `source`: The `PeerId` of the source peer.
	ReplicaDataIncoming {
		/// Data
		data: StringVector,
		/// Timestamp at which the message left the sending node
		outgoing_timestamp: Seconds,
		/// Timestamp at which the message arrived
		incoming_timestamp: Seconds,
		/// Message ID to prevent deduplication. It is usually a hash of the incoming message
		message_id: String,
		/// Sender PeerId
		source: PeerId,
	},
	/// Event that announces the arrival of a forwarded sharded data
	///
	/// # Fields
	///
	/// - `data`: The data contained in the gossip message.
	IncomingForwardedData {
		/// Data
		data: StringVector,
		/// Sender's PeerId
		source: PeerId,
	},
	/// Event that announces the arrival of a gossip message.
	///
	/// # Fields
	///
	/// - `source`: The `PeerId` of the source peer.
	/// - `data`: The data contained in the gossip message.
	GossipsubIncomingMessageHandled { source: PeerId, data: StringVector },
	// /// Event that announces the beginning of the filtering and authentication of the incoming
	// /// gossip message.
	// ///
	// /// # Fields
	// ///
	// /// - `propagation_source`: The `PeerId` of the peer from whom the message was received.
	// /// - `message_id`: The ID of the incoming message.
	// /// - `source`: The `PeerId` of the original sender, if known.
	// /// - `topic`: The topic of the message.
	// /// - `data`: The data contained in the message.
	// GossipsubIncomingMessageFiltered {
	//     propagation_source: PeerId,
	//     message_id: MessageId,
	//     source: Option<PeerId>,
	//     topic: String,
	//     data: StringVector,
	// },
}

/// The struct that contains incoming information about a peer returned by the `Identify` protocol.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IdentifyInfo {
	/// The public key of the remote peer.
	pub public_key: PublicKey,
	/// The address the remote peer is listening on.
	pub listen_addrs: Vec<Multiaddr>,
	/// The protocols supported by the remote peer.
	pub protocols: Vec<StreamProtocol>,
	/// The address we are listened on, observed by the remote peer.
	pub observed_addr: Multiaddr,
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
	/// The function that handles incoming RPC data request and produces a response.
	pub rpc_handler_fn: fn(RpcData) -> RpcData,
	/// The function to filter incoming gossip messages.
	pub gossip_filter_fn: fn(PeerId, MessageId, Option<PeerId>, String, StringVector) -> bool,
	/// Important information to manage `Replication` operations.
	pub replication: replication::ReplInfo,
	/// Important information to manage `sharding` operations.
	pub sharding: sharding::ShardingInfo,
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
pub mod gossipsub_cfg {
	use super::*;

	/// The struct containing the list of blacklisted peers.
	#[derive(Clone, Debug, Default)]
	pub struct Blacklist {
		// Blacklist
		pub list: HashSet<PeerId>,
	}

	/// GossipSub configuration.
	pub enum GossipsubConfig {
		/// A default configuration.
		Default,
		/// A custom configuration.
		///
		/// # Fields
		///
		/// - `config`: The custom configuration for gossipsub.
		/// - `auth`: The signature authenticity check.
		Custom {
			config: gossipsub::Config,
			auth: gossipsub::MessageAuthenticity,
		},
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

/// Queue that stores and removes data in a FIFO manner.
#[derive(Clone)]
pub(super) struct DataQueue<T: Debug + Clone + Eq + PartialEq + Hash> {
	buffer: Arc<Mutex<VecDeque<T>>>,
}

impl<T> DataQueue<T>
where
	T: Debug + Clone + Eq + PartialEq + Hash,
{
	/// The initial buffer capacity, to optimize for speed and defer allocation
	const INITIAL_BUFFER_CAPACITY: usize = 300;

	/// Create new queue.
	pub fn new() -> Self {
		Self {
			buffer: Arc::new(Mutex::new(VecDeque::with_capacity(
				DataQueue::<T>::INITIAL_BUFFER_CAPACITY,
			))),
		}
	}

	/// Remove an item from the top of the queue.
	pub async fn pop(&self) -> Option<T> {
		self.buffer.lock().await.pop_front()
	}

	/// Append an item to the queue.
	pub async fn push(&self, item: T) {
		let mut buffer = self.buffer.lock().await;
		if buffer.len() >= MAX_QUEUE_ELEMENTS {
			buffer.pop_front();
		}
		buffer.push_back(item);
	}

	/// Return the inner data structure of the queue.
	pub async fn into_inner(&self) -> VecDeque<T> {
		self.buffer.lock().await.clone()
	}

	/// Drain the contents of the queue.
	pub async fn drain(&mut self) {
		self.buffer.lock().await.drain(..);
	}
}

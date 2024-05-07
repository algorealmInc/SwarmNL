/// Copyright (c) 2024 Algorealm
use libp2p::request_response::OutboundRequestId;
use rand::random;
use serde::{Deserialize, Serialize};
use std::{any::Any, time::Instant};
use thiserror::Error;

use self::ping_config::PingInfo;

use super::*;

/// Type to indicate the duration (in seconds) to wait for data from the network layer before timing
/// out
pub const NETWORK_READ_TIMEOUT: u64 = 60;

/// Data exchanged over a stream between the application and network layer
#[derive(Debug, Clone)]
pub(super) enum StreamData {
	/// Application data sent over the stream
	Application(StreamId, AppData),
	/// Network data sent over the stream
	Network(StreamId, NetworkData),
}

/// Data sent from the application layer to the networking layer
#[derive(Debug, Clone)]
pub enum AppData {
	/// A simple echo message
	Echo(String),
	/// Store a value associated with a given key in the Kademlia DHT
	KademliaStoreRecord {
		key: Vec<u8>,
		value: Vec<u8>,
		// expiration time for local records
		expiration_time: Option<Instant>,
		// store on explicit peers
		explicit_peers: Option<Vec<PeerIdString>>,
	},
	/// Perform a lookup of a value associated with a given key in the Kademlia DHT
	KademliaLookupRecord { key: Vec<u8> },
	/// Perform a lookup of peers that store a record
	KademliaGetProviders { key: Vec<u8> },
	/// Stop providing a record on the network
	KademliaStopProviding { key: Vec<u8> },
	/// Remove record from local store
	KademliaDeleteRecord { key: Vec<u8> },
	/// Return important information about the local routing table
	KademliaGetRoutingTableInfo,
	/// Fetch data(s) quickly from a peer over the network
	FetchData { keys: Vec<String>, peer: PeerId },
	// Get network information
	// Gossip related requests
}

/// Data sent from the networking to itself
#[derive(Debug, Clone)]
pub(super) enum NetworkData {
	/// Dail peer
	DailPeer(MultiaddrString),
}

/// Results from Kademlia DHT operation
pub enum Kademlia {
	/// Return important information about the DHT, this will be increased shortly
	Info { protocol_id: String },
}

/// Network error type containing errors encountered during network operations
#[derive(Error, Debug)]
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
}

/// Operations performed on the Kademlia DHT
#[derive(Debug)]
pub enum DhtOps {
	/// Value found in the DHT
	RecordFound { key: Vec<u8>, value: Vec<u8> },
	/// No value found
	RecordNotFound,
	/// Nodes found that provide value for key
	ProvidersFound {
		key: Vec<u8>,
		providers: Vec<PeerIdString>,
	},
	/// No providers returned (This is most likely due to an error)
	NoProvidersFound,
}

/// A simple struct used to track requests sent from the application layer to the network layer
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct StreamId(u32);

impl StreamId {
	/// Generate a new random stream id.
	/// Must only be called once
	pub fn new() -> Self {
		StreamId(random())
	}

	/// Generate a new random stream id, using the current as guide
	pub fn next(current_id: StreamId) -> Self {
		StreamId(current_id.0.wrapping_add(1))
	}
}

/// Enum that represents various ids that can be tracked on the [`StreamResponseBuffer`]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum TrackableStreamId {
	/// Our custom stream id
	Id(StreamId),
	/// RPC request id
	Outbound(OutboundRequestId),
	/// Kademlia Key
	Kad(Vec<u8>),
}

/// Type that specifies the result of querying the network layer
pub type NetworkResult = Result<Box<dyn StreamResponseType>, NetworkError>;

/// Marker trait that indicates a stream reponse data object
pub trait StreamResponseType
where
	Self: Send + Sync + Any + 'static,
{
}

/// Macro that implements [`StreamResponseType`] for various types to occupy the
/// [`StreamResponseBuffer`] [`StreamResponseBuffer`]
macro_rules! impl_stream_response_for_types { ( $( $t:ident )* ) => {
	$(
		impl StreamResponseType for $t {}
	)* };
}

impl_stream_response_for_types!(u8 i8 u16 i16 u32 i32 u64 i64 u128 i128
	usize isize f32 f64 String bool Kademlia StreamId SwarmNlError NetworkError DhtOps);

/// Implement [`StreamResponseType`] for `Vec<T>`
impl<T> StreamResponseType for Vec<T> where T: StreamResponseType {}

/// Type that keeps track of the requests from the application layer.
/// This type has a maximum buffer size and will drop subsequent requests when full.
/// It is unlikely to be ever full as the default is usize::MAX except otherwise specified during
/// configuration. It is always good practice to read responses from the internal stream buffer
/// using `fetch_from_network()` or explicitly using `recv_from_network`
#[derive(Clone)]
pub(super) struct StreamRequestBuffer {
	/// Max requests we can keep track of
	size: usize,
	buffer: HashSet<StreamId>,
}

impl StreamRequestBuffer {
	/// Create a new request buffer
	pub fn new(buffer_size: usize) -> Self {
		Self {
			size: buffer_size,
			buffer: HashSet::new(),
		}
	}

	/// Push [`StreamId`]s into buffer.
	/// Returns `false` if the buffer is full and request cannot be stored
	pub fn insert(&mut self, id: StreamId) -> bool {
		if self.buffer.len() < self.size {
			self.buffer.insert(id);
			return true;
		}
		false
	}

	/// Remove a [`StreamId`] from the buffer
	pub fn remove(&mut self, id: &StreamId) {
		self.buffer.remove(&id);
	}
}

/// Type that keeps track of the response to the requests from the application layer.
pub(super) struct StreamResponseBuffer {
	/// Max responses we can keep track of
	size: usize,
	buffer: HashMap<TrackableStreamId, Box<dyn StreamResponseType>>,
}

impl StreamResponseBuffer {
	/// Create a new request buffer
	pub fn new(buffer_size: usize) -> Self {
		Self {
			size: buffer_size,
			buffer: HashMap::new(),
		}
	}

	/// Push a [`TrackableStreamId`] into buffer.
	/// Returns `false` if the buffer is full and request cannot be stored
	pub fn insert(&mut self, id: TrackableStreamId, response: Box<dyn StreamResponseType>) -> bool {
		if self.buffer.len() < self.size {
			self.buffer.insert(id, response);
			return true;
		}
		false
	}

	/// Remove a [`TrackableStreamId`] from the buffer
	pub fn remove(&mut self, id: &TrackableStreamId) -> Option<Box<dyn StreamResponseType>> {
		self.buffer.remove(&id)
	}
}

/// Type representing the RPC data structure sent between nodes in the network
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) enum Rpc {
	/// Using request-response
	ReqResponse { data: Vec<Vec<u8>> },
}

/// The configuration for the RPC protocol
pub struct RpcConfig {
	/// Timeout for inbound and outbound requests
	pub timeout: Duration,
	/// Maximum number of concurrent inbound + outbound streams
	pub max_concurrent_streams: usize,
}

/// The high level trait that provides default implementations to handle most supported network
/// swarm events.
pub trait EventHandler {
	/// Event that informs the network core that we have started listening on a new multiaddr.
	fn new_listen_addr(
		&mut self,
		_channel: NetworkChannel,
		_local_peer_id: PeerId,
		_listener_id: ListenerId,
		_addr: Multiaddr,
	) {
	}

	/// Event that informs the network core about a newly established connection to a peer.
	fn connection_established(
		&mut self,
		_channel: NetworkChannel,
		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_established_in: Duration,
	) {
		// Default implementation
	}

	/// Event that informs the network core about a closed connection to a peer.
	fn connection_closed(
		&mut self,
		_channel: NetworkChannel,
		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: u32,
		_cause: Option<ConnectionError>,
	) {
		// Default implementation
	}

	/// Event that announces expired listen address.
	fn expired_listen_addr(
		&mut self,
		_channel: NetworkChannel,
		_listener_id: ListenerId,
		_address: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that announces a closed listener.
	fn listener_closed(
		&mut self,
		_channel: NetworkChannel,
		_listener_id: ListenerId,
		_addresses: Vec<Multiaddr>,
	) {
		// Default implementation
	}

	/// Event that announces a listener error.
	fn listener_error(&mut self, _channel: NetworkChannel, _listener_id: ListenerId) {
		// Default implementation
	}

	/// Event that announces a dialing attempt.
	fn dialing(
		&mut self,
		_channel: NetworkChannel,
		_peer_id: Option<PeerId>,
		_connection_id: ConnectionId,
	) {
		// Default implementation
	}

	/// Event that announces a new external address candidate.
	fn new_external_addr_candidate(&mut self, _channel: NetworkChannel, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces a confirmed external address.
	fn external_addr_confirmed(&mut self, _channel: NetworkChannel, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces an expired external address.
	fn external_addr_expired(&mut self, _channel: NetworkChannel, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces new connection arriving on a listener and in the process of
	/// protocol negotiation.
	fn incoming_connection(
		&mut self,
		_channel: NetworkChannel,
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
		_channel: NetworkChannel,
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
		_channel: NetworkChannel,
		_connection_id: ConnectionId,
		_peer_id: Option<PeerId>,
	) {
		// Default implementation
	}

	/// Event that announces the arrival of a ping message from a peer.
	/// The duration it took for a round trip is also returned
	fn inbound_ping_success(
		&mut self,
		_channel: NetworkChannel,
		_peer_id: PeerId,
		_duration: Duration,
	) {
		// Default implementation
	}

	/// Event that announces a `Ping` error
	fn outbound_ping_error(
		&mut self,
		_channel: NetworkChannel,
		_peer_id: PeerId,
		_err_type: Failure,
	) {
		// Default implementation
	}

	/// Event that announces the arrival of a `PeerInfo` via the `Identify` protocol
	fn identify_info_recieved(&mut self, _channel: NetworkChannel, _peer_id: PeerId, _info: Info) {
		// Default implementation
	}

	/// Event that announces the successful write of a record to the DHT
	fn kademlia_put_record_success(&mut self, _channel: NetworkChannel, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to save a record
	fn kademlia_put_record_error(&mut self, _channel: NetworkChannel) {
		// Default implementation
	}

	/// Event that announces a node as a provider of a record in the DHT
	fn kademlia_start_providing_success(&mut self, _channel: NetworkChannel, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to become a provider of a record in the DHT
	fn kademlia_start_providing_error(&mut self, _channel: NetworkChannel) {
		// Default implementation
	}

	/// Event that announces the arrival of an RPC message
	fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>>;
}

/// Default network event handler
pub struct DefaultHandler;

/// Implement [`EventHandler`] for [`DefaultHandler`]
impl EventHandler for DefaultHandler {
	/// Echo the message back to the sender
	fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		data
	}
}

/// Important information to obtain from the [`CoreBuilder`], to properly handle network
/// operations
pub(super) struct NetworkInfo {
	/// The name/id of the network
	pub id: StreamProtocol,
	/// Important information to manage `Ping` operations
	pub ping: PingInfo,
	/// Application's event handler network communication channel
	pub event_comm_channel: NetworkChannel,
}

/// Module that contains important data structures to manage `Ping` operations on the network
pub mod ping_config {
	use libp2p_identity::PeerId;
	use std::{collections::HashMap, time::Duration};

	/// Policies to handle a `Ping` error
	/// - All connections to peers are closed during a disconnect operation.
	pub enum PingErrorPolicy {
		/// Do not disconnect under any circumstances
		NoDisconnect,
		/// Disconnect after a number of outbound errors
		DisconnectAfterMaxErrors(u16),
		/// Disconnect after a certain number of concurrent timeouts
		DisconnectAfterMaxTimeouts(u16),
	}

	/// Struct that stores critical information for the execution of the [`PingErrorPolicy`]
	#[derive(Debug)]
	pub struct PingManager {
		/// The number of timeout errors encountered from a peer
		pub timeouts: HashMap<PeerId, u16>,
		/// The number of outbound errors encountered from a peer
		pub outbound_errors: HashMap<PeerId, u16>,
	}

	/// The configuration for the `Ping` protocol
	pub struct PingConfig {
		/// The interval between successive pings.
		/// Default is 15 seconds
		pub interval: Duration,
		/// The duration before which the request is considered failure.
		/// Default is 20 seconds
		pub timeout: Duration,
		/// Error policy
		pub err_policy: PingErrorPolicy,
	}

	/// Critical information to manage `Ping` operations
	pub struct PingInfo {
		pub policy: PingErrorPolicy,
		pub manager: PingManager,
	}
}

/// Struct that allows the application layer comunicate with the network layer during event handling
#[derive(Clone)]
pub struct NetworkChannel {
	/// The request buffer that keeps track of application requests being handled
	stream_request_buffer: Arc<Mutex<StreamRequestBuffer>>,
	/// The consuming end of the stream that recieves data from the network layer
	// application_receiver: Receiver<StreamData>,
	/// This serves as a buffer for the results of the requests to the network layer.
	/// With this, applications can make async requests and fetch their results at a later time
	/// without waiting. This is made possible by storing a [`StreamId`] for a particular stream
	/// request.
	stream_response_buffer: Arc<Mutex<StreamResponseBuffer>>,
	/// The stream end for writing to the network layer
	application_sender: Sender<StreamData>,
	/// Current stream id. Useful for opening new streams, we just have to bump the number by 1
	current_stream_id: Arc<Mutex<StreamId>>,
	/// The network read timeout
	network_read_delay: AsyncDuration,
}

impl NetworkChannel {
	/// Create a new application's event handler network communication channel
	pub(super) fn new(
		stream_request_buffer: Arc<Mutex<StreamRequestBuffer>>,
		stream_response_buffer: Arc<Mutex<StreamResponseBuffer>>,
		application_sender: Sender<StreamData>,
		current_stream_id: Arc<Mutex<StreamId>>,
		network_read_delay: AsyncDuration,
	) -> NetworkChannel {
		Self {
			stream_request_buffer,
			stream_response_buffer,
			application_sender,
			current_stream_id,
			network_read_delay,
		}
	}

	/// Send data to the network layer and recieve a unique `StreamId` to track the request
	/// If the internal stream buffer is full, `None` will be returned.
	pub async fn send_to_network(&mut self, request: AppData) -> Option<StreamId> {
		// Generate stream id
		let stream_id = StreamId::next(*self.current_stream_id.lock().await);
		let request = StreamData::Application(stream_id, request);

		// Add to request buffer
		if !self.stream_request_buffer.lock().await.insert(stream_id) {
			// Buffer appears to be full
			return None;
		}

		// Send request
		if let Ok(_) = self.application_sender.send(request).await {
			// Store latest stream id
			*self.current_stream_id.lock().await = stream_id;
			Some(stream_id)
		} else {
			return None;
		}
	}

	pub async fn recv_from_network(&mut self, stream_id: StreamId) -> NetworkResult {
		let network_data_future = async {
			loop {
				// tight loop
				if let Some(response_value) = self
					.stream_response_buffer
					.lock()
					.await
					.remove(&TrackableStreamId::Id(stream_id))
				{
					// Make sure the value is not `StreamId`. If it is, then the request is being
					// processed and the final value we want has not arrived.

					// Get inner boxed value
					let inner_value = &response_value as &dyn Any;
					if let None = inner_value.downcast_ref::<StreamId>() {
						return response_value;
					}
				}
			}
		};

		Ok(network_data_future
			.timeout(self.network_read_delay)
			.await
			.map_err(|_| NetworkError::NetworkReadTimeout)?)
	}

	/// Perform an atomic `send` and `recieve` from the network layer. This function is atomic and
	/// blocks until the result of the request is returned from the network layer. This function
	/// should mostly be used when the result of the request is needed immediately and delay can be
	/// condoned. It will still timeout if the delay exceeds the configured period.
	/// If the internal buffer is full, it will return an error.
	pub async fn fetch_from_network(&mut self, request: AppData) -> NetworkResult {
		// send request
		if let Some(stream_id) = self.send_to_network(request).await {
			// wait to recieve response from the network
			self.recv_from_network(stream_id).await
		} else {
			Err(NetworkError::StreamBufferOverflow)
		}
	}
}

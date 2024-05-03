use rand::random;
/// Copyright (c) 2024 Algorealm
use std::time::Instant;
use thiserror::Error;

use super::*;

/// Type to indicate the duration (in seconds) to wait for data from the network layer before timing
/// out
pub const NETWORK_READ_TIMEOUT: u64 = 60;

/// Data exchanged over a stream between the application and network layer
pub(super) enum StreamData {
	/// Application data sent over the stream
	Application(StreamId, AppData),
	/// Network data sent over the stream
	Network(StreamId, NetworkData),
}

/// Data sent from the application layer to the networking layer
#[derive(Debug)]
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
pub(super) enum NetworkData {
	/// A simple echo message
	Echo(StreamId, String),
	/// Dail peer
	DailPeer(MultiaddrString),
}

/// Results from Kademlia DHT operation
pub enum Kademlia {
	/// Return important information about the DHT, this will be increased shortly
	Info { protocol_id: String },
	/// Return the result of a DHT lookup operation
	Result(DhtOps),
}

/// Network error type containing errors encountered during network operations
#[derive(Error, Debug)]
pub enum NetworkError {
	#[error("timeout occured waiting for data from network layer")]
	NetworkReadTimeout,
	#[error("internal request stream buffer is full")]
	StreamBufferOverflow,
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

/// Type that specifies the result of querying the network layer
pub type NetworkResult = Result<Box<dyn StreamResponseType>, NetworkError>;

/// Marker trait that indicates a stream reponse data object
pub trait StreamResponseType
where
	Self: Send + Sync + 'static,
{
}

/// Macro that implements [`StreamResponseType`] for various types to occupy the
/// [`StreamResponseBuffer`]
macro_rules! impl_stream_response_for_types { ( $( $t:ident )* ) => {
	$(
		impl StreamResponseType for $t {}
	)* };
}

impl_stream_response_for_types!(u8 i8 u16 i16 u32 i32 u64 i64 u128 i128
	usize isize f32 f64 String bool Kademlia);

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
	buffer: HashMap<StreamId, Box<dyn StreamResponseType>>,
}

impl StreamResponseBuffer {
	/// Create a new request buffer
	pub fn new(buffer_size: usize) -> Self {
		Self {
			size: buffer_size,
			buffer: HashMap::new(),
		}
	}

	/// Push [`StreamId`]s into buffer.
	/// Returns `false` if the buffer is full and request cannot be stored
	pub fn insert(&mut self, id: StreamId, response: Box<dyn StreamResponseType>) -> bool {
		if self.buffer.len() < self.size {
			self.buffer.insert(id, response);
			return true;
		}
		false
	}

	/// Remove a [`StreamId`] from the buffer
	pub fn remove(&mut self, id: &StreamId) -> Option<Box<dyn StreamResponseType>> {
		self.buffer.remove(&id)
	}

	/// Check if buffer contains a value
	pub fn contains(&mut self, id: &StreamId) -> bool {
		self.buffer.contains_key(id)
	}
}

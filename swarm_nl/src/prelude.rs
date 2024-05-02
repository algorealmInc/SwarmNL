use libp2p_identity::{KeyType, PeerId};
/// Copyright (c) 2024 Algorealm
///  
/// This file is part of the SwarmNL library.
use std::time::Instant;
use thiserror::Error;

/// Library error type containing all custom errors that could be encountered
#[derive(Error, Debug)]
pub enum SwarmNlError {
	#[error("could not read bootstrap config file")]
	BoostrapFileReadError(String),
	#[error("could not parse data read from bootstrap config file")]
	BoostrapDataParseError(String),
	#[error("could not configure transport. It is likely not supported on machine")]
	TransportConfigError(TransportOpts),
	#[error("could not configure DNS resolution into transport")]
	DNSConfigError,
	#[error("could not configure the selected protocols")]
	ProtocolConfigError,
	#[error("could not listen on specified address")]
	MultiaddressListenError(String),
	#[error("could not dial remote peer")]
	RemotePeerDialError(String),
	#[error("could not parse provided network id")]
	NetworkIdParseError(String),
}

/// Generic SwarmNl result type
pub type SwarmNlResult<T> = Result<T, SwarmNlError>;
/// Port type
pub type Port = u16;
/// Seconds type
pub type Seconds = u64;
/// The stringified PeerId type
pub type PeerIdString = String;
/// The stringified Multiaddr type
pub type MultiaddrString = String;

/// Port ranges
pub const MIN_PORT: u16 = 49152;
pub const MAX_PORT: u16 = 65535;

/// Default network id
pub static DEFAULT_NETWORK_ID: &str = "/swarmnl/1.0";
/// Minimum network (protocol) id. This helps ensure that the protocol id is well formed and
/// contains a reasonable value because it is what identifies a network, makes it unique and
/// separates it from others.
pub static MIN_NETWORK_ID_LENGTH: u8 = 4;

/// Implement From<&str> for libp2p2_identity::KeyType.
/// We'll define a custom trait because of the Rust visibility rule to solve this problem
pub trait CustomFrom {
	fn from(string: &str) -> Option<Self>
	where
		Self: Sized;
}

impl CustomFrom for KeyType {
	fn from(s: &str) -> Option<Self> {
		match s.to_lowercase().as_str() {
			"ed25519" => Some(KeyType::Ed25519),
			"rsa" => Some(KeyType::RSA),
			"secp256k1" => Some(KeyType::Secp256k1),
			"ecdsa" => Some(KeyType::Ecdsa),
			_ => None,
		}
	}
}

/// Supported transport protocols
#[derive(Hash, Eq, PartialEq, Debug)]
pub enum TransportOpts {
	/// QUIC transport protocol enabled with TCP/IP as fallback.
	/// DNS lookup is also configured by default
	TcpQuic { tcp_config: TcpConfig },
}

/// TCP setup Config
#[derive(Hash, Eq, PartialEq, Debug, Clone, Copy)]
pub enum TcpConfig {
	/// Default configuration specified in the [libp2p docs](https://docs.rs/libp2p/latest/libp2p/tcp/struct.Config.html#method.new).
	Default,
	Custom {
		/// Configures the IP_TTL option for new sockets.
		ttl: u32,
		/// Configures the TCP_NODELAY option for new sockets.
		nodelay: bool,
		/// Configures the listen backlog for new listen sockets.
		backlog: u32,
		// false by default, we're not dealing with NAT traversal for now.
		// port_resuse: bool
	},
}

/// A unique type that indicates that a struct is not yet initialized to its default state
pub struct NotInitialiazed;

/// A unique type that indicates that a struct has been default configured
pub struct Initialized;

/// Data exchanged over a stream between the application and network layer
#[derive(Debug)]
pub enum StreamData {
	/// This is the first message sent through the stream from the networking layer to the
	/// application. It indicates a successful setup and readiness to begin operations.
	Ready,
	/// A simple echo message
	Echo(String),
	/// Application data sent over the stream
	Application(AppData),
	/// Network data sent over the stream
	Network(NetworkData),
}

/// Data sent from the application layer to the networking layer
#[derive(Debug)]
pub enum AppData {
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
}

/// Data sent from the networking layer to the application layer or to itself
#[derive(Debug)]
pub(crate) enum NetworkData {
	/// Return the result of a DHT lookup
	Kademlia(DhtOps),
	/// Return important information about the DHT, this will be increased shortly
	KademliaDhtInfo { protocol_id: String },
	/// Dail peer
	DailPeer(MultiaddrString),
}

/// Operations performed on the Kademlia DHT
#[derive(Debug)]
pub(crate) enum DhtOps {
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

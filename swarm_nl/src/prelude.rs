// Copyright 2024 Algorealm, Inc.
// Apache 2.0 License

//! Types and traits that are used throughout SwarmNL.

use libp2p_identity::KeyType;
use std::{collections::HashMap, net::Ipv4Addr};
use thiserror::Error;

/// Default IP address when no address is specified.
pub static DEFAULT_IP_ADDRESS: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

/// Default amount of time to keep a connection alive.
pub static DEFAULT_KEEP_ALIVE_DURATION: Seconds = 60;

/// Library error type containing all custom errors that could be encountered.
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
	#[error("could not configure node for gossiping")]
	GossipConfigError,
}

/// Generic SwarmNl result type.
pub type SwarmNlResult<T> = Result<T, SwarmNlError>;
/// Port type.
pub type Port = u16;
/// Seconds type.
pub type Seconds = u64;
/// The stringified `PeerId` type.
pub type PeerIdString = String;
/// The stringified `Multiaddr` type.
pub type MultiaddrString = String;
/// A collection of nodes described purely by their addresses
pub type Nodes = HashMap<PeerIdString, MultiaddrString>;

/// Lower bound port range (u16::MIN).
pub const MIN_PORT: u16 = 1000;
/// Upper bound port range (u16::MAX).
pub const MAX_PORT: u16 = 65535;

/// Default network ID.
pub static DEFAULT_NETWORK_ID: &str = "/swarmnl/1.0";
/// This constant sets the shortest acceptable length for a network ID.
/// The network ID identifies a network and ensures it's distinct from others.
pub static MIN_NETWORK_ID_LENGTH: u8 = 4;

/// An implementation of [`From<&str>`] for [`KeyType`] to read a key type from a bootstrap config
/// file.
///
/// We define a custom trait because of the Rust visibility rule.
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

/// Supported transport protocols.
#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub enum TransportOpts {
	/// QUIC transport protocol enabled with TCP/IP as fallback. DNS lookup is also configured by
	/// default.
	TcpQuic { tcp_config: TcpConfig },
}

/// TCP setup configuration.
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

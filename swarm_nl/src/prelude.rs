/// Copyright (c) 2024 Algorealm
use libp2p_identity::KeyType;
use libp2p_identity::{Keypair, rsa::Keypair as RsaKeypair};
use thiserror::Error;

/// Library error type containing all custom errors that could be encountered
#[derive(Error, Debug)]
pub enum SwarmNlError {
    #[error("could not read bootstrap config file")]
    BoostrapFileReadError(String),
    #[error("could not parse data read from bootstrap config file")]
    BoostrapDataParseError(String),
}

/// Generic SwarmNl result type
pub type SwarmNlResult<T> = Result<T, SwarmNlError>;
/// Port type
pub type Port = u16;

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

/// A wrapper for the cryptographic Keypair for node identification and message signing.
/// This is only necessary because of the distinction between an RSA keypair and the others
#[derive(Debug, Clone)]
pub enum WrappedKeyPair {
    Rsa(RsaKeypair),
    Other(Keypair),
}

impl WrappedKeyPair {
    /// Return the internal RSA keypair if it exists
    pub fn into_inner_rsa(&self) -> Option<RsaKeypair> {
        if let WrappedKeyPair::Rsa(keypair) = self {
            Some(keypair.clone())
        } else {
            None
        }
    }

    /// Return the internal keypair that is not of the RSA type
    pub fn into_inner(&self) -> Option<Keypair> {
        if let WrappedKeyPair::Other(keypair) = self {
            Some(keypair.clone())
        } else {
            None
        }
    }
}

/// Supported runtimes and executor
#[derive(Hash, Eq, PartialEq)]
pub enum Runtime {
    AsyncStd,
    Tokio
}

/// Supported transport protocols
#[derive(Hash, Eq, PartialEq)]
pub enum TransportOpts {
    /// TCP/IP transport protocol
    TCP(TcpConfig),
    /// QUIC transport protocol
    QUIC
}

/// TCP setup Config
#[derive(Hash, Eq, PartialEq)]
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
    }
}
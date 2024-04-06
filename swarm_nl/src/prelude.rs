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
pub enum Runtime {
    AsyncStd,
    Tokio
}

/// Supported transport protocols
pub enum Transport {
    /// TCP/IP transport protocol
    TCP,
    /// QUIC transport protocol
    QUIC
}
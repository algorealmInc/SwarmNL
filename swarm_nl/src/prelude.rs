/// Copyright (c) 2024 Algorealm
use libp2p_identity::KeyType;
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
/// We'll define a custom crate because of the Rust crate visibility rule to solve this problem
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

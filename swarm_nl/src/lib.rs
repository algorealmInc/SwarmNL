/// Copyright (c) 2024 Algorealm
mod prelude;
mod util;

/// re-export all the prelude content
pub use libp2p_identity::{KeyType, rsa::Keypair as RsaKeypair};

/// libp2p utilities
use crate::prelude::*;
use libp2p_identity::{Keypair, PeerId};

/// This module contains data structures and functions to setup a node identity and configure it for networking
pub mod setup {
    /// import the content of the [prelude module](crate::prelude) into this module
    use super::*;

    /// The Cryptographic Keypair for node identification and message signing.
    /// This is only necessary because of the distinction between an RSA keypair and the others
    #[derive(Debug)]
    enum KeypairType {
        Rsa(RsaKeypair),
        Other(Keypair)
    }

    /// Read the configuration from a config file
    #[derive(Debug)]
    pub struct BootstrapConfig {
        /// The port to listen on if using the TCP/IP protocol
        tcp_port: Port,
        /// The port to listen on if using the UDP or QUIC protocol
        udp_port: Port,
        /// The Cryptographic Keypair for node identification and message auth
        keypair: KeypairType,
    }

    impl BootstrapConfig {
        /// Read from a bootstrap config file on disk
        /// # Panics
        ///
        /// This function will panic if the file is not found at the specified path
        pub fn from_file(file_path: &str) -> Self {
            util::read_ini_file(file_path).unwrap()
        }

        /// Return a new `BootstrapConfig` struct populated by default (empty) values.
        /// Must be called first if the config is to be explicitly built without reading `.ini` file from disk
        pub fn new() -> Self {
            BootstrapConfig {
                // Default TCP/IP port if not specified
                tcp_port: 1509,
                // Default UDP port if not specified
                udp_port: 2707,
                // Default node keypair type i.e Ed25519
                keypair: KeypairType::Other(Keypair::generate_ed25519()),
            }
        }

        /// Configure the TCP/IP port
        pub fn with_tcp(self, tcp_port: Port) -> Self {
            BootstrapConfig { tcp_port, ..self }
        }

        /// Configure the UDP port
        pub fn with_udp(self, udp_port: Port) -> Self {
            BootstrapConfig { udp_port, ..self }
        }

        /// Generate a Cryptographic Keypair.
        /// Please note that calling this function overrides whatever might have been read from the `.ini` file
        pub fn generate_keypair(self, keytype: KeyType) -> Self {
            let keypair = match keytype {
                // generate a Ed25519 Keypair
                KeyType::Ed25519 => KeypairType::Other(Keypair::generate_ed25519()),
                KeyType::RSA => {
                    // first generate an Ed25519 Keypair and then try to cast it into RSA.
                    // Return an Ed25519 keyType if casting fails
                    let keypair = Keypair::generate_ed25519();
                    match keypair.clone().try_into_rsa() {
                        Ok(rsa_keypair) => KeypairType::Rsa(rsa_keypair),
                        Err(_) => KeypairType::Other(keypair)
                    }
                },
                KeyType::Secp256k1 => KeypairType::Other(Keypair::generate_secp256k1()),
                KeyType::Ecdsa => KeypairType::Other(Keypair::generate_ecdsa())
            };

            BootstrapConfig { keypair, ..self }
        }
    }
}

/// Copyright (c) 2024 Algorealm
mod prelude;
mod util;

pub use crate::prelude::*;
/// re-exports
pub use libp2p_identity::{rsa::Keypair as RsaKeypair, KeyType, Keypair};

/// This module contains data structures and functions to setup a node identity and configure it for networking
pub mod setup {
    /// import the contents of the exported modules into this module
    use super::*;

    /// Read the configuration from a config file
    #[derive(Debug)]
    pub struct BootstrapConfig {
        /// The port to listen on if using the TCP/IP protocol
        tcp_port: Port,
        /// The port to listen on if using the UDP or QUIC protocol
        udp_port: Port,
        /// The Cryptographic Keypair for node identification and message auth
        keypair: WrappedKeyPair,
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
                keypair: WrappedKeyPair::Other(Keypair::generate_ed25519()),
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
        pub fn generate_keypair(self, key_type: KeyType) -> Self {
            let keypair = match key_type {
                // generate a Ed25519 Keypair
                KeyType::Ed25519 => WrappedKeyPair::Other(Keypair::generate_ed25519()),
                KeyType::RSA => {
                    // first generate an Ed25519 Keypair and then try to cast it into RSA.
                    // Return an Ed25519 keyType if casting fails
                    let keypair = Keypair::generate_ed25519();
                    match keypair.clone().try_into_rsa() {
                        Ok(rsa_keypair) => WrappedKeyPair::Rsa(rsa_keypair),
                        Err(_) => WrappedKeyPair::Other(keypair),
                    }
                }
                KeyType::Secp256k1 => WrappedKeyPair::Other(Keypair::generate_secp256k1()),
                KeyType::Ecdsa => WrappedKeyPair::Other(Keypair::generate_ecdsa()),
            };

            BootstrapConfig { keypair, ..self }
        }

        /// Generate a Cryptographic Keypair from a protobuf format.
        /// This will override any already set keypair
        /// # Panics
        ///
        /// This function will panic if the `u8` buffer is not parsable into the specified key type
        pub fn generate_keypair_from_protobuf(self, key_type_str: &str, bytes: &mut [u8]) -> Self {
            // parse the key type
            let key_type = <KeyType as CustomFrom>::from(key_type_str)
                .ok_or(SwarmNlError::BoostrapDataParseError(
                    key_type_str.to_owned(),
                ))
                .unwrap();

            let raw_keypair = Keypair::from_protobuf_encoding(bytes).unwrap();
            let keypair = match key_type {
                // generate a Ed25519 Keypair
                KeyType::Ed25519 => {
                    WrappedKeyPair::Other(Keypair::try_into_ed25519(raw_keypair).unwrap().into())
                }
                // generate a RSA Keypair
                KeyType::RSA => WrappedKeyPair::Rsa(raw_keypair.try_into_rsa().unwrap()),
                // generate a Secp256k1 Keypair
                KeyType::Secp256k1 => {
                    WrappedKeyPair::Other(Keypair::try_into_secp256k1(raw_keypair).unwrap().into())
                }
                // generate a Ecdsa Keypair
                KeyType::Ecdsa => {
                    WrappedKeyPair::Other(Keypair::try_into_ecdsa(raw_keypair).unwrap().into())
                }
            };

            BootstrapConfig { keypair, ..self }
        }
    }
}

/// The module containing the core data structures for SwarmNl
mod core {
    use super::*;

    /// The core library struct
    struct Core {
        keypair: WrappedKeyPair,
    }

    impl Core {
        /// Serialize keypair to protobuf format and write to config file on disk.
        /// It returns a boolean to indicate success of operation.
        pub fn save_keypair_offline(&self, config_file_path: &str) -> bool {
            // check if key type is something other than RSA
            if let Some(keypair) = self.keypair.into_inner() {
                if let Ok(protobuf_keypair) = keypair.to_protobuf_encoding() {
                    // write to config file
                    return util::write_config(
                        "auth",
                        "protobuf_keypair",
                        &format!("{:?}", protobuf_keypair),
                        config_file_path,
                    );
                }
            } else {
                // it is most certainly RSA
            }

            false
        }
    }
}

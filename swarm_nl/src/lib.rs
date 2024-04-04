/// Copyright (c) 2024 Algorealm
mod prelude;
mod util;

/// re-export all the prelude content
use crate::prelude::*;

/// This module contains data structures and functions to setup a node identity and configure it for networking
mod setup {
    /// import the content of the [prelude module](crate::prelude) into this module
    use super::*;

    /// Read the configuration from a config file
    #[derive(Default, Debug)]
    pub struct BootstrapConfig {
        /// The port to listen on if using the TCP/IP protocol
        tcp_port: Port,
        /// The port to listen on if using the UDP or QUIC protocol
        udp_port: Port,
        /// The serialied keypair in byte format
        protobuf_keypair: Vec<u8>,
    }

    impl BootstrapConfig {
        /// Read from a bootstrap config file on disk
        /// # Panics
        ///
        /// This function may panic if the file is not found at the specified path
        pub fn from_file(file_path: &str) -> Self {
            util::read_ini_file(file_path).unwrap()
        }

        /// Return a new `BootstrapConfig` struct populated by default (empty) values.
        /// Must be called first if the config is to be explicitly built without reading from disk
        pub fn new() -> Self {
            Default::default()
        }

        /// Configure the TCP/IP port
        pub fn with_tcp(mut self, tcp_port: Port) -> Self {
            BootstrapConfig { tcp_port, ..self }
        }

        /// Configure the TCP/IP port
        pub fn with_udp(mut self, udp_port: Port) -> Self {
            BootstrapConfig { udp_port, ..self }
        }
    }
}

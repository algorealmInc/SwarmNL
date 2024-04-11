/// Copyright (c) 2024 Algorealm
mod prelude;
mod util;

/// Re-exports
pub use crate::prelude::*;
pub use libp2p_identity::{rsa::Keypair as RsaKeypair, KeyType, Keypair};

/// This module contains data structures and functions to setup a node identity and configure it for networking
pub mod setup {
    use std::collections::HashMap;

    /// Import the contents of the exported modules into this module
    use super::*;

    /// Configuration data required for node bootstrap
    #[derive(Debug)]
    pub struct BootstrapConfig {
        /// The port to listen on if using the TCP/IP protocol
        tcp_port: Port,
        /// The port to listen on if using the UDP or QUIC protocol
        udp_port: Port,
        /// The Cryptographic Keypair for node identification and message auth
        keypair: WrappedKeyPair,
        /// Bootstrap peers
        boot_nodes: HashMap<PeerIdString, MultiaddrString>
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
                boot_nodes: Default::default()
            }
        }

        /// Configure available bootnodes
        pub fn with_bootnodes(self, boot_nodes: HashMap<PeerIdString, MultiaddrString>) -> Self {
            BootstrapConfig { boot_nodes, ..self }
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
        /// TODO! Generate RSA properly by reading from its binary
        pub fn generate_keypair(self, key_type: KeyType) -> Self {
            let keypair = match key_type {
                // Generate a Ed25519 Keypair
                KeyType::Ed25519 => WrappedKeyPair::Other(Keypair::generate_ed25519()),
                KeyType::RSA => {
                    // First generate an Ed25519 Keypair and then try to cast it into RSA.
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
            // Parse the key type
            let key_type = <KeyType as CustomFrom>::from(key_type_str)
                .ok_or(SwarmNlError::BoostrapDataParseError(
                    key_type_str.to_owned(),
                ))
                .unwrap();

            let raw_keypair = Keypair::from_protobuf_encoding(bytes).unwrap();
            let keypair = match key_type {
                // Generate a Ed25519 Keypair
                KeyType::Ed25519 => {
                    WrappedKeyPair::Other(Keypair::try_into_ed25519(raw_keypair).unwrap().into())
                }
                // Generate a RSA Keypair
                KeyType::RSA => WrappedKeyPair::Rsa(raw_keypair.try_into_rsa().unwrap()),
                // Generate a Secp256k1 Keypair
                KeyType::Secp256k1 => {
                    WrappedKeyPair::Other(Keypair::try_into_secp256k1(raw_keypair).unwrap().into())
                }
                // Generate a Ecdsa Keypair
                KeyType::Ecdsa => {
                    WrappedKeyPair::Other(Keypair::try_into_ecdsa(raw_keypair).unwrap().into())
                }
            };

            BootstrapConfig { keypair, ..self }
        }

        /// Return a node's (wrapped) cryptographic keypair
        pub fn keypair(&self) -> WrappedKeyPair {
            self.keypair.clone()
        }

        /// Return the configured ports in a tuple i.e (TCP Port, UDP port)
        pub fn ports(&self) -> (Port, Port) {
            (self.tcp_port, self.udp_port)
        }   
    }
}

/// The module containing the core data structures for SwarmNl
mod core {
    use std::{
        net::{IpAddr, Ipv4Addr},
        time::Duration, collections::HashMap,
    };

    use libp2p::{
        kad::{self, store::MemoryStore}, noise, ping, swarm::NetworkBehaviour, tcp, tls, yamux, SwarmBuilder, Multiaddr, multiaddr::{Protocol, self}, Swarm, StreamProtocol,
    };
    use libp2p_identity::PeerId;
    use futures::channel::mpsc;

    use super::*;
    use crate::setup::BootstrapConfig;

    /// The Core Behaviour implemented which highlights the various protocols
    /// we'll be adding support for
    #[derive(NetworkBehaviour)]
    #[behaviour(to_swarm = "CoreEvent")]
    struct CoreBehaviour {
        ping: ping::Behaviour,
        kademlia: kad::Behaviour<MemoryStore>
    }

    /// Network events generated as a result of supported and configured `NetworkBehaviour`'s
    #[derive(Debug)]
    enum CoreEvent {
        Ping(ping::Event),
        Kademlia(kad::Event),
    }

    /// Implement ping events for [`CoreEvent`]
    impl From<ping::Event> for CoreEvent {
        fn from(event: ping::Event) -> Self {
            CoreEvent::Ping(event)
        }
    }

    /// Implement kademlia events for [`CoreEvent`]
    impl From<kad::Event> for CoreEvent {
        fn from(event: kad::Event) -> Self {
            CoreEvent::Kademlia(event)
        }
    }

    /// Structure containing necessary data to build [`Core`]
    pub struct CoreBuilder {
        network_id: StreamProtocol,
        keypair: WrappedKeyPair,
        tcp_udp_port: (Port, Port),
        boot_nodes: HashMap<PeerIdString, MultiaddrString>,
        ip_address: IpAddr,
        provider: Runtime,
        /// Connection keep-alive duration while idle
        keep_alive_duration: Seconds,
        transport: TransportOpts, // Maybe this can be a collection in the future to support additive transports
        /// The `Behaviour` of the `Ping` protocol
        ping: ping::Behaviour,
        /// The `Behaviour` of the `Kademlia` protocol
        kademlia: kad::Behaviour<kad::store::MemoryStore>
    }

    impl CoreBuilder {
        /// Return a [`CoreBuilder`] struct configured with [BootstrapConfig](setup::BootstrapConfig) and default values. 
        /// Here, it is certain that [BootstrapConfig](setup::BootstrapConfig) contains valid data
        pub fn with_config(config: BootstrapConfig) -> Self {
            // The default network id
            let network_id = "/swarmnl/1.0";

            // TCP/IP and QUIC are supported by default
            let default_transport = TransportOpts::TcpQuic {
                tcp_config: TcpConfig::Default,
            };

            // Peer Id
            let peer_id = config.keypair().into_inner().unwrap().public().to_peer_id();

            // Set up default config for Kademlia
            let mut cfg = kad::Config::default();
            cfg.set_protocol_names(vec![StreamProtocol::new(network_id)]);

            let store = kad::store::MemoryStore::new(peer_id);
            let kademlia = kad::Behaviour::with_config(peer_id, store, cfg);

            // Initialize struct with information from `BootstrapConfig`
            CoreBuilder {
                network_id: StreamProtocol::new(network_id),
                keypair: config.keypair(),
                tcp_udp_port: config.ports(),
                boot_nodes: Default::default(),
                // Default is to listen on all interfaces (ipv4)
                ip_address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                // Runtime & executor, tokio by default
                provider: Runtime::Tokio,
                // Default to 60 seconds
                keep_alive_duration: 60,
                transport: default_transport,
                ping: Default::default(),
                kademlia
            }
        }

        /// TODO! To be able to explitcly provide a network id for the network
        pub fn with_network_id(self, id: &str) -> Self {
            unimplemented!()
        }

        /// Configure the IP address to listen on
        /// TODO! Accept custom domain names e.g swarmnl.com
        /// TODO! Type-stating
        pub fn listen_on(self, ip_address: IpAddr) -> Self {
            CoreBuilder { ip_address, ..self }
        }

        /// How long to keep a connection alive once it is idling, in seconds
        pub fn with_idle_connection_timeout(self, keep_alive_duration: Seconds) -> Self {
            CoreBuilder { keep_alive_duration, ..self }
        }

        /// Configure the `Ping` protocol for the network
        pub fn with_ping(self, config: PingConfig) -> Self {
            // Set the ping protocol
            CoreBuilder {
                ping: ping::Behaviour::new(
                    ping::Config::new()
                        .with_interval(config.interval)
                        .with_timeout(config.timeout),
                ),
                ..self
            }
        }

        /// Configure the `Kademlia` protocol for the network
        pub fn with_kademlia(self, config: kad::Config) -> Self {
            // PeerId
            let peer_id = self.keypair.into_inner().unwrap().public().to_peer_id();
            let store = kad::store::MemoryStore::new(peer_id);
            let kademlia = kad::Behaviour::with_config(peer_id, store, config);

            CoreBuilder { kademlia, ..self }
        }

        /// Configure the Runtime & Executor to support.
        /// It's basically async-std vs tokio. 
        /// Tokio does not yet support DNS translation, so you have to splicitly spec
        pub fn with_provider(self, provider: Runtime) -> Self {
            CoreBuilder { provider, ..self }
        }

        /// Configure the transports to support
        pub fn with_transports(self, transport: TransportOpts) -> Self {
            CoreBuilder { transport, ..self }
        }

        /// Build the [`Core`] data structure
        pub async fn build(self) -> SwarmNlResult<Core>  {
            // Build and configure the libp2p Swarm structure. Thereby configuring the selected transport protocols, behaviours and node identity.
            // The Swarm is wrapped in the Core construct which serves as the interface to interact with the internal networking layer
            let mut swarm = if self.provider == Runtime::AsyncStd {
                // Configure for async-std

                // Configure transports
                let swarm_builder: SwarmBuilder<_, _> = match self.transport {
                    TransportOpts::TcpQuic { tcp_config } => match tcp_config {
                        TcpConfig::Default => {
                            // Use the default config
                            libp2p::SwarmBuilder::with_existing_identity(
                                self.keypair.into_inner().unwrap(),
                            )
                            .with_async_std()
                            .with_tcp(
                                tcp::Config::default(),
                                (tls::Config::new, noise::Config::new),
                                yamux::Config::default,
                            ).map_err(|_| SwarmNlError::TransportConfigError(TransportOpts::TcpQuic { tcp_config: TcpConfig::Default }))?
                            .with_quic()
                            .with_dns().await.map_err(|_| SwarmNlError::DNSConfigError)?
                        }

                        TcpConfig::Custom {
                            ttl,
                            nodelay,
                            backlog,
                        } => {
                            // Use the provided config
                            let tcp_config = tcp::Config::default()
                                .ttl(ttl)
                                .nodelay(nodelay)
                                .listen_backlog(backlog);

                            libp2p::SwarmBuilder::with_existing_identity(
                                self.keypair.into_inner().unwrap(),
                            )
                            .with_async_std()
                            .with_tcp(
                                tcp_config,
                                (tls::Config::new, noise::Config::new),
                                yamux::Config::default,
                            ).map_err(|_| SwarmNlError::TransportConfigError(TransportOpts::TcpQuic { tcp_config: TcpConfig::Custom { ttl, nodelay, backlog } }))?
                            .with_quic()
                            .with_dns().await.map_err(|_| SwarmNlError::DNSConfigError)?
                        }
                    },
                };

                // Configure the selected protocols and their corresponding behaviours
                swarm_builder
                    .with_behaviour(|_| 
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping,
                            kademlia: self.kademlia
                        }
                    ).map_err(|_| SwarmNlError::ProtocolConfigError)?
                    .with_swarm_config(|cfg| {
                        cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
                    })
                    .build()
            } else {
                // We're dealing with tokio here
                // Configure transports
                let swarm_builder: SwarmBuilder<_, _> = match self.transport {
                    TransportOpts::TcpQuic { tcp_config } => match tcp_config {
                        TcpConfig::Default => {
                            // Use the default config
                            libp2p::SwarmBuilder::with_existing_identity(
                                self.keypair.into_inner().unwrap(),
                            )
                            .with_tokio()
                            .with_tcp(
                                tcp::Config::default(),
                                (tls::Config::new, noise::Config::new),
                                yamux::Config::default,
                            ).map_err(|_| SwarmNlError::TransportConfigError(TransportOpts::TcpQuic { tcp_config: TcpConfig::Default }))?
                            .with_quic()
                        }

                        TcpConfig::Custom {
                            ttl,
                            nodelay,
                            backlog,
                        } => {
                            // Use the provided config
                            let tcp_config = tcp::Config::default()
                                .ttl(ttl)
                                .nodelay(nodelay)
                                .listen_backlog(backlog);

                            libp2p::SwarmBuilder::with_existing_identity(
                                self.keypair.into_inner().unwrap(),
                            )
                            .with_tokio()
                            .with_tcp(
                                tcp_config,
                                (tls::Config::new, noise::Config::new),
                                yamux::Config::default,
                            ).map_err(|_| SwarmNlError::TransportConfigError(TransportOpts::TcpQuic { tcp_config: TcpConfig::Custom { ttl, nodelay, backlog } }))?
                            .with_quic()
                        }
                    },
                };

                // Configure the selected protocols and their corresponding behaviours
                swarm_builder
                    .with_behaviour(|_| 
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping,
                            kademlia: self.kademlia
                        }
                    ).map_err(|_| SwarmNlError::ProtocolConfigError)?
                    .with_swarm_config(|cfg| {
                        cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
                    })
                    .build()
            };
                    
            // Configure the transport multiaddress and begin listening.
            // It can handle multiple future tranports based on configuration e.g WebRTC
            match self.transport {
                // TCP/IP and QUIC
                TransportOpts::TcpQuic { tcp_config } => {
                    // Configure TCP/IP multiaddress
                    let listen_addr_tcp = Multiaddr::empty()
                    .with(match self.ip_address {
                        IpAddr::V4(address) => Protocol::from(address),
                        IpAddr::V6(address) => Protocol::from(address),
                    })
                    .with(Protocol::Tcp(self.tcp_udp_port.0));

                    // Configure QUIC multiaddress
                    let listen_addr_quic = Multiaddr::empty()
                    .with(match self.ip_address {
                        IpAddr::V4(address) => Protocol::from(address),
                        IpAddr::V6(address) => Protocol::from(address),
                    })
                    .with(Protocol::Udp(self.tcp_udp_port.1))
                    .with(Protocol::QuicV1);

                    // Begin listening
                    swarm.listen_on(listen_addr_tcp.clone()).map_err(|_| SwarmNlError::MultiaddressListenError(listen_addr_tcp.to_string()))?;
                    swarm.listen_on(listen_addr_quic.clone()).map_err(|_| SwarmNlError::MultiaddressListenError(listen_addr_quic.to_string()))?;
                }
            }
           
            // Add bootnodes to local routing table, if any
            for peer_info in self.boot_nodes {
                // PeerId
                if let Ok(peer_id) = PeerId::from_bytes(peer_info.0.as_bytes()) {
                    // Multiaddress
                    if let Ok(multiaddr) = multiaddr::from_url(&peer_info.1) {
                        swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, multiaddr.clone());

                        // Dial them
                        swarm.dial(multiaddr.clone()).map_err(|_| SwarmNlError::RemotePeerDialError(multiaddr.to_string()))?;
                    }
                }
            }

            // TODO!
            // There must be a way for the application to communicate with the underlying networking core.
            // This could be pro-active or reactive. We will open a single-consumer (the core) and multiple producers stream to serve as 
            // the bridge from the application layer to the networking layer.
            // let (msg_sender, msg_receiver) = mpsc::channel::<ChannelMsg>(0);

            // passing to the appication:
            // multiple producers: to send commands to the network
            // single comsumer: To recieve data from the network
            // They are diffrent streams

            // Listening for commands 

            // Pushing data 

            // A stream wer're polling
            // Messages are coming in from the application layer
            // switch (msg) {
            //     case StreamBridge::Gossip {
            //         topic_id,
            //         peer_id,
            //         data = "blow_kitchen::deji15"
            //     }
            //     handle_it
            //     case 'y':
            //     handle_it
            //     ...
            // }

            
            // Spin up a loop that polls the event stream and other important streams. This will be run in a separate asynchronous task
            // if self.provider == Runtime::Tokio {
            //     // spinup a tokio task
            //     tokio
            // } else {
            //     // async-std
            // }

            // Build the network core
            Ok(Core {
                keypair: self.keypair,
                swarm
            })
        }
    }

    /// The core library struct for SwarmNl
    struct Core {
        keypair: WrappedKeyPair,
        swarm: Swarm<CoreBehaviour>
    }

    impl Core {
        /// Serialize keypair to protobuf format and write to config file on disk.
        /// It returns a boolean to indicate success of operation.
        /// Only key types other than RSA can be serialized to protobuf format for now
        /// 
        /// ; TODO; Save keyType automatically to file
        pub fn save_keypair_offline(&self, config_file_path: &str) -> bool {
            // Check if key type is something other than RSA
            if let Some(keypair) = self.keypair.into_inner() {
                if let Ok(protobuf_keypair) = keypair.to_protobuf_encoding() {
                    // Write to config file
                    return util::write_config(
                        "auth",
                        "protobuf_keypair",
                        &format!("{:?}", protobuf_keypair),
                        config_file_path,
                    );
                }
            } else {
                // It is most certainly RSA
            }

            false
        }
    }

    /// The configuration for the `Ping` protocol
    pub struct PingConfig {
        /// The interval between successive pings.
        /// Default is 15 seconds
        pub interval: Duration,
        /// The duration before which the request is considered failure.
        /// Default is 20 seconds
        pub timeout: Duration,
    }
}

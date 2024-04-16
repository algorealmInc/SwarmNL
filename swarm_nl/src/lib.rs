/// Copyright (c) 2024 Algorealm
///
/// This file is part of the SwarmNL library.
mod prelude;
mod util;

/// Re-exports
pub use crate::prelude::*;
pub use futures::{
    channel::mpsc::{self, Receiver, Sender},
    SinkExt, StreamExt,
};
pub use libp2p::{
    core::{transport::ListenerId, ConnectedPoint, Multiaddr},
    swarm::ConnectionId,
};
pub use libp2p_identity::{rsa::Keypair as RsaKeypair, KeyType, Keypair, PeerId};

/// The module containing the data structures and functions to setup a node identity and configure it for networking.
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
        boot_nodes: HashMap<PeerIdString, MultiaddrString>,
    }

    impl BootstrapConfig {
        /// Read from a bootstrap config file on disk
        ///
        /// # Panics
        ///
        /// This function will panic if the file is not found at the specified path.
        pub fn from_file(file_path: &str) -> Self {
            util::read_ini_file(file_path).unwrap()
        }

        /// Return a new `BootstrapConfig` struct populated by default (empty) values.
        ///
        /// Must be called first if the config is to be explicitly built without reading `.ini` file from disk
        pub fn new() -> Self {
            BootstrapConfig {
                // Default TCP/IP port if not specified
                tcp_port: 49352,
                // Default UDP port if not specified
                udp_port: 49852,
                // Default node keypair type i.e Ed25519
                keypair: WrappedKeyPair::Other(Keypair::generate_ed25519()),
                boot_nodes: Default::default(),
            }
        }

        /// Configure available bootnodes
        pub fn with_bootnodes(self, boot_nodes: HashMap<PeerIdString, MultiaddrString>) -> Self {
            BootstrapConfig { boot_nodes, ..self }
        }

        /// Configure the TCP/IP port
        /// Port must range between [`MIN_PORT`] and [`MAX_PORT`]
        pub fn with_tcp(self, tcp_port: Port) -> Self {
            if tcp_port > MIN_PORT && tcp_port < MAX_PORT {
                BootstrapConfig { tcp_port, ..self }
            } else {
                self
            }
        }

        /// Configure the UDP port
        /// Port must range between [`MIN_PORT`] and [`MAX_PORT`]
        pub fn with_udp(self, udp_port: Port) -> Self {
            if udp_port > MIN_PORT && udp_port < MAX_PORT {
                BootstrapConfig { udp_port, ..self }
            } else {
                self
            }
        }

        /// Generate a Cryptographic Keypair.
        ///
        /// Please note that calling this function overrides whatever might have been read from the `.ini` file
        ///
        /// TODO! Generate RSA properly by reading from its binary.
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
        ///
        /// This will override any already set keypair.
        ///
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

/// The module containing the core data structures for SwarmNl.
pub mod core {
    use std::{
        collections::HashMap,
        io::Error,
        net::{IpAddr, Ipv4Addr},
        num::NonZeroU32,
        time::Duration,
    };

    use futures::{
        channel::mpsc::{self, Receiver, Sender},
        SinkExt, StreamExt,
    };
    use libp2p::{
        identify::{self, Info},
        kad::{self, store::MemoryStore},
        multiaddr::{self, Protocol},
        noise,
        ping::{self, Failure},
        swarm::{ConnectionError, NetworkBehaviour, SwarmEvent},
        tcp, tls, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder, TransportError,
    };

    use super::*;
    use crate::setup::BootstrapConfig;

    /// The Core Behaviour implemented which highlights the various protocols
    /// we'll be adding support for
    #[derive(NetworkBehaviour)]
    #[behaviour(to_swarm = "CoreEvent")]
    struct CoreBehaviour {
        ping: ping::Behaviour,
        kademlia: kad::Behaviour<MemoryStore>,
        identify: identify::Behaviour,
    }

    /// Network events generated as a result of supported and configured `NetworkBehaviour`'s
    #[derive(Debug)]
    enum CoreEvent {
        Ping(ping::Event),
        Kademlia(kad::Event),
        Identify(identify::Event),
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

    /// Implement identify events for [`CoreEvent`]
    impl From<identify::Event> for CoreEvent {
        fn from(event: identify::Event) -> Self {
            CoreEvent::Identify(event)
        }
    }

    /// Structure containing necessary data to build [`Core`]
    pub struct CoreBuilder<T: EventHandler + Send + Sync + 'static> {
        network_id: StreamProtocol,
        keypair: WrappedKeyPair,
        tcp_udp_port: (Port, Port),
        boot_nodes: HashMap<PeerIdString, MultiaddrString>,
        /// the network event handler
        handler: T,
        ip_address: IpAddr,
        /// Connection keep-alive duration while idle
        keep_alive_duration: Seconds,
        transport: TransportOpts, // Maybe this can be a collection in the future to support additive transports
        /// The `Behaviour` of the `Ping` protocol
        ping: (ping::Behaviour, PingErrorPolicy),
        /// The `Behaviour` of the `Kademlia` protocol
        kademlia: kad::Behaviour<kad::store::MemoryStore>,
        /// The `Behaviour` of the `Identify` protocol
        identify: identify::Behaviour,
    }

    impl<T: EventHandler + Send + Sync + 'static> CoreBuilder<T> {
        /// Return a [`CoreBuilder`] struct configured with [`BootstrapConfig`] and default values.
        /// Here, it is certain that [`BootstrapConfig`] contains valid data.
        /// A type that implements [`EventHandler`] is passed to handle and react to network events.
        pub fn with_config(config: BootstrapConfig, handler: T) -> Self {
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

            // Set up default config config for Kademlia
            let cfg = identify::Config::new(
                network_id.to_owned(),
                config.keypair().into_inner().unwrap().public(),
            )
            .with_push_listen_addr_updates(true);
            let identify = identify::Behaviour::new(cfg);

            // Initialize struct with information from `BootstrapConfig`
            CoreBuilder {
                network_id: StreamProtocol::new(network_id),
                keypair: config.keypair(),
                tcp_udp_port: config.ports(),
                boot_nodes: Default::default(),
                handler,
                // Default is to listen on all interfaces (ipv4)
                ip_address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                // Default to 60 seconds
                keep_alive_duration: 60,
                transport: default_transport,
                // The peer will be disconnected after 20 successive timeout errors are recorded
                ping: (
                    Default::default(),
                    PingErrorPolicy::DisconnectAfterMaxTimeouts(20),
                ),
                kademlia,
                identify,
            }
        }

        /// Explicitly configure the network (protocol) id e.g /swarmnl/1.0.
        /// Note that it must be of the format "/protocol-name/version" else it will default to "/swarmnl/1.0"
        pub fn with_network_id(self, protocol: String) -> Self {
            if protocol.len() > 2 && protocol.starts_with("/") {
                CoreBuilder {
                    network_id: StreamProtocol::try_from_owned(protocol).unwrap(),
                    ..self
                }
            } else {
                self
            }
        }

        /// Configure the IP address to listen on
        pub fn listen_on(self, ip_address: IpAddr) -> Self {
            CoreBuilder { ip_address, ..self }
        }

        /// Configure how long to keep a connection alive (in seconds) once it is idling.
        pub fn with_idle_connection_timeout(self, keep_alive_duration: Seconds) -> Self {
            CoreBuilder {
                keep_alive_duration,
                ..self
            }
        }

        /// Configure the `Ping` protocol for the network.
        pub fn with_ping(self, config: PingConfig) -> Self {
            // Set the ping protocol
            CoreBuilder {
                ping: (
                    ping::Behaviour::new(
                        ping::Config::new()
                            .with_interval(config.interval)
                            .with_timeout(config.timeout),
                    ),
                    self.ping.1,
                ),
                ..self
            }
        }

        /// Configure the `Kademlia` protocol for the network.
        pub fn with_kademlia(self, config: kad::Config) -> Self {
            // PeerId
            let peer_id = self.keypair.into_inner().unwrap().public().to_peer_id();
            let store = kad::store::MemoryStore::new(peer_id);
            let kademlia = kad::Behaviour::with_config(peer_id, store, config);

            CoreBuilder { kademlia, ..self }
        }

        /// Configure the transports to support.
        pub fn with_transports(self, transport: TransportOpts) -> Self {
            CoreBuilder { transport, ..self }
        }

        /// Configure network event handler.
        /// This configures the functions to be called when various network events take place
        pub fn configure_network_events(self, handler: T) -> Self {
            CoreBuilder { handler, ..self }
        }

        /// Build the [`Core`] data structure.
        ///
        /// Handles the configuration of the libp2p Swarm structure and the selected transport protocols, behaviours and node identity.
        pub async fn build(self) -> SwarmNlResult<Core> {
            // Build and configure the libp2p Swarm structure. Thereby configuring the selected transport protocols, behaviours and node identity.
            // The Swarm is wrapped in the Core construct which serves as the interface to interact with the internal networking layer

            #[cfg(feature = "async-std-runtime")]
            let mut swarm = {
                // We're dealing with async-std here
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
                            )
                            .map_err(|_| {
                                SwarmNlError::TransportConfigError(TransportOpts::TcpQuic {
                                    tcp_config: TcpConfig::Default,
                                })
                            })?
                            .with_quic()
                            .with_dns()
                            .await
                            .map_err(|_| SwarmNlError::DNSConfigError)?
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
                            )
                            .map_err(|_| {
                                SwarmNlError::TransportConfigError(TransportOpts::TcpQuic {
                                    tcp_config: TcpConfig::Custom {
                                        ttl,
                                        nodelay,
                                        backlog,
                                    },
                                })
                            })?
                            .with_quic()
                            .with_dns()
                            .await
                            .map_err(|_| SwarmNlError::DNSConfigError)?
                        }
                    },
                };

                // Configure the selected protocols and their corresponding behaviours
                swarm_builder
                    .with_behaviour(|_|
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping.0,
                            kademlia: self.kademlia,
                            identify: self.identify
                        })
                    .map_err(|_| SwarmNlError::ProtocolConfigError)?
                    .with_swarm_config(|cfg| {
                        cfg.with_idle_connection_timeout(Duration::from_secs(
                            self.keep_alive_duration,
                        ))
                    })
                    .build()
            };

            #[cfg(feature = "tokio-runtime")]
            let mut swarm = {
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
                            )
                            .map_err(|_| {
                                SwarmNlError::TransportConfigError(TransportOpts::TcpQuic {
                                    tcp_config: TcpConfig::Default,
                                })
                            })?
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
                            )
                            .map_err(|_| {
                                SwarmNlError::TransportConfigError(TransportOpts::TcpQuic {
                                    tcp_config: TcpConfig::Custom {
                                        ttl,
                                        nodelay,
                                        backlog,
                                    },
                                })
                            })?
                            .with_quic()
                        }
                    },
                };

                // Configure the selected protocols and their corresponding behaviours
                swarm_builder
                    .with_behaviour(|_|
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping.0,
                            kademlia: self.kademlia,
                            identify: self.identify
                        })
                    .map_err(|_| SwarmNlError::ProtocolConfigError)?
                    .with_swarm_config(|cfg| {
                        cfg.with_idle_connection_timeout(Duration::from_secs(
                            self.keep_alive_duration,
                        ))
                    })
                    .build()
            };

            // Configure the transport multiaddress and begin listening.
            // It can handle multiple future tranports based on configuration e.g WebRTC
            match self.transport {
                // TCP/IP and QUIC
                TransportOpts::TcpQuic { tcp_config: _ } => {
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
                    #[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime"))]
                    swarm.listen_on(listen_addr_tcp.clone()).map_err(|_| {
                        SwarmNlError::MultiaddressListenError(listen_addr_tcp.to_string())
                    })?;

                    #[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime"))]
                    swarm.listen_on(listen_addr_quic.clone()).map_err(|_| {
                        SwarmNlError::MultiaddressListenError(listen_addr_quic.to_string())
                    })?;
                }
            }

            // Add bootnodes to local routing table, if any
            #[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime"))]
            for peer_info in self.boot_nodes {
                // PeerId
                if let Ok(peer_id) = PeerId::from_bytes(peer_info.0.as_bytes()) {
                    // Multiaddress
                    if let Ok(multiaddr) = multiaddr::from_url(&peer_info.1) {
                        swarm
                            .behaviour_mut()
                            .kademlia
                            .add_address(&peer_id, multiaddr.clone());

                        println!("fdinkmf");

                        // Dial them
                        swarm.dial(multiaddr.clone()).map_err(|_| {
                            SwarmNlError::RemotePeerDialError(multiaddr.to_string())
                        })?;
                    }
                }
            }

            // There must be a way for the application to communicate with the underlying networking core.
            // This will involve acceptiing data and pushing data to the application layer.
            // Two streams will be opened: The first mpsc stream will allow SwarmNL push data to the application and the application will comsume it (single consumer)
            // The second stream will have SwarmNl (being the consumer) recieve data and commands from multiple areas in the application;
            let (mut network_sender, application_receiver) = mpsc::channel::<StreamData>(3);
            let (application_sender, network_receiver) = mpsc::channel::<StreamData>(3);

            // Set up the ping network info.
            // `PeerId` does not implement `Default` so we will add the peerId of this node as seed and set the count to 0.
            // The count can NEVER increase because we cannot `Ping` ourselves.
            let peer_id = self.keypair.into_inner().unwrap().public().to_peer_id();

            // Timeouts
            let mut timeouts = HashMap::<PeerId, u16>::new();
            timeouts.insert(peer_id.clone(), 0);

            // Outbound errors
            let mut outbound_errors = HashMap::<PeerId, u16>::new();
            outbound_errors.insert(peer_id.clone(), 0);

            // Ping manager
            let manager = PingManager {
                timeouts,
                outbound_errors,
            };

            // Set up Ping network information
            let ping_info = PingInfo {
                policy: self.ping.1,
                manager,
            };

            // Construct the useful network information
            let network_info = NetworkInfo { ping: ping_info };

            // Build the network core
            let network_core = Core {
                keypair: self.keypair,
                application_sender,
                application_receiver,
            };

            // Send message to application to indicate readiness
            let _ = network_sender.send(StreamData::Ready).await;

            // Spin up task to handle messages from the application layer
            #[cfg(feature = "async-std-runtime")]
            async_std::task::spawn(Core::handle_app_stream(network_receiver));

            // Spin up task to handle the events generated by the network.
            // It notifies the application layer with important information over a (mpsc) stream
            #[cfg(feature = "async-std-runtime")]
            async_std::task::spawn(Core::handle_network_events(
                swarm,
                network_info,
                network_sender,
                self.handler,
            ));

            // Spin up task to handle messages from the application layer
            #[cfg(feature = "tokio-runtime")]
            tokio::task::spawn(Core::handle_app_stream(network_receiver));

            // Spin up task to handle the events generated by the network.
            // It notifies the application layer with important information over a (mpsc) stream
            #[cfg(feature = "tokio-runtime")]
            tokio::task::spawn(Core::handle_network_events(
                swarm,
                network_info,
                network_sender,
                self.handler,
            ));

            Ok(network_core)
        }
    }

    /// The core interface for the application layer to interface with the networking layer
    pub struct Core {
        keypair: WrappedKeyPair,
        /// The producing end of the stream that sends data to the network layer from the application
        pub application_sender: Sender<StreamData>,
        /// The consuming end of the stream that recieves data from the network layer
        pub application_receiver: Receiver<StreamData>,
    }

    impl Core {
        /// Serialize keypair to protobuf format and write to config file on disk.
        /// It returns a boolean to indicate success of operation.
        /// Only key types other than RSA can be serialized to protobuf format for now
        pub fn save_keypair_offline(&self, config_file_path: &str) -> bool {
            // Check if key type is something other than RSA
            if let Some(keypair) = self.keypair.into_inner() {
                if let Ok(protobuf_keypair) = keypair.to_protobuf_encoding() {
                    // Write key type and serialized array key to config file
                    return util::write_config(
                        "auth",
                        "protobuf_keypair",
                        &format!("{:?}", protobuf_keypair),
                        config_file_path,
                    ) && util::write_config(
                        "auth",
                        "Crypto",
                        &format!("{}", self.keypair.into_inner().unwrap().key_type()),
                        config_file_path,
                    );
                }
            } else {
                // It is most certainly RSA
            }

            false
        }

        /// Return the node's `PeerId`
        pub fn peer_id(&self) -> String {
            self.keypair
                .into_inner()
                .unwrap()
                .public()
                .to_peer_id()
                .to_string()
        }

        /// Handles streams coming from the application layer.
        async fn handle_app_stream(mut receiver: Receiver<StreamData>) {
            // Loop to handle incoming application streams indefinitely.
            loop {
                match receiver.next().await {
                    Some(stream_data) => {
                        // handle incoming stream data
                        match stream_data {
                            StreamData::Ready => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        /// Handles events generated by network activities.
        /// Important information are sent to the application layer over a (mpsc) stream
        async fn handle_network_events<T: EventHandler + Send + Sync + 'static>(
            mut swarm: Swarm<CoreBehaviour>,
            mut network_info: NetworkInfo,
            sender: Sender<StreamData>,
            handler: T,
        ) {
            // Loop to handle network events indefinitely.
            loop {
                match swarm.select_next_some().await {
                    SwarmEvent::NewListenAddr {
                        listener_id,
                        address,
                    } => {
                        // call configured handler
                        handler.new_listen_addr(listener_id, address);
                    }
                    SwarmEvent::Behaviour(event) => match event {
                        CoreEvent::Ping(ping::Event {
                            peer,
                            connection: _,
                            result,
                        }) => {
                            match result {
                                // Inbound ping succes
                                Ok(duration) => {
                                    // In handling the ping error policies, we only bump up an error count when there is CONCURRENT failure.
                                    // If the peer becomes responsive, its recorded error count decays by 50% on every success, until it gets to 1

                                    // Enforce a 50% decay on the count of outbound errors
                                    if let Some(err_count) =
                                        network_info.ping.manager.outbound_errors.get(&peer)
                                    {
                                        let new_err_count = (err_count / 2) as u16;
                                        network_info
                                            .ping
                                            .manager
                                            .outbound_errors
                                            .insert(peer, new_err_count);
                                    }

                                    // Enforce a 50% decay on the count of outbound errors
                                    if let Some(timeout_err_count) =
                                        network_info.ping.manager.timeouts.get(&peer)
                                    {
                                        let new_err_count = (timeout_err_count / 2) as u16;
                                        network_info
                                            .ping
                                            .manager
                                            .timeouts
                                            .insert(peer, new_err_count);
                                    }

                                    // Call custom handler
                                    handler.inbound_ping_success(peer, duration);
                                }
                                // Outbound ping failure
                                Err(err_type) => {
                                    // Handle error by examining selected policy
                                    match network_info.ping.policy {
                                        PingErrorPolicy::NoDisconnect => {
                                            // Do nothing, we can't disconnect from peer under any circumstances
                                        }
                                        PingErrorPolicy::DisconnectAfterMaxErrors(max_errors) => {
                                            // Disconnect after we've recorded a certain number of concurrent errors

                                            // Get peer entry for outbound errors or initialize peer
                                            let err_count = network_info
                                                .ping
                                                .manager
                                                .outbound_errors
                                                .entry(peer)
                                                .or_insert(0);

                                            if *err_count != max_errors {
                                                // Disconnect peer
                                                let _ = swarm.disconnect_peer_id(peer);

                                                // Remove entry to clear peer record incase it connects back and becomes responsive
                                                network_info
                                                    .ping
                                                    .manager
                                                    .outbound_errors
                                                    .remove(&peer);
                                            } else {
                                                // Bump the count up
                                                *err_count += 1;
                                            }
                                        }
                                        PingErrorPolicy::DisconnectAfterMaxTimeouts(
                                            max_timeout_errors,
                                        ) => {
                                            // Disconnect after we've recorded a certain number of concurrent TIMEOUT errors

                                            // First make sure we're dealing with only the timeout errors
                                            if let Failure::Timeout = err_type {
                                                // Get peer entry for outbound errors or initialize peer
                                                let err_count = network_info
                                                    .ping
                                                    .manager
                                                    .timeouts
                                                    .entry(peer)
                                                    .or_insert(0);

                                                if *err_count != max_timeout_errors {
                                                    // Disconnect peer
                                                    let _ = swarm.disconnect_peer_id(peer);

                                                    // Remove entry to clear peer record incase it connects back and becomes responsive
                                                    network_info
                                                        .ping
                                                        .manager
                                                        .timeouts
                                                        .remove(&peer);
                                                } else {
                                                    // Bump the count up
                                                    *err_count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        CoreEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                            result, ..
                        }) => match result {
                            kad::QueryResult::GetProviders(Ok(
                                kad::GetProvidersOk::FoundProviders { key, providers, .. },
                            )) => {
                                for peer in providers {
                                    println!(
                                        "Peer {peer:?} provides key {:?}",
                                        std::str::from_utf8(key.as_ref()).unwrap()
                                    );
                                }
                            }
                            kad::QueryResult::GetProviders(Err(err)) => {
                                eprintln!("Failed to get providers: {err:?}");
                            }
                            kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                                kad::PeerRecord {
                                    record: kad::Record { key, value, .. },
                                    ..
                                },
                            ))) => {
                                println!(
                                    "Got record {:?} {:?}",
                                    std::str::from_utf8(key.as_ref()).unwrap(),
                                    std::str::from_utf8(&value).unwrap(),
                                );
                            }
                            kad::QueryResult::GetRecord(Ok(_)) => {}
                            kad::QueryResult::GetRecord(Err(err)) => {
                                eprintln!("Failed to get record: {err:?}");
                            }
                            kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                                println!(
                                    "Successfully put record {:?}",
                                    std::str::from_utf8(key.as_ref()).unwrap()
                                );
                            }
                            kad::QueryResult::PutRecord(Err(err)) => {
                                eprintln!("Failed to put record: {err:?}");
                            }
                            kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                                println!(
                                    "Successfully put provider record {:?}",
                                    std::str::from_utf8(key.as_ref()).unwrap()
                                );
                            }
                            kad::QueryResult::StartProviding(Err(err)) => {
                                eprintln!("Failed to put provider record: {err:?}");
                            }
                            _ => {}
                        },
                        CoreEvent::Identify(event) => match event {
                            identify::Event::Received { peer_id, info } => {
                                // We just recieved an `Identify` info from a peer.
                                handler.identify_info_recieved(peer_id, info);
                            }
                            identify::Event::Sent { peer_id } => {
                                // We just recieved an `Identify` info from a peer.
                                println!("Sent it {}", peer_id);
                            }
                            // Remaining `Identify` events not actively handled
                            _ => {}
                        },
                        _ => {}
                    },
                    SwarmEvent::ConnectionEstablished {
                        peer_id,
                        connection_id,
                        endpoint,
                        num_established,
                        concurrent_dial_errors,
                        established_in,
                    } => {
                        // call configured handler
                        handler.connection_established(
                            peer_id,
                            connection_id,
                            &endpoint,
                            num_established,
                            concurrent_dial_errors,
                            established_in,
                        );
                    }
                    SwarmEvent::ConnectionClosed {
                        peer_id,
                        connection_id,
                        endpoint,
                        num_established,
                        cause,
                    } => {
                        // call configured handler
                        handler.connection_closed(
                            peer_id,
                            connection_id,
                            &endpoint,
                            num_established,
                            cause,
                        );
                    }
                    SwarmEvent::ExpiredListenAddr {
                        listener_id,
                        address,
                    } => {
                        // call configured handler
                        handler.expired_listen_addr(listener_id, address);
                    }
                    SwarmEvent::ListenerClosed {
                        listener_id,
                        addresses,
                        reason: _,
                    } => {
                        // call configured handler
                        handler.listener_closed(listener_id, addresses);
                    }
                    SwarmEvent::ListenerError {
                        listener_id,
                        error: _,
                    } => {
                        // call configured handler
                        handler.listener_error(listener_id);
                    }
                    SwarmEvent::Dialing {
                        peer_id,
                        connection_id,
                    } => {
                        // call configured handler
                        handler.dialing(peer_id, connection_id);
                    }
                    SwarmEvent::NewExternalAddrCandidate { address } => {
                        // call configured handler
                        handler.new_external_addr_candidate(address);
                    }
                    SwarmEvent::ExternalAddrConfirmed { address } => {
                        // call configured handler
                        handler.external_addr_confirmed(address);
                    }
                    SwarmEvent::ExternalAddrExpired { address } => {
                        // call configured handler
                        handler.external_addr_expired(address);
                    }
                    SwarmEvent::IncomingConnection {
                        connection_id,
                        local_addr,
                        send_back_addr,
                    } => {
                        // call configured handler
                        handler.incoming_connection(connection_id, local_addr, send_back_addr);
                    }
                    SwarmEvent::IncomingConnectionError {
                        connection_id,
                        local_addr,
                        send_back_addr,
                        error: _,
                    } => {
                        // call configured handler
                        handler.incoming_connection_error(
                            connection_id,
                            local_addr,
                            send_back_addr,
                        );
                    }
                    SwarmEvent::OutgoingConnectionError {
                        connection_id,
                        peer_id,
                        error: _,
                    } => {
                        // call configured handler
                        handler.outgoing_connection_error(connection_id, peer_id);
                    }
                    _ => todo!(),
                }
            }
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

    /// The high level trait that provides default implementations to handle most supported network swarm events.
    pub trait EventHandler {
        /// Event that informs the network core that we have started listening on a new multiaddr.
        fn new_listen_addr(&self, _listener_id: ListenerId, _addr: Multiaddr) {}

        /// Event that informs the network core about a newly established connection to a peer.
        fn connection_established(
            &self,
            _peer_id: PeerId,
            _connection_id: ConnectionId,
            _endpoint: &ConnectedPoint,
            _num_established: NonZeroU32,
            _concurrent_dial_errors: Option<Vec<(Multiaddr, TransportError<Error>)>>,
            _established_in: Duration,
        ) {
            // Default implementation
        }

        /// Event that informs the network core about a closed connection to a peer.
        fn connection_closed(
            &self,
            _peer_id: PeerId,
            _connection_id: ConnectionId,
            _endpoint: &ConnectedPoint,
            _num_established: u32,
            _cause: Option<ConnectionError>,
        ) {
            // Default implementation
        }

        /// Event that announces expired listen address.
        fn expired_listen_addr(&self, _listener_id: ListenerId, _address: Multiaddr) {
            // Default implementation
        }

        /// Event that announces a closed listener.
        fn listener_closed(&self, _listener_id: ListenerId, _addresses: Vec<Multiaddr>) {
            // Default implementation
        }

        /// Event that announces a listener error.
        fn listener_error(&self, _listener_id: ListenerId) {
            // Default implementation
        }

        /// Event that announces a dialing attempt.
        fn dialing(&self, _peer_id: Option<PeerId>, _connection_id: ConnectionId) {
            // Default implementation
        }

        /// Event that announces a new external address candidate.
        fn new_external_addr_candidate(&self, _address: Multiaddr) {
            // Default implementation
        }

        /// Event that announces a confirmed external address.
        fn external_addr_confirmed(&self, _address: Multiaddr) {
            // Default implementation
        }

        /// Event that announces an expired external address.
        fn external_addr_expired(&self, _address: Multiaddr) {
            // Default implementation
        }

        /// Event that announces new connection arriving on a listener and in the process of protocol negotiation.
        fn incoming_connection(
            &self,
            _connection_id: ConnectionId,
            _local_addr: Multiaddr,
            _send_back_addr: Multiaddr,
        ) {
            // Default implementation
        }

        /// Event that announces an error happening on an inbound connection during its initial handshake.
        fn incoming_connection_error(
            &self,
            _connection_id: ConnectionId,
            _local_addr: Multiaddr,
            _send_back_addr: Multiaddr,
        ) {
            // Default implementation
        }

        /// Event that announces an error happening on an outbound connection during its initial handshake.
        fn outgoing_connection_error(
            &self,
            _connection_id: ConnectionId,
            _peer_id: Option<PeerId>,
        ) {
            // Default implementation
        }

        /// Event that announces the arrival of a ping message from a peer.
        /// The duration it took for a round trip is also returned
        fn inbound_ping_success(&self, _peer_id: PeerId, _duration: Duration) {
            // Default implementation
        }

        /// Event that announces the arrival of a `PeerInfo` via the `Identify` protocol
        fn identify_info_recieved(&self, _peer_id: PeerId, info: Info) {
            // Default implementation
        }
    }

    /// Default network event handler
    pub struct DefaultHandler;

    /// Implement [`EventHandler`] for [`DefaultHandler`]
    impl EventHandler for DefaultHandler {}

    /// Policies to handle a `Ping` error
    /// - All connections to peers are closed during a disconnect operation.
    enum PingErrorPolicy {
        /// Do not disconnect under any circumstances
        NoDisconnect,
        /// Disconnect after a number of outbound errors
        DisconnectAfterMaxErrors(u16),
        /// Disconnect after a certain number of timeouts
        DisconnectAfterMaxTimeouts(u16),
    }

    /// Important information to obtain from the [`CoreBuilder`], to properly handle network operations
    struct NetworkInfo {
        /// Important information to manage `Ping` operations
        ping: PingInfo,
    }

    /// Struct that stores critical information for the execution of the [`PingErrorPolicy`]
    #[derive(Debug)]
    struct PingManager {
        /// The number of timeout errors encountered from a peer
        timeouts: HashMap<PeerId, u16>,
        /// The number of outbound errors encountered from a peer
        outbound_errors: HashMap<PeerId, u16>,
    }

    /// Critical information to manage `Ping` operations
    struct PingInfo {
        policy: PingErrorPolicy,
        manager: PingManager,
    }
}

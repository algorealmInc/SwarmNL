// Copyright 2024 Algorealm
// Apache 2.0 License

//! Core data structures and protocol implementations for building a swarm.

#![doc = include_str!("../../doc/core/NetworkBuilder.md")]
#![doc = include_str!("../../doc/core/ApplicationInteraction.md")]

use std::{
	collections::{vec_deque::IntoIter, HashMap, HashSet},
	fs,
	net::IpAddr,
	num::NonZeroU32,
	sync::Arc,
	time::Duration,
};

use base58::FromBase58;

use futures::{
	channel::mpsc::{self, Receiver, Sender},
	select, SinkExt, StreamExt,
};
use libp2p::{
	gossipsub::{self, IdentTopic, TopicHash},
	identify::{self},
	kad::{self, store::MemoryStore, Mode, Record, RecordKey},
	multiaddr::Protocol,
	noise,
	ping::{self, Failure},
	request_response::{self, cbor::Behaviour, ProtocolSupport},
	swarm::{behaviour, ConnectionError, NetworkBehaviour, SwarmEvent},
	tcp, tls, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder,
};

use self::{
	gossipsub_cfg::{Blacklist, GossipsubConfig, GossipsubInfo},
	ping_config::*,
};

use super::*;
use crate::{setup::BootstrapConfig, util::string_to_peer_id};

#[cfg(feature = "async-std-runtime")]
use async_std::sync::Mutex;

#[cfg(feature = "tokio-runtime")]
use tokio::sync::Mutex;

mod prelude;
pub use prelude::*;
mod tests;

/// The Core Behaviour implemented which highlights the various protocols
/// we'll be adding support for.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "CoreEvent")]
struct CoreBehaviour {
	request_response: request_response::cbor::Behaviour<Rpc, Rpc>,
	kademlia: kad::Behaviour<MemoryStore>,
	ping: ping::Behaviour,
	identify: identify::Behaviour,
	gossipsub: gossipsub::Behaviour,
}

/// Network events generated as a result of supported and configured [`NetworkBehaviour`]'s
#[derive(Debug)]
enum CoreEvent {
	Ping(ping::Event),
	RequestResponse(request_response::Event<Rpc, Rpc>),
	Kademlia(kad::Event),
	Identify(identify::Event),
	Gossipsub(gossipsub::Event),
}

/// Implement ping events for [`CoreEvent`].
impl From<ping::Event> for CoreEvent {
	fn from(event: ping::Event) -> Self {
		CoreEvent::Ping(event)
	}
}

/// Implement kademlia events for [`CoreEvent`].
impl From<kad::Event> for CoreEvent {
	fn from(event: kad::Event) -> Self {
		CoreEvent::Kademlia(event)
	}
}

/// Implement identify events for [`CoreEvent`].
impl From<identify::Event> for CoreEvent {
	fn from(event: identify::Event) -> Self {
		CoreEvent::Identify(event)
	}
}

/// Implement request_response events for [`CoreEvent`].
impl From<request_response::Event<Rpc, Rpc>> for CoreEvent {
	fn from(event: request_response::Event<Rpc, Rpc>) -> Self {
		CoreEvent::RequestResponse(event)
	}
}

/// Implement gossipsub events for [`CoreEvent`].
impl From<gossipsub::Event> for CoreEvent {
	fn from(event: gossipsub::Event) -> Self {
		CoreEvent::Gossipsub(event)
	}
}

/// Structure containing necessary data to build [`Core`].
pub struct CoreBuilder<T: Clone + Send + Sync + 'static> {
	/// The network ID of the network.
	network_id: StreamProtocol,
	/// The cryptographic keypair of the node.
	keypair: Keypair,
	/// The TCP and UDP ports to listen on.
	tcp_udp_port: (Port, Port),
	/// The bootnodes to connect to.
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
	/// The blacklist of peers to ignore.
	blacklist: Blacklist,
	/// The network data state
	state: Option<T>,
	/// The size of the stream buffers to use to track application requests to the network layer
	/// internally.
	stream_size: usize,
	/// The IP address to listen on.
	ip_address: IpAddr,
	/// Connection keep-alive duration while idle.
	keep_alive_duration: Seconds,
	/// The transport protocols being used.
	/// TODO: This can be a collection in the future to support additive transports.
	transport: TransportOpts,
	/// The `Behaviour` of the `Ping` protocol.
	ping: (ping::Behaviour, PingErrorPolicy),
	/// The `Behaviour` of the `Kademlia` protocol.
	kademlia: kad::Behaviour<kad::store::MemoryStore>,
	/// The `Behaviour` of the `Identify` protocol.
	identify: identify::Behaviour,
	/// The `Behaviour` of the `Request-Response` protocol. The second field value is the function
	/// to handle an incoming request from a peer.
	request_response: (Behaviour<Rpc, Rpc>, fn(RpcData) -> RpcData),
	/// The `Behaviour` of the `GossipSub` protocol. The second tuple value is a filter function
	/// that filters incoming gossip before passing it to the application
	gossipsub: (
		gossipsub::Behaviour,
		fn(PeerId, MessageId, Option<PeerId>, String, Vec<String>) -> bool,
	),
}

impl<T: Clone + Send + Sync + 'static> CoreBuilder<T> {
	/// Return a [`CoreBuilder`] struct configured with [`BootstrapConfig`] and default values.
	/// Here, it is certain that [`BootstrapConfig`] contains valid data.
	/// A type that implements [`EventHandler`] is passed to handle and responde to network events.
	pub fn with_config(config: BootstrapConfig) -> Self {
		// The default network id
		let network_id = DEFAULT_NETWORK_ID;

		// The default transports (TCP/IP and QUIC)
		let default_transport = TransportOpts::TcpQuic {
			tcp_config: TcpConfig::Default,
		};

		// The peer ID of the node
		let peer_id = config.keypair().public().to_peer_id();

		// Set up default config for Kademlia
		let mut cfg = kad::Config::default();
		cfg.set_protocol_names(vec![StreamProtocol::new(network_id)]);
		let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));

		// Set up default config for Kademlia
		let cfg = identify::Config::new(network_id.to_owned(), config.keypair().public())
			.with_push_listen_addr_updates(true);
		let identify = identify::Behaviour::new(cfg);

		// Set up default config for Request-Response
		let request_response_behaviour = Behaviour::new(
			[(StreamProtocol::new(network_id), ProtocolSupport::Full)],
			request_response::Config::default(),
		);

		// Set up default config for gossiping
		let cfg = gossipsub::Config::default();
		let gossipsub_behaviour = gossipsub::Behaviour::new(
			gossipsub::MessageAuthenticity::Signed(config.keypair()),
			cfg,
		)
		.map_err(|_| SwarmNlError::GossipConfigError)
		.unwrap();

		// The filter function to handle incoming gossip data.
		// The default behaviour is to allow all incoming messages
		let gossip_filter_fn = |_, _, _, _, _| true;

		// Set up default config for RPC handling.
		// The incoming RPC will simply be forwarded back to it's sender
		let rpc_handler_fn = |incoming_data: RpcData| incoming_data;

		// Initialize struct with information from `BootstrapConfig`
		CoreBuilder {
			network_id: StreamProtocol::new(network_id),
			keypair: config.keypair(),
			tcp_udp_port: config.ports(),
			boot_nodes: config.bootnodes(),
			blacklist: config.blacklist(),
			state: None,
			stream_size: usize::MAX,
			// Default is to listen on all interfaces (ipv4).
			ip_address: IpAddr::V4(DEFAULT_IP_ADDRESS),
			keep_alive_duration: DEFAULT_KEEP_ALIVE_DURATION,
			transport: default_transport,
			// The peer will be disconnected after 20 successive timeout errors are recorded
			ping: (
				Default::default(),
				PingErrorPolicy::DisconnectAfterMaxTimeouts(20),
			),
			kademlia,
			identify,
			request_response: (request_response_behaviour, rpc_handler_fn),
			gossipsub: (gossipsub_behaviour, gossip_filter_fn),
		}
	}

	/// Explicitly configure the network (protocol) id.
	///
	/// Note that it must be of the format "/protocol-name/version" otherwise it will default to
	/// "/swarmnl/1.0". See: [`DEFAULT_NETWORK_ID`].
	pub fn with_network_id(self, protocol: String) -> Self {
		if protocol.len() > MIN_NETWORK_ID_LENGTH.into() && protocol.starts_with("/") {
			CoreBuilder {
				network_id: StreamProtocol::try_from_owned(protocol.clone())
					.map_err(|_| SwarmNlError::NetworkIdParseError(protocol))
					.unwrap(),
				..self
			}
		} else {
			panic!("Could not parse provided network id");
		}
	}

	/// Configure the IP address to listen on.
	///
	/// If none is specified, the default value is `Ipv4Addr::new(0, 0, 0, 0)`. See:
	/// [`DEFAULT_IP_ADDRESS`].
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

	/// Configure the size of the stream buffers to use to track application requests to the network
	/// layer internally. This should be as large an possible to prevent dropping off requests to
	/// the network layer. Defaults to [`usize::MAX`].
	pub fn with_stream_size(self, size: usize) -> Self {
		CoreBuilder {
			stream_size: size,
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
				config.err_policy,
			),
			..self
		}
	}

	/// Configure the RPC protocol for the network.
	pub fn with_rpc<F>(self, config: RpcConfig, handler: fn(RpcData) -> RpcData) -> Self {
		// Set the request-response protocol
		CoreBuilder {
			request_response: (
				Behaviour::new(
					[(self.network_id.clone(), ProtocolSupport::Full)],
					request_response::Config::default()
						.with_request_timeout(config.timeout)
						.with_max_concurrent_streams(config.max_concurrent_streams),
				),
				handler,
			),
			..self
		}
	}

	/// Configure the `Kademlia` protocol for the network.
	pub fn with_kademlia(self, config: kad::Config) -> Self {
		// PeerId
		let peer_id = self.keypair.public().to_peer_id();
		let store = kad::store::MemoryStore::new(peer_id);
		let kademlia = kad::Behaviour::with_config(peer_id, store, config);

		CoreBuilder { kademlia, ..self }
	}

	/// Configure the `Gossipsub` protocol for the network.
	///
	/// # Panics
	///
	/// This function panics if `Gossipsub` cannot be configured properly.
	pub fn with_gossipsub(
		self,
		config: GossipsubConfig,
		filter_fn: fn(PeerId, MessageId, Option<PeerId>, String, Vec<String>) -> bool,
	) -> Self {
		let behaviour = match config {
			GossipsubConfig::Default => self.gossipsub.0,
			GossipsubConfig::Custom { config, auth } => gossipsub::Behaviour::new(auth, config)
				.map_err(|_| SwarmNlError::GossipConfigError)
				.unwrap(),
		};

		CoreBuilder {
			gossipsub: (behaviour, filter_fn),
			..self
		}
	}

	/// Configure the transports to support.
	pub fn with_transports(self, transport: TransportOpts) -> Self {
		CoreBuilder { transport, ..self }
	}

	/// Return the id of the network.
	pub fn network_id(&self) -> String {
		self.network_id.to_string()
	}

	/// Configure the application data state to manage
	pub fn with_state(self, state: T) -> Self {
		CoreBuilder {
			state: Some(state),
			..self
		}
	}

	/// Build the [`Core`] data structure.
	///
	/// Handles the configuration of the libp2p Swarm structure and the selected transport
	/// protocols, behaviours and node identity for tokio and async-std runtimes. The Swarm is
	/// wrapped in the Core construct which serves as the interface to interact with the internal
	/// networking layer.
	pub async fn build(self) -> SwarmNlResult<Core<T>> {
		#[cfg(feature = "async-std-runtime")]
		let mut swarm = {
			// Configure transports for default and custom configurations
			let swarm_builder: SwarmBuilder<_, _> = match self.transport {
				TransportOpts::TcpQuic { tcp_config } => match tcp_config {
					TcpConfig::Default => {
						libp2p::SwarmBuilder::with_existing_identity(self.keypair.clone())
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
					},
					TcpConfig::Custom {
						ttl,
						nodelay,
						backlog,
					} => {
						let tcp_config = tcp::Config::default()
							.ttl(ttl)
							.nodelay(nodelay)
							.listen_backlog(backlog);

						libp2p::SwarmBuilder::with_existing_identity(self.keypair.clone())
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
					},
				},
			};

			// Configure the selected protocols and their corresponding behaviours
			swarm_builder
				.with_behaviour(|_| CoreBehaviour {
					ping: self.ping.0,
					kademlia: self.kademlia,
					identify: self.identify,
					request_response: self.request_response.0,
					gossipsub: self.gossipsub.0,
				})
				.map_err(|_| SwarmNlError::ProtocolConfigError)?
				.with_swarm_config(|cfg| {
					cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
				})
				.build()
		};

		#[cfg(feature = "tokio-runtime")]
		let mut swarm = {
			let swarm_builder: SwarmBuilder<_, _> = match self.transport {
				TransportOpts::TcpQuic { tcp_config } => match tcp_config {
					TcpConfig::Default => {
						libp2p::SwarmBuilder::with_existing_identity(self.keypair.clone())
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
					},
					TcpConfig::Custom {
						ttl,
						nodelay,
						backlog,
					} => {
						let tcp_config = tcp::Config::default()
							.ttl(ttl)
							.nodelay(nodelay)
							.listen_backlog(backlog);

						libp2p::SwarmBuilder::with_existing_identity(self.keypair.clone())
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
					},
				},
			};

			// Configure the selected protocols and their corresponding behaviours
			swarm_builder
				.with_behaviour(|_| CoreBehaviour {
					ping: self.ping.0,
					kademlia: self.kademlia,
					identify: self.identify,
					request_response: self.request_response.0,
					gossipsub: self.gossipsub.0,
				})
				.map_err(|_| SwarmNlError::ProtocolConfigError)?
				.with_swarm_config(|cfg| {
					cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
				})
				.build()
		};

		// Configure the transport multiaddress and begin listening.
		// It can handle multiple future tranports based on configuration e.g, in the future,
		// WebRTC.
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
			},
		}

		// Add bootnodes to local routing table, if any
		for peer_info in self.boot_nodes {
			// PeerId
			if let Some(peer_id) = string_to_peer_id(&peer_info.0) {
				// Multiaddress
				if let Ok(multiaddr) = peer_info.1.parse::<Multiaddr>() {
					// Strange but make sure the peers are not a part of our blacklist
					if !self.blacklist.list.iter().any(|&id| id == peer_id) {
						swarm
							.behaviour_mut()
							.kademlia
							.add_address(&peer_id, multiaddr.clone());

						println!("Dailing {}", multiaddr);

						// Dial them
						swarm
							.dial(multiaddr.clone().with(Protocol::P2p(peer_id)))
							.map_err(|_| {
								SwarmNlError::RemotePeerDialError(multiaddr.to_string())
							})?;
					}
				}
			}
		}

		// Begin DHT bootstrap, hopefully bootnodes were supplied
		let _ = swarm.behaviour_mut().kademlia.bootstrap();

		// Set node as SERVER
		swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

		// Register and inform swarm of our blacklist
		for peer_id in &self.blacklist.list {
			swarm.behaviour_mut().gossipsub.blacklist_peer(peer_id);
		}

		// There must be a way for the application to communicate with the underlying networking
		// core. This will involve accepting and pushing data to the application layer.
		// Two streams will be opened: The first mpsc stream will allow SwarmNL push data to the
		// application and the application will consume it (single consumer). The second stream
		// will have SwarmNl (being the consumer) recieve data and commands from multiple areas
		// in the application.
		let (application_sender, network_receiver) =
			mpsc::channel::<StreamData>(STREAM_BUFFER_CAPACITY);
		let (network_sender, application_receiver) =
			mpsc::channel::<StreamData>(STREAM_BUFFER_CAPACITY);

		// Set up the ping network info.
		// `PeerId` does not implement `Default` so we will add the peerId of this node as seed
		// and set the count to 0. The count can NEVER increase because we cannot `Ping`
		// ourselves.
		let peer_id = self.keypair.public().to_peer_id();

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

		// Set up Gossipsub network information
		let gossip_info = GossipsubInfo {
			blacklist: self.blacklist,
		};

		// Initials stream id
		let stream_id = StreamId::new();
		let stream_request_buffer =
			Arc::new(Mutex::new(StreamRequestBuffer::new(self.stream_size)));
		let stream_response_buffer =
			Arc::new(Mutex::new(StreamResponseBuffer::new(self.stream_size)));

		// Aggregate the useful network information
		let network_info = NetworkInfo {
			id: self.network_id,
			ping: ping_info,
			gossipsub: gossip_info,
			rpc_handler_fn: self.request_response.1,
			gossip_filter_fn: self.gossipsub.1,
		};

		// Build the network core
		let network_core = Core {
			keypair: self.keypair,
			application_sender,
			stream_request_buffer: stream_request_buffer.clone(),
			stream_response_buffer: stream_response_buffer.clone(),
			current_stream_id: Arc::new(Mutex::new(stream_id)),
			// Save handler as the state of the application
			state: self.state,
			// Initialize an empty event queue
			event_queue: DataQueue::new(),
		};

		// Spin up task to handle async operations and data on the network
		#[cfg(feature = "async-std-runtime")]
		async_std::task::spawn(Core::handle_async_operations(
			swarm,
			network_info,
			network_sender,
			network_receiver,
			network_core.clone(),
		));

		// Spin up task to handle async operations and data on the network
		#[cfg(feature = "tokio-runtime")]
		tokio::task::spawn(Core::handle_async_operations(
			swarm,
			network_info,
			network_sender,
			network_receiver,
			network_core.clone(),
		));

		// Spin up task to listen for responses from the network layer
		#[cfg(feature = "async-std-runtime")]
		async_std::task::spawn(Core::handle_network_response(
			application_receiver,
			network_core.clone(),
		));

		// Spin up task to listen for responses from the network layer
		#[cfg(feature = "tokio-runtime")]
		tokio::task::spawn(Core::handle_network_response(
			application_receiver,
			network_core.clone(),
		));

		// Wait for a few seconds before passing control to the application
		#[cfg(feature = "async-std-runtime")]
		async_std::task::sleep(Duration::from_secs(BOOT_WAIT_TIME)).await;

		// Wait for a few seconds before passing control to the application
		#[cfg(feature = "tokio-runtime")]
		tokio::time::sleep(Duration::from_secs(BOOT_WAIT_TIME)).await;

		Ok(network_core)
	}
}

/// The core interface for the application layer to interface with the networking layer.
#[derive(Clone)]
pub struct Core<T: Clone + Send + Sync + 'static> {
	keypair: Keypair,
	/// The producing end of the stream that sends data to the network layer from the
	/// application.
	application_sender: Sender<StreamData>,
	/// The consuming end of the stream that recieves data from the network layer.
	// application_receiver: Receiver<StreamData>,
	/// The producing end of the stream that sends data from the network layer to the application.
	// network_sender: Sender<StreamData>,
	/// This serves as a buffer for the results of the requests to the network layer.
	/// With this, applications can make async requests and fetch their results at a later time
	/// without waiting. This is made possible by storing a [`StreamId`] for a particular stream
	/// request.
	stream_response_buffer: Arc<Mutex<StreamResponseBuffer>>,
	/// Store a [`StreamId`] representing a network request
	stream_request_buffer: Arc<Mutex<StreamRequestBuffer>>,
	/// Current stream id. Useful for opening new streams, we just have to bump the number by 1
	current_stream_id: Arc<Mutex<StreamId>>,
	/// The state of the application
	state: Option<T>,
	/// The network event queue
	event_queue: DataQueue<NetworkEvent>,
}

impl<T: Clone + Send + Sync + 'static> Core<T> {
	/// Serialize keypair to protobuf format and write to config file on disk. This could be useful
	/// for saving a keypair for future use when going offline.
	///
	/// It returns a boolean to indicate success of operation. Only key types other than RSA can be
	/// serialized to protobuf format and only a single keypair can be saved at a time.
	pub fn save_keypair_offline(&self, config_file_path: &str) -> bool {
		// Check the file exists, and create one if not
		if let Err(_) = fs::metadata(config_file_path) {
			fs::File::create(config_file_path).expect("could not create config file");
		}

		// Check if key type is something other than RSA
		if KeyType::RSA != self.keypair.key_type() {
			if let Ok(protobuf_keypair) = self.keypair.to_protobuf_encoding() {
				// Write key type and serialized array key to config file
				return util::write_config(
					"auth",
					"protobuf_keypair",
					&format!("{:?}", protobuf_keypair),
					config_file_path,
				) && util::write_config(
					"auth",
					"Crypto",
					&format!("{}", self.keypair.key_type()),
					config_file_path,
				);
			}
		}

		false
	}

	/// Return the node's `PeerId`.
	pub fn peer_id(&self) -> PeerId {
		self.keypair.public().to_peer_id()
	}

	/// Return a reference to the data state stored by the network
	pub fn state(&self) -> Option<T> {
		self.state.clone()
	}

	/// Modifies the state of the application data passed to the network
	pub fn add_state(&mut self, state: T) {
		self.state = Some(state);
	}

	/// Return an iterator to the buffered network layer events and clears the queue of them
	pub async fn events(&mut self) -> IntoIter<NetworkEvent> {
		let events = self.event_queue.into_inner().await.into_iter();

		// Clear all buffered events
		self.event_queue.drain().await;
		events
	}

	/// Return the next event in the network event queue
	pub async fn next_event(&mut self) -> Option<NetworkEvent> {
		self.event_queue.pop().await
	}

	/// Send data to the network layer and recieve a unique `StreamId` to track the request.
	///
	/// If the internal stream buffer is full, `None` will be returned.
	pub async fn send_to_network(&mut self, app_request: AppData) -> Option<StreamId> {
		// Generate stream id
		let stream_id = StreamId::next(*self.current_stream_id.lock().await);
		let request = StreamData::FromApplication(stream_id, app_request.clone());

		// Only a few requests should be tracked internally
		match app_request {
			// Doesn't need any tracking
			AppData::KademliaDeleteRecord { .. } | AppData::KademliaStopProviding { .. } => {
				// Send request
				let _ = self.application_sender.send(request).await;
				return None;
			},
			// Okay with the rest
			_ => {
				// Acquire lock on stream_request_buffer
				let mut stream_request_buffer = self.stream_request_buffer.lock().await;

				// Add to request buffer
				if !stream_request_buffer.insert(stream_id) {
					// Buffer appears to be full
					return None;
				}

				// Send request
				if let Ok(_) = self.application_sender.send(request).await {
					// Store latest stream id
					*self.current_stream_id.lock().await = stream_id;
					return Some(stream_id);
				} else {
					return None;
				}
			},
		}
	}

	/// Explicitly retrieve the reponse to a request sent to the network layer.
	/// This function is decoupled from the [`Core::send_to_network()`] method so as to prevent
	/// blocking until the response is returned.
	pub async fn recv_from_network(&mut self, stream_id: StreamId) -> NetworkResult {
		#[cfg(feature = "async-std-runtime")]
		{
			let channel = self.clone();
			let response_handler = async_std::task::spawn(async move {
				let mut loop_count = 0;
				loop {
					// Attempt to acquire the lock without blocking
					let mut buffer_guard = channel.stream_response_buffer.lock().await;

					// Check if the value is available in the response buffer
					if let Some(result) = buffer_guard.remove(&stream_id) {
						return Ok(result);
					}

					// Timeout after 5 trials
					if loop_count < 5 {
						loop_count += 1;
					} else {
						return Err(NetworkError::NetworkReadTimeout);
					}

					// Response has not arrived, sleep and retry
					async_std::task::sleep(Duration::from_secs(TASK_SLEEP_DURATION)).await;
				}
			});

			// Wait for the spawned task to complete
			match response_handler.await {
				Ok(result) => result,
				Err(_) => Err(NetworkError::InternalTaskError),
			}
		}

		#[cfg(feature = "tokio-runtime")]
		{
			let channel = self.clone();
			let response_handler = tokio::task::spawn(async move {
				let mut loop_count = 0;
				loop {
					// Attempt to acquire the lock without blocking
					let mut buffer_guard = channel.stream_response_buffer.lock().await;

					// Check if the value is available in the response buffer
					if let Some(result) = buffer_guard.remove(&stream_id) {
						return Ok(result);
					}

					// Timeout after 5 trials
					if loop_count < 5 {
						loop_count += 1;
					} else {
						return Err(NetworkError::NetworkReadTimeout);
					}

					// Response has not arrived, sleep and retry
					tokio::time::sleep(Duration::from_secs(TASK_SLEEP_DURATION)).await;
				}
			});

			// Wait for the spawned task to complete
			match response_handler.await {
				Ok(result) => result?,
				Err(_) => Err(NetworkError::InternalTaskError),
			}
		}
	}

	/// Perform an atomic `send` and `recieve` to and from the network layer. This function is
	/// atomic and blocks until the result of the request is returned from the network layer.
	///
	/// This function should mostly be used when the result of the request is needed immediately and
	/// delay can be condoned. It will still timeout if the delay exceeds the configured period.
	///
	/// If the internal buffer is full, it will return an error.
	pub async fn query_network(&mut self, request: AppData) -> NetworkResult {
		// Send request
		if let Some(stream_id) = self.send_to_network(request).await {
			// Wait to recieve response from the network
			self.recv_from_network(stream_id).await
		} else {
			Err(NetworkError::StreamBufferOverflow)
		}
	}

	/// Handle the responses coming from the network layer. This is usually as a result of a request
	/// from the application layer
	async fn handle_network_response(mut receiver: Receiver<StreamData>, network_core: Core<T>) {
		loop {
			select! {
				response = receiver.select_next_some() => {
					// Acquire mutex to write to buffer
					let mut buffer_guard = network_core.stream_response_buffer.lock().await;
					match response {
						// Send response to request operations specified by the application layer
						StreamData::ToApplication(stream_id, response) => match response {
							// Error
							AppResponse::Error(error) => buffer_guard.insert(stream_id, Err(error)),
							// Success
							res @ AppResponse::Echo(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::DailPeerSuccess(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaStoreRecordSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaLookupSuccess(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaGetProviders{..} => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaNoProvidersFound => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaGetRoutingTableInfo { .. } => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::FetchData(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GetNetworkInfo{..} => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GossipsubBroadcastSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GossipsubJoinSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GossipsubExitSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GossipsubBlacklistSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::GossipsubGetInfo{..} => buffer_guard.insert(stream_id, Ok(res)),
						},
						_ => false
					};
				}
			}
		}
	}

	/// Handle async operations, which basically involved handling two major data sources:
	///
	/// - Streams coming from the application layer.
	/// - Events generated by (libp2p) network activities.
	///
	/// Important information are sent to the application layer over a (mpsc) stream.
	async fn handle_async_operations(
		mut swarm: Swarm<CoreBehaviour>,
		mut network_info: NetworkInfo,
		mut network_sender: Sender<StreamData>,
		mut receiver: Receiver<StreamData>,
		network_core: Core<T>,
	) {
		// Network queue that tracks the execution of application requests in the network layer.
		let data_queue_1 = DataQueue::new();
		let data_queue_2 = DataQueue::new();
		let data_queue_3 = DataQueue::new();
		let data_queue_4 = DataQueue::new();

		// Loop to handle incoming application streams indefinitely
		loop {
			select! {
				// Handle incoming stream data
				stream_data = receiver.next() => {
					match stream_data {
						Some(incoming_data) => {
							match incoming_data {
								StreamData::FromApplication(stream_id, app_data) => {
									// Trackable stream id
									let stream_id = stream_id;
									match app_data {
										// Put back into the stream what we read from it
										AppData::Echo(message) => {
											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Echo(message))).await;
										},
										AppData::DailPeer(peer_id, multiaddr) => {
											if let Ok(multiaddr) = multiaddr.parse::<Multiaddr>() {
												// Add to routing table
												swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr.clone());
												if let Ok(_) = swarm.dial(multiaddr.clone().with(Protocol::P2p(peer_id))) {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::DailPeerSuccess(multiaddr.to_string()))).await;
												} else {
													// Return error
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::DailPeerError))).await;
												}
											}
										},
										// Store a value in the DHT and (optionally) on explicit specific peers
										AppData::KademliaStoreRecord { key, value, expiration_time, explicit_peers } => {
											// Create a kad record
											let mut record = Record::new(key.clone(), value);

											// Set (optional) expiration time
											record.expires = expiration_time;

											// Insert into DHT
											if let Ok(_) = swarm.behaviour_mut().kademlia.put_record(record.clone(), kad::Quorum::One) {
												// The node automatically becomes a provider in the network
												let _ = swarm.behaviour_mut().kademlia.start_providing(RecordKey::new(&key));

												// Send streamId to libp2p events, to track response
												data_queue_1.push(stream_id).await;

												// Cache record on peers explicitly (if specified)
												if let Some(explicit_peers) = explicit_peers {
													// Extract PeerIds
													let peers = explicit_peers.iter().map(|peer_id_string| {
														PeerId::from_bytes(&peer_id_string.from_base58().unwrap_or_default())
													}).filter_map(Result::ok).collect::<Vec<_>>();
													swarm.behaviour_mut().kademlia.put_record_to(record, peers.into_iter(), kad::Quorum::One);
												}
											} else {
												// Return error
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadStoreRecordError(key)))).await;
											}
										},
										// Perform a lookup in the DHT
										AppData::KademliaLookupRecord { key } => {
											let _ = swarm.behaviour_mut().kademlia.get_record(key.clone().into());

											// Send streamId to libp2p events, to track response
											data_queue_2.push(stream_id).await;
										},
										// Perform a lookup of peers that store a record
										AppData::KademliaGetProviders { key } => {
											swarm.behaviour_mut().kademlia.get_providers(key.clone().into());

											// Send streamId to libp2p events, to track response
											data_queue_3.push(stream_id).await;
										}
										// Stop providing a record on the network
										AppData::KademliaStopProviding { key } => {
											swarm.behaviour_mut().kademlia.stop_providing(&key.into());
										}
										// Remove record from local store
										AppData::KademliaDeleteRecord { key } => {
											swarm.behaviour_mut().kademlia.remove_record(&key.into());
										}
										// Return important routing table info. We could return kbuckets depending on needs, for now it's just the network ID.
										AppData::KademliaGetRoutingTableInfo => {
											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaGetRoutingTableInfo{protocol_id: network_info.id.to_string()})).await;
										},
										// Fetch data quickly from a peer over the network
										AppData::FetchData { keys, peer } => {
											// Construct the RPC object
											let rpc = Rpc::ReqResponse { data: keys.clone() };

											// Inform the swarm to make the request
											let _ = swarm
												.behaviour_mut()
												.request_response
												.send_request(&peer, rpc);

											// Send streamId to libp2p events, to track response
											data_queue_4.push(stream_id).await;
										},
										// Return important information about the node
										AppData::GetNetworkInfo => {
											// Connected peers
											let connected_peers = swarm.connected_peers().map(|peer| peer.to_owned()).collect::<Vec<_>>();

											// External Addresses
											let external_addresses = swarm.listeners().map(|multiaddr| multiaddr.to_string()).collect::<Vec<_>>();

											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GetNetworkInfo { peer_id: swarm.local_peer_id().clone(), connected_peers, external_addresses })).await;
										},
										// Send gossip message to peers
										AppData::GossipsubBroadcastMessage { message, topic } => {
											// Get the topic hash
											let topic_hash = TopicHash::from_raw(topic);

											// Marshall message into a single string
											let message = message.join(GOSSIP_MESSAGE_SEPARATOR);

											// Check if we're already subscribed to the topic
											let is_subscribed = swarm.behaviour().gossipsub.mesh_peers(&topic_hash).any(|peer| peer == swarm.local_peer_id());

											// Gossip
											if swarm
												.behaviour_mut().gossipsub
												.publish(topic_hash, message.as_bytes()).is_ok() && !is_subscribed {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubBroadcastSuccess)).await;
											} else {
												// Return error
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::GossipsubBroadcastMessageError))).await;
											}
										},
										// Join a mesh network
										AppData::GossipsubJoinNetwork(topic) => {
											// Create a new topic
											let topic = IdentTopic::new(topic);

											// Subscribe
											if swarm.behaviour_mut().gossipsub.subscribe(&topic).is_ok() {
												// Send the response back to the application layer
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubJoinSuccess)).await;
											} else {
												// Return error
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::GossipsubJoinNetworkError))).await;
											}
										},
										// Get information concerning our gossiping
										AppData::GossipsubGetInfo => {
											// Topics we're subscribed to
											let subscribed_topics = swarm.behaviour().gossipsub.topics().map(|topic| topic.clone().into_string()).collect::<Vec<_>>();

											// Peers we know and the topics they are subscribed too
											let mesh_peers = swarm.behaviour().gossipsub.all_peers().map(|(peer, topics)| {
												(peer.to_owned(), topics.iter().map(|&t| t.clone().as_str().to_owned()).collect::<Vec<_>>())
											}).collect::<Vec<_>>();

											// Retrieve blacklist
											let blacklist = network_info.gossipsub.blacklist.into_inner();

											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubGetInfo { topics: subscribed_topics, mesh_peers, blacklist })).await;
										},
										// Exit a network we're a part of
										AppData::GossipsubExitNetwork(topic) => {
											// Create a new topic
											let topic = IdentTopic::new(topic);

											// Subscribe
											if swarm.behaviour_mut().gossipsub.unsubscribe(&topic).is_ok() {
												// Send the response back to the application layer
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubExitSuccess)).await;
											} else {
												// Return error
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::GossipsubJoinNetworkError))).await;
											}
										}
										// Blacklist a peer explicitly
										AppData::GossipsubBlacklistPeer(peer) => {
											// Add to list
											swarm.behaviour_mut().gossipsub.blacklist_peer(&peer);

											// Add peer to blacklist
											network_info.gossipsub.blacklist.list.insert(peer);

											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubBlacklistSuccess)).await;
										},
										// Remove a peer from the blacklist
										AppData::GossipsubFilterBlacklist(peer) => {
											// Add to list
											swarm.behaviour_mut().gossipsub.remove_blacklisted_peer(&peer);

											// Add peer to blacklist
											network_info.gossipsub.blacklist.list.remove(&peer);

											// Send the response back to the application layer
											let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::GossipsubBlacklistSuccess)).await;
										},
									}
								}
								_ => {}
							}
						},
						_ => {}
					}
				},
				swarm_event = swarm.next() => {
					match swarm_event {
						Some(event) => {
							match event {
								SwarmEvent::NewListenAddr {
									listener_id,
									address,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::NewListenAddr{
										local_peer_id: swarm.local_peer_id().to_owned(),
										listener_id,
										address
									}).await;
								}
								SwarmEvent::Behaviour(event) => match event {
									// Ping
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

												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::OutboundPingSuccess{
													peer_id: peer,
													duration
												}).await;
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

												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::OutboundPingError{
													peer_id: peer
												}).await;
											}
										}
									}
									// Kademlia
									CoreEvent::Kademlia(event) => match event {
										kad::Event::OutboundQueryProgressed { result, .. } => match result {
											kad::QueryResult::GetProviders(Ok(success)) => {
												match success {
													kad::GetProvidersOk::FoundProviders { key, providers, .. } => {
														// Stringify the PeerIds
														let peer_id_strings = providers.iter().map(|peer_id| {
															peer_id.to_base58()
														}).collect::<Vec<_>>();

														// Receive data from our one-way channel
														if let Some(stream_id) = data_queue_3.pop().await {
															// Send the response back to the application layer
															let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaGetProviders{ key: key.to_vec(), providers: peer_id_strings })).await;
														}
													},
													// No providers found
													kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. } => {
														// Receive data from our one-way channel
														if let Some(stream_id) = data_queue_3.pop().await {
															// Send the response back to the application layer
															let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaNoProvidersFound)).await;
														}
													}
												}
											},

											kad::QueryResult::GetProviders(Err(_)) => {
												// Receive data from our one-way channel
												if let Some(stream_id) = data_queue_3.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaNoProvidersFound)).await;
												}
											},
											kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
												kad::PeerRecord { record:kad::Record{ value, .. }, .. },
											))) => {
												// Receive data from out one-way channel
												if let Some(stream_id) = data_queue_2.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaLookupSuccess(value))).await;
												}
											}
											kad::QueryResult::GetRecord(Ok(_)) => {
												// Receive data from out one-way channel
												if let Some(stream_id) = data_queue_2.pop().await {
													// Send the error back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadFetchRecordError(vec![])))).await;
												}
											},
											kad::QueryResult::GetRecord(Err(e)) => {
												let key = match e {
													kad::GetRecordError::NotFound { key, .. } => key,
													kad::GetRecordError::QuorumFailed { key, .. } => key,
													kad::GetRecordError::Timeout { key } => key,
												};

												// Receive data from out one-way channel
												if let Some(stream_id) = data_queue_2.pop().await {
													// Send the error back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadFetchRecordError(key.to_vec())))).await;
												}
											}
											kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
												// Receive data from our one-way channel
												if let Some(stream_id) = data_queue_1.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaStoreRecordSuccess)).await;
												}

												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::KademliaPutRecordSuccess{
													key: key.to_vec()
												}).await;
											}
											kad::QueryResult::PutRecord(Err(e)) => {
												let key = match e {
													kad::PutRecordError::QuorumFailed { key, .. } => key,
													kad::PutRecordError::Timeout { key, .. } => key,
												};

												if let Some(stream_id) = data_queue_1.pop().await {
													// Send the error back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadStoreRecordError(key.to_vec())))).await;
												}

												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::KademliaPutRecordError).await;
											}
											kad::QueryResult::StartProviding(Ok(kad::AddProviderOk {
												key,
											})) => {
												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::KademliaStartProvidingSuccess{
													key: key.to_vec()
												}).await;
											}
											kad::QueryResult::StartProviding(Err(_)) => {
												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::KademliaStartProvidingError).await;
											}
											_ => {}
										}
										kad::Event::RoutingUpdated { peer, .. } => {
											// Append to network event queue
											network_core.event_queue.push(NetworkEvent::RoutingTableUpdated{
												peer_id: peer
											}).await;
										}
										// Other events we don't care about
										_ => {}
									},
									// Identify
									CoreEvent::Identify(event) => match event {
										identify::Event::Received { peer_id, info } => {
											// We just recieved an `Identify` info from a peer
											network_core.event_queue.push(NetworkEvent::IdentifyInfoReceived {
												peer_id,
												info: IdentifyInfo {
													public_key: info.public_key,
													listen_addrs: info.listen_addrs.clone(),
													protocols: info.protocols,
													observed_addr: info.observed_addr
												}
											}).await;

											// Disconnect from peer of the network id is different
											if info.protocol_version != network_info.id.as_ref() {
												// Disconnect
												let _ = swarm.disconnect_peer_id(peer_id);
											} else {
												// Add to routing table if not present already
												let _ = swarm.behaviour_mut().kademlia.add_address(&peer_id, info.listen_addrs[0].clone());
											}
										}
										// Remaining `Identify` events are not actively handled
										_ => {}
									},
									// Request-response
									CoreEvent::RequestResponse(event) => match event {
										request_response::Event::Message { peer: _, message } => match message {
												// A request just came in
												request_response::Message::Request { request_id: _, request, channel } => {
													// Parse request
													match request {
														Rpc::ReqResponse { data } => {
															// Pass request data to configured request handler
															let response_data = (network_info.rpc_handler_fn)(data);

															// Construct an RPC
															let response_rpc = Rpc::ReqResponse { data: response_data };

															// Send the response
															let _ = swarm.behaviour_mut().request_response.send_response(channel, response_rpc);
														}
													}
												},
												// We have a response message
												request_response::Message::Response { response, .. } => {
													// Receive data from our one-way channel
													if let Some(stream_id) = data_queue_4.pop().await {
														match response {
															Rpc::ReqResponse { data } => {
																// Send the response back to the application layer
																let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::FetchData(data))).await;
															},
														}
													}
												},
										},
										request_response::Event::OutboundFailure { .. } => {
											// Receive data from out one-way channel
											if let Some(stream_id) = data_queue_4.pop().await {
												// Send the error back to the application layer
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::RpcDataFetchError))).await;
											}
										},
										_ => {}
									},
									// Gossipsub
									CoreEvent::Gossipsub(event) => match event {
										// We've recieved an inbound message
										gossipsub::Event::Message { propagation_source, message_id, message } => {
											// Break data into its constituents. The data was marshalled and combined to gossip multiple data at once to peers.
											// Now we will break them up and pass for handling
											let data_string = String::from_utf8(message.data).unwrap_or_default();
											let gossip_data = data_string.split(GOSSIP_MESSAGE_SEPARATOR).map(|msg| msg.to_string()).collect::<Vec<_>>();

											// First trigger the configured application filter event
											if (network_info.gossip_filter_fn)(propagation_source.clone(), message_id, message.source, message.topic.to_string(), gossip_data.clone()) {
												// Append to network event queue
												network_core.event_queue.push(NetworkEvent::GossipsubIncomingMessageHandled { source: propagation_source, data: gossip_data }).await;
											}
											// else { // drop message }
										},
										// A peer just subscribed
										gossipsub::Event::Subscribed { peer_id, topic } => {
											// Append to network event queue
											network_core.event_queue.push(NetworkEvent::GossipsubSubscribeMessageReceived { peer_id, topic: topic.to_string() }).await;
										},
										// A peer just unsubscribed
										gossipsub::Event::Unsubscribed { peer_id, topic } => {
											// Append to network event queue
											network_core.event_queue.push(NetworkEvent::GossipsubUnsubscribeMessageReceived { peer_id, topic: topic.to_string() }).await;
										},
										_ => {},
									}
								},
								SwarmEvent::ConnectionEstablished {
									peer_id,
									connection_id,
									endpoint,
									num_established,
									concurrent_dial_errors: _,
									established_in,
								} => {
									// Before a node dails a peer, it firstg adds the peer to its routing table.
									// To enable DHT operations, the listener must do the same on establishing a new connection.
									if let ConnectedPoint::Listener { send_back_addr, .. } = endpoint.clone() {
										// Add peer to routing table
										let _ = swarm.behaviour_mut().kademlia.add_address(&peer_id, send_back_addr);
									}
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ConnectionEstablished {
										peer_id,
										connection_id,
										endpoint,
										num_established,
										established_in,
									}).await;
								}
								SwarmEvent::ConnectionClosed {
									peer_id,
									connection_id,
									endpoint,
									num_established,
									cause: _,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ConnectionClosed {
										peer_id,
										connection_id,
										endpoint,
										num_established
									}).await;
								}
								SwarmEvent::ExpiredListenAddr {
									listener_id,
									address,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ExpiredListenAddr {
										listener_id,
										address
									}).await;
								}
								SwarmEvent::ListenerClosed {
									listener_id,
									addresses,
									reason: _,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ListenerClosed {
										listener_id,
										addresses
									}).await;
								}
								SwarmEvent::ListenerError {
									listener_id,
									error: _,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ListenerError {
										listener_id,
									}).await;
								}
								SwarmEvent::Dialing {
									peer_id,
									connection_id,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::Dialing { peer_id, connection_id }).await;
								}
								SwarmEvent::NewExternalAddrCandidate { address } => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::NewExternalAddrCandidate { address }).await;
								}
								SwarmEvent::ExternalAddrConfirmed { address } => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ExternalAddrConfirmed { address }).await;
								}
								SwarmEvent::ExternalAddrExpired { address } => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::ExternalAddrExpired { address }).await;
								}
								SwarmEvent::IncomingConnection {
									connection_id,
									local_addr,
									send_back_addr,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::IncomingConnection { connection_id, local_addr, send_back_addr }).await;
								}
								SwarmEvent::IncomingConnectionError {
									connection_id,
									local_addr,
									send_back_addr,
									error: _,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::IncomingConnectionError { connection_id, local_addr, send_back_addr }).await;
								}
								SwarmEvent::OutgoingConnectionError {
									connection_id,
									peer_id,
									error: _,
								} => {
									// Append to network event queue
									network_core.event_queue.push(NetworkEvent::OutgoingConnectionError { connection_id,  peer_id }).await;
								}
								_ => {},
							}
						},
						_ => {}
					}
				}
			}
		}
	}
}

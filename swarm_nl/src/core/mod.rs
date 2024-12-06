// Copyright 2024 Algorealm, Inc.
// Apache 2.0 License

//! Core data structures and protocol implementations for building a swarm.

#![doc = include_str!("../../doc/core/NetworkBuilder.md")]
#![doc = include_str!("../../doc/core/ApplicationInteraction.md")]

use std::{
	cmp,
	collections::{vec_deque::IntoIter, BTreeSet, HashMap, HashSet},
	fs,
	net::IpAddr,
	num::NonZeroU32,
	path::Path,
	rc::Rc,
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
	swarm::{NetworkBehaviour, SwarmEvent},
	tcp, tls, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder,
};
use replication::{
	ConsistencyModel, ReplBufferData, ReplConfigData, ReplInfo, ReplNetworkConfig,
	ReplicaBufferQueue,
};

use self::{
	gossipsub_cfg::{Blacklist, GossipsubConfig, GossipsubInfo},
	ping_config::*,
	sharding::{ShardingCfg, ShardingInfo},
};

use super::*;
use crate::{setup::BootstrapConfig, util::*};

#[cfg(feature = "async-std-runtime")]
use async_std::sync::Mutex;

#[cfg(feature = "tokio-runtime")]
use tokio::sync::Mutex;

pub(crate) mod prelude;
pub use prelude::*;
pub mod replication;
pub mod sharding;
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
pub struct CoreBuilder {
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
	/// that filters incoming gossip messages before passing them to the application.
	gossipsub: (
		gossipsub::Behaviour,
		fn(PeerId, MessageId, Option<PeerId>, String, Vec<String>) -> bool,
	),
	/// The network data for replication operations
	replication_cfg: ReplNetworkConfig,
	/// The name of the entire shard network. This is important for quick broadcasting of changes
	/// in the shard network as a whole.
	sharding: ShardingInfo,
}

impl CoreBuilder {
	/// Return a [`CoreBuilder`] struct configured with [`BootstrapConfig`] and default values.
	/// Here, it is certain that [`BootstrapConfig`] contains valid data.
	/// A type that implements [`EventHandler`] is passed to handle and respond to network events.
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
		// The default behaviour is to allow all incoming messages.
		let gossip_filter_fn = |_, _, _, _, _| true;

		// Set up default config for RPC handling.
		// The incoming RPC will simply be forwarded back to its sender.
		let rpc_handler_fn = |incoming_data: RpcData| incoming_data;

		// Initialize struct with information from `BootstrapConfig`
		CoreBuilder {
			network_id: StreamProtocol::new(network_id),
			keypair: config.keypair(),
			tcp_udp_port: config.ports(),
			boot_nodes: config.bootnodes(),
			blacklist: config.blacklist(),
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
			replication_cfg: ReplNetworkConfig::Default,
			// The default peers to be forwarded sharded data must be 25% of the total in a shard
			sharding: ShardingInfo {
				id: Default::default(),
				config: ShardingCfg {
					callback: rpc_handler_fn,
				},
				state: Default::default(),
			},
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

	/// Configure the `Replication` protocol for the network.
	pub fn with_replication(mut self, repl_cfg: ReplNetworkConfig) -> Self {
		self.replication_cfg = repl_cfg;
		CoreBuilder { ..self }
	}

	/// Configure the `Sharding` protocol for the network.
	pub fn with_sharding(self, network_id: String, callback: fn(RpcData) -> RpcData) -> Self {
		CoreBuilder {
			sharding: ShardingInfo {
				id: network_id,
				config: ShardingCfg { callback },
				state: Default::default(),
			},
			..self
		}
	}

	/// Configure the RPC protocol for the network.
	pub fn with_rpc(self, config: RpcConfig, handler: fn(RpcData) -> RpcData) -> Self {
		// Set the request-response protocol
		CoreBuilder {
			request_response: (
				match config {
					RpcConfig::Default => self.request_response.0,
					RpcConfig::Custom {
						timeout,
						max_concurrent_streams,
					} => Behaviour::new(
						[(self.network_id.clone(), ProtocolSupport::Full)],
						request_response::Config::default()
							.with_request_timeout(timeout)
							.with_max_concurrent_streams(max_concurrent_streams),
					),
				},
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

	/// Build the [`Core`] data structure.
	///
	/// Handles the configuration of the libp2p Swarm structure and the selected transport
	/// protocols, behaviours and node identity for tokio and async-std runtimes. The Swarm is
	/// wrapped in the Core construct which serves as the interface to interact with the internal
	/// networking layer.
	pub async fn build(self) -> SwarmNlResult<Core> {
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

		// Set up Replication information
		let repl_info = ReplInfo {
			state: Arc::new(Mutex::new(Default::default())),
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
			replication: repl_info,
			sharding: self.sharding.clone(),
		};

		// Build the network core
		let network_core = Core {
			keypair: self.keypair,
			application_sender,
			stream_request_buffer: stream_request_buffer.clone(),
			stream_response_buffer: stream_response_buffer.clone(),
			current_stream_id: Arc::new(Mutex::new(stream_id)),
			// Initialize an empty event queue
			event_queue: DataQueue::new(),
			replica_buffer: Arc::new(ReplicaBufferQueue::new(self.replication_cfg.clone())),
			network_info,
		};

		// Check if sharding is configured
		if !self.sharding.id.is_empty() {
			// Spin up task to init the network
			let mut core = network_core.clone();
			#[cfg(feature = "async-std-runtime")]
			async_std::task::spawn(
				async move { core.init_sharding(self.sharding.id.clone()).await },
			);

			#[cfg(feature = "tokio-runtime")]
			tokio::task::spawn(async move { core.init_sharding(self.sharding.id.clone()).await });
		}

		// Ensure there is support for replication
		if !network_core
			.network_info
			.replication
			.state
			.lock()
			.await
			.is_empty()
		{
			// Now check whether the synchronization model is eventual consistency, if it is we need
			// to spin up tasks that handle the synchronization per network
			let core = network_core.clone();
			match self.replication_cfg {
				// Default consistency model is eventual
				ReplNetworkConfig::Default => {
					// Spin up task to ensure data consistency across the network
					#[cfg(feature = "async-std-runtime")]
					async_std::task::spawn(
						async move { core.sync_with_eventual_consistency().await },
					);

					#[cfg(feature = "tokio-runtime")]
					tokio::task::spawn(async move { core.sync_with_eventual_consistency().await });
				},
				ReplNetworkConfig::Custom {
					consistency_model, ..
				} if consistency_model == ConsistencyModel::Eventual => {
					// Spin up task to ensure data consistency across the network
					#[cfg(feature = "async-std-runtime")]
					async_std::task::spawn(
						async move { core.sync_with_eventual_consistency().await },
					);

					#[cfg(feature = "tokio-runtime")]
					tokio::task::spawn(async move { core.sync_with_eventual_consistency().await });
				},
				_ => {},
			}
		}

		// Spin up task to handle async operations and data on the network
		#[cfg(feature = "async-std-runtime")]
		async_std::task::spawn(Core::handle_async_operations(
			swarm,
			network_sender,
			network_receiver,
			network_core.clone(),
		));

		// Spin up task to handle async operations and data on the network
		#[cfg(feature = "tokio-runtime")]
		tokio::task::spawn(Core::handle_async_operations(
			swarm,
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
pub struct Core {
	keypair: Keypair,
	/// The producing end of the stream that sends data to the network layer from the
	/// application.
	application_sender: Sender<StreamData>,
	// The consuming end of the stream that recieves data from the network layer.
	// application_receiver: Receiver<StreamData>,
	// The producing end of the stream that sends data from the network layer to the application.
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
	/// The network event queue
	event_queue: DataQueue<NetworkEvent>,
	/// The internal buffer storing incoming replicated content before they are expired or consumed
	replica_buffer: Arc<ReplicaBufferQueue>,
	/// Important information about the network
	network_info: NetworkInfo,
}

impl Core {
	/// The flag used to initialize the replication protocol. When replica nodes recieve
	/// an RPC with the flag as the first field element, they then join the replication network
	/// whose key is specified in the second field.
	pub const REPL_CFG_FLAG: &'static str = "REPL_CFG_FLAG__@@";

	/// The gossip flag to indicate that incoming gossipsub message is actually data sent for
	/// replication.
	pub const REPL_GOSSIP_FLAG: &'static str = "REPL_GOSSIP_FLAG__@@";

	/// The RPC flag to indicate that incoming message is data that has been forwarded to the node
	/// because it is a member of the logical shard to store the data.
	pub const RPC_DATA_FORWARDING_FLAG: &'static str = "RPC_DATA_FORWARDING_FLAG__@@";

	/// The gossip flag to indicate that incoming (or outgoing) gossipsub message is a part of the
	/// strong consistency algorithm, intending to increase the confirmation count of a particular
	/// data item in the replicas temporary buffer.
	pub const STRONG_CONSISTENCY_FLAG: &'static str = "STRONG_CON__@@";

	/// The gossip flag to indicate that incoming (or outgoing) gossipsub message is a part of the
	/// eventual consistency algorithm seeking to synchronize data across nodes.
	pub const EVENTUAL_CONSISTENCY_FLAG: &'static str = "EVENTUAL_CON_@@";

	/// The RPC flag to pull missing data from a replica node for eventual consistency
	/// synchronization.
	pub const RPC_SYNC_PULL_FLAG: &'static str = "RPC_SYNC_PULL_FLAG__@@";

	/// The RPC flag to add a remote node to the shard network.
	pub const SHARD_RPC_INVITATION_FLAG: &'static str = "SHARD_RPC_INVITATION_FLAG__@@";

	/// The sharding gossip flag to indicate that a node has joined a shard network.
	pub const SHARD_GOSSIP_JOIN_FLAG: &'static str = "SHARD_GOSSIP_JOIN_FLAG__@@";

	/// The sharding gossip flag to indicate that a node has exited a shard network.
	pub const SHARD_GOSSIP_EXIT_FLAG: &'static str = "SHARD_GOSSIP_EXIT_FLAG__@@";

	/// The RPC flag to request a data from a node in a logical shard.
	pub const SHARD_RPC_REQUEST_FLAG: &'static str = "SHARD_RPC_REQUEST_FLAG__@@";

	/// The delimeter between the data fields of an entry in a dataset requested by a replica peer.
	pub const FIELD_DELIMITER: &'static str = "_@_";

	/// The delimeter between the data entries that has been requested by a replica peer.
	pub const ENTRY_DELIMITER: &'static str = "@@@";

	/// The delimeter to separate messages during RPC data marshalling
	pub const DATA_DELIMITER: &'static str = "$$";

	/// Serialize keypair to protobuf format and write to config file on disk. This could be useful
	/// for saving a keypair for future use when going offline.
	///
	/// It returns a boolean to indicate success of operation. Only key types other than RSA can be
	/// serialized to protobuf format and only a single keypair can be saved at a time.
	pub fn save_keypair_offline<T: AsRef<Path> + ?Sized>(&self, config_file_path: &T) -> bool {
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

	/// Return an iterator to the buffered network layer events and consume them.
	pub async fn events(&mut self) -> IntoIter<NetworkEvent> {
		let events = self.event_queue.into_inner().await.into_iter();

		// Clear all buffered events
		self.event_queue.drain().await;
		events
	}

	/// Return the next event in the network event queue.
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
	pub async fn recv_from_network(&mut self, stream_id: StreamId) -> NetworkResult<AppResponse> {
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

					// Timeout after 10 trials
					if loop_count < 10 {
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
				Err(_) => Err(NetworkError::NetworkReadTimeout),
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

					// Timeout after 10 trials
					if loop_count < 10 {
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
				Err(_) => Err(NetworkError::NetworkReadTimeout),
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
	pub async fn query_network(&mut self, request: AppData) -> NetworkResult<AppResponse> {
		// Send request
		if let Some(stream_id) = self.send_to_network(request).await {
			// Wait to recieve response from the network
			self.recv_from_network(stream_id).await
		} else {
			Err(NetworkError::StreamBufferOverflow)
		}
	}

	/// Setup the necessary preliminaries for the sharding protocol
	async fn init_sharding(&mut self, network_id: String) {
		// We will setup the indelying gossip group that all nodes in the nwtwork must be a part of,
		// to keep the state of the network consistent
		let gossip_request = AppData::GossipsubJoinNetwork(network_id.clone());
		let _ = self.query_network(gossip_request).await;
	}

	/// Update the state of the shard network. This is relevant when nodes join and leave the shard
	/// network.
	async fn update_shard_state(&mut self, peer: PeerId, shard_id: ShardId, join: bool) {
		// Update state
		let mut shard_state = self.network_info.sharding.state.lock().await;
		let shard_entry = shard_state
			.entry(shard_id.clone())
			.or_insert(Default::default());

		// If the node is joining
		if join {
			shard_entry.push(peer);

			// Free `Core`
			drop(shard_state);

			// If you're the one that is joining, you must join the replica (shard) network for
			// synchronization among the nodes of a particular logical shard
			if peer == self.peer_id() {
				// Join network
				let _ = self.join_repl_network(shard_id).await;
			}
		} else {
			shard_entry.retain(|entry| entry != &peer);

			// Free `Core`
			drop(shard_state);

			// Leave replica network
			let _ = self.leave_repl_network(shard_id);
		}
	}

	/// Handle incoming replicated data.
	/// The first element of the incoming data vector contains the name of the replica network.
	async fn handle_incoming_repl_data(&mut self, repl_network: String, repl_data: ReplBufferData) {
		// First, we generate an event announcing the arrival of some replicated data.
		// Application developers can listen for this
		let replica_data = repl_data.clone();
		self.event_queue
			.push(NetworkEvent::ReplicaDataIncoming {
				data: replica_data.data,
				outgoing_timestamp: replica_data.outgoing_timestamp,
				incoming_timestamp: replica_data.incoming_timestamp,
				message_id: replica_data.message_id,
				source: replica_data.sender,
			})
			.await;

		// Compare and increment the lamport's clock for the replica node
		if let Some(repl_network_data) = self
			.network_info
			.replication
			.state
			.lock()
			.await
			.get_mut(&repl_network)
		{
			// Update clock
			(*repl_network_data).lamport_clock =
				cmp::max(repl_network_data.lamport_clock, repl_data.lamport_clock)
					.saturating_add(1);

			// Then push into buffer queue
			self.replica_buffer
				.push(self.clone(), repl_network, repl_data)
				.await;
		}
	}

	/// Handle incmoing shard data. We will not be doing any internal buffering as the data would be
	/// exposed as an event.
	async fn handle_incoming_shard_data(&mut self, shard_id: String, incoming_data: ByteVector) {
		// Push into event queue
		self.event_queue
			.push(NetworkEvent::IncomingForwardedData {
				data: byte_vec_to_string_vec(incoming_data.clone()),
			})
			.await;

		// Notify other nodes in the shard
		let _ = self.replicate(incoming_data, &shard_id).await;
	}

	/// Consume data in replication buffer.
	pub async fn consume_repl_data(&mut self, replica_network: &str) -> Option<ReplBufferData> {
		self.replica_buffer.pop_front(replica_network).await
	}

	/// Join a replica network and get up to speed with the current nextwork data state.
	/// If the consistency model is eventual, in no time the node's buffer will be up to date. But
	/// if it's the case of a strong consistency model, we might have to call
	/// [`Core::replicate_buffer`] to get up to date.
	pub async fn join_repl_network(&mut self, repl_network: String) -> NetworkResult<()> {
		// Set up replica network config
		let mut cfg = self.network_info.replication.state.lock().await;
		cfg.entry(repl_network.clone()).or_insert(ReplConfigData {
			lamport_clock: 0,
			nodes: Default::default(),
		});

		// Free `Core`
		drop(cfg);

		// Join the replication (gossip) network
		let gossip_request = AppData::GossipsubJoinNetwork(repl_network.clone());
		let _ = self.query_network(gossip_request).await?;

		Ok(())
	}

	/// Leave a replica network. The messages on the internal replica queue are not discarded so as
	/// to aid speedy recorvery in case of reconnection.
	pub async fn leave_repl_network(&mut self, repl_network: String) -> NetworkResult<AppResponse> {
		// Leave the replication (gossip) network
		let gossip_request = AppData::GossipsubExitNetwork(repl_network.clone());
		self.query_network(gossip_request).await
	}

	/// Replicate a replica node's current buffer image. This is necessary in case of
	/// joining a replica network with a strong consistency model.
	pub async fn replicate_buffer(
		&self,
		repl_network: String,
		replica_node: PeerId,
	) -> Result<(), NetworkError> {
		// First make sure i'm a part of the replica network
		if self
			.network_info
			.replication
			.state
			.lock()
			.await
			.contains_key(&repl_network)
		{
			// Populate buffer
			self.replica_buffer
				.replicate_buffer(self.clone(), repl_network, replica_node)
				.await
		} else {
			Err(NetworkError::MissingReplNetwork)
		}
	}

	/// Handle network sychronization using the eventual consistency data model
	async fn sync_with_eventual_consistency(&self) {
		// First we want to check out the replica networks the node is a part of
		let network_data = self.network_info.replication.state.lock().await;
		for (repl_network, _) in network_data.iter() {
			// We will spawn a task for each replica network to manage synchronization respectively
			let network = repl_network.to_owned();
			let repl_buffer = self.replica_buffer.clone();
			let core = self.clone();

			#[cfg(feature = "tokio-runtime")]
			tokio::task::spawn(async move {
				let buffer = repl_buffer;
				buffer.sync_with_eventual_consistency(core, network).await;
			});

			#[cfg(feature = "async-std-runtime")]
			async_std::task::spawn(async move {
				let buffer = repl_buffer;
				buffer.sync_with_eventual_consistency(core, network).await;
			});
		}
	}

	/// Send data to replica nodes. Function returns false if node is not a member of the replica
	/// network specified, meaning the replication network has not been configured or joined.
	pub async fn replicate(
		&mut self,
		mut replica_data: ByteVector,
		replica_network: &str,
	) -> NetworkResult<()> {
		// Extract the replica network data with minimal lock time
		let replica_network_data = {
			let mut state = self.network_info.replication.state.lock().await;
			if let Some(data) = state.get_mut(replica_network) {
				// Increase the clock atomically before releasing the lock
				data.lamport_clock = data.lamport_clock.saturating_add(1);
				data.clone()
			} else {
				return Err(NetworkError::MissingReplNetwork);
			}
		};

		// Prepare the replication message
		let mut message = vec![
			Core::REPL_GOSSIP_FLAG.as_bytes().to_vec(), // Replica Gossip Flag
			get_unix_timestamp().to_string().into(),    // Timestamp
			replica_network_data.lamport_clock.to_string().into(), // Clock
			replica_network.to_owned().into(),          // Replica network
		];
		message.append(&mut replica_data);

		// Prepare a gossip request
		let gossip_request = AppData::GossipsubBroadcastMessage {
			topic: replica_network.to_owned(),
			message,
		};

		// Gossip data to replica nodes
		self.query_network(gossip_request).await?;

		Ok(())
	}

	/// Handle the responses coming from the network layer. This is usually as a result of a request
	/// from the application layer.
	async fn handle_network_response(mut receiver: Receiver<StreamData>, network_core: Core) {
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
		mut network_sender: Sender<StreamData>,
		mut receiver: Receiver<StreamData>,
		mut network_core: Core,
	) {
		// Network queue that tracks the execution of application requests in the network layer.
		let data_queue_1 = DataQueue::new();
		let data_queue_2 = DataQueue::new();
		let data_queue_3 = DataQueue::new();
		let data_queue_4 = DataQueue::new();

		// Network information
		let mut network_info = network_core.network_info.clone();

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
												let message = message.join(GOSSIP_MESSAGE_SEPARATOR.as_bytes());

												// Check if we're already subscribed to the topic
												let is_subscribed = swarm.behaviour().gossipsub.mesh_peers(&topic_hash).any(|peer| peer == swarm.local_peer_id());

												// Gossip
												if swarm
													.behaviour_mut().gossipsub
													.publish(topic_hash, message).is_ok() && !is_subscribed {
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
															if let Ok(byte_str) = std::str::from_utf8(&data[0][..]) {
																match byte_str {
																	// It is a request to join a replication network
																	Core::REPL_CFG_FLAG => {
																		// Attempt to decode the key
																		if let Ok(repl_network) = String::from_utf8(data[1].clone()) {
																			// Join the replication (gossip) network
																			let mut core = network_core.clone();
																			let repl_network = repl_network.clone();

																			#[cfg(feature = "tokio-runtime")]
																			tokio::task::spawn(async move {
																				let _ = core.join_repl_network(repl_network.clone()).await;
																			});

																			#[cfg(feature = "async-std-runtime")]
																			async_std::task::spawn(async move {
																				let _ = core.join_repl_network(repl_network.clone()).await;
																			});

																			// Return a response
																			let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data });
																		}
																	}
																	// It is a request to retrieve missing data the RPC sender node lacks
																	Core::RPC_SYNC_PULL_FLAG => {
																		// Get replica network that the requested data belong to
																		let repl_network = String::from_utf8(data[1].clone()).unwrap_or_default();

																		// Retrieve missing data from local data buffer
																		let requested_msgs = network_core.replica_buffer.pull_missing_data(repl_network, &data[2..]).await;

																		// Send the response
																		let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: requested_msgs });
																	}
																	// It is a request to join a shard network
																	Core::SHARD_RPC_INVITATION_FLAG => {
																		// Attempt to decode the sharding network id
																		if let Ok(network_id) = String::from_utf8(data[1].clone()) {
																			// Join the shard (gossip) network
																			let gossip_request = AppData::GossipsubJoinNetwork(network_id.clone());
																			if network_core.query_network(gossip_request).await.is_ok() {
																				// Join the specific shard
																				if let Ok(shard_id) = String::from_utf8(data[2].clone()) {
																					let gossip_request = AppData::GossipsubJoinNetwork(shard_id.clone());
																					let _ = network_core.query_network(gossip_request).await;

																					// Initialize the shard network image
																					let network_image = bytes_to_shard_image(data[3].clone());
																					*network_core.network_info.sharding.state.lock().await = network_image;
																				}
																			}
																		}

																		// Send the response
																		let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: data.clone() });
																	}
																	// It is an incoming shard message forwarded from peer not permitted to store the data
																	Core::RPC_DATA_FORWARDING_FLAG => {
																		// Send the response, so as to return the RPC immediately
																		let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: data.clone() });

																		let shard_id = String::from_utf8_lossy(&data[1]).to_string();
																		// Handle incoming shard data
																		network_core.handle_incoming_shard_data(shard_id, data[2..].into()).await;
																	}
																	// It is an incmoing request to ask for data on this node because it is a member of a logical shard
																	Core::SHARD_RPC_REQUEST_FLAG => {
																		// Pass request data to configured shard request handler
																		let response_data = (network_info.sharding.config.callback)(data[1..].into());
																		// Send the response
																		let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: response_data });
																	}
																	// Normal RPC
																	_ => {
																		// Pass request data to configured request handler
																		let response_data = (network_info.rpc_handler_fn)(data);
																		// Send the response
																		let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: response_data });
																	}
																}
															}
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

											match gossip_data[0].as_str() {
												// It is an incoming replication message
												Core::REPL_GOSSIP_FLAG =>  {
													// Construct buffer data
													let queue_data = ReplBufferData {
														data: gossip_data[4..].to_owned(),
														lamport_clock: gossip_data[2].parse::<u64>().unwrap_or(0),	// It can never unwrap()
														outgoing_timestamp: gossip_data[1].parse::<u64>().unwrap_or(0),
														incoming_timestamp: get_unix_timestamp(),
														message_id: message_id.to_string(),
														sender: if let Some(peer) = message.source { peer.clone() } else { propagation_source.clone() },
														confirmations: if network_core.replica_buffer.consistency_model() == ConsistencyModel::Eventual {
															// No confirmations needed for eventual consistency
															None
														} else {
															// Set count to 1
															Some(1)
														}
													};

													// Handle incoming replicated data
													let mut core = network_core.clone();
													let queue_data = queue_data.clone();
													let data = gossip_data[3].clone().into();

													#[cfg(feature = "tokio-runtime")]
													tokio::task::spawn(async move {
														let _ = core.handle_incoming_repl_data(data, queue_data).await;
													});

													#[cfg(feature = "async-std-runtime")]
													async_std::task::spawn(async move {
														let _ = core.handle_incoming_repl_data(data, queue_data).await;
													});
												},
												// It is a broadcast from replica nodes to ensure strong consistency
												Core::STRONG_CONSISTENCY_FLAG => {
													// Handle incoming replicated data
													let mut core = network_core.clone();
													let data = gossip_data[2].clone().into();
													let network = gossip_data[1].to_owned();

													#[cfg(feature = "tokio-runtime")]
													tokio::task::spawn(async move {
														let _ = core.replica_buffer.handle_data_confirmation(core.clone(), network, data).await;
													});

													#[cfg(feature = "async-std-runtime")]
													async_std::task::spawn(async move {
														let _ = core.replica_buffer.handle_data_confirmation(core.clone(), network, data).await;
													});
												},
												// It is a broadcast from replica nodes to ensure eventual consistency
												Core::EVENTUAL_CONSISTENCY_FLAG => {
													// Lower bound of Lamport's Clock
													let min_clock = gossip_data[3].parse::<u64>().unwrap_or_default();
													// Higher bound of Lamport's Clock
													let max_clock = gossip_data[4].parse::<u64>().unwrap_or_default();

													// Synchronize the incoming replica node's buffer image with the local buffer image
													network_core.replica_buffer.sync_buffer_image(network_core.clone(), gossip_data[1].clone(), gossip_data[2].clone(), (min_clock, max_clock), gossip_data[5..].to_owned()).await;
												}
												// It is a broadcast to inform us about the addition of a new node to a shard network
												Core::SHARD_GOSSIP_JOIN_FLAG => {
													// Upload sharding network state
													if let Ok(peer_id) = gossip_data[1].parse::<PeerId>() {
														let _ = network_core.update_shard_state(peer_id, gossip_data[2].clone(), true /* join */).await;
													}
												}
												// It is a broadcast to inform us about the exit of a node from a shard network
												Core::SHARD_GOSSIP_EXIT_FLAG => {
													// Upload sharding network state
													if let Ok(peer_id) = gossip_data[1].parse::<PeerId>() {
														let _ = network_core.update_shard_state(peer_id, gossip_data[2].clone(), false /* exit */).await;
													}
												}
												// Normal gossip
												_ => {
													// First trigger the configured application filter event
													if (network_info.gossip_filter_fn)(propagation_source.clone(), message_id, message.source, message.topic.to_string(), gossip_data.clone()) {
														// Append to network event queue
														network_core.event_queue.push(NetworkEvent::GossipsubIncomingMessageHandled { source: propagation_source, data: gossip_data }).await;
													}
													// else { // drop message }
												}
											}
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

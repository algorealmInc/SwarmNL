/// Copyright (c) 2024 Algorealm
/// Core data structures and protocol implementations for building a swarm.
use std::{
	collections::{HashMap, HashSet},
	net::{IpAddr, Ipv4Addr},
	num::NonZeroU32,
	sync::Arc,
	time::Duration,
};

use base58::FromBase58;

use futures::{
	channel::mpsc::{self, Receiver, Sender},
	select, SinkExt, StreamExt,
};
use futures_time::time::Duration as AsyncDuration;
use libp2p::{
	identify::{self, Info},
	kad::{self, store::MemoryStore, Record},
	multiaddr::Protocol,
	noise,
	ping::{self, Failure},
	request_response::{self, cbor::Behaviour, ProtocolSupport},
	swarm::{ConnectionError, NetworkBehaviour, SwarmEvent},
	tcp, tls, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder,
};

use self::ping_config::*;

use super::*;
use crate::{setup::BootstrapConfig, util::string_to_peer_id};

#[cfg(feature = "async-std-runtime")]
pub use async_std::sync::Mutex;

use tokio::sync::broadcast;
#[cfg(feature = "tokio-runtime")]
pub use tokio::sync::Mutex;

mod prelude;
pub use prelude::*;

/// The Core Behaviour implemented which highlights the various protocols
/// we'll be adding support for
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "CoreEvent")]
struct CoreBehaviour {
	ping: ping::Behaviour,
	kademlia: kad::Behaviour<MemoryStore>,
	identify: identify::Behaviour,
	request_response: request_response::cbor::Behaviour<Rpc, Rpc>,
}

/// Network events generated as a result of supported and configured `NetworkBehaviour`'s
#[derive(Debug)]
enum CoreEvent {
	Ping(ping::Event),
	Kademlia(kad::Event),
	Identify(identify::Event),
	RequestResponse(request_response::Event<Rpc, Rpc>),
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

/// Implement request_response events for [`CoreEvent`]
impl From<request_response::Event<Rpc, Rpc>> for CoreEvent {
	fn from(event: request_response::Event<Rpc, Rpc>) -> Self {
		CoreEvent::RequestResponse(event)
	}
}

/// Structure containing necessary data to build [`Core`]
pub struct CoreBuilder<T: EventHandler + Clone + Send + Sync + 'static> {
	network_id: StreamProtocol,
	keypair: Keypair,
	tcp_udp_port: (Port, Port),
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
	/// the network event handler
	handler: T,
	/// Prevents blocking forever due to absence of expected data from the network layer
	network_read_delay: AsyncDuration,
	/// The size of the stream buffers to use to track application requests to the network layer
	/// internally.
	stream_size: usize,
	ip_address: IpAddr,
	/// Connection keep-alive duration while idle
	keep_alive_duration: Seconds,
	transport: TransportOpts, /* Maybe this can be a collection in the future to support
	                           * additive transports */
	/// The `Behaviour` of the `Ping` protocol
	ping: (ping::Behaviour, PingErrorPolicy),
	/// The `Behaviour` of the `Kademlia` protocol
	kademlia: kad::Behaviour<kad::store::MemoryStore>,
	/// The `Behaviour` of the `Identify` protocol
	identify: identify::Behaviour,
	/// The `Behaviour` of the `Request-Response` protocol.
	/// The second field value is the function to handle an incoming request from a peer
	request_response: Behaviour<Rpc, Rpc>,
}

impl<T: EventHandler + Clone + Send + Sync + 'static> CoreBuilder<T> {
	/// Return a [`CoreBuilder`] struct configured with [`BootstrapConfig`] and default values.
	/// Here, it is certain that [`BootstrapConfig`] contains valid data.
	/// A type that implements [`EventHandler`] is passed to handle and react to network events.
	pub fn with_config(config: BootstrapConfig, handler: T) -> Self {
		// The default network id
		let network_id = DEFAULT_NETWORK_ID;

		// TCP/IP and QUIC are supported by default
		let default_transport = TransportOpts::TcpQuic {
			tcp_config: TcpConfig::Default,
		};

		// Peer Id
		let peer_id = config.keypair().public().to_peer_id();

		// Set up default config for Kademlia
		let mut cfg = kad::Config::default();
		cfg.set_protocol_names(vec![StreamProtocol::new(network_id)]);

		let store = kad::store::MemoryStore::new(peer_id);
		let kademlia = kad::Behaviour::with_config(peer_id, store, cfg);

		// Set up default config for Kademlia
		let cfg = identify::Config::new(network_id.to_owned(), config.keypair().public())
			.with_push_listen_addr_updates(true);
		let identify = identify::Behaviour::new(cfg);

		// Set up default config for Request-Response
		let request_response = Behaviour::new(
			[(StreamProtocol::new(network_id), ProtocolSupport::Full)],
			request_response::Config::default(),
		);

		// Initialize struct with information from `BootstrapConfig`
		CoreBuilder {
			network_id: StreamProtocol::new(network_id),
			keypair: config.keypair(),
			tcp_udp_port: config.ports(),
			boot_nodes: config.bootnodes(),
			handler,
			// Timeout defaults to 60 seconds
			network_read_delay: AsyncDuration::from_secs(NETWORK_READ_TIMEOUT),
			stream_size: usize::MAX,
			// Default is to listen on all interfaces (ipv4)
			ip_address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
			// Default to 60 seconds
			keep_alive_duration: DEFAULT_KEEP_ALIVE_DURATION,
			transport: default_transport,
			// The peer will be disconnected after 20 successive timeout errors are recorded
			ping: (
				Default::default(),
				PingErrorPolicy::DisconnectAfterMaxTimeouts(20),
			),
			kademlia,
			identify,
			request_response,
		}
	}

	/// Explicitly configure the network (protocol) id e.g /swarmnl/1.0.
	/// Note that it must be of the format "/protocol-name/version" else it will default to
	/// "/swarmnl/1.0"
	pub fn with_network_id(self, protocol: String) -> Self {
		if protocol.len() > MIN_NETWORK_ID_LENGTH.into() && protocol.starts_with("/") {
			CoreBuilder {
				network_id: StreamProtocol::try_from_owned(protocol.clone())
					.map_err(|_| SwarmNlError::NetworkIdParseError(protocol))
					.unwrap(),
				..self
			}
		} else {
			panic!("Could not parse provided network id: it must be of the format '/protocol-name/version'");
		}
	}

	/// Configure the IP address to listen on
	pub fn listen_on(self, ip_address: IpAddr) -> Self {
		CoreBuilder { ip_address, ..self }
	}

	/// Configure the timeout for requests to read from the network layer.
	/// Reading from the network layer could potentially block if the data corresponding to the
	/// [`StreamId`] specified could not be found (or has been read already). This prevents the
	/// future from `await`ing forever. Defaults to 60 seconds
	pub fn with_network_read_delay(self, network_read_delay: AsyncDuration) -> Self {
		CoreBuilder {
			network_read_delay,
			..self
		}
	}

	/// Configure how long to keep a connection alive (in seconds) once it is idling.
	pub fn with_idle_connection_timeout(self, keep_alive_duration: Seconds) -> Self {
		CoreBuilder {
			keep_alive_duration,
			..self
		}
	}

	/// Configure the size of the stream buffers to use to track application requests to the network
	/// layer internally. This should be as large an possible to prevent dropping of requests to the
	/// network layer. Defaults to [`usize::MAX`]
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
	pub fn with_rpc<F>(self, config: RpcConfig) -> Self
	where
		F: Fn(Vec<String>) -> Vec<String>,
	{
		// Set the request-response protocol
		CoreBuilder {
			request_response: Behaviour::new(
				[(self.network_id.clone(), ProtocolSupport::Full)],
				request_response::Config::default()
					.with_request_timeout(config.timeout)
					.with_max_concurrent_streams(config.max_concurrent_streams),
			),
			..self
		}
	}

	/// TODO! Kademlia Config has to be cutom because of some setting exposed
	/// Configure the `Kademlia` protocol for the network.
	pub fn with_kademlia(self, config: kad::Config) -> Self {
		// PeerId
		let peer_id = self.keypair.public().to_peer_id();
		let store = kad::store::MemoryStore::new(peer_id);
		let kademlia = kad::Behaviour::with_config(peer_id, store, config);

		CoreBuilder { kademlia, ..self }
	}

	/// Configure the transports to support.
	pub fn with_transports(self, transport: TransportOpts) -> Self {
		CoreBuilder { transport, ..self }
	}

	/// Configure network event handler
	/// This configures the functions to be called when various network events take place
	pub fn configure_network_events(self, handler: T) -> Self {
		CoreBuilder { handler, ..self }
	}

	/// Return the id of the network
	fn network_id(&self) -> String {
		self.network_id.to_string()
	}

	/// Build the [`Core`] data structure.
	///
	/// Handles the configuration of the libp2p Swarm structure and the selected transport
	/// protocols, behaviours and node identity.
	pub async fn build(self) -> SwarmNlResult<Core<T>> {
		// Build and configure the libp2p Swarm structure. Thereby configuring the selected
		// transport protocols, behaviours and node identity. The Swarm is wrapped in the Core
		// construct which serves as the interface to interact with the internal networking
		// layer

		#[cfg(feature = "async-std-runtime")]
		let mut swarm = {
			// We're dealing with async-std here
			// Configure transports
			let swarm_builder: SwarmBuilder<_, _> = match self.transport {
				TransportOpts::TcpQuic { tcp_config } => match tcp_config {
					TcpConfig::Default => {
						// Use the default config
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
						// Use the provided config
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
				.with_behaviour(|_|
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping.0,
                            kademlia: self.kademlia,
                            identify: self.identify,
							request_response: self.request_response
                        })
				.map_err(|_| SwarmNlError::ProtocolConfigError)?
				.with_swarm_config(|cfg| {
					cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
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
						// Use the provided config
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
				.with_behaviour(|_|
                        // Configure the selected behaviours
                        CoreBehaviour {
                            ping: self.ping.0,
                            kademlia: self.kademlia,
                            identify: self.identify,
							request_response: self.request_response
                        })
				.map_err(|_| SwarmNlError::ProtocolConfigError)?
				.with_swarm_config(|cfg| {
					cfg.with_idle_connection_timeout(Duration::from_secs(self.keep_alive_duration))
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
			},
		}

		// Add bootnodes to local routing table, if any
		for peer_info in self.boot_nodes {
			// PeerId
			if let Some(peer_id) = string_to_peer_id(&peer_info.0) {
				// Multiaddress
				if let Ok(multiaddr) = peer_info.1.parse::<Multiaddr>() {
					swarm
						.behaviour_mut()
						.kademlia
						.add_address(&peer_id, multiaddr.clone());

					println!("Dailing {}", multiaddr);

					// Dial them
					swarm
						.dial(peer_id)
						.map_err(|_| SwarmNlError::RemotePeerDialError(multiaddr.to_string()))?;
				}
			}
		}

		// Begin DHT bootstrap, hopefully bootnodes were supplied
		let _ = swarm.behaviour_mut().kademlia.bootstrap();

		// There must be a way for the application to communicate with the underlying networking
		// core. This will involve acceptiing data and pushing data to the application layer.
		// Two streams will be opened: The first mpsc stream will allow SwarmNL push data to the
		// application and the application will comsume it (single consumer) The second stream
		// will have SwarmNl (being the consumer) recieve data and commands from multiple areas
		// in the application;
		let (application_sender, network_receiver) = mpsc::channel::<StreamData>(100);
		let (network_sender, application_receiver) = mpsc::channel::<StreamData>(100);

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
		};

		// Build the network core
		let network_core = Core {
			keypair: self.keypair,
			application_sender,
			// network_sender,
			// application_receiver,
			stream_request_buffer: stream_request_buffer.clone(),
			stream_response_buffer: stream_response_buffer.clone(),
			network_read_delay: self.network_read_delay,
			current_stream_id: Arc::new(Mutex::new(stream_id)),
			// Save handler as the state of the application
			state: self.handler,
		};

		// Spin up task to handle async operations and data on the network.
		#[cfg(feature = "async-std-runtime")]
		async_std::task::spawn(Core::handle_async_operations(
			swarm,
			network_info,
			network_sender,
			network_receiver,
			network_core.clone(),
		));

		// Spin up task to handle async operations and data on the network.
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

		Ok(network_core)
	}
}

/// The core interface for the application layer to interface with the networking layer
#[derive(Clone)]
pub struct Core<T: EventHandler + Clone + Send + Sync + 'static> {
	keypair: Keypair,
	/// The producing end of the stream that sends data to the network layer from the
	/// application
	application_sender: Sender<StreamData>,
	/// The consuming end of the stream that recieves data from the network layer
	// application_receiver: Receiver<StreamData>,
	/// The producing end of the stream that sends data from the network layer to the application
	// network_sender: Sender<StreamData>,
	/// This serves as a buffer for the results of the requests to the network layer.
	/// With this, applications can make async requests and fetch their results at a later time
	/// without waiting. This is made possible by storing a [`StreamId`] for a particular stream
	/// request.
	stream_response_buffer: Arc<Mutex<StreamResponseBuffer>>,
	/// Store a [`StreamId`] representing a network request
	stream_request_buffer: Arc<Mutex<StreamRequestBuffer>>,
	/// The network read timeout
	network_read_delay: AsyncDuration,
	/// Current stream id. Useful for opening new streams, we just have to bump the number by 1
	current_stream_id: Arc<Mutex<StreamId>>,
	/// The state of the application
	pub state: T,
}

impl<T: EventHandler + Clone + Send + Sync + 'static> Core<T> {
	/// Serialize keypair to protobuf format and write to config file on disk.
	/// It returns a boolean to indicate success of operation.
	/// Only key types other than RSA can be serialized to protobuf format.
	pub fn save_keypair_offline(&self, config_file_path: &str) -> bool {
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

	/// Return the node's `PeerId`
	pub fn peer_id(&self) -> String {
		self.keypair.public().to_peer_id().to_string()
	}

	/// Send data to the network layer and recieve a unique `StreamId` to track the request
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

	/// TODO! Buffer cleanup algorithm
	/// Explicitly rectrieve the reponse to a request sent to the network layer.
	/// This function is decoupled from the [`send_to_network()`] function so as to prevent delay
	/// and read immediately as the response to the request should already be in the stream response
	/// buffer.
	pub async fn recv_from_network(&mut self, stream_id: StreamId) -> NetworkResult {
		#[cfg(feature = "async-std-runtime")]
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

					// Failed to acquire the lock, sleep and retry
					async_std::task::sleep(Duration::from_secs(TASK_SLEEP_DURATION)).await;
				}
			});

			// Wait for the spawned task to complete
			match response_handler.await {
				Ok(result) => result?,
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

					// Failed to acquire the lock, sleep and retry
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

	/// Perform an atomic `send` and `recieve` from the network layer. This function is atomic and
	/// blocks until the result of the request is returned from the network layer. This function
	/// should mostly be used when the result of the request is needed immediately and delay can be
	/// condoned. It will still timeout if the delay exceeds the configured period.
	/// If the internal buffer is full, it will return an error.
	pub async fn fetch_from_network(&mut self, request: AppData) -> NetworkResult {
		// send request
		if let Some(stream_id) = self.send_to_network(request).await {
			// wait to recieve response from the network
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
							res @ AppResponse::Echo(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::DailPeer(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaStoreRecordSuccess => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaLookupRecord(..) => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaGetProviders{..} => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::KademliaGetRoutingTableInfo { .. } => buffer_guard.insert(stream_id, Ok(res)),
							res @ AppResponse::FetchData(..) => buffer_guard.insert(stream_id, Ok(res)),
							// Error
							AppResponse::Error(error) => buffer_guard.insert(stream_id, Err(error))
						},
						_ => false
					};
				}
			}
		}
	}

	/// Handle async operations, which basically involved handling two major data sources:
	/// - Streams coming from the application layer.
	/// - Events generated by (libp2p) network activities.
	/// Important information are sent to the application layer over a (mpsc) stream
	async fn handle_async_operations(
		mut swarm: Swarm<CoreBehaviour>,
		mut network_info: NetworkInfo,
		mut network_sender: Sender<StreamData>,
		mut receiver: Receiver<StreamData>,
		mut network_core: Core<T>,
	) {

		let mut exec_queue_1 = ExecQueue::new();
		let mut exec_queue_2 = ExecQueue::new();
		let mut exec_queue_3 = ExecQueue::new();
		let mut exec_queue_4 = ExecQueue::new();

		// Loop to handle incoming application streams indefinitely.
		loop {
			select! {
				// handle incoming stream data
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
										AppData::DailPeer(multiaddr) => {
											if let Ok(multiaddr) = multiaddr.parse::<Multiaddr>() {
												if let Ok(_) = swarm.dial(multiaddr.clone()) {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::DailPeer(multiaddr.to_string()))).await;
												} else {
													// Return error
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::DailPeerError))).await;
												}
											}
										},
										// Store a value in the DHT and (optionally) on explicit specific peers
										AppData::KademliaStoreRecord { key, value, expiration_time, explicit_peers } => {
											// create a kad record
											let mut record = Record::new(key.clone(), value);

											// Set (optional) expiration time
											record.expires = expiration_time;

											// Insert into DHT
											if let Ok(_) = swarm.behaviour_mut().kademlia.put_record(record.clone(), kad::Quorum::One) {
												// Send streamId to libp2p events, to track response
												exec_queue_1.push(stream_id).await;

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
											exec_queue_2.push(stream_id).await;
										},
										// Perform a lookup of peers that store a record
										AppData::KademliaGetProviders { key } => {
											let _ = swarm.behaviour_mut().kademlia.get_providers(key.clone().into());

											// Send streamId to libp2p events, to track response
											exec_queue_3.push(stream_id).await;
										}
										// Stop providing a record on the network
										AppData::KademliaStopProviding { key } => {
											swarm.behaviour_mut().kademlia.stop_providing(&key.into());
										}
										// Remove record from local store
										AppData::KademliaDeleteRecord { key } => {
											swarm.behaviour_mut().kademlia.remove_record(&key.into());
										}
										// Return important routing table info
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
											exec_queue_4.push(stream_id).await;
										}
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
									// call configured handler
									network_core.state.new_listen_addr(swarm.local_peer_id().to_owned(), listener_id, address).await;
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

												// Call custom handler
												network_core.state.inbound_ping_success(peer, duration).await;
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

												// Call custom handler
												network_core.state.outbound_ping_error(peer, err_type).await;
											}
										}
									}
									// Kademlia
									CoreEvent::Kademlia(event) => match event {
										kad::Event::OutboundQueryProgressed { result, .. } => match result {
											kad::QueryResult::GetProviders(Ok(
												kad::GetProvidersOk::FoundProviders { key, providers, .. },
											)) => {
												// Stringify the PeerIds
												let peer_id_strings = providers.iter().map(|peer_id| {
													peer_id.to_base58()
												}).collect::<Vec<_>>();

												// Receive data from our one-way channel
												if let Some(stream_id) = exec_queue_3.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaGetProviders{ key: key.to_vec(), providers: peer_id_strings })).await;
												}
											}
											kad::QueryResult::GetProviders(Err(_)) => {},
											kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
												kad::PeerRecord { record:kad::Record{ value, .. }, .. },
											))) => {
												// Receive data from out one-way channel
												if let Some(stream_id) = exec_queue_2.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaLookupRecord(value))).await;
												}
											}
											kad::QueryResult::GetRecord(Ok(_)) => {
												// TODO!: How do we track this?
											},
											kad::QueryResult::GetRecord(Err(e)) => {
												let key = match e {
													kad::GetRecordError::NotFound { key, .. } => key,
													kad::GetRecordError::QuorumFailed { key, .. } => key,
													kad::GetRecordError::Timeout { key } => key,
												};

												// Receive data from out one-way channel
												if let Some(stream_id) = exec_queue_2.pop().await {
													// Send the error back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadFetchRecordError(key.to_vec())))).await;
												}
											}
											kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
												// Receive data from our one-way channel
												if let Some(stream_id) = exec_queue_1.pop().await {
													// Send the response back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::KademliaStoreRecordSuccess)).await;
												}

												// Call handler
												network_core.state.kademlia_put_record_success(key.to_vec()).await;
											}
											kad::QueryResult::PutRecord(Err(e)) => {
												let key = match e {
													kad::PutRecordError::QuorumFailed { key, .. } => key,
													kad::PutRecordError::Timeout { key, .. } => key,
												};

												if let Some(stream_id) = exec_queue_1.pop().await {
													// Send the error back to the application layer
													let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::KadStoreRecordError(key.to_vec())))).await;
												}

												// Call handler
												network_core.state.kademlia_put_record_error().await;
											}
											kad::QueryResult::StartProviding(Ok(kad::AddProviderOk {
												key,
											})) => {
												// Call handler
												network_core.state.kademlia_start_providing_success(key.to_vec()).await;
											}
											kad::QueryResult::StartProviding(Err(_)) => {
												// Call handler
												network_core.state.kademlia_start_providing_error().await;
											}
											_ => {}
										},
										// Other events we don't care about
										_ => {}
									},
									// Identify
									CoreEvent::Identify(event) => match event {
										identify::Event::Received { peer_id, info } => {
											// We just recieved an `Identify` info from a peer.s
											network_core.state.identify_info_recieved(peer_id, info.clone()).await;

											// disconnect from peer of the network id is different
											if info.protocol_version != network_info.id.as_ref() {
												// disconnect
												let _ = swarm.disconnect_peer_id(peer_id);
											} else {
												// add to routing table if not present already
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
															let response_data = network_core.state.handle_incoming_message(data);

															// construct an RPC
															let response_rpc = Rpc::ReqResponse { data: response_data };

															// Send the response
															let _ = swarm.behaviour_mut().request_response.send_response(channel, response_rpc);
														}
													}
												},
												// We have a response message
												request_response::Message::Response { response, .. } => {
													// Receive data from our one-way channel
													if let Some(stream_id) = exec_queue_4.pop().await {
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
											if let Some(stream_id) = exec_queue_4.pop().await {
												// Send the error back to the application layer
												let _ = network_sender.send(StreamData::ToApplication(stream_id, AppResponse::Error(NetworkError::RpcDataFetchError))).await;
											} 
										},
										_ => {}
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
									// call configured handler
									network_core.state.connection_established(
										peer_id,
										connection_id,
										&endpoint,
										num_established,
										established_in,
									).await;
								}
								SwarmEvent::ConnectionClosed {
									peer_id,
									connection_id,
									endpoint,
									num_established,
									cause,
								} => {
									// call configured handler
									network_core.state.connection_closed(
										peer_id,
										connection_id,
										&endpoint,
										num_established,
										cause,
									).await;
								}
								SwarmEvent::ExpiredListenAddr {
									listener_id,
									address,
								} => {
									// call configured handler
									network_core.state.expired_listen_addr(listener_id, address).await;
								}
								SwarmEvent::ListenerClosed {
									listener_id,
									addresses,
									reason: _,
								} => {
									// call configured handler
									network_core.state.listener_closed(listener_id, addresses).await;
								}
								SwarmEvent::ListenerError {
									listener_id,
									error: _,
								} => {
									// call configured handler
									network_core.state.listener_error(listener_id).await;
								}
								SwarmEvent::Dialing {
									peer_id,
									connection_id,
								} => {
									// call configured handler
									network_core.state.dialing(peer_id, connection_id).await;
								}
								SwarmEvent::NewExternalAddrCandidate { address } => {
									// call configured handler
									network_core.state.new_external_addr_candidate(address).await;
								}
								SwarmEvent::ExternalAddrConfirmed { address } => {
									// call configured handler
									network_core.state.external_addr_confirmed(address).await;
								}
								SwarmEvent::ExternalAddrExpired { address } => {
									// call configured handler
									network_core.state.external_addr_expired(address).await;
								}
								SwarmEvent::IncomingConnection {
									connection_id,
									local_addr,
									send_back_addr,
								} => {
									// call configured handler
									network_core.state.incoming_connection(connection_id, local_addr, send_back_addr).await;
								}
								SwarmEvent::IncomingConnectionError {
									connection_id,
									local_addr,
									send_back_addr,
									error: _,
								} => {
									// call configured handler
									network_core.state.incoming_connection_error(
										connection_id,
										local_addr,
										send_back_addr,
									).await;
								}
								SwarmEvent::OutgoingConnectionError {
									connection_id,
									peer_id,
									error: _,
								} => {
									// call configured handler
									network_core.state.outgoing_connection_error(connection_id,  peer_id).await;
								}
								_ => todo!(),
							}
						},
						_ => {}
					}
				}
			}
		}
	}
}

#[cfg(test)]

mod tests {

use super::*;
use async_std::task;
use futures::TryFutureExt;
use ini::Ini;
use std::fs::File;
use std::net::Ipv6Addr;
use std::fs;

// set up a default node helper
pub fn setup_core_builder() -> CoreBuilder<DefaultHandler> {
	let config = BootstrapConfig::default();
	let handler = DefaultHandler;

	// return default network core builder
	CoreBuilder::with_config(config, handler)
}

// define custom ports for testing
const CUSTOM_TCP_PORT: Port = 49666;
const CUSTOM_UDP_PORT: Port = 49852;

// used to test saving keypair to file
fn create_test_ini_file(file_path: &str) {
	let mut config = Ini::new();
	config
		.with_section(Some("ports"))
		.set("tcp", CUSTOM_TCP_PORT.to_string())
		.set("udp", CUSTOM_UDP_PORT.to_string());

	config.with_section(Some("bootstrap")).set(
		"boot_nodes",
		"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
	);
	// write config to a new INI file
	config.write_to_file(file_path).unwrap_or_default();
}

#[test]
fn default_behavior_works() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// assert that the default network id is '/swarmnl/1.0'
	assert_eq!(default_node.network_id, DEFAULT_NETWORK_ID);

	// default transport is TCP/QUIC
	assert_eq!(
		default_node.transport,
		TransportOpts::TcpQuic {
			tcp_config: TcpConfig::Default
		}
	);

	// default keep alive duration is 60 seconds
	assert_eq!(default_node.keep_alive_duration, 60);

	// default listen on is 0:0:0:0
	assert_eq!(
		default_node.ip_address,
		IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
	);

	// default tcp/udp port is MIN_PORT and MAX_PORT
	assert_eq!(default_node.tcp_udp_port, (MIN_PORT, MAX_PORT));
}

#[test]
fn custom_node_setup_works() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// custom node configuration
	let mut custom_network_id = "/custom-protocol/1.0".to_string();
	let mut custom_transport = TransportOpts::TcpQuic {
		tcp_config: TcpConfig::Custom {
			ttl: 10,
			nodelay: true,
			backlog: 10,
		},
	};
	let mut custom_keep_alive_duration = 20;
	let mut custom_ip_address = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));

	// pass in the custom node configuration and assert it works as expected
	let custom_node = default_node
		.with_network_id(custom_network_id.clone())
		.with_transports(custom_transport.clone())
		.with_idle_connection_timeout(custom_keep_alive_duration.clone())
		.listen_on(custom_ip_address.clone());

	// TODO: with_ping
	// e.g. if the node is unreachable after a specific amount of time, it should be
	// disconnected if 10th inteval is configured, if failed 9th time, test decay as each ping
	// comes in

	// TODO: with_kademlia
	// e.g. if a record is not found, it should return a specific message

	// TODO: configure_network_events
	// test recorded logs. Create a custom handler and test if the logs are recorded.

	// assert that the custom network id is '/custom/protocol/1.0'
	assert_eq!(custom_node.network_id(), custom_network_id);

	// assert that the custom transport is 'TcpQuic'
	assert_eq!(custom_node.transport, custom_transport);

	// assert that the custom keep alive duration is 20
	assert_eq!(custom_node.keep_alive_duration, custom_keep_alive_duration);
}

#[test]
fn network_id_custom_behavior_works_as_expected() {
	// setup a node with the default config builder
	let mut custom_builder = setup_core_builder();

	// configure builder with custom protocol and assert it works as expected
	let custom_protocol: &str = "/custom-protocol/1.0";
	let custom_builder = custom_builder.with_network_id(custom_protocol.to_string());

	// cannot be less than MIN_NETWORK_ID_LENGTH
	assert_eq!(
		custom_builder.network_id().len() >= MIN_NETWORK_ID_LENGTH.into(),
		true
	);

	// must start with a forward slash
	assert!(custom_builder.network_id().starts_with("/"));

	// assert that the custom network id is '/custom/protocol/1.0'
	assert_eq!(custom_builder.network_id(), custom_protocol.to_string());
}

#[test]
#[should_panic("Could not parse provided network id: it must be of the format '/protocol-name/version'")]
fn network_id_custom_behavior_fails() {
	// build a node with the default network id
	let mut custom_builder = setup_core_builder();

	// pass in an invalid network ID: network ID length is less than MIN_NETWORK_ID_LENGTH
	let invalid_protocol_1 = "/1.0".to_string();
	assert!(invalid_protocol_1.len() < MIN_NETWORK_ID_LENGTH.into());
	let custom_builder = custom_builder.with_network_id(invalid_protocol_1);

	// pass in an invalid network ID: network ID must start with a forward slash
	let invalid_protocol_2 = "1.0".to_string();
	custom_builder.with_network_id(invalid_protocol_2);
}

#[cfg(feature = "tokio-runtime")]
#[test]
fn save_keypair_offline_works_tokio() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// use tokio runtime to test async function
	let result = tokio::runtime::Runtime::new().unwrap().block_on(
		default_node
			.build()
			.unwrap_or_else(|_| panic!("Could not build node")),
	);

	// make a saved_keys.ini file
	let file_path_1 = "saved_keys.ini";
	create_test_ini_file(file_path_1);

	// save the keypair to existing file
	let saved_1 = result.save_keypair_offline(&file_path_1);

	// assert that the keypair was saved successfully
	assert_eq!(saved_1, true);

	// now test if it works for a file name that does not exist
	let file_path_2 = "test.txt";
	let saved_2 = result.save_keypair_offline(file_path_2);

	// assert that the keypair was saved successfully
	assert_eq!(saved_2, true);

	// clean up
	fs::remove_file(file_path_1).unwrap_or_default();
	fs::remove_file(file_path_2).unwrap_or_default();
}

#[cfg(feature = "async-std-runtime")]
#[test]
fn save_keypair_offline_works_async_std() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// use tokio runtime to test async function
	let result = task::block_on(
		default_node
			.build()
			.unwrap_or_else(|_| panic!("Could not build node")),
	);

	// make a saved_keys.ini file
	let file_path_1 = "saved_keys.ini";
	create_test_ini_file(file_path_1);

	// save the keypair to existing file
	let saved_1 = result.save_keypair_offline(file_path_1);

	// assert that the keypair was saved successfully
	assert_eq!(saved_1, true);

	// now test if it works for a file name that does not exist
	let file_path_2 = "test.txt";
	let saved_2 = result.save_keypair_offline(file_path_2);

	// assert that the keypair was saved successfully
	assert_eq!(saved_2, true);

	// clean up
	fs::remove_file(file_path_1).unwrap_or_default();
	fs::remove_file(file_path_2).unwrap_or_default();
}
}

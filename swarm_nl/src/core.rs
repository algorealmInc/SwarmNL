//! Core data structures and protocol implementations for building a swarm.

use std::{
	collections::HashMap,
	io::Error,
	net::{IpAddr, Ipv4Addr},
	num::NonZeroU32,
	time::Duration,
};

use futures::{
	channel::mpsc::{self, Receiver, Sender},
	select, SinkExt, StreamExt,
};
use libp2p::{
	identify::{self, Info},
	kad::{self, store::MemoryStore, Record},
	multiaddr::{self, Protocol},
	noise,
	ping::{self, Failure},
	swarm::{ConnectionError, NetworkBehaviour, SwarmEvent},
	tcp, tls, yamux, Multiaddr, StreamProtocol, Swarm, SwarmBuilder, TransportError,
};

use super::*;
use crate::setup::BootstrapConfig;
use ping_config::*;

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
	keypair: Keypair,
	tcp_udp_port: (Port, Port),
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
	/// the network event handler
	handler: T,
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
}

impl<T: EventHandler + Send + Sync + 'static> CoreBuilder<T> {
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

		// Set up default config config for Kademlia
		let cfg = identify::Config::new(network_id.to_owned(), config.keypair().public())
			.with_push_listen_addr_updates(true);
		let identify = identify::Behaviour::new(cfg);

		// Initialize struct with information from `BootstrapConfig`
		CoreBuilder {
			network_id: StreamProtocol::new(network_id),
			keypair: config.keypair(),
			tcp_udp_port: config.ports(),
			boot_nodes: config.bootnodes(),
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
	/// Note that it must be of the format "/protocol-name/version".
	/// # Panics
	///
	/// This function will panic if the specified protocol id could not be parsed.
	pub fn with_network_id(self, protocol: String) -> Self {
		if protocol.len() > MIN_NETWORK_ID_LENGTH.into() && protocol.starts_with("/") { 
			CoreBuilder {
				network_id: StreamProtocol::try_from_owned(protocol.clone())
					.map_err(|_| SwarmNlError::NetworkIdParseError(protocol))
					.unwrap(),
				..self
			}
		} else {
			panic!("could not parse provided network id");
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
				config.err_policy,
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

	/// Configure network event handler.
	/// This configures the functions to be called when various network events take place
	pub fn configure_network_events(self, handler: T) -> Self {
		CoreBuilder { handler, ..self }
	}

	/// Build the [`Core`] data structure.
	///
	/// Handles the configuration of the libp2p Swarm structure and the selected transport
	/// protocols, behaviours and node identity.
	pub async fn build(self) -> SwarmNlResult<Core> {
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
                            identify: self.identify
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
                            identify: self.identify
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
			if let Ok(peer_id) = PeerId::from_bytes(peer_info.0.as_bytes()) {
				// Multiaddress
				if let Ok(multiaddr) = multiaddr::from_url(&peer_info.1) {
					swarm
						.behaviour_mut()
						.kademlia
						.add_address(&peer_id, multiaddr.clone());

					println!("{:?}", multiaddr);

					// Dial them
					swarm
						.dial(multiaddr.clone())
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
		let (mut network_sender, application_receiver) = mpsc::channel::<StreamData>(3);
		let (application_sender, network_receiver) = mpsc::channel::<StreamData>(3);

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

		// Aggregate the useful network information
		let network_info = NetworkInfo {
			id: self.network_id,
			ping: ping_info,
		};

		// Build the network core
		let network_core = Core {
			keypair: self.keypair,
			application_sender,
			application_receiver,
		};

		// Send message to application to indicate readiness
		let _ = network_sender.send(StreamData::Ready).await;

		// Spin up task to handle async operations and data on the network.
		#[cfg(feature = "async-std-runtime")]
		async_std::task::spawn(Core::handle_async_operations(
			swarm,
			network_info,
			network_sender,
			self.handler,
			network_receiver,
		));

		// Spin up task to handle async operations and data on the network.
		#[cfg(feature = "tokio-runtime")]
		tokio::task::spawn(Core::handle_async_operations(
			swarm,
			network_info,
			network_sender,
			self.handler,
			network_receiver,
		));

		Ok(network_core)
	}

	/// Return the id of the network
	fn network_id(&self) -> String {
		self.network_id.to_string()
	}
}

/// The core interface for the application layer to interface with the networking layer
pub struct Core {
	keypair: Keypair,
	/// The producing end of the stream that sends data to the network layer from the
	/// application
	pub application_sender: Sender<StreamData>,
	/// The consuming end of the stream that recieves data from the network layer
	pub application_receiver: Receiver<StreamData>,
}

impl Core {
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

	/// Explicitly dial a peer at runtime.
	/// We will trigger the dial by sending a message into the stream. This is an intra-network
	/// layer communication, multiplexed over the undelying open stream.
	pub async fn dial_peer(&mut self, multiaddr: String) {
		// send message into stream
		let _ = self
			.application_sender // `application_sender` is being used here to speak to the network layer (itself)
			.send(StreamData::Network(NetworkData::DailPeer(multiaddr)))
			.await;
	}

	/// Handle async operations, which basically involved handling two major data sources:
	/// - Streams coming from the application layer.
	/// - Events generated by (libp2p) network activities.
	/// Important information are sent to the application layer over a (mpsc) stream
	async fn handle_async_operations<T: EventHandler + Send + Sync + 'static>(
		mut swarm: Swarm<CoreBehaviour>,
		mut network_info: NetworkInfo,
		mut sender: Sender<StreamData>,
		mut handler: T,
		mut receiver: Receiver<StreamData>,
	) {
		// Loop to handle incoming application streams indefinitely.
		loop {
			select! {
				// handle incoming stream data
				stream_data = receiver.select_next_some() => match stream_data {
						// Not handled
						StreamData::Ready => {}
						// Put back into the stream what we read from it
						StreamData::Echo(message) => {
							// Echo message back into stream
							let _ = sender.send(StreamData::Echo(message)).await;
						}
						StreamData::Application(app_data) => {
							match app_data {
								// Store a value in the DHT and (optionally) on explicit specific peers
								AppData::KademliaStoreRecord { key,value,expiration_time, explicit_peers } => {
									// create a kad record
									let mut record = Record::new(key, value);

									// Set (optional) expiration time
									record.expires = expiration_time;

									// Insert into DHT
									let _ = swarm.behaviour_mut().kademlia.put_record(record.clone(), kad::Quorum::One);

									// Cache record on peers explicitly (if specified)
									if let Some(explicit_peers) = explicit_peers {
										// Extract PeerIds
										let peers = explicit_peers.iter().map(|peer_id_string| {
											PeerId::from_bytes(peer_id_string.as_bytes())
										}).filter_map(Result::ok).collect::<Vec<_>>();

										let _ = swarm.behaviour_mut().kademlia.put_record_to(record, peers.into_iter(), kad::Quorum::One);
									}
								},
								// Perform a lookup in the DHT
								AppData::KademliaLookupRecord { key } => {
									swarm.behaviour_mut().kademlia.get_record(key.into());
								},
								// Perform a lookup of peers that store a record
								AppData::KademliaGetProviders { key } => {
									swarm.behaviour_mut().kademlia.get_providers(key.into());
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
									// send information
									let _ = sender.send(StreamData::Network(NetworkData::KademliaDhtInfo { protocol_id: network_info.id.to_string() })).await;
								},
								// Fetch data quickly from a peer over the network
								AppData::FetchData { keys, peer } => {
									// inform the swarm to make the request

								}
							}
						}
						StreamData::Network(network_data) => {
							match network_data {
								// Dail peer
								NetworkData::DailPeer(multiaddr) => {
									if let Ok(multiaddr) = multiaddr::from_url(&multiaddr) {
										let _ = swarm.dial(multiaddr);
									}
								}
								// Ignore the remaining network messages, they'll never come
								_ => {}
							}
						}
				},
				event = swarm.select_next_some() => match event {
					SwarmEvent::NewListenAddr {
						listener_id,
						address,
					} => {
						// call configured handler
						handler.new_listen_addr(listener_id, address);
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

									// Call custom handler
									handler.outbound_ping_error(peer, err_type);
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

									// Inform the application that requested for it
									let _ = sender.send(StreamData::Network(NetworkData::Kademlia(DhtOps::ProvidersFound { key: key.to_vec(), providers: peer_id_strings }) )).await;
								}
								kad::QueryResult::GetProviders(Err(_)) => {
									// No providers for a particular key found
									let _ = sender.send(StreamData::Network(NetworkData::Kademlia(DhtOps::NoProvidersFound))).await;
								}
								kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
									kad::PeerRecord {
										record: kad::Record {key ,value, .. },
										..
									},
								))) => {
									// Send result of the Kademlia DHT lookup to the application layer
									let _ = sender.send(StreamData::Network(NetworkData::Kademlia(DhtOps::RecordFound {
										key: key.to_vec(), value
									}))).await;
								}
								kad::QueryResult::GetRecord(Ok(_)) => {
									// No record found
									let _ = sender.send(StreamData::Network(NetworkData::Kademlia(DhtOps::RecordNotFound))).await;
								}
								kad::QueryResult::GetRecord(Err(_)) => {
									// No record found
									let _ = sender.send(StreamData::Network(NetworkData::Kademlia(DhtOps::RecordNotFound))).await;
								}
								kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
									// Call handler
									handler.kademlia_put_record_success(key.to_vec());
								}
								kad::QueryResult::PutRecord(Err(_)) => {
									// Call handler
									handler.kademlia_put_record_error();
								}
								kad::QueryResult::StartProviding(Ok(kad::AddProviderOk {
									key,
								})) => {
									// Call handler
									handler.kademlia_start_providing_success(key.to_vec());
								}
								kad::QueryResult::StartProviding(Err(_)) => {
									// Call handler
									handler.kademlia_start_providing_error();
								}
								_ => {}
							},
							kad::Event::InboundRequest { request } => match request {
								kad::InboundRequest::GetProvider {
									num_closer_peers,
									num_provider_peers,
								} => {

								},
								kad::InboundRequest::AddProvider { record } => {

								},
								kad::InboundRequest::GetRecord {
									num_closer_peers,
									present_locally,
								} => {

								},
								kad::InboundRequest::PutRecord {
									source,
									connection,
									record,
								} => {

								},
								_ => {}
							},
							kad::Event::RoutingUpdated {
								peer,
								is_new_peer,
								addresses,
								bucket_range,
								old_peer,
							} => todo!(),
							kad::Event::UnroutablePeer { peer } => todo!(),
							kad::Event::RoutablePeer { peer, address } => todo!(),
							kad::Event::PendingRoutablePeer { peer, address } => todo!(),
							kad::Event::ModeChanged { new_mode } => todo!(),
						},
						// Identify
						CoreEvent::Identify(event) => match event {
							identify::Event::Received { peer_id, info } => {
								// We just recieved an `Identify` info from a peer.s
								handler.identify_info_recieved(peer_id, info.clone());

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
							sender.clone()
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
}

/// The high level trait that provides default implementations to handle most supported network
/// swarm events.
pub trait EventHandler {
	/// Event that informs the network core that we have started listening on a new multiaddr.
	fn new_listen_addr(&mut self, _listener_id: ListenerId, _addr: Multiaddr) {}

	/// Event that informs the network core about a newly established connection to a peer.
	fn connection_established(
		&mut self,
		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_concurrent_dial_errors: Option<Vec<(Multiaddr, TransportError<Error>)>>,
		_established_in: Duration,
		application_sender: Sender<StreamData>,
	) {
		// Default implementation
	}

	/// Event that informs the network core about a closed connection to a peer.
	fn connection_closed(
		&mut self,
		_peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: u32,
		_cause: Option<ConnectionError>,
	) {
		// Default implementation
	}

	/// Event that announces expired listen address.
	fn expired_listen_addr(&mut self, _listener_id: ListenerId, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces a closed listener.
	fn listener_closed(&mut self, _listener_id: ListenerId, _addresses: Vec<Multiaddr>) {
		// Default implementation
	}

	/// Event that announces a listener error.
	fn listener_error(&mut self, _listener_id: ListenerId) {
		// Default implementation
	}

	/// Event that announces a dialing attempt.
	fn dialing(&mut self, _peer_id: Option<PeerId>, _connection_id: ConnectionId) {
		// Default implementation
	}

	/// Event that announces a new external address candidate.
	fn new_external_addr_candidate(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces a confirmed external address.
	fn external_addr_confirmed(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces an expired external address.
	fn external_addr_expired(&mut self, _address: Multiaddr) {
		// Default implementation
	}

	/// Event that announces new connection arriving on a listener and in the process of
	/// protocol negotiation.
	fn incoming_connection(
		&mut self,
		_connection_id: ConnectionId,
		_local_addr: Multiaddr,
		_send_back_addr: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that announces an error happening on an inbound connection during its initial
	/// handshake.
	fn incoming_connection_error(
		&mut self,
		_connection_id: ConnectionId,
		_local_addr: Multiaddr,
		_send_back_addr: Multiaddr,
	) {
		// Default implementation
	}

	/// Event that announces an error happening on an outbound connection during its initial
	/// handshake.
	fn outgoing_connection_error(
		&mut self,
		_connection_id: ConnectionId,
		_peer_id: Option<PeerId>,
	) {
		// Default implementation
	}

	/// Event that announces the arrival of a ping message from a peer.
	/// The duration it took for a round trip is also returned
	fn inbound_ping_success(&mut self, _peer_id: PeerId, _duration: Duration) {
		// Default implementation
	}

	/// Event that announces a `Ping` error
	fn outbound_ping_error(&mut self, _peer_id: PeerId, _err_type: Failure) {
		// Default implementation
	}

	/// Event that announces the arrival of a `PeerInfo` via the `Identify` protocol
	fn identify_info_recieved(&mut self, _peer_id: PeerId, _info: Info) {
		// Default implementation
	}

	/// Event that announces the successful write of a record to the DHT
	fn kademlia_put_record_success(&mut self, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to save a record
	fn kademlia_put_record_error(&mut self) {
		// Default implementation
	}

	/// Event that announces a node as a provider of a record in the DHT
	fn kademlia_start_providing_success(&mut self, _key: Vec<u8>) {
		// Default implementation
	}

	/// Event that announces the failure of a node to become a provider of a record in the DHT
	fn kademlia_start_providing_error(&mut self) {
		// Default implementation
	}
}

/// Default network event handler
pub struct DefaultHandler;

/// Implement [`EventHandler`] for [`DefaultHandler`]
impl EventHandler for DefaultHandler {}

/// Important information to obtain from the [`CoreBuilder`], to properly handle network
/// operations
struct NetworkInfo {
	/// The name/id of the network
	id: StreamProtocol,
	/// Important information to manage `Ping` operations
	ping: PingInfo,
}

/// Module that contains important data structures to manage `Ping` operations on the network
mod ping_config {
	use libp2p_identity::PeerId;
	use std::{collections::HashMap, time::Duration};

	/// Policies to handle a `Ping` error
	/// - All connections to peers are closed during a disconnect operation.
	pub enum PingErrorPolicy {
		/// Do not disconnect under any circumstances
		NoDisconnect,
		/// Disconnect after a number of outbound errors
		DisconnectAfterMaxErrors(u16),
		/// Disconnect after a certain number of concurrent timeouts
		DisconnectAfterMaxTimeouts(u16),
	}

	/// Struct that stores critical information for the execution of the [`PingErrorPolicy`]
	#[derive(Debug)]
	pub struct PingManager {
		/// The number of timeout errors encountered from a peer
		pub timeouts: HashMap<PeerId, u16>,
		/// The number of outbound errors encountered from a peer
		pub outbound_errors: HashMap<PeerId, u16>,
	}

	/// The configuration for the `Ping` protocol
	pub struct PingConfig {
		/// The interval between successive pings.
		/// Default is 15 seconds
		pub interval: Duration,
		/// The duration before which the request is considered failure.
		/// Default is 20 seconds
		pub timeout: Duration,
		/// Error policy
		pub err_policy: PingErrorPolicy,
	}

	/// Critical information to manage `Ping` operations
	pub struct PingInfo {
		pub policy: PingErrorPolicy,
		pub manager: PingManager,
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	// set up a default node helper
	pub fn setup_core_builder() -> CoreBuilder<DefaultHandler> {
		let config = BootstrapConfig::default();
		let handler = DefaultHandler;

		// return default network core builder
		CoreBuilder::with_config(config, handler)
	}

	#[test]
	fn network_id_default_behavior_works() {
		// build a node with the default network id
		let default_node = setup_core_builder();

		// assert that the default network id is '/swarmnl/1.0'
		assert_eq!(default_node.network_id(), DEFAULT_NETWORK_ID.to_string());
	}

	#[test]
	fn network_id_custom_behavior_works_as_expected() {
		// build a node with the default network id
		let mut custom_builder = setup_core_builder();

		// pass in a custom network id and assert it works as expected
		let custom_protocol: &str = "/custom-protocol/1.0";
		
		let custom_builder = custom_builder.with_network_id(custom_protocol.to_string());

		// cannot be less than MIN_NETWORK_ID_LENGTH
		assert_eq!(custom_builder.network_id().len() >= MIN_NETWORK_ID_LENGTH.into(), true);

		// must start with a forward slash
		assert!(custom_builder.network_id().starts_with("/"));

		// assert that the custom network id is '/custom/protocol/1.0'
		assert_eq!(custom_builder.network_id(), custom_protocol.to_string());
	}

	#[test]
	#[should_panic(expected="could not parse provided network id")]
	fn network_id_custom_behavior_fails() {
		// build a node with the default network id
		let mut custom_builder = setup_core_builder();

		// pass in an invalid network ID
		// illegal: network ID length is less than MIN_NETWORK_ID_LENGTH
		let invalid_protocol_1 = "/1.0".to_string();

		let custom_builder = custom_builder.with_network_id(invalid_protocol_1);

		// pass in an invalid network ID
		// illegal: network ID must start with a forward slash
		let invalid_protocol_2 = "1.0".to_string();

		custom_builder.with_network_id(invalid_protocol_2);
	}

	// -- CoreBuilder tests --
}

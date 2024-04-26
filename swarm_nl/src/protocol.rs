/// Copyright (c) 2024 Algorealm

/// This module contains an implementation of the [fetch protocol](https://github.com/libp2p/specs/tree/master/fetch) specified in the original libp2p spec

/// The fetch protocol is a means for a libp2p node to get data from another, based on a given key.
/// It generally fulfills the contract: `Fetch(key) (value, statusCode)`

/// The fetch protocol is important and useful in performing a simple retrieval. It can be used to
/// augment other protocols e.g `Kademlia`. The fetch protocol implemented here is represented by
/// the protocol string `swarmnl-fetch/1.0`. This is especially chosen in case the official
/// `rust-libp2p` project decides to standardize the protocol, using the ideal protocol id:
/// "/libp2p/fetch/0.0.1". This protocol contains an implementation of the libp2p `fetch` protocol.
mod fetch {
	use self::handler::{Config, StatusCode};
	use libp2p::{swarm::NetworkBehaviour, StreamProtocol};

	/// The protocol name
	const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("swarmnl-fetch/1.0");
	/// The type that describes data returned from the queried peer
	type FetchPayload = (Vec<u8>, StatusCode);

	/// A [`NetworkBehaviour`] that sends out one-time requests to fetch
	/// data from a connected peer. It is light and can aggregate multiple "asks" into a single
	/// request.
	// pub struct Behaviour {
	// 	/// Configuration for fetching and sending data
	// 	config: Config,
	// 	/// Queue of events to yield to the swarm.
	// 	events: VecDeque<Event>,
	// }

	mod handler {
		use super::*;

		use std::{
			collections::{HashSet, VecDeque},
			error::Error,
			fmt, io,
			task::{Context, Poll},
			time::Duration,
		};

		use libp2p::{
			core::upgrade::ReadyUpgrade,
			swarm::{
				handler::DialUpgradeError, ConnectionHandler, ConnectionHandlerEvent,
				StreamUpgradeError, SubstreamProtocol,
			},
			StreamProtocol, Stream
		};
		use libp2p_identity::PeerId;
		use void::Void;
        use futures::{future::{BoxFuture}, FutureExt};


		/// The configuration for the `fetch` protocol
		#[derive(Debug, Clone)]
		pub struct Config {
			/// The epoch to consider to enforce restrictions
			epoch: Duration,
			/// Maximum fetch request to respond to, per epoch. This is to prevent the
			/// node from being spammed by a dishonest peer
			max_requests: u32,
			/// Block-list containing peers that violate the configured policy.
			/// Every request from peers that are in this list will be ignored.
			/// Peers get removed from this list after a decay period where they are given the
			/// chance to act accordingly again.
			block_list: HashSet<PeerId>,
			/// Decay period to remove a peer from the block list. Must bbe greater than 60 seconds
			decay_period: Duration,
			/// List that contains peers to completely ignore, most likely due to spam.
			/// This can be set due to other reasons or behaviours observed in the network.
			/// This is a sticky parameter as it does not go away when the offending peer
			/// disconnects, it has to be removed manually.
			black_list: HashSet<PeerId>,
		}

		impl Config {
			/// Creates a new [`Config`] with the following defaulst settings:
			///
			///   * [`Config::with_epoch`] 30s
			///   * [`Config::max_requests`] 25
			///   * [`Config::with_decay_periood`] 120s
			///
			///  For the `black_list, it is up to the user to implement policies that warrant nodes
			/// being added to the black list
			pub fn new() -> Self {
				Self {
					epoch: Duration::from_secs(30),
					max_requests: 25,
					block_list: HashSet::new(),
					decay_period: Duration::from_secs(120),
					black_list: HashSet::new(),
				}
			}

			/// Sets the epoch to consider to enforce restrictions
			pub fn with_epoch(&mut self, duration: Duration) {
				self.epoch = duration;
			}

			/// Sets the max fetch requests to respond to, per epoch. This is to prevent the
			/// node from being spammed by a dishonest peer
			pub fn max_requests(&mut self, max: u32) {
				self.max_requests = max;
			}

			/// Sets the decay period to remove a peer from the block list. Must be greater than 60
			/// seconds
			pub fn with_decay_period(&mut self, duration: Duration) {
				self.decay_period = duration;
			}
		}

		impl Default for Config {
			fn default() -> Self {
				Self::new()
			}
		}

		/// Status codes to indicate state of response from a peer
		pub enum StatusCode {
			Ok,
			NotFound,
			Error,
		}

		/// The `fetch` protocl errors
		#[derive(Debug)]
		pub enum Failure {
			/// The peer does not have the data requested for
			NotFound { key: Vec<u8> },
			/// The fetch protocol is unsupported,
			Unsupported,
			/// Our node has been restricted and is temporarily added to the peers blocklist. We
			/// have most likely made more requests than configured
			Restricted,
			/// Our node has been blocked
			Blocked,
			/// The fetch failed due to other errors
			Other {
				error: Box<dyn std::error::Error + Send + 'static>,
			},
		}

		impl Failure {
			fn other(e: impl std::error::Error + Send + 'static) -> Self {
				Self::Other { error: Box::new(e) }
			}
		}

		impl fmt::Display for Failure {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				match self {
					Failure::NotFound { key } => f.write_str("No data found"),
					Failure::Unsupported => write!(f, "Fetch protocol not supported"),
					Failure::Restricted => f.write_str("Node has been restricted"),
					Failure::Blocked => f.write_str("Node has been blocked"),
					Failure::Other { error } => write!(f, "Ping error: {error}"),
				}
			}
		}

		impl Error for Failure {
			fn source(&self) -> Option<&(dyn Error + 'static)> {
				match self {
					Failure::NotFound { key } => None,
					Failure::Unsupported => None,
					Failure::Restricted => None,
					Failure::Blocked => None,
					Failure::Other { error } => Some(&**error),
				}
			}
		}

		/// Protocol handler that handles quering the remote peer for data and handling incoming
		/// fetch requests
		pub struct Handler {
			/// Configuration options.
			config: Config,
			/// Outbound fetch failures that are pending to be processed by `poll()`.
			pending_errors: VecDeque<Failure>,
			/// The outbound fetch state.
			outbound: Option<OutboundState>,
			/// The inbound fetch handler
			inbound: Option<FetchFuture>,
			/// Tracks the state of our handler.
			state: State,
		}

		#[derive(Debug, Clone, Copy, PartialEq, Eq)]
		enum State {
			/// We are inactive because the other peer doesn't support fetch.
			Inactive {
				/// Whether or not we've reported the missing support yet.
				///
				/// This is used to avoid repeated events being emitted for a specific connection.
				reported: bool,
			},
			/// We are actively requesting data from the other peer.
			Active,
		}

		impl Handler {
			/// Builds a new [`Handler`] with the given configuration.
			pub fn new(config: Config) -> Self {
				Handler {
					config,
					pending_errors: VecDeque::with_capacity(2),
					outbound: None,
					inbound: None,
					state: State::Active,
				}
			}

			fn on_dial_upgrade_error(
				&mut self,
				DialUpgradeError { error, .. }: DialUpgradeError<
					<Self as ConnectionHandler>::OutboundOpenInfo,
					<Self as ConnectionHandler>::OutboundProtocol,
				>,
			) {
				self.outbound = None; // Request a new substream on the next `poll`.

				let error = match error {
					StreamUpgradeError::NegotiationFailed => {
						debug_assert_eq!(self.state, State::Active);

						self.state = State::Inactive { reported: false };
						return;
					},
					// Note: This timeout only covers protocol negotiation.
					StreamUpgradeError::Timeout => Failure::Other {
						error: Box::new(std::io::Error::new(
							std::io::ErrorKind::TimedOut,
							"fetch protocol negotiation timed out",
						)),
					},
					StreamUpgradeError::Apply(e) => void::unreachable(e),
					StreamUpgradeError::Io(e) => Failure::Other { error: Box::new(e) },
				};

				self.pending_errors.push_front(error);
			}
		}

		impl ConnectionHandler for Handler {
			type FromBehaviour = Void;
			type ToBehaviour = Result<Duration, Failure>;
			type InboundProtocol = ReadyUpgrade<StreamProtocol>;
			type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
			type OutboundOpenInfo = ();
			type InboundOpenInfo = ();

			fn listen_protocol(&self) -> SubstreamProtocol<ReadyUpgrade<StreamProtocol>, ()> {
				SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ())
			}

			fn on_behaviour_event(&mut self, _: Void) {}

			#[tracing::instrument(
				level = "trace",
				name = "ConnectionHandler::poll",
				skip(self, cx)
			)]
			fn poll(
				&mut self,
				cx: &mut Context<'_>,
			) -> Poll<
				ConnectionHandlerEvent<
					ReadyUpgrade<StreamProtocol>,
					(),
					Result<FetchPayload, Failure>,
				>,
			> {
				match self.state {
					State::Inactive { reported: true } => {
						return Poll::Pending; // nothing to do on this connection
					},
					State::Inactive { reported: false } => {
						self.state = State::Inactive { reported: true };
						return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Err(
							Failure::Unsupported,
						)));
					},
					State::Active => {},
				}

				// Respond to fetch request
				if let Some(fut) = self.inbound.as_mut() {
					match fut.poll_unpin(cx) {
						Poll::Pending => {},
						Poll::Ready(Err(e)) => {
							tracing::debug!("Fetch request error: {:?}", e);
							self.inbound = None;
						},
						Poll::Ready(Ok(stream)) => {
							tracing::trace!("answered fetch request from peer");

							// A ping from a remote peer has been answered, wait for the next.
							self.inbound = Some(protocol::recv_ping(stream).boxed());
						},
					}
				}

				loop {
					// Check for outbound ping failures.
					if let Some(error) = self.pending_errors.pop_back() {
						tracing::debug!("Ping failure: {:?}", error);

						self.failures += 1;

						// Note: For backward-compatibility the first failure is always "free"
						// and silent. This allows peers who use a new substream
						// for each ping to have successful ping exchanges with peers
						// that use a single substream, since every successful ping
						// resets `failures` to `0`.
						if self.failures > 1 {
							return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Err(
								error,
							)));
						}
					}

					// Continue outbound pings.
					match self.outbound.take() {
						Some(OutboundState::Ping(mut ping)) => match ping.poll_unpin(cx) {
							Poll::Pending => {
								self.outbound = Some(OutboundState::Ping(ping));
								break;
							},
							Poll::Ready(Ok((stream, rtt))) => {
								tracing::debug!(?rtt, "ping succeeded");
								self.failures = 0;
								self.interval.reset(self.config.interval);
								self.outbound = Some(OutboundState::Idle(stream));
								return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Ok(
									rtt,
								)));
							},
							Poll::Ready(Err(e)) => {
								self.interval.reset(self.config.interval);
								self.pending_errors.push_front(e);
							},
						},
						Some(OutboundState::Idle(stream)) => match self.interval.poll_unpin(cx) {
							Poll::Pending => {
								self.outbound = Some(OutboundState::Idle(stream));
								break;
							},
							Poll::Ready(()) => {
								self.outbound = Some(OutboundState::Ping(
									send_ping(stream, self.config.timeout).boxed(),
								));
							},
						},
						Some(OutboundState::OpenStream) => {
							self.outbound = Some(OutboundState::OpenStream);
							break;
						},
						None => match self.interval.poll_unpin(cx) {
							Poll::Pending => break,
							Poll::Ready(()) => {
								self.outbound = Some(OutboundState::OpenStream);
								let protocol =
									SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ());
								return Poll::Ready(
									ConnectionHandlerEvent::OutboundSubstreamRequest { protocol },
								);
							},
						},
					}
				}

				Poll::Pending
			}

			fn on_connection_event(
				&mut self,
				event: libp2p::swarm::handler::ConnectionEvent<
					Self::InboundProtocol,
					Self::OutboundProtocol,
					Self::InboundOpenInfo,
					Self::OutboundOpenInfo,
				>,
			) {
				todo!()
			}
		}

		type FetchFuture = BoxFuture<'static, Result<Stream, io::Error>>;

		/// The current state w.r.t. fetch requests
		enum OutboundState {
			/// A new substream is being negotiated for the fetch protocol.
			OpenStream,
			/// The substream is idle, waiting to send the next fetch request.
			Idle(Stream),
			/// A fetch request is being sent and the payload awaited.
			Fetch(FetchFuture),
		}
	}
}

use super::{*, behaviour::FetchPayload};

use std::{
	collections::{HashSet, VecDeque},
	error::Error,
	fmt, io,
	task::{Context, Poll},
	time::{Duration, Instant},
};

use futures::{future::BoxFuture, FutureExt};
use futures_timer::Delay;
use libp2p::{
	core::upgrade::ReadyUpgrade,
	swarm::{
		handler::DialUpgradeError, ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError,
		SubstreamProtocol,
	},
	Stream, StreamProtocol,
};
use void::Void;

/// The configuration for the `fetch` protocol
#[derive(Debug, Clone)]
pub struct Config<F, T>
where
	F: Fn(Vec<u8>, T) -> Vec<u8> + Send + Sync + 'static,
	T: Send + Sync + 'static,
{
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
	/// The function to call to handle the fetch request on the local node and return the
	/// bytes to be sent to the requesting peer
	pub(crate) request_handler: F,
	/// The state to pass to the `handler_function` to provide more flexiblity for handling
	/// requests
	pub(crate) state: Option<T>,
}

impl<F, T> Config<F, T> {
	/// Creates a new [`Config`] with the following default settings:
	///
	///   * [`Config::with_epoch`] 30s
	///   * [`Config::max_requests`] 25
	///   * [`Config::with_decay_periood`] 120s
	///
	///  For the `black_list, it is up to the user to implement policies that warrant nodes
	/// being added to the black list
	pub fn new(request_handler: F, state: Option<T>) -> Self {
		Self {
			epoch: Duration::from_secs(30),
			max_requests: 25,
			block_list: HashSet::new(),
			decay_period: Duration::from_secs(120),
			black_list: HashSet::new(),
			request_handler: Default::default(),
			state,
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
	/// Our node has been restricted and is temporarily added to the peer's blocklist. We
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

/// A message sent from the behaviour to the handler.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum HandlerIn {
	/// A fetch request list
	Fetch(Vec<Vec<u8>>),
}

/// The event emitted by the Handler. This informs the behaviour of various events created
/// by the handler.
#[derive(Debug)]
pub enum HandlerEvent {
    /// A Fetch request message has been recieved
    Fetch(Vec<Vec<u8>>)
}

/// Protocol handler that handles quering the remote peer for data and handling incoming
/// fetch requests
pub struct Handler<F, T>
where
	F: Fn(Vec<u8>, T) -> Vec<u8> + Send + Sync + 'static,
	T: Send + Sync + 'static,
{
	/// Configuration options.
	config: Config<F, T>,
	/// The timer used for the delay to the next outbound epoch after reaching the limit of
	/// the max requests allowed. This is important when the node has exceeded the maximum
	/// outbound fetch request for the current outbound epoch.
	interval: Delay,
	/// The outbound fetch state.
	outbound: Option<OutboundState>,
	/// The inbound fetch handler
	inbound: Option<FetchFuture>,
	/// Tracks the state of our handler.
	state: State,
	/// Maintains internal state of our handler.
	data: DataState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DataState {
	/// The end of the current outbound request sending epoch
	outbound_epoch_end: Instant,
	/// Max number of outbound request alreqady sent in the current epoch
	outbound_request_count: u32,
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

impl<F, T> Handler<F, T> {
	/// Builds a new [`Handler`] with the given configuration.
	pub fn new(config: Config<F, T>) -> Self {
		Handler {
			config,
            interval: Default::default(),
			outbound: None,
			inbound: None,
			state: State::Active,
            data: DataState { outbound_epoch_end: Instant::now() + Duration::from_secs(config.epoch), outbound_request_count: 0 }
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
	type FromBehaviour = HandlerIn;
	type ToBehaviour = HandlerEvent;
	type InboundProtocol = ReadyUpgrade<StreamProtocol>;
	type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
	type OutboundOpenInfo = ();
	type InboundOpenInfo = ();

	fn listen_protocol(&self) -> SubstreamProtocol<ReadyUpgrade<StreamProtocol>, ()> {
		SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ())
	}

	fn on_behaviour_event(&mut self, _: Void) {}

	#[tracing::instrument(level = "trace", name = "ConnectionHandler::poll", skip(self, cx))]
	fn poll(
		&mut self,
		cx: &mut Context<'_>,
	) -> Poll<ConnectionHandlerEvent<ReadyUpgrade<StreamProtocol>, (), Result<FetchPayload, Failure>>>
	{
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
					tracing::trace!("Answered fetch request from peer");

					// A fetch request from a remote peer has been answered, wait for the
					// next.
					self.inbound = Some(
						handle_fetch_request(
							stream,
							self.config.request_handler,
							self.config.state,
						)
						.boxed(),
					);
				},
			}
		}

		// 	/// The epoch to consider to enforce restrictions
		// epoch: Duration,
		// /// Maximum fetch request to respond to, per epoch. This is to prevent the
		// /// node from being spammed by a dishonest peer
		// max_requests: u32,
		// /// Block-list containing peers that violate the configured policy.
		// /// Every request from peers that are in this list will be ignored.
		// /// Peers get removed from this list after a decay period where they are given
		// the /// chance to act accordingly again.
		// block_list: HashSet<PeerId>,
		// /// Decay period to remove a peer from the block list. Must bbe greater than 60
		// seconds decay_period: Duration,
		// /// List that contains peers to completely ignore, most likely due to spam.
		// /// This can be set due to other reasons or behaviours observed in the network.
		// /// This is a sticky parameter as it does not go away when the offending peer
		// /// disconnects, it has to be removed manually.
		// black_list: HashSet<PeerId>,
		// /// The function to call to handle the fetch request on the local node and return
		// the /// bytes to be sent to the requesting peer
		// request_handler: F,
		// /// The state to pass to the `handler_function` to provide more flexiblity for
		// handling /// requests

		loop {
			// Continue handling outbound fetch requests
			match self.outbound.take() {
				Some(OutboundState::Fetch(mut fetch_request)) => match fetch_request.poll_unpin(cx)
				{
					Poll::Pending => {
						self.outbound = Some(OutboundState::Fetch(fetch_request));
						break;
					},
					Poll::Ready(Ok((stream, (payload, status)))) => {
						tracing::debug!(?payload, "fetch succeeded");

						// Get current time
						let now = Instant::now();

						// Make sure we do not exceed the maximum number of requests in the
						// configured epoch
						if now < self.data.outbound_epoch_end {
							// Perform check
							if self.data.outbound_request_count == self.config.max_requests {
								// Sleep, we don't want to get blocked
								self.outbound = Some(OutboundState::Idle(stream));
								// Set interval to wake up
								self.interval
									.reset(self.data.outbound_epoch_end - Instant::now());
							} else {
								// Increase outbound requests count
								self.data.outbound_request_count += 1;
							}
						} else {
							// This is a new request that is out of the former epoch. Set a
							// new time bound
							self.data.outbound_epoch_end = Instant::now() + self.config.epoch;
							// Reset request count
							self.data.outbound_request_count = 1;
						}

						// Emit event
						return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Ok((
							payload, status,
						))));
					},
					Poll::Ready(Err(e)) => {
						return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(Err(e)));
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
						let protocol = SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ());
						return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
							protocol,
						});
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

type FetchFuture = BoxFuture<'static, Result<Stream, Failure>>;

/// The current state w.r.t. fetch requests
enum OutboundState {
	/// A new substream is being negotiated for the fetch protocol.
	OpenStream,
	/// The substream is idle, waiting to send the next fetch request.
	Idle(Stream),
	/// A fetch request is being sent and the payload awaited.
	Fetch(FetchFuture),
}

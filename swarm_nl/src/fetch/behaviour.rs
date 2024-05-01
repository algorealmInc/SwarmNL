use self::handler::{Config, Handler, StatusCode};
use super::{handler::HandlerEvent, *};
use futures::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::{
	core::Endpoint,
	swarm::{
		ConnectionDenied, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent,
		ToSwarm,
	},
	StreamProtocol,
};
use std::{
	collections::VecDeque,
	io,
	task::{Context, Poll},
};

/// The protocol name
pub(crate) const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/libp2p/fetch/0.0.1");
/// The type that describes data returned from the queried peer
pub(crate) type FetchPayload = (Vec<u8>, StatusCode);

/// A [`NetworkBehaviour`] that sends out one-time requests to fetch
/// data from a connected peer. It is light and can aggregate multiple "asks" into a single
/// request.
pub struct Behaviour<F, T>
where
	F: Fn(Vec<u8>, T) -> Vec<u8> + Send + Sync + 'static,
	T: Send + Sync + 'static,
{
	/// Configuration for fetching and sending data
	config: Config<F, T>,
	/// Queue of events to yield to the swarm.
	events: VecDeque<Event>,
}

/// Event emmitted by the `Fetch` network behaviour
pub enum Event {
	/// Fetch request has been recieved by our node
	DataRequest {
		/// The keys that inform the values we need to return
		keys: Vec<Vec<u8>>,
	},
	/// A response to the fetch request has been sent
	DataResponse {
		/// The values and status code of each request data in order
		keys: Vec<(Vec<u8>, StatusCode)>,
	},
}

impl<F, T> Behaviour<F, T> {
	/// Creates a new `Fetch` network behaviour with the given configuration.
	pub fn new(config: Config<F, T>) -> Self {
		Self {
			config,
			events: VecDeque::new(),
		}
	}
}

impl<F, T> Default for Behaviour<F, T> {
	fn default() -> Self {
		Self::new(Config::new(Default::default(), None))
	}
}

impl<F, T> NetworkBehaviour for Behaviour<F, T> {
	type ConnectionHandler = Handler<F, T>;
	type ToSwarm = Event;

	fn handle_established_inbound_connection(
		&mut self,
		_: ConnectionId,
		_: PeerId,
		_: &Multiaddr,
		_: &Multiaddr,
	) -> Result<THandler<Self>, ConnectionDenied> {
		Ok(Handler::new(self.config.clone()))
	}

	fn handle_established_outbound_connection(
		&mut self,
		_: ConnectionId,
		_: PeerId,
		_: &Multiaddr,
		_: Endpoint,
	) -> Result<THandler<Self>, ConnectionDenied> {
		Ok(Handler::new(self.config.clone()))
	}

	fn on_connection_handler_event(
		&mut self,
		peer: PeerId,
		connection: ConnectionId,
		handler_event: THandlerOutEvent<Self>,
	) {
		match handler_event {
			// Receive and handle the fetch requests
			HandlerEvent::Fetch(keys) => {
				// call the supplied function on each member of the list and get the returned data
				let fetch_response = keys
					.iter()
					.map(|req| self.config.request_handler(req, self.config.state))
					.collect::<Vec<_>>();

				// Emit Event
				self.events
					.push_back(ToSwarm::GenerateEvent(Event::DataRequest { keys }));

				// Return the response to the requesting node
                self.events.push_back(ToSwarm::NotifyHandler {
                    peer_id,
                    event: HandlerIn::JoinedMesh,
                    handler: NotifyHandler::One(connection_id),
                });
			},
		}
	}

	#[tracing::instrument(level = "trace", name = "NetworkBehaviour::poll", skip(self))]
	fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
		if let Some(e) = self.events.pop_back() {
			Poll::Ready(ToSwarm::GenerateEvent(e))
		} else {
			Poll::Pending
		}
	}

	fn on_swarm_event(&mut self, _event: FromSwarm) {}
}

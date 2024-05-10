/// Copyright (c) 2024 Algorealm

/// Age of Empires
/// Objective: Form alliances and conquer as much empires as possible!
/// It is a multi-player game
/// Enjoy!
use std::{borrow::Cow, num::NonZeroU32, time::Duration};
use swarm_nl::{
	core::{AppData, Core, CoreBuilder},
	core::{AppResponse, EventHandler},
	setup::BootstrapConfig,
	util::string_to_peer_id,
	ConnectedPoint, ConnectionId, Keypair, PeerId,
};

#[tokio::main]
async fn main() {
	// ping_test::run_ping_example().await;

	// Communication example
	layer_communication::run_comm_example().await;
}

mod age_of_empire {
	use super::*;

	#[derive(Clone)]
	pub struct Empire {
		name: String,
		soldiers: u8,
		farmers: u8,
		blacksmith: u8,
		land_mass: u8,
		gold_reserve: u8,
	}

	impl Empire {
		/// Create a new empire and assign the assets to begin with
		pub fn new(name: String) -> Self {
			Empire {
				name,
				soldiers: 100,
				farmers: 100,
				blacksmith: 100,
				land_mass: 100,
				gold_reserve: 100,
			}
		}
	}

	impl EventHandler for Empire {
		fn new_listen_addr(
			&mut self,
			local_peer_id: PeerId,
			_listener_id: swarm_nl::ListenerId,
			addr: swarm_nl::Multiaddr,
		) {
			// announce interfaces we're listening on
			println!("Peer id: {}", local_peer_id);
			println!("We're listening on the {}", addr);
			println!(
				"There are {} soldiers guarding the {} Empire gate",
				self.soldiers, self.name
			);
		}

		fn connection_established(
			&mut self,
			peer_id: PeerId,
			_connection_id: ConnectionId,
			_endpoint: &ConnectedPoint,
			_num_established: NonZeroU32,
			_established_in: Duration,
		) {
			println!("Connection established with peer: {}", peer_id);
		}

		/// Handle any incoming RPC from any neighbouring empire
		fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
			// The semantics is left to the application to handle
			match String::from_utf8_lossy(&data[0]) {
				// Handle the request to get military status
				Cow::Borrowed("military_status") => {
					// Get empire name
					let empire_name = self.name.as_bytes().to_vec();

					// Get military capacity
					let military_capacity = self.soldiers;

					// marshall into accepted format andd then return it
					vec![empire_name, vec![military_capacity]]
				},
				_ => Default::default(),
			}
		}
	}

	/// Setup game (This is for the persian Empire)
	/// This requires no bootnodes connection
	// #[cfg(not(feature = "macedonian"))]
	// pub async fn setup_game() -> Core<Empire> {
	// 	// First, we want to configure our node
	// 	let config = BootstrapConfig::default();

	// 	// State kept by this node
	// 	let empire = Empire::new(String::from("Spartan"));

	// 	// Set up network
	// 	CoreBuilder::with_config(config, empire)
	// 		.build()
	// 		.await
	// 		.unwrap()
	// }

	/// The Macedonian Empire setup.
	/// These require bootnodes of empires to form alliance.
	/// We will be providing the location (peer id and multiaddress) of the Spartan Empire as boot
	/// parameters
	// #[cfg(feature = "macedonian")]
	pub async fn setup_game() -> Core<Empire> {
		// First, we want to configure our node with the bootstrap config file on disk
		let config = BootstrapConfig::from_file("bootstrap_config.ini");

		// State kept by this node
		let empire = Empire::new(String::from("Macedonian"));

		// Set up network
		CoreBuilder::with_config(config, empire)
			.build()
			.await
			.unwrap()
	}

	/// Play game
	pub async fn play_game() {
		// Setup network
		let mut core = setup_game().await;

		// TODO: DELAY FOR A WHILE

		// Print game state
		println!("Empire Information:");
		println!("Name: {}", core.state.soldiers);
		println!("Farmers: {}", core.state.farmers);
		println!("Black smiths: {}", core.state.blacksmith);
		println!("Land mass: {}", core.state.land_mass);
		println!("Gold reserve: {}", core.state.gold_reserve);

		// TODO! FUNCTION TO CHECK NODES I'M CONNECTED WITH

		let request = vec!["military_status".as_bytes().to_vec()];

		// Spartan Empire
		let remote_peer_id = "12D3KooWMD3kvZ7hSngeu1p7HAoCCYusSXqPPYDPvzxsa9T4vz3a";

		// Prepare request
		let status_request = AppData::FetchData {
			keys: request,
			peer: string_to_peer_id(remote_peer_id).unwrap(),
		};

		// Send request
		// let stream_id = core.send_to_network(status_request).await.unwrap();

		// Get response
		// AppData::Fetch returns a Vec<Vec<u8>>, hence we can parse the response from it
		if let Ok(status_response) = core.fetch_from_network(status_request).await {
			if let AppResponse::FetchData(status) = status_response {
				let empire_name = String::from_utf8_lossy(&status[0]);
				let military_status = status[1][0];

				// Print the military status of the empire we just contacted
				println!("Empire Contacted:");
				println!("Name: {} Empire", empire_name);
				println!("Military Capacity: {} Soldiers", military_status);
			}
		}

		// Keep looping so we can record network events
		loop {}
	}
}

mod ping_test {
	use swarm_nl::{
		core::ping_config::{PingConfig, PingErrorPolicy},
		Failure,
	};

	use super::*;
	/// Sate of the Application
	#[derive(Clone)]
	pub struct Ping;

	impl EventHandler for Ping {
		fn new_listen_addr(
			&mut self,
			local_peer_id: PeerId,
			_listener_id: swarm_nl::ListenerId,
			addr: swarm_nl::Multiaddr,
		) {
			// announce interfaces we're listening on
			println!("Peer id: {}", local_peer_id);
			println!("We're listening on the {}", addr);
		}

		fn connection_established(
			&mut self,
			peer_id: PeerId,
			_connection_id: ConnectionId,
			_endpoint: &ConnectedPoint,
			_num_established: NonZeroU32,
			_established_in: Duration,
		) {
			println!("Connection established with peer: {}", peer_id);
		}

		fn outbound_ping_success(&mut self, peer_id: PeerId, duration: Duration) {
			println!("we just pinged {:?}. RTT = {:?}", peer_id, duration);
		}

		fn outbound_ping_error(&mut self, peer_id: PeerId, err_type: Failure) {
			println!("Tried to ping {:?}. Error: {:?}", peer_id, err_type);
		}

		fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
			data
		}
	}

	#[cfg(not(feature = "second-node"))]
	pub async fn setup_node(buffer: &mut [u8], ports: (u16, u16)) -> Core<Ping> {
		let app_state = Ping;

		// First, we want to configure our node with the bootstrap config file on disk
		let config = BootstrapConfig::default()
			.generate_keypair_from_protobuf("ed25519", buffer)
			.with_tcp(ports.0)
			.with_udp(ports.1);

		println!("First node here!");

		// Set up network
		CoreBuilder::with_config(config, app_state)
			.build()
			.await
			.unwrap()
	}

	pub async fn run_ping_example() {
		// Our test keypair for the first node
		let mut protobuf = vec![
			8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3,
			27, 202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50,
			147, 85, 72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39,
			152, 42, 164, 148, 159, 36, 170, 109, 178,
		];
		// Ports for the first node
		let ports = (49500, 49501);

		// The PeerId of the first node
		let peer_id = Keypair::from_protobuf_encoding(&protobuf)
			.unwrap()
			.public()
			.to_peer_id();

		#[cfg(not(feature = "second-node"))]
		let node = ping_test::setup_node(&mut protobuf[..], ports).await;

		#[cfg(feature = "second-node")]
		let node = ping_test::setup_node(peer_id, ports).await;

		loop {}
	}

	/// Setup node
	#[cfg(feature = "second-node")]
	pub async fn setup_node(peer_id: PeerId, ports: (u16, u16)) -> Core<Ping> {
		use std::collections::HashMap;
		// App state
		let app_state = Ping;

		// Custom ping configuration
		let custom_ping = PingConfig {
			interval: Duration::from_secs(3),
			timeout: Duration::from_secs(5),
			err_policy: PingErrorPolicy::DisconnectAfterMaxErrors(3),
		};

		// Set up bootnode to query node 1
		let mut bootnode = HashMap::new();
		bootnode.insert(
			peer_id.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports.0),
		);

		println!("Second node here!");

		// First, we want to configure our node
		let config = BootstrapConfig::new().with_bootnodes(bootnode);

		// Set up network by passing in a default handler or application state
		CoreBuilder::with_config(config, app_state)
			.with_ping(custom_ping)
			.build()
			.await
			.unwrap()
	}
}

mod layer_communication {
	use super::*;

	/// Sate of the Application
	#[derive(Clone)]
	pub struct AppState;

	impl EventHandler for AppState {
		fn new_listen_addr(
			&mut self,
			local_peer_id: PeerId,
			_listener_id: swarm_nl::ListenerId,
			addr: swarm_nl::Multiaddr,
		) {
			// announce interfaces we're listening on
			println!("Peer id: {}", local_peer_id);
			println!("We're listening on the {}", addr);
		}

		fn connection_established(
			&mut self,
			peer_id: PeerId,
			_connection_id: ConnectionId,
			_endpoint: &ConnectedPoint,
			_num_established: NonZeroU32,
			_established_in: Duration,
		) {
			println!("Connection established with peer: {}", peer_id);
		}

		fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
			data
		}
	}

	pub async fn run_comm_example() {
		// Our test keypair for the first node
		let mut protobuf = vec![
			8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3,
			27, 202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50,
			147, 85, 72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39,
			152, 42, 164, 148, 159, 36, 170, 109, 178,
		];
		// Ports for the first node
		let ports = (49500, 49501);

		// The PeerId of the first node
		let peer_id = Keypair::from_protobuf_encoding(&protobuf)
			.unwrap()
			.public()
			.to_peer_id();

		let node = setup_node(&mut protobuf[..], ports).await;

		// Test that AppData::Echo works (using fetch)
		test_echo_atomically(node.clone()).await;

		// Test that AppData::Echo works
		test_echo(node.clone()).await;


		loop {}
	}

	#[cfg(not(feature = "second-node"))]
	pub async fn setup_node(buffer: &mut [u8], ports: (u16, u16)) -> Core<AppState> {
		let app_state = AppState;

		// First, we want to configure our node with the bootstrap config file on disk
		let config = BootstrapConfig::default()
			.generate_keypair_from_protobuf("ed25519", buffer)
			.with_tcp(ports.0)
			.with_udp(ports.1);

		println!("First node here!");

		// Set up network
		CoreBuilder::with_config(config, app_state)
			.build()
			.await
			.unwrap()
	}

	pub async fn test_echo_atomically(mut node: Core<AppState>) {
		// Prepare an echo request
		let echo_string = "Sacha rocks!".to_string();
		if let Ok(status_response) = node.fetch_from_network(AppData::Echo(echo_string.clone())).await {
			if let AppResponse::Echo(echoed_response) = status_response {
				// Assert that what was sent was gotten back
				assert_eq!(echo_string, echoed_response);

				println!("{} === {}", echo_string, echoed_response);
			}
		}
	}

	pub async fn test_echo(mut node: Core<AppState>) {
		// Prepare an echo request
		let echo_string = "Sacha rocks!".to_string();

		// Get request stream id
		let stream_id = node.send_to_network(AppData::Echo(echo_string.clone())).await.unwrap();

		println!("This is between the sending and the recieving of the payload. It is stored in an internal buffer, until polled for");

		if let Ok(status_response) = node.recv_from_network(stream_id).await {
			if let AppResponse::Echo(echoed_response) = status_response {
				// Assert that what was sent was gotten back
				assert_eq!(echo_string, echoed_response);

				println!("{} === {}", echo_string, echoed_response);
			}
		}
	}
}

// make pr
// merge to main
// loggings
// network data
// gossip
// examples
// appdata
// configure logger

// TEST
// Events, dailing, AppData, RPC, Kad, Ping, Gossip
// check for rexeports e.g to initialize gossipsub

// check if i'm subscribed to topics

// BootstrapConfig
// CoreBuilder

// INTEGRATION
// Core
// Ping
// Events
// App requests i.e kad, rpc, echo

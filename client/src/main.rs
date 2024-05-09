/// Copyright (c) 2024 Algorealm

/// Age of Empires
/// Objective: Form alliances and conquer as much empires as possible!
/// It is a multi-player game
/// Enjoy!
use std::{borrow::Cow, num::NonZeroU32, time::Duration};
use swarm_nl::{
	async_trait,
	core::{EventHandler, AppResponse},
	core::{AppData, Core, CoreBuilder},
	setup::BootstrapConfig,
	util::string_to_peer_id,
	ConnectedPoint, ConnectionId, PeerId,
};

#[tokio::main]
async fn main() {
	// Start our game! Age of Empires!
	play_game().await
}

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

#[async_trait]
impl EventHandler for Empire {
	async fn new_listen_addr(
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

	async fn connection_established(
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

	// TODO: Wait a little to help the network boot

	// Let them connect first
	tokio::time::sleep(Duration::from_secs(6)).await;

	let request = vec!["military_status".as_bytes().to_vec()];

	// Spartan Empire
	let remote_peer_id = "12D3KooWMD3kvZ7hSngeu1p7HAoCCYusSXqPPYDPvzxsa9T4vz3a";

	// Prepare request
	let status_request = AppData::FetchData {
		keys: request,
		peer: string_to_peer_id(remote_peer_id).unwrap(),
	};

	// Send request
	let stream_id = core.send_to_network(status_request).await.unwrap();

	// Get response
	// AppData::Fetch returns a Vec<Vec<u8>>, hence we can parse the response from it
	if let Ok(status_response) = core.recv_from_network(stream_id).await {
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

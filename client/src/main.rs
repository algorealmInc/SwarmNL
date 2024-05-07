/// Copyright (c) 2024 Algorealm

/// Age of Empires
/// Objective: Form alliances and conquer as much empires as possible!
/// It is a multi-player game
/// Enjoy!
use std::{any::Any, borrow::Cow, num::NonZeroU32, time::Duration};
use swarm_nl::{
	async_trait,
	core::EventHandler,
	core::{AppData, Core, CoreBuilder, NetworkChannel},
	setup::BootstrapConfig,
	ConnectedPoint, ConnectionId, PeerId,
};

pub static CONFIG_FILE_PATH: &str = "test_config.ini";

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
		_channel: NetworkChannel,
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
		mut channel: NetworkChannel,
		peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_established_in: Duration,
	) {
		// When we establish a new connection, the empires send message to the other empire to knoe
		// their military status
		let request = vec!["military_status".as_bytes().to_vec()];

		// Prepare request
		let status_request = AppData::FetchData {
			keys: request,
			peer: peer_id,
		};

		// Send request
		if let Some(stream_id) = channel.send_to_network(status_request).await {
			// Get response
			// AppData::Fetch returns a Vec<Vec<u8>>, hence we can parse the response from it
			let status_response = channel.recv_from_network(stream_id).await;

			let inner_value = &status_response as &dyn Any;
			if let Some(status) = inner_value.downcast_ref::<Vec<Vec<u8>>>() {
				// Get empire name
				let empire_name = String::from_utf8_lossy(&status[0]);
				let military_status = status[1][0];

				// Print the military status of the empire we just contacted
				println!("Empire Contacted:");
				println!("Name: {} Empire", empire_name);
				println!("Military Capacity: {} Soldiers", military_status);
			} else {
				println!("Could not decode response")
			}
		} else {
			println!("Could not get military status of the empire at {}", peer_id);
		}
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
// 	let empire = Empire::new(String::from("Macedonian"));

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
	let core = setup_game().await;

	// Print game state
	println!("Empire Information:");
	println!("Name: {}", core.state.soldiers);
	println!("Farmers: {}", core.state.farmers);
	println!("Black smiths: {}", core.state.blacksmith);
	println!("Land mass: {}", core.state.land_mass);
	println!("Gold reserve: {}", core.state.gold_reserve);

    // Keep looping so we can record network events
    loop { }
}

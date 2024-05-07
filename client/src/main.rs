/// Copyright (c) 2024 Algorealm

/// Age of Empires
/// Objective: Form alliances and conquer as much empires as possible!
/// It is a multi-player game
/// Enjoy!
use std::{num::NonZeroU32, time::Duration};
use swarm_nl::{
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
	soldiers: u32,
	farmers: u32,
	blacksmith: u32,
	land_mass: u32,
	gold_reserve: u32,
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
		channel: NetworkChannel,
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
		channel: NetworkChannel,
		peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_established_in: Duration,
	) {
	}

	fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		println!("Incoming data: {:?}", data);
		data
	}
}

/// Setup game (This is for the persian Empire)
/// This requires no bootnodes connection
#[cfg(not(feature = "macedonian"))]
pub async fn setup_game() -> Core<Empire> {
	// First, we want to configure our node
	let config = BootstrapConfig::default();

	// State kept by this node
	let empire = Empire::new(String::from("Spartan"));

	// Set up network
	CoreBuilder::with_config(config, empire)
		.build()
		.await
		.unwrap()
}

/// The Macedonian Empire setup.
/// These require bootnodes of empires to form alliance.
/// We will be providing the location (peer id and multiaddress) of the Spartan Empire as boot
/// parameters
#[cfg(feature = "macedonian")]
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
}

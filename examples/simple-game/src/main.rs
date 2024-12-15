//! Copyright (c) Algorealm 2024

//! BEST GUESSER GAME!
//!
//! This crate demonstrates how to use SwarmNl by building a simple game using two nodes.
//!
//! The game's logic is as follows:
//! - Node 1 and Node 2 connect to each other
//! - Then at an interval, they each guess a value
//! - The value is gossiped to the other peer
//! - The incoming value is then compared with the local value
//! - If the local value is greater, a score count is increased
//! - Whoever gets to the HIGHT_SCORE first wins the game and gossips the result
//! - The result is exchanged and the winner is announced on both nodes
//! - Once the winner is announced, the game ends and both nodes exit

use std::{collections::HashMap, time::Duration};

use rand::Rng;
use swarm_nl::{
	core::{gossipsub_cfg::GossipsubConfig, AppData, Core, CoreBuilder, NetworkEvent},
	setup::BootstrapConfig,
	Keypair, MessageId, PeerId, Port,
};

/// High score to determine winner
const HIGH_SCORE: i32 = 5;

/// Our test keypair for node 1. It is always deterministic, so that node 2 can always connect to it
/// at boot time
const PROTOBUF_KEYPAIR: [u8; 68] = [
	8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27, 202,
	193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85, 72, 64,
	174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164, 148, 159,
	36, 170, 109, 178,
];

/// Mesh topic tha the peers will gather round
const GOSSIP_NETWORK: &str = "game_group";

/// Time to wait for propagation.
const WAIT_TIME: u64 = 4;

/// Time to sleep for before generating new number.
const GAME_SLEEP_TIME: u64 = 2;

/// Game state
#[derive(Clone)]
struct Game {
	pub node: u8,
	pub score: i32,
	pub current_guess: i32,
}

/// Event that announces the beginning of the filtering and authentication of the incoming
/// gossip message. It returns a boolean to specify whether the massage should be dropped or
/// should reach the application. All incoming messages are allowed in by default.
fn gossipsub_filter_fn(
	propagation_source: PeerId,
	message_id: MessageId,
	source: Option<PeerId>,
	topic: String,
	data: Vec<String>,
) -> bool {
	// Drop all guesses that a greater than 10
	// Parse our data
	match data[0].as_str() {
		"guess" => {
			// Our remote peer has made a guess
			let remote_peer_guess = data[1].parse::<i32>().unwrap();

			if remote_peer_guess > 10 {
				println!("Message dropped, remote guess: {}", remote_peer_guess);
				return false;
			}
		},
		_ => {},
	}

	true
}

/// Handle the arrival of a gossip message.
fn handle_gossipsub_incoming_message(game: &mut Game, _source: PeerId, data: Vec<String>) {
	println!(
		"[[Node {}]] >> incoming data from peer -> {}: {}",
		game.node, data[0], data[1]
	);

	// Parse our data
	match data[0].as_str() {
		"guess" => {
			// Our remote peer has made a guess
			let remote_peer_guess = data[1].parse::<i32>().unwrap();

			// Compare
			if game.current_guess > remote_peer_guess {
				game.score += 1;
			}
		},
		"win" => {
			// Set our score to -1
			// Game over
			game.score = -1;
		},
		_ => {},
	}

	if game.score != -1 && game.score != HIGH_SCORE {
		println!(
			"[[Node {}]] >> Node ({}) score: {}",
			game.node, game.node, game.score
		);
	}
}

/// Used to create a detereministic node 1.
async fn setup_node_1(ports: (Port, Port)) -> Core {
	let mut protobuf = PROTOBUF_KEYPAIR.clone();

	// First, we want to configure our node by specifying a static keypair (for easy connection by
	// node 2)
	let config = BootstrapConfig::default()
		.generate_keypair_from_protobuf("ed25519", &mut protobuf)
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network with default configuration
	let builder = CoreBuilder::with_config(config);

	// Configure gossipsub
	// Specify the gossip filter algorithm
	let filter_fn = gossipsub_filter_fn;
	let builder = builder.with_gossipsub(GossipsubConfig::Default, filter_fn);

	// Build
	builder.build().await.unwrap()
}

/// Setup node 2.
async fn setup_node_2(node_1_ports: (Port, Port), ports: (Port, Port)) -> (Core, PeerId) {
	// The PeerId of the node 1
	let peer_id = Keypair::from_protobuf_encoding(&PROTOBUF_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	// Set up node 1 as bootnode, so we can connect to it immediately we start up
	let mut bootnode = HashMap::new();
	bootnode.insert(
		peer_id.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", node_1_ports.0),
	);

	// First, we want to configure our node (we'll be generating a new identity)
	let config = BootstrapConfig::new()
		.with_bootnodes(bootnode)
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network
	(
		CoreBuilder::with_config(config).build().await.unwrap(),
		peer_id,
	)
}

/// Run node 1
async fn run_node_1() {
	// Application state
	let mut game_state = Game {
		node: 1,
		score: 0,
		current_guess: -1,
	};

	// Set up node
	let mut node_1 = setup_node_1((49666, 49606)).await;

	// Read events generated at setup
	while let Some(event) = node_1.next_event().await {
		match event {
			NetworkEvent::NewListenAddr {
				local_peer_id,
				listener_id: _,
				address,
			} => {
				// Announce interfaces we're listening on
				println!("[[Node {}]] >> Peer id: {}", game_state.node, local_peer_id);
				println!(
					"[[Node {}]] >> We're listening on the {}",
					game_state.node, address
				);
			},
			NetworkEvent::ConnectionEstablished {
				peer_id,
				connection_id: _,
				endpoint: _,
				num_established: _,
				established_in: _,
			} => {
				println!("Connection established with peer: {:?}", peer_id);
			},
			_ => {},
		}
	}

	// Join a network (subscribe to a topic)
	let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

	// Send request to network
	let stream_id = node_1.send_to_network(gossip_request).await.unwrap();
	if let Ok(_) = node_1.recv_from_network(stream_id).await {
		// Wait for a few seconds
		tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;

		// We will be looping forever to generate new numbers, send them and compare them.
		// The loop stops when we have a winner
		loop {
			let mut rng = rand::thread_rng();
			let random_u32: i32 = rng.gen_range(1..=20);

			// Save as current
			game_state.current_guess = random_u32;

			// Prepare a gossip request
			let gossip_request = AppData::GossipsubBroadcastMessage {
				topic: GOSSIP_NETWORK.to_string(),
				message: vec!["guess".to_string().into(), random_u32.to_string().into()],
			};

			// Gossip our random value to our peers
			let _ = node_1.query_network(gossip_request).await;

			// Check score
			// If the remote has won, our handler will set our score to -1
			if game_state.score == HIGH_SCORE {
				// We've won!
				println!(
					"[[Node {}]] >> Congratulations! Node 1 is the winner.",
					game_state.node
				);

				// Inform Node 2

				// Prepare a gossip request
				let gossip_request = AppData::GossipsubBroadcastMessage {
					topic: GOSSIP_NETWORK.to_string(),
					message: vec!["win".to_string().into(), random_u32.to_string().into()],
				};

				// Gossip our random value to our peers
				let _ = node_1.query_network(gossip_request).await;

				break;
			} else if game_state.score == -1 {
				// We lost :(
				println!(
					"[[Node {}]] >> Game Over! Node 2 is the winner.",
					game_state.node
				);
				break;
			}

			// Check for gossip events
			let _ = node_1
				.events()
				.await
				.map(|e| {
					match e {
						NetworkEvent::GossipsubSubscribeMessageReceived { peer_id, topic } => {
							println!(
								"[[Node {}]] >> Peer {:?} just joined the mesh network for topic: {}",
								game_state.node, peer_id, topic
							);
						},
						NetworkEvent::GossipsubIncomingMessageHandled { source, data } => {
							// Call handler
							handle_gossipsub_incoming_message(&mut game_state, source, data);
						},
						_ => {},
					}
				})
				.collect::<Vec<_>>();

			// sleep for a few seconds
			tokio::time::sleep(Duration::from_secs(GAME_SLEEP_TIME)).await;
		}
	} else {
		panic!("Could not join mesh network");
	}
}

/// Run node 2
async fn run_node_2() {
	// Application state
	let mut game_state = Game {
		node: 2,
		score: 0,
		current_guess: -1,
	};

	// Set up node 2 and initiate connection to node 1
	let (mut node_2, _) = setup_node_2((49666, 49606), (49667, 49607)).await;

	// Read all currently buffered network events
	let events = node_2.events().await;

	let _ = events
		.map(|e| {
			match e {
				NetworkEvent::NewListenAddr {
					local_peer_id,
					listener_id: _,
					address,
				} => {
					// Announce interfaces we're listening on
					println!("[[Node {}]] >> Peer id: {}", game_state.node, local_peer_id);
					println!(
						"[[Node {}]] >> We're listening on the {}",
						game_state.node, address
					);
				},
				NetworkEvent::ConnectionEstablished {
					peer_id,
					connection_id: _,
					endpoint: _,
					num_established: _,
					established_in: _,
				} => {
					println!("Connection established with peer: {:?}", peer_id);
				},
				_ => {},
			}
		})
		.collect::<Vec<_>>();

	// Join a network (subscribe to a topic)
	let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

	// Send request to network
	let stream_id = node_2.send_to_network(gossip_request).await.unwrap();
	if let Ok(_) = node_2.recv_from_network(stream_id).await {
		// Wait for a few seconds
		tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;

		// We will be looping forever to generate new numbers, send them and compare them.
		// The loop stops when we have a winner
		loop {
			let mut rng = rand::thread_rng();
			let random_u32: i32 = rng.gen_range(1..=20);

			// Save as current
			game_state.current_guess = random_u32;

			// Prepare a gossip request
			let gossip_request = AppData::GossipsubBroadcastMessage {
				topic: GOSSIP_NETWORK.to_string(),
				message: vec!["guess".to_string().into(), random_u32.to_string().into()],
			};

			// Gossip our random value to our peers
			let _ = node_2.query_network(gossip_request).await;

			// Check score
			// If the remote has won, our handler will set our score to -1
			if game_state.score == HIGH_SCORE {
				// We've won!
				println!(
					"[[Node {}]] >> Congratulations! Node 2 is the winner.",
					game_state.node
				);

				// Inform Node 2

				// Prepare a gossip request
				let gossip_request = AppData::GossipsubBroadcastMessage {
					topic: GOSSIP_NETWORK.to_string().into(),
					message: vec!["win".to_string().into(), random_u32.to_string().into()],
				};

				// Gossip our random value to our peers
				let _ = node_2.query_network(gossip_request).await;

				break;
			} else if game_state.score == -1 {
				// We lost :(
				println!(
					"[[Node {}]] >> Game Over! Node 1 is the winner.",
					game_state.node
				);
				break;
			}

			// Check for gossip events
			let _ = node_2
				.events()
				.await
				.map(|e| {
					match e {
						NetworkEvent::GossipsubSubscribeMessageReceived { peer_id, topic } => {
							println!(
								"[[Node {}]] >> Peer {:?} just joined the mesh network for topic: {}",
								game_state.node, peer_id, topic
							);
						},
						NetworkEvent::GossipsubIncomingMessageHandled { source, data } => {
							// Call handler
							handle_gossipsub_incoming_message(&mut game_state, source, data);
						},
						_ => {},
					}
				})
				.collect::<Vec<_>>();

			// sleep for a few seconds
			tokio::time::sleep(Duration::from_secs(GAME_SLEEP_TIME)).await;
		}
	} else {
		panic!("Could not join mesh network");
	}
}

#[tokio::main]
async fn main() {
	#[cfg(feature = "first-node")]
	run_node_1().await;

	#[cfg(feature = "second-node")]
	run_node_2().await;
}

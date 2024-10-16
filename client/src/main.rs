//! Copyright 2024 Algorealm, Inc.

//! This example models a distributed network and is a rough sketch to help in determining concrete
//! features to be built

//! To model a basic distributed network, we will perform the following operations:
//! 1. Broadcast (Gossip) a random number we generate locally to our two other peers
//! 2. Send an RPC to our peers to ask for the sum of the number they recived from broadcast
//! 3. Add the three numbers together
//! 4. Save to a queue

use std::{collections::HashMap, time::Duration};

use rand::random;
use swarm_nl::{
	core::{
		gossipsub_cfg::GossipsubConfig, AppData, AppResponse, Core, CoreBuilder, NetworkEvent,
		RpcConfig,
	},
	setup::BootstrapConfig,
	Keypair, MessageId, MultiaddrString, PeerId, PeerIdString, Port,
};

/// The id of the gossip mesh network
pub const GOSSIP_NETWORK: &str = "random";

/// Amount of time to wait for proper sync
pub const WAIT_TIME: u64 = 3;

/// Basic node information
pub struct NodeInfo {
	/// Node identifier,
	name: String,
	/// Broadcast state, used to make sure we've recieved from both our peers
	state: HashMap<PeerId, u8>,
	/// Nonce
	nonce: Vec<u32>,
}

/// Handle incoming RPC
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	// Geenerate and return random number to peer
	let random_number = util::generate_random_number();

	vec![vec![random_number]]
}

/// Handle gissiping
fn gossipsub_filter_fn(
	propagation_source: PeerId,
	message_id: MessageId,
	source: Option<PeerId>,
	topic: String,
	data: Vec<String>,
) -> bool {
	true
}

/// Create a detereministic node
async fn setup_node(
	ports: (Port, Port),
	deterministic_protobuf: &[u8],
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
) -> Core {
	// Configure the node deterministically so we can connect to it
	let mut protobuf = &mut deterministic_protobuf.to_owned()[..];

	let config = BootstrapConfig::default()
		.generate_keypair_from_protobuf("ed25519", &mut protobuf)
		.with_tcp(ports.0)
		.with_udp(ports.1)
		// configure bootnodes, so we can connect to our sister nodes
		.with_bootnodes(boot_nodes);

	// Set up network
	let mut builder = CoreBuilder::with_config(config);

	// Configure RPC handling
	builder = builder.with_rpc(RpcConfig::Default, rpc_incoming_message_handler);

	// Configure gossipsub
	// Specify the gossip filter algorithm
	let filter_fn = gossipsub_filter_fn;
	let builder = builder.with_gossipsub(GossipsubConfig::Default, filter_fn);

	// Finish build
	builder.build().await.unwrap()
}

// #[cfg(feature = "first-node")]
async fn run_node(
	name: &str,
	ports_1: (Port, Port),
	ports_2: (Port, Port),
	ports_3: (Port, Port),
	peer_ids: (PeerId, PeerId),
	keypair: [u8; 68],
	http_port: Port,
) {
	// Define node information
	let mut node_info = NodeInfo {
		name: name.to_string(),
		state: HashMap::new(),
		nonce: Vec::new(),
	};

	// Setup HTTP server component
	util::setup_server(name, http_port).await;

	// Bootnodes
	let mut bootnodes = HashMap::new();
	bootnodes.insert(
		peer_ids.0.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
	);

	bootnodes.insert(
		peer_ids.1.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
	);

	// Setup node 1 and try to connect to node 2 and 3
	let mut node = setup_node(ports_1, &keypair[..], bootnodes).await;

	// Wait a little for setup and connections
	tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;

	// Read events generated at setup
	while let Some(event) = node.next_event().await {
		match event {
			NetworkEvent::NewListenAddr {
				local_peer_id,
				listener_id: _,
				address,
			} => {
				// Announce interfaces we're listening on
				println!("Peer id: {}", local_peer_id);
				println!("We're listening on {}", address);
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

	// Join a common network (subscribe to a topic)
	let join_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

	// Send request to network
	let stream_id = node.send_to_network(join_request).await.unwrap();
	if let Ok(_) = node.recv_from_network(stream_id).await {
		println!("[{name}]>>> Setup and ready to roll!");

		// Wait a little for joining decision to propagate
		util::sleep_for(WAIT_TIME).await;

		// Stay alive
		loop {
			println!("[{name}]>>> Generating random number");

			// Generate random number
			let random_number = util::generate_random_number();

			// Broadcast it to peers
			// Prepare message
			let gossip_request = AppData::GossipsubBroadcastMessage {
				topic: GOSSIP_NETWORK.to_string(),
				message: vec![random_number.to_string()],
			};

			// Send request
			if let Ok(_) = node.query_network(gossip_request).await {
				println!("[{name}]>>> Random number sent to peers: {}", random_number);

				// Wait a little for broadcasts to come in
				util::sleep_for(WAIT_TIME).await;

				// Consume the broadcast events and read the values
				while let Some(event) = node.next_event().await {
					match event {
						// Handle incoming broadcast of random numbers generated by our peers
						NetworkEvent::GossipsubIncomingMessageHandled { source, data } => {
							// Extract data
							let number = data[0].clone();

							// Parse u8 from String
							let rand_number = number.parse::<u8>().unwrap();

							println!(
								"[{name}]>>> Random number recieved from peer: {}",
								rand_number
							);

							// Insert and save state
							node_info.state.insert(source, rand_number);

							// Check the length of the hashmap. If it is 2, then we can send an RPC
							// to get the remaining numbers to compute our nonce

							if node_info.state.len() > 1 {
								// Prepare the RPC
								let rpc_request = AppData::FetchData {
									keys: vec!["random".as_bytes().to_vec()],
									peer: peer_ids.0.clone(), // The peer to query for data
								};

								// We break the flow into send and recv explicitly here
								let stream_id = node.send_to_network(rpc_request).await.unwrap();

								// If we used `query_network(_)`, we won't have been able to print
								// here
								println!(
									"[{name}]>>> A RPC request has been sent to peer: {:?}",
									peer_ids.0
								);

								// Poll the network for the result
								if let Ok(response) = node.recv_from_network(stream_id).await {
									if let AppResponse::FetchData(response) = response {
										// Get the random number
										let random_number = response[0].clone()[0];

										// Add it to the broadcast entry
										node_info.state.entry(peer_ids.0).and_modify(|num| {
											let _ = (*num).saturating_add(random_number);
										});

										println!(
											"[{name}]>>> RPC response recieved from peer: {:?}",
											peer_ids.0
										);

										// Now send a second a second RPC to the other node
										// Prepare the RPC
										let rpc_request = AppData::FetchData {
											keys: vec!["random".as_bytes().to_vec()],
											peer: peer_ids.1.clone(), // The peer to query for data
										};

										// We break the flow into send and recv explicitly here
										let stream_id =
											node.send_to_network(rpc_request).await.unwrap();

										// If we used `query_network(_)`, we won't have been able to
										// print here
										println!(
											">>> A RPC request has been sent to peer: {:?}",
											peer_ids.1
										);

										// Poll the network for the result
										if let Ok(response) =
											node.recv_from_network(stream_id).await
										{
											if let AppResponse::FetchData(response) = response {
												// Get the random number
												let random_number = response[0].clone()[0];

												// Add it to the broadcast entry
												node_info.state.entry(peer_ids.1).and_modify(
													|num| {
														let _ = num.saturating_add(random_number);
													},
												);

												println!(
													"[{name}]>>> RPC response recieved from peer: {:?}",
													peer_ids.1
												);

												// Now we can compute our nonce
												let nonce =
													*node_info.state.get(&peer_ids.1).unwrap()
														as u32 + *node_info
														.state
														.get(&peer_ids.1)
														.unwrap() as u32;
												node_info.nonce.push(nonce);

												println!(
													"[{name}]>>> New nonce computed and saved"
												);

												// Clear broadcast state
												node_info.state.clear();

												// Important to ensure consistent random generation
												break;
											}
										} else {
											println!("[{name}]>>> RPC Error: {}", peer_ids.1);
										}
									}
								} else {
									println!("[{name}]>>> RPC Error: {}", peer_ids.0);
								}
							}
						},
						_ => {},
					}
				}
			} else {
				println!("[{name}]>>> Could not gossip to peers");
			}
		}
	} else {
		panic!("[{name}]>>> Could not join mesh network");
	}
}

#[tokio::main]
async fn main() {
	// Node 1 keypair
	let node_1_keypair: [u8; 68] = [
		8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27,
		202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85,
		72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164,
		148, 159, 36, 170, 109, 178,
	];

	// Node 2 Keypair
	let node_2_keypair: [u8; 68] = [
		8, 1, 18, 64, 37, 37, 86, 103, 79, 48, 103, 83, 170, 172, 131, 160, 15, 138, 237, 128, 114,
		144, 239, 7, 37, 6, 217, 25, 202, 210, 55, 89, 55, 93, 0, 153, 82, 226, 1, 54, 240, 36,
		110, 110, 173, 119, 143, 79, 44, 82, 126, 121, 247, 154, 252, 215, 43, 21, 101, 109, 235,
		10, 127, 128, 52, 52, 68, 31,
	];

	// Node 3 Keypair
	let node_3_keypair: [u8; 68] = [
		8, 1, 18, 64, 211, 172, 68, 234, 95, 121, 188, 130, 107, 113, 212, 215, 211, 189, 219, 190,
		137, 91, 250, 222, 34, 152, 190, 117, 139, 199, 250, 5, 33, 65, 14, 180, 214, 5, 151, 109,
		184, 106, 73, 186, 126, 52, 59, 220, 170, 158, 195, 249, 110, 74, 222, 161, 88, 194, 187,
		112, 95, 131, 113, 251, 106, 94, 61, 177,
	];

	// Get Peer Id's
	let peer_id_1 = Keypair::from_protobuf_encoding(&node_1_keypair)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_2 = Keypair::from_protobuf_encoding(&node_2_keypair)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_3 = Keypair::from_protobuf_encoding(&node_3_keypair)
		.unwrap()
		.public()
		.to_peer_id();

	// Ports
	let ports_1: (Port, Port) = (49152, 55003);
	let ports_2: (Port, Port) = (49153, 55001);
	let ports_3: (Port, Port) = (49154, 55002);

	// HTTP Ports
	let http_ports = (4000, 4001, 4002);

	#[cfg(feature = "first-node")]
	{
		run_node(
			"Node 1",
			ports_1,
			ports_2,
			ports_3,
			(peer_id_2, peer_id_3),
			node_1_keypair,
			http_ports.0,
		)
		.await;
	}

	#[cfg(feature = "second-node")]
	{
		run_node(
			"Node 2",
			ports_2,
			ports_1,
			ports_3,
			(peer_id_1, peer_id_3),
			node_2_keypair,
			http_ports.1,
		)
		.await;
	}

	#[cfg(feature = "third-node")]
	{
		run_node(
			"Node 3",
			ports_3,
			ports_1,
			ports_2,
			(peer_id_1, peer_id_2),
			node_3_keypair,
			http_ports.2,
		)
		.await;
	}
}

/// Module containing utility functions
mod util {
	use super::*;
	use rand::Rng;
	use warp::{hyper::body::Bytes, Filter};

	/// Generate random number
	pub fn generate_random_number() -> u8 {
		let mut rng = rand::thread_rng(); // Initialize the random number generator
		rng.gen_range(1..=100) // Generate a number between 1 and 100 inclusive
	}

	/// Sleep for a specified duration
	pub async fn sleep_for(duration: u64) {
		tokio::time::sleep(Duration::from_secs(duration)).await;
	}

	/// Setup a HTTP server to recieve data from outside the network
	pub async fn setup_server(name: &str, http_port: Port) {
		// Define a POST route that accepts binary data
		let upload_route = warp::post()
			.and(warp::path("upload"))
			.and(warp::body::content_length_limit(1024 * 32))
			.and(warp::body::bytes()) // Accept raw bytes as the body
			.map(|bytes: Bytes| {
				// Print the binary data (you may want to log or process it)
				println!("Received binary data of length: {}", bytes.len());
				println!("{:?}", bytes);

				// Return a success response
				warp::reply::with_status("Binary data received", warp::http::StatusCode::OK)
			});

		// Start the server on specified port
		warp::serve(upload_route)
			.run(([127, 0, 0, 1], http_port))
			.await;

		println!("[{name}]>>> HTTP server ready!");
	}
}

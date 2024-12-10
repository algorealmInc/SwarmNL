//! Copyright 2024 Algorealm, Inc.

//! Sharding Example.
//! This example demonstrates the sharding configurations and capabilities of
//! SwarmNL. Here we will be implementing a range-based sharding policy. Also, it is important to
//! note that replication must be configured for sharding to take place. It is a prerequisite.
//! If no replication is configured, then the default behaviour will be assumed.

use std::{
	collections::{BTreeMap, HashMap},
	io::{self, Write},
	time::Duration,
};

use swarm_nl::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig},
		sharding::Sharding,
		ByteVector, Core, CoreBuilder, NetworkEvent, RpcConfig,
	},
	setup::BootstrapConfig,
	Keypair, MessageId, MultiaddrString, PeerId, PeerIdString, Port,
};

/// The constant that represents the id of the sharding network. Should be kept as a secret.
pub const NETWORK_SHARDING_ID: &'static str = "sharding_xx";
/// The time to wait for events, if necessary
pub const WAIT_TIME: u64 = 2;

/// Handle incoming RPC
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	// Just return incomding data
	data
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

// Implement the `Sharding` trait
/// Range-based sharding implementation
pub struct RangeSharding<T>
where
	T: ToString + Send + Sync,
{
	/// A map where the key represents the upper bound of a range, and the value is the
	/// corresponding shard ID
	ranges: BTreeMap<u64, T>,
}

impl<T> RangeSharding<T>
where
	T: ToString + Send + Sync,
{
	/// Creates a new RangeSharding instance
	pub fn new(ranges: BTreeMap<u64, T>) -> Self {
		Self { ranges }
	}
}

impl<T> Sharding for RangeSharding<T>
where
	T: ToString + Send + Sync + Clone,
{
	type Key = u64;
	type ShardId = T;

	/// Locate the shard corresponding to the given key
	fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId> {
		// Find the first range whose upper bound is greater than or equal to the key
		self.ranges
			.iter()
			.find(|(&upper_bound, _)| key <= &upper_bound)
			.map(|(_, shard_id)| shard_id.clone())
	}
}

/// Function to respond to a request to read data off a node explicitly (data forwarding fetch
/// request)
fn callback(req: ByteVector) -> ByteVector {
	// Return the request + some additional data
	let mut response = req[0].clone();
	response.push(b'@');

	vec![response]
}

// Create a determininstic node
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

	// Configure node for replication, we will be using a strong consistency model here
	let repl_config = ReplNetworkConfig::Custom {
		queue_length: 150,
		expiry_time: Some(10),
		sync_wait_time: 5,
		consistency_model: ConsistencyModel::Strong(ConsensusModel::All),
		data_aging_period: 2,
	};

	builder
		.with_replication(repl_config)
		.with_sharding(NETWORK_SHARDING_ID.into(), callback)
		.build()
		.await
		.unwrap()
}

// #[cfg(feature = "first-node")]
async fn run_node(
	name: &str,
	ports_1: (Port, Port),
	ports_2: (Port, Port),
	ports_3: (Port, Port),
	peer_ids: (PeerId, PeerId),
	keypair: [u8; 68],
) {
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

	// Setup node
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

	// Spin up a task to listen for replication events
	let new_node = node.clone();
	tokio::task::spawn(async move {
		let mut node = new_node.clone();
		loop {
			// Check for incoming data events
			if let Some(event) = node.next_event().await {
				// Check for only incoming repl data
				if let NetworkEvent::IncomingForwardedData { data, source } = event {
					println!(
						"Recieved forwarded data: {} from peer: {}",
						data[0],
						source.to_base58()
					);
				}
			}

			// Sleep
			tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;
		}
	});

	// Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;
	let shard_id_3 = 3;

	// Define shard ranges
	let mut ranges = BTreeMap::new();

	// Key ranges => Shard id
	ranges.insert(100, shard_id_1);
	ranges.insert(200, shard_id_2);
	ranges.insert(300, shard_id_3);

	// Initialize the range-based sharding policy
	let shard_manager = RangeSharding::new(ranges);

	// Join appropriate shards each
	match name {
		"Node1" => {
			shard_manager.join_network(node.clone(), &shard_id_1).await;
		},
		"Node2" => {
			shard_manager.join_network(node.clone(), &shard_id_2).await;
		},
		"Node3" => {
			shard_manager.join_network(node.clone(), &shard_id_3).await;
		},
		_ => {},
	}

	println!("\n===================");
	println!("Sharding Example Menu");
	println!("Usage:");
	println!("shard <key> <data>          - Place data in appropriate shards");
	println!("Fetch <key> <request>       - Request data from the network");
	println!("exit                        - Exit the application");

	loop {
		// Read user input
		let mut input = String::new();
		print!("> ");

		io::stdout().flush().unwrap(); // Flush stdout to display prompt
		io::stdin().read_line(&mut input).unwrap();

		// Trim input and split into parts
		let mut parts = input.trim().split_whitespace();
		let command = parts.next(); // Get the first word
		let data = parts.collect::<Vec<_>>();

		// Match the first word and take action
		match command {
			Some("shard") => {
				if data.len() >= 2 {
					if let Ok(key) = data[0].parse::<u64>() {
						let shard_data = &data[2..].join(" "); // Join the remaining parts as data
						println!("Sharding data with key '{}': {}", key, shard_data);
						// Call your sharding logic here
						match shard_manager
							.shard(node.clone(), &key, vec![(*shard_data).clone().into()])
							.await
						{
							Ok(response) => match response {
								Some(data) => {
									println!(
										"The data to shard is '{}'",
										String::from_utf8_lossy(&data[0])
									);
									println!("It falls into the range of the current node and will be stored locally.");
								},
								None => println!("Successfully placed data in the right shard."),
							},
							Err(e) => println!("Sharding failed: {}", e.to_string()),
						}
					} else {
						println!("Error: 'key' must be a u64");
					}
				} else {
					println!("Error: 'shard' command requires at least a key and data.");
				}
			},
			Some("fetch") => {
				if data.len() >= 2 {
					if let Ok(key) = data[0].parse::<u64>() {
						let request = &data[2..].join(" "); // Join the remaining parts as data
						println!("Requesting data with key '{}': {}", key, request);
						// Call your sharding logic here
						match shard_manager
							.fetch(node.clone(), &key, vec![(*request).clone().into()])
							.await
						{
							Ok(response) => match response {
								Some(data) => {
									println!(
										"The response data is '{}'",
										String::from_utf8_lossy(&data[0])
									);
									println!("Successfully pulled data from the network.");
								},
								None => println!("Data exists locally on node."),
							},
							Err(e) => println!("Fetching failed: {}", e.to_string()),
						}
					} else {
						println!("Error: 'key' must be a u64");
					}
				} else {
					println!("Error: 'fetch' command requires at least a key and request data.");
				}
			},
			Some("exit") => {
				println!("Exiting the application. Goodbye!");
				break;
			},
			Some(unknown) => println!("Unknown command: '{}'. Please try again.", unknown),
			None => println!("No command entered. Please try again."),
		}
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
	let ports_1: (Port, Port) = (49555, 55003);
	let ports_2: (Port, Port) = (49153, 55001);
	let ports_3: (Port, Port) = (49154, 55002);

	// Spin up the coordinator node
	#[cfg(feature = "third-node")]
	{
		run_node(
			"Node 1",
			ports_1,
			ports_2,
			ports_3,
			(peer_id_2, peer_id_3),
			node_1_keypair,
		)
		.await;
	}

	// Spin up second node
	#[cfg(feature = "second-node")]
	{
		run_node(
			"Node 2",
			ports_2,
			ports_1,
			ports_3,
			(peer_id_1, peer_id_3),
			node_2_keypair,
		)
		.await;
	}

	// Spin up third node
	#[cfg(feature = "first-node")]
	{
		run_node(
			"Node 3",
			ports_3,
			ports_1,
			ports_2,
			(peer_id_1, peer_id_2),
			node_3_keypair,
		)
		.await;
	}
}

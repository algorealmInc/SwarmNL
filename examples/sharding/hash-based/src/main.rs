//! © 2024 Algorealm, Inc. All rights reserved.

//! Example: Hash-Based Sharding
//!
//! This example demonstrates the sharding configurations and capabilities of SwarmNL
//! using a hash-based sharding policy. The key points to note are as follows:
//!
//! 1. **Replication Prerequisite**:
//!    - Sharding requires replication to be configured beforehand.
//!    - If replication is not explicitly configured, the system will assume default replication
//!      behavior.
//!    - REPLICATION MUST BE SET TO EVENTUAL CONSISTENCY
//!
//! 2. **Sharding Configuration**:
//!    - When setting up the sharded network, you must provide the local storage configuration.
//!    - Local storage is used to respond to forwarded data requests within the network.
//!
//! 3. **Local Storage**:
//!    - In this example, local storage is implemented as a directory in the local filesystem.
//!
//! This setup highlights the flexibility of SwarmNL in implementing custom sharding policies
//! while maintaining seamless integration with replication mechanisms.

use std::{
	collections::{BTreeMap, HashMap},
	fs,
	io::{self, Read, Write},
	path::Path,
	sync::Arc,
	time::Duration,
};

use swarm_nl::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig},
		sharding::{ShardStorage, Sharding},
		ByteVector, Core, CoreBuilder, NetworkEvent, RpcConfig,
	},
	setup::BootstrapConfig,
	Keypair, MessageId, MultiaddrString, PeerId, PeerIdString, Port,
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};

/// The constant that represents the id of the sharded network. Should be kept as a secret.
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

/// The shard local storage which is a directory in the local filesystem.
#[derive(Debug)]
struct LocalStorage;

impl LocalStorage {
	/// Reads a file's content from the working directory.
	fn read_file(&self, key: &str) -> Option<ByteVector> {
		let mut file = fs::File::open(key).ok()?;
		let mut content = Vec::new();
		file.read_to_end(&mut content).ok()?;
		// Wrap the content in an outer Vec
		Some(vec![content])
	}
}

// Implement the `ShardStorage` trait for our local storage
impl ShardStorage for LocalStorage {
	fn fetch_data(&self, key: ByteVector) -> ByteVector {
		// Process each key in the ByteVector
		for sub_key in key.iter() {
			let key_str = String::from_utf8_lossy(sub_key);
			// Attempt to read the file corresponding to the key
			if let Some(data) = self.read_file(&format!("storage/{}", key_str.as_ref())) {
				return data;
			}
		}
		// If no match is found, return an empty ByteVector
		Default::default()
	}
}

/// Hash-based sharding implementation.
pub struct HashSharding;

impl HashSharding {
	/// Compute a simple hash for the key.
	fn hash_key(&self, key: &str) -> u64 {
		// Convert the key to bytes
		let key_bytes = key.as_bytes();

		// Generate a hash from the first byte
		if let Some(&first_byte) = key_bytes.get(0) {
			key_bytes.iter().fold(first_byte as u64, |acc, &byte| {
				acc.wrapping_add(byte as u64)
			})
		} else {
			0
		}
	}
}

/// Implement the `Sharding` trait.
impl Sharding for HashSharding {
	type Key = str;
	type ShardId = String;

	/// Locate the shard corresponding to the given key.
	fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId> {
		// Calculate and return hash
		Some(self.hash_key(key).to_string())
	}
}

/// Utility function to append content to a file.
async fn append_to_file(file_name: &str, content: &str) -> Result<(), std::io::Error> {
	let location = format!("storage/{}", file_name);
	let file_path = Path::new(&location);

	// Open the file in append mode, create it if it doesn't exist
	let mut file = OpenOptions::new()
		.create(true) // Create file if it doesn't exist
		.append(true) // Open in append mode
		.open(file_path)
		.await?;

	// Prepare content to append (adding a new line)
	let content_to_write = format!("{}\n", content);

	// Write content to the file asynchronously
	file.write_all(content_to_write.as_bytes()).await?;

	Ok(())
}

// Create a determininstic node
async fn setup_node(
	ports: (Port, Port),
	deterministic_protobuf: &[u8],
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
	shard_storage: Arc<Mutex<LocalStorage>>,
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
		consistency_model: ConsistencyModel::Eventual,
		data_aging_period: 2,
	};

	builder
		.with_replication(repl_config)
		.with_sharding(NETWORK_SHARDING_ID.into(), shard_storage)
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

	// Local shard storage
	let local_storage = Arc::new(Mutex::new(LocalStorage));

	// Setup node
	let mut node = setup_node(ports_1, &keypair[..], bootnodes, local_storage.clone()).await;

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

	// Spin up a task to listen for network events
	let new_node = node.clone();
	tokio::task::spawn(async move {
		let mut node = new_node.clone();
		loop {
			// Check for incoming data events
			if let Some(event) = node.next_event().await {
				// Check for only incoming repl data
				match event {
					NetworkEvent::IncomingForwardedData { data, source } => {
						println!(
							"recieved forwarded data: {:?} from peer: {}",
							data,
							source.to_base58()
						);

						// Split the contents of the incoming data
						let data_vec = data[0].split(" ").collect::<Vec<_>>();

						// Extract file name and content
						if let [file_name, content] = &data_vec[..] {
							let _ = append_to_file(file_name, content).await;
						}
					},
					NetworkEvent::ReplicaDataIncoming {
						data,
						network,
						source,
						..
					} => {
						println!(
							"recieved replica data: {:?} from shard peer: {}",
							data,
							source.to_base58()
						);

						if let Some(repl_data) = node.consume_repl_data(&network).await {
							// Split the contents of the incoming data
							let data = repl_data.data[0].split(" ").collect::<Vec<_>>();

							// Extract file name and content
							if let [file_name, content] = &data[..] {
								let _ = append_to_file(file_name, content).await;
							}
						} else {
							println!("Error: No message in replica buffer");
						}
					},
					_ => {},
				}
			}

			// Sleep
			tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;
		}
	});

	// Initialize the hash-based sharding policy
	let shard_executor = HashSharding;

	// Convert shard keys to their IDs
	let shard_id_1 = shard_executor.locate_shard("earth").unwrap();
	let shard_id_2 = shard_executor.locate_shard("mars").unwrap();
	// let shard_id_3 = "venus";

	// Join appropriate shards each
	// Node 2 and 3 will join the same shard. Then replication will happen among them to maintain a
	// consistent shard network state across the nodes.
	match name {
		"Node 1" => {
			if shard_executor
				.join_network(node.clone(), &shard_id_1)
				.await
				.is_ok()
			{
				println!("Successfully joined shard: {}", shard_id_1);
			}
		},
		"Node 2" => {
			if shard_executor
				.join_network(node.clone(), &shard_id_2)
				.await
				.is_ok()
			{
				println!("Successfully joined shard: {}", shard_id_2);
			}
		},
		"Node 3" => {
			if shard_executor
				.join_network(node.clone(), &shard_id_2)
				.await
				.is_ok()
			{
				println!("Successfully joined shard: {}", shard_id_2);
			}
		},
		_ => {},
	}

	// Menu section
	println!("\n===================");
	println!("Sharding Example Menu");
	println!("Usage:");
	println!("shard <key> <file> <data>          - Place data in appropriate shard");
	println!("Fetch <key> <file>                 - Request data from the network");
	println!("read <file>      				     - Read data stored locally on this shard");
	println!("exit                               - Exit the application");

	loop {
		// Read user input
		let mut input = String::new();
		print!("> ");

		// Flush stdout to display prompt
		io::stdout().flush().unwrap();
		io::stdin().read_line(&mut input).unwrap();

		// Trim input and split into parts
		let mut parts = input.trim().split_whitespace();
		let command = parts.next();
		let data = parts.collect::<Vec<_>>();

		// Match the first word and take action
		match command {
			Some("shard") => {
				if data.len() >= 3 {
					let key = data[0];
					let file_name = &data[1];
					let shard_data = &data[1..].join(" ");
					println!(
						"Sharding data with key '{}': {} to file '{}'",
						key, shard_data, file_name
					);

					// Shard data across the network
					match shard_executor
						.shard(
							node.clone(),
							&key.to_string(),
							vec![(*shard_data).clone().into()],
						)
						.await
					{
						Ok(response) => match response {
							Some(data) => {
								println!(
									"The data to shard is '{}'.",
									String::from_utf8_lossy(&data[0])
								);
								println!("It matches the hash of the shard of the current node and will be stored locally.");

								// Save locally
								if let Err(e) =
									append_to_file(file_name, &String::from_utf8_lossy(&data[0]))
										.await
								{
									println!("Failed to save data to file: {}", e);
								}
							},
							None => println!("Successfully placed data in the right shard."),
						},
						Err(e) => println!("Sharding failed: {}", e.to_string()),
					}
				} else {
					println!(
						"Error: 'shard' command requires at least a key, file name, and data."
					);
				}
			},
			Some("fetch") => {
				if data.len() >= 2 {
					let key = data[0];
					let file_name = &data[1];
					println!("Requesting data with key '{}': {}", key, file_name);

					// Fetch data from network
					match shard_executor
						.fetch(
							node.clone(),
							&key.to_string(),
							vec![file_name.to_owned().into()],
						)
						.await
					{
						Ok(response) => match response {
							Some(data) => {
								if !data.is_empty() {
									// Parse incoming data
									let remote_data = String::from_utf8_lossy(&data[0]); // Process the entire buffer
									let trimmed_data = remote_data
										.trim_matches('\'')
										.split_whitespace()
										.collect::<Vec<_>>();

									println!(
										"The response data is:\n '{}'",
										trimmed_data.join(", ")
									);
								} else {
									println!("The remote node does not have the data stored.");
								}
								println!("Successfully pulled data from the network.");
							},
							None => println!("Data exists locally on node."),
						},
						Err(e) => println!("Fetching failed: {}", e.to_string()),
					}
				} else {
					println!("Error: 'fetch' command requires at least a key and file name.");
				}
			},
			Some("read") => {
				if data.len() >= 1 {
					let file_name = &data[0];
					println!("Reading data from file: {}", file_name);

					// Read from file
					match tokio::fs::read_to_string(&format!("storage/{}", file_name)).await {
						Ok(contents) => {
							for line in contents.lines() {
								println!("- {}", line);
							}
						},
						Err(e) => println!("Error reading from file: {}", e),
					}
				} else {
					println!("Error: 'read' command requires a file name.");
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

	// Node 2 keypair
	let node_2_keypair: [u8; 68] = [
		8, 1, 18, 64, 37, 37, 86, 103, 79, 48, 103, 83, 170, 172, 131, 160, 15, 138, 237, 128, 114,
		144, 239, 7, 37, 6, 217, 25, 202, 210, 55, 89, 55, 93, 0, 153, 82, 226, 1, 54, 240, 36,
		110, 110, 173, 119, 143, 79, 44, 82, 126, 121, 247, 154, 252, 215, 43, 21, 101, 109, 235,
		10, 127, 128, 52, 52, 68, 31,
	];

	// Node 3 keypair
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
	let ports_1: (Port, Port) = (49999, 55111);
	let ports_2: (Port, Port) = (49998, 55123);
	let ports_3: (Port, Port) = (49997, 55124);

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

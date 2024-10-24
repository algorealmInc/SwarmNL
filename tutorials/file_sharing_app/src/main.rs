// Copyright 2024 Algorealm

/// This crate demonstrates how to use SwarmNl. Here, we build a simple file sharing
/// application using two nodes. One nodes writes a record to the DHT and specifies itself as a
/// provider for a file it has locally. The other node reads the DHT and then uses an RPC to
/// fetch the file from the first peer.
use std::{
	collections::HashMap,
	fs::File,
	io::{self, BufRead, Read},
	time::Duration,
};

use swarm_nl::{
	core::{AppData, AppResponse, Core, CoreBuilder, NetworkEvent, RpcConfig},
	setup::BootstrapConfig,
	Keypair, PeerId, Port,
};

/// The key we're writing to the DHT
const KADEMLIA_KEY: &str = "config_file"; // File name
const KADEMLIA_VALUE: &str = "bootstrap_config.ini"; // Location on fs (it is in the same directory as our binary)

/// Our test keypair for node 1. It is always deterministic, so that node 2 can always connect to it
/// at boot time.
pub const PROTOBUF_KEYPAIR: [u8; 68] = [
	8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27, 202,
	193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85, 72, 64,
	174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164, 148, 159,
	36, 170, 109, 178,
];

/// Node 1 wait time (for node 2 to initiate connection).
/// This is useful because we need at least one connected peer (Quorum) to successfully write to the
/// DHT.
const NODE_1_WAIT_TIME: u64 = 10;

/// Node 2 wait time (for node 1 to write to the DHT).
const NODE_2_WAIT_TIME: u64 = 10;

// We need to handle the incoming RPC here
// What we're going to do is to look in our file system for the file specified in the RPC data and
// return it's binary content
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	println!("Received incoming RPC: {:?}", data);

	// Extract the file name from the incoming data
	let file_name = String::from_utf8_lossy(&data[0]);
	// Trim any potential whitespace
	let file_name = file_name.trim();

	// Read the contents of the file
	let mut file_content = Vec::new();
	match File::open(&file_name) {
		Ok(mut file) => match file.read_to_end(&mut file_content) {
			Ok(_) => {
				println!("File read successfully: {}", file_name);
			},
			Err(e) => {
				println!("Failed to read file content: {}", e);
				return vec![b"Error: Failed to read file content".to_vec()];
			},
		},
		Err(e) => {
			println!("Failed to open file: {}", e);
			return vec![b"Error: Failed to open file".to_vec()];
		},
	}

	// Return the file content as a Vec<Vec<u8>>
	vec![file_content]
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

	// Set up network
	let mut builder = CoreBuilder::with_config(config);

	// Configure RPC handling
	builder = builder.with_rpc(RpcConfig::Default, rpc_incoming_message_handler);

	// Finish build
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
	let mut builder = CoreBuilder::with_config(config);

	// Configure RPC handling
	builder = builder.with_rpc(RpcConfig::Default, rpc_incoming_message_handler);

	// Set up network
	(builder.build().await.unwrap(), peer_id)
}

/// Run node 1.
async fn run_node_1() {
	// Set up node
	let mut node = setup_node_1((49666, 49606)).await;

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
				println!("We're listening on the {}", address);
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

	// Sleep for a few seconds to allow node 2 to reach out
	async_std::task::sleep(Duration::from_secs(NODE_1_WAIT_TIME)).await;

	// What are we writing to the DHT?
	// A file we have on the fs and the location of the file, so it can be easily retrieved

	println!(
		"[1] >>>> Writing file location to DHT: {}",
		String::from_utf8_lossy(KADEMLIA_KEY.as_bytes())
	);

	// Prepare a query to write to the DHT
	let (key, value, expiration_time, explicit_peers) = (
		KADEMLIA_KEY.as_bytes().to_vec(),
		KADEMLIA_VALUE.as_bytes().to_vec(),
		None,
		None,
	);

	let kad_request = AppData::KademliaStoreRecord {
		key,
		value,
		expiration_time,
		explicit_peers,
	};

	// Submit query to the network
	node.query_network(kad_request).await.unwrap();


	// Check for DHT events
	let _ = node
	.events()
	.await
	.map(|e| {
		match e {
			NetworkEvent::KademliaPutRecordSuccess { key } => {
				// Call handler
				println!("Record successfully written to DHT. Key: {:?}", key);
			},
			_ => {},
		}
	})
	.collect::<Vec<_>>();

	loop {}
}

/// Run node 2.
async fn run_node_2() {
	// Set up node 2 and initiate connection to node 1
	let (mut node_2, node_1_peer_id) = setup_node_2((49666, 49606), (49667, 49607)).await;

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
					println!("Peer id: {}", local_peer_id);
					println!("We're listening on the {}", address);
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

	// Sleep for a few seconds to allow node 1 write to the DHT
	async_std::task::sleep(Duration::from_secs(NODE_2_WAIT_TIME)).await;

	// Prepare a query to read from the DHT
	let kad_request = AppData::KademliaLookupRecord {
		key: KADEMLIA_KEY.as_bytes().to_vec(),
	};

	// Submit query to the network
	if let Ok(result) = node_2.query_network(kad_request).await {
		// We have our response
		if let AppResponse::KademliaLookupSuccess(value) = result {
			println!(
				"[2] >>>> File read from DHT: {}",
				String::from_utf8_lossy(&value)
			);
			// Now prepare an RPC query to fetch the file from the remote node
			let fetch_key = vec![value];

			// prepare fetch request
			let fetch_request = AppData::FetchData {
				keys: fetch_key.clone(),
				peer: node_1_peer_id.clone(), // The peer to query for data
			};

			// We break the flow into send and recv explicitly here
			let stream_id = node_2.send_to_network(fetch_request).await.unwrap();

			// If we used `query_network(0)`, we won't have been able to print here
			println!(
				"[2] >>>> A fetch request has been sent to peer: {:?}",
				node_1_peer_id
			);

			// Poll the network for the result
			if let Ok(response) = node_2.recv_from_network(stream_id).await {
				if let AppResponse::FetchData(response_file) = response {
					// Get the file
					let file = response_file[0].clone();

					// Convert it to string
					let file_str = String::from_utf8_lossy(&file);

					// Print to stdout
					println!("[2] >>>> Here is the file delivered from the remote peer:");
					println!();
					println!("{}", file_str);
				}
			} else {
				println!("An error occured");
			}
		}
	}
}

#[async_std::main]
async fn main() {
	#[cfg(feature = "first-node")]
	run_node_1().await;

	#[cfg(feature = "second-node")]
	run_node_2().await;
}

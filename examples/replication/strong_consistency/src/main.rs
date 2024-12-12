//! Copyright 2024 Algorealm, Inc.

//! This example demonstrates the replication of data accross nodes in a network using the
//! strong data consistency synchronization model. Here we are spinning up three replica nodes that accept data
//! from standard input and then immedately replicates the data across the replica network.

use std::{collections::HashMap, io, time::Duration};

use swarm_nl::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplConfigData, ReplNetworkConfig},
		Core, CoreBuilder, NetworkEvent, RpcConfig,
	},
	setup::BootstrapConfig,
	Keypair, MessageId, MultiaddrString, PeerId, PeerIdString, Port,
};

/// The constant that represents the id of the replica network. Should be kept as a secret
pub const REPL_NETWORK_ID: &'static str = "replica_xx";
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

	builder.with_replication(repl_config).build().await.unwrap()
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

	// Setup node 1 and try to connect to node 2 and 3
	let mut node = setup_node(ports_1, &keypair[..], bootnodes).await;

	// Join replica network
	println!("Joining replication network");
	if let Ok(_) = node.join_repl_network(REPL_NETWORK_ID.into()).await {
		println!("Replica network successfully joined");
	} else {
		panic!("Failed to join replica network");
	}

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
				if let NetworkEvent::ReplicaDataIncoming { source, .. } = event {
					println!("Recieved incoming replica data from {}", source.to_base58());
				}
			}

			// Try to read the data from the buffer. Since we are using a strong
			// consistency model, we will not be able to read anything unless the
			// confirmations are complete
			if let Some(repl_data) = node.consume_repl_data(REPL_NETWORK_ID).await {
				println!(
					"Data gotten from replica: {} ({} confirmations)",
					repl_data.data[0],
					repl_data.confirmations.unwrap()
				);
			} 

			// Sleep
			tokio::time::sleep(Duration::from_secs(WAIT_TIME)).await;
		}
	});

	// Wait for some time for replication protocol intitialization across the network
	tokio::time::sleep(Duration::from_secs(WAIT_TIME + 3)).await;

	// Read input from standard input and then replicate it to peers
	let stdin = io::stdin();
	loop {
		print!("Enter some input: ");
		io::Write::flush(&mut io::stdout()).unwrap(); // Ensure the prompt is displayed immediately

		let mut input = String::new();
		stdin.read_line(&mut input).unwrap();
		let trimmed_input = input.trim();

		if trimmed_input == "exit" {
			break;
		}

		println!("Replicating...");

		// Replicate input
		match node
			.replicate(vec![trimmed_input.into()], REPL_NETWORK_ID)
			.await
		{
			Ok(_) => println!("Replication successful"),
			Err(e) => println!("Replication failed: {}", e.to_string()),
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
	let ports_1: (Port, Port) = (29555, 55009);
	let ports_2: (Port, Port) = (29153, 55008);
	let ports_3: (Port, Port) = (29154, 55007);

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

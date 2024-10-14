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
	Keypair, MultiaddrString, PeerId, PeerIdString, Port,
};

/// Amount of time to wait for proper sync
pub const WAIT_TIME: u64 = 3;

/// Node 1 keypair
pub const NODE_1_KEYPAIR: [u8; 68] = [
	8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27, 202,
	193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85, 72, 64,
	174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164, 148, 159,
	36, 170, 109, 178,
];

/// Node 2 Keypair
pub const NODE_2_KEYPAIR: [u8; 68] = [
	8, 1, 18, 64, 37, 37, 86, 103, 79, 48, 103, 83, 170, 172, 131, 160, 15, 138, 237, 128, 114,
	144, 239, 7, 37, 6, 217, 25, 202, 210, 55, 89, 55, 93, 0, 153, 82, 226, 1, 54, 240, 36, 110,
	110, 173, 119, 143, 79, 44, 82, 126, 121, 247, 154, 252, 215, 43, 21, 101, 109, 235, 10, 127,
	128, 52, 52, 68, 31,
];

/// Node 3 Keypair
pub const NODE_3_KEYPAIR: [u8; 68] = [
	8, 1, 18, 64, 211, 172, 68, 234, 95, 121, 188, 130, 107, 113, 212, 215, 211, 189, 219, 190,
	137, 91, 250, 222, 34, 152, 190, 117, 139, 199, 250, 5, 33, 65, 14, 180, 214, 5, 151, 109, 184,
	106, 73, 186, 126, 52, 59, 220, 170, 158, 195, 249, 110, 74, 222, 161, 88, 194, 187, 112, 95,
	131, 113, 251, 106, 94, 61, 177,
];

// Ports
pub const PORTS_1: (Port, Port) = (49152, 55003);
pub const PORTS_2: (Port, Port) = (49153, 55001);
pub const PORTS_3: (Port, Port) = (49154, 55002);

/// Handle incoming RPC
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	Default::default()
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

	// Finish build
	builder.build().await.unwrap()
}

#[cfg(feature = "first-node")]
async fn run_node() {
	// Get Peer Id's
	let peer_id_1 = Keypair::from_protobuf_encoding(&NODE_1_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_2 = Keypair::from_protobuf_encoding(&NODE_2_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_3 = Keypair::from_protobuf_encoding(&NODE_3_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	// Bootnodes (2 & 3)
	let mut bootnodes = HashMap::new();
	bootnodes.insert(
		peer_id_2.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_2.0),
	);

	bootnodes.insert(
		peer_id_3.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_3.0),
	);

	// Setup node 1 and try to connect to node 2 and 3
	let mut node = setup_node(PORTS_1, &NODE_1_KEYPAIR[..], bootnodes).await;

	// Wait a little for setup and connections
	async_std::task::sleep(Duration::from_secs(WAIT_TIME)).await;

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

	println!(">>> Node 1 setup and ready to roll!");

	// Stay alive
	loop {}
}

#[cfg(feature = "second-node")]
async fn run_node() {
	// Get Peer Id's
	let peer_id_1 = Keypair::from_protobuf_encoding(&NODE_1_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_2 = Keypair::from_protobuf_encoding(&NODE_2_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_3 = Keypair::from_protobuf_encoding(&NODE_3_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	// Bootnodes (1 & 3)
	let mut bootnodes = HashMap::new();
	bootnodes.insert(
		peer_id_1.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_1.0),
	);

	bootnodes.insert(
		peer_id_3.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_3.0),
	);

	// Setup node 1 and try to connect to node 2 and 3
	let mut node = setup_node(PORTS_2, &NODE_2_KEYPAIR[..], bootnodes).await;

	// Wait a little for setup and connections
	async_std::task::sleep(Duration::from_secs(WAIT_TIME)).await;

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

	println!(">>> Node 2 setup and ready to roll!");

	// Stay alive
	loop {}
}

#[cfg(feature = "third-node")]
async fn run_node() {
	// Get Peer Id's
	let peer_id_1 = Keypair::from_protobuf_encoding(&NODE_1_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_2 = Keypair::from_protobuf_encoding(&NODE_2_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_3 = Keypair::from_protobuf_encoding(&NODE_3_KEYPAIR)
		.unwrap()
		.public()
		.to_peer_id();

	// Bootnodes (1 & 2)
	let mut bootnodes = HashMap::new();
	bootnodes.insert(
		peer_id_1.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_1.0),
	);

	bootnodes.insert(
		peer_id_2.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", PORTS_2.0),
	);

	// Setup node 1 and try to connect to node 2 and 3
	let mut node = setup_node(PORTS_3, &NODE_3_KEYPAIR[..], bootnodes).await;

	// Wait a little for setup and connections
	async_std::task::sleep(Duration::from_secs(WAIT_TIME)).await;

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

	println!(">>> Node 3 setup and ready to roll!");

	// Stay alive
	loop {}
}

#[async_std::main]
async fn main() {
	#[cfg(any(
		feature = "first-node",
		feature = "second-node",
		feature = "third-node"
	))]
	run_node().await;
}

// Replication tests

use libp2p::{gossipsub::MessageId, PeerId};
use libp2p_identity::Keypair;
use std::{collections::HashMap, io, time::Duration};

use crate::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig},
		Core, CoreBuilder, NetworkEvent, ReplicaBufferQueue, RpcConfig,
	},
	setup::BootstrapConfig,
	MultiaddrString, PeerIdString, Port,
};

/// The constant that represents the id of the replica network.
pub const REPL_NETWORK_ID: &'static str = "replica_xx";
/// The time to wait for events, if necessary.
pub const WAIT_TIME: u64 = 2;

/// Handle incoming RPCs.
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	// Just return incomding data
	data
}

/// Handle gossiping.
fn gossipsub_filter_fn(
	propagation_source: PeerId,
	message_id: MessageId,
	source: Option<PeerId>,
	topic: String,
	data: Vec<String>,
) -> bool {
	true
}

/// Create a determininstic node.
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
	let mut builder = CoreBuilder::with_config(config)
		.with_rpc(RpcConfig::Default, rpc_incoming_message_handler)
		.with_gossipsub(GossipsubConfig::Default, gossipsub_filter_fn);

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

// - joining and exit

#[tokio::test]
async fn repl_itest_join_and_exit_works() {
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

	// Setup node 1
	tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_2.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
		);

		bootnodes.insert(
			peer_id_3.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
		);

		let mut node = setup_node(ports_1, &node_1_keypair[..], bootnodes).await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// exit replica network works
		let _ = node.leave_repl_network(REPL_NETWORK_ID.into()).await;
	});

	// setup node 2
	tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		bootnodes.insert(
			peer_id_3.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
		);

		let mut node = setup_node(ports_2, &node_2_keypair[..], bootnodes).await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// exit replica network works
		let _ = node.leave_repl_network(REPL_NETWORK_ID.into()).await;
	});

	// setup node 3
	tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		bootnodes.insert(
			peer_id_2.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
		);

		let mut node = setup_node(ports_3, &node_3_keypair[..], bootnodes).await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// assert that 2 nodes have joined
		assert_eq!(node.replica_peers(REPL_NETWORK_ID.into()).await.len(), 2);

		// after sleeping for 5 secons we expect there to be no more nodes in the replication network
		tokio::time::sleep(Duration::from_secs(5)).await;

		// assert that 2 nodes have left
		assert_eq!(node.replica_peers(REPL_NETWORK_ID.into()).await.len(), 0);
	});

	// Keep the main thread running
    loop {}
}

#[tokio::test]
async fn repl_itest_fully_replicate_node() {

		// Node 1 keypair
		let node_1_keypair: [u8; 68] = [
			8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3,
			27, 202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50,
			147, 85, 72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39,
			152, 42, 164, 148, 159, 36, 170, 109, 178,
		];

		// Node 2 Keypair
		let node_2_keypair: [u8; 68] = [
			8, 1, 18, 64, 37, 37, 86, 103, 79, 48, 103, 83, 170, 172, 131, 160, 15, 138, 237, 128,
			114, 144, 239, 7, 37, 6, 217, 25, 202, 210, 55, 89, 55, 93, 0, 153, 82, 226, 1, 54,
			240, 36, 110, 110, 173, 119, 143, 79, 44, 82, 126, 121, 247, 154, 252, 215, 43, 21,
			101, 109, 235, 10, 127, 128, 52, 52, 68, 31,
		];

		// Node 3 Keypair
		let node_3_keypair: [u8; 68] = [
			8, 1, 18, 64, 211, 172, 68, 234, 95, 121, 188, 130, 107, 113, 212, 215, 211, 189, 219,
			190, 137, 91, 250, 222, 34, 152, 190, 117, 139, 199, 250, 5, 33, 65, 14, 180, 214, 5,
			151, 109, 184, 106, 73, 186, 126, 52, 59, 220, 170, 158, 195, 249, 110, 74, 222, 161,
			88, 194, 187, 112, 95, 131, 113, 251, 106, 94, 61, 177,
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

		// Setup node 1
		let task_1 = tokio::task::spawn(async move {
			// Bootnodes
			let mut bootnodes = HashMap::new();

			bootnodes.insert(
				peer_id_2.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
			);

			bootnodes.insert(
				peer_id_3.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
			);

			let mut node = setup_node(ports_1, &node_1_keypair[..], bootnodes).await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Send to replica node 2
			node.replicate(vec!["Apples".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();
			node.replicate(vec!["Papayas".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();

			// Keep node running
			tokio::time::sleep(Duration::from_secs(10)).await;
		});

		// Setup node 2
		let task_2 = tokio::task::spawn(async move {
			// Bootnodes
			let mut bootnodes = HashMap::new();

			bootnodes.insert(
				peer_id_1.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
			);

			bootnodes.insert(
				peer_id_3.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
			);

			let mut node = setup_node(ports_2, &node_2_keypair[..], bootnodes).await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Send to replica node 1
			node.replicate(vec!["Oranges".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();
			node.replicate(vec!["Kiwis".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();

			// Keep node running
			tokio::time::sleep(Duration::from_secs(10)).await;
		});

		// Setup node 3
		let task_3 = tokio::task::spawn(async move {
			// Bootnodes
			let mut bootnodes = HashMap::new();

			bootnodes.insert(
				peer_id_1.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
			);

			bootnodes.insert(
				peer_id_2.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
			);

			let mut node = setup_node(ports_3, &node_3_keypair[..], bootnodes).await;

			// Sleep to wait for nodes 1 and 2 to replicate data
			tokio::time::sleep(Duration::from_secs(5)).await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Asset that this node (node 3) has nothing in its buffer
			assert_eq!(node.consume_repl_data(REPL_NETWORK_ID.into()).await, None);

			// Replicate the data from node 1's buffer (node 1 is the node that published node 2's data)
            
            // node.replicate_buffer(REPL_NETWORK_ID.into(), peer_id_1)
			// 	.await
			// 	.unwrap();

			// // Assert that this node (node 3) has the data from node 2
			// assert_eq!(
			// 	node.consume_repl_data(REPL_NETWORK_ID.into())
			// 		.await
			// 		.unwrap()
			// 		.data,
			// 	vec!["Oranges".to_string()]
			// );

			// assert_eq!(
			// 	node.consume_repl_data(REPL_NETWORK_ID.into())
			// 		.await
			// 		.unwrap()
			// 		.data,
			// 	vec!["Kiwis".to_string()]
			// );

			// Replicate the data from node 2's buffer (node 2 is the node that published node 1's data)
			node.replicate_buffer(REPL_NETWORK_ID.into(), peer_id_2)
				.await
				.unwrap();

			// Assert that this node (node 3) has the data from node 2
			assert_eq!(
				node.consume_repl_data(REPL_NETWORK_ID.into())
					.await
					.unwrap()
					.data,
				vec!["Apples".to_string()]
			);

			assert_eq!(
				node.consume_repl_data(REPL_NETWORK_ID.into())
					.await
					.unwrap()
					.data,
				vec!["Papayas".to_string()]
			);
		});

		for task in vec![task_1, task_2, task_3] {
			task.await.unwrap();
		}
}

// - confirmations (2 nodes)

// - temporary buffer
// - public buffer
// - confirmations (2 nodes)
// - confirmations (3 nodes)
// - joining on the fly
// - events on incoming messages
// - consistency model (All and Some(n))
// - Early detection of nodes that leave and join the network
// - completely replicate a node

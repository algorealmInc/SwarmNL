//! Tests for node replication.

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use crate::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig},
		Core, CoreBuilder, NetworkEvent, ReplicaBufferQueue, RpcConfig,
	},
	setup::BootstrapConfig,
	MultiaddrString, PeerIdString, Port,
};
use futures::{
	channel::mpsc::{self, Receiver, Sender},
	select, SinkExt, StreamExt,
};
use libp2p::{gossipsub::MessageId, PeerId};
use libp2p_identity::Keypair;
use std::{collections::HashMap, io, time::Duration};

use super::constants::*;

/// The constant that represents the id of the replica network.
pub const REPL_NETWORK_ID: &'static str = "replica_xx";

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
	consistency_model: ConsistencyModel,
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
	let builder = CoreBuilder::with_config(config)
		.with_rpc(RpcConfig::Default, rpc_incoming_message_handler)
		.with_gossipsub(GossipsubConfig::Default, gossipsub_filter_fn);

	// Configure node for replication, we will be using a strong consistency model here
	let repl_config = ReplNetworkConfig::Custom {
		queue_length: 150,
		expiry_time: Some(10),
		sync_wait_time: 5,
		consistency_model,
		data_aging_period: 2,
	};

	builder.with_replication(repl_config).build().await.unwrap()
}

// - joining and exit

#[tokio::test]
async fn repl_itest_join_and_exit_works() {
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

	// Ports
	let ports_1: (Port, Port) = (49152, 55103);
	let ports_2: (Port, Port) = (49153, 55101);
	let ports_3: (Port, Port) = (49154, 55102);

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

		let mut node = setup_node(
			ports_1,
			&NODE_1_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// exit replica network works
		let _ = node.leave_repl_network(REPL_NETWORK_ID.into()).await;
	});

	// setup node 2
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

		let mut node = setup_node(
			ports_2,
			&NODE_2_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// exit replica network works
		let _ = node.leave_repl_network(REPL_NETWORK_ID.into()).await;
	});

	// setup node 3
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

		let mut node = setup_node(
			ports_3,
			&NODE_3_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

		// join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// assert that 2 nodes have joined
		assert_eq!(node.replica_peers(REPL_NETWORK_ID.into()).await.len(), 2);

		// after sleeping for 5 seconds we expect there to be no more nodes in the replication
		// network
		tokio::time::sleep(Duration::from_secs(5)).await;

		// assert that 2 nodes have left
		assert_eq!(node.replica_peers(REPL_NETWORK_ID.into()).await.len(), 0);
	});

	for task in vec![task_1, task_2, task_3] {
		task.await.unwrap();
	}
}

#[tokio::test]
async fn repl_itest_fully_replicate_node() {
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

	// Ports
	let ports_1: (Port, Port) = (49255, 55203);
	let ports_2: (Port, Port) = (49253, 55201);
	let ports_3: (Port, Port) = (49254, 55202);

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
		let mut node = setup_node(
			ports_1,
			&NODE_1_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

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
		let mut node = setup_node(
			ports_2,
			&NODE_2_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

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
		let mut node = setup_node(
			ports_3,
			&NODE_3_KEYPAIR[..],
			bootnodes,
			ConsistencyModel::Strong(ConsensusModel::All),
		)
		.await;

		// Sleep to wait for nodes 1 and 2 to replicate data
		tokio::time::sleep(Duration::from_secs(20)).await;

		// Join replica network works
		let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

		// Assert that this node (node 3) has nothing in its buffer
		assert_eq!(node.consume_repl_data(REPL_NETWORK_ID.into()).await, None);

		// Replicate the data from node 1's buffer (node 1 is the node that published node 2's data)
		node.replicate_buffer(REPL_NETWORK_ID.into(), peer_id_1)
			.await
			.unwrap();

		// Assert that this node (node 3) has the data from node 2
		assert_eq!(
			node.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap()
				.data,
			vec!["Oranges".to_string()]
		);
		assert_eq!(
			node.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap()
				.data,
			vec!["Kiwis".to_string()]
		);

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

mod strong_consistency {
	use super::*;
	use crate::core::replication::ReplBufferData;

	#[tokio::test]
	async fn two_nodes_confirmations_with_all_consistency_model() {
		// Get Peer Id's
		let peer_id_1 = Keypair::from_protobuf_encoding(&NODE_1_KEYPAIR)
			.unwrap()
			.public()
			.to_peer_id();
		let peer_id_2 = Keypair::from_protobuf_encoding(&NODE_2_KEYPAIR)
			.unwrap()
			.public()
			.to_peer_id();

		// Ports
		let ports_1: (Port, Port) = (49355, 55303);
		let ports_2: (Port, Port) = (49353, 55301);

		// Setup node 1
		let task_1 = tokio::task::spawn(async move {
			// Bootnodes
			let mut bootnodes = HashMap::new();
			bootnodes.insert(
				peer_id_2.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
			);
			let mut node = setup_node(
				ports_1,
				&NODE_1_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::All),
			)
			.await;

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

			let mut node = setup_node(
				ports_2,
				&NODE_2_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::All),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Sleep for 4 seconds
			tokio::time::sleep(Duration::from_secs(4)).await;

			let first_repl_data = node
				.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap();
			let second_repl_data = node
				.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap();

			assert_eq!(first_repl_data.confirmations, Some(1));
			assert_eq!(first_repl_data.data, vec!["Apples".to_string()]);

			assert_eq!(second_repl_data.confirmations, Some(1));
			assert_eq!(second_repl_data.data, vec!["Papayas".to_string()]);
		});

		for task in vec![task_1, task_2] {
			task.await.unwrap();
		}
	}

	#[tokio::test]
	async fn multi_nodes_confirmations_with_all_consistency_model() {
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

		// Ports
		let ports_1: (Port, Port) = (49455, 55403);
		let ports_2: (Port, Port) = (49453, 55401);
		let ports_3: (Port, Port) = (49454, 55402);

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
			let mut node = setup_node(
				ports_1,
				&NODE_1_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::All),
			)
			.await;

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
			let mut node = setup_node(
				ports_2,
				&NODE_2_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::All),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

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
			let mut node = setup_node(
				ports_3,
				&NODE_3_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::All),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Sleep for 7 seconds to give time for confirmation
			tokio::time::sleep(Duration::from_secs(20)).await;

			let first_repl_data = node
				.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap();
			let second_repl_data = node
				.consume_repl_data(REPL_NETWORK_ID.into())
				.await
				.unwrap();

			// We expect two confirmations
			assert_eq!(first_repl_data.confirmations, Some(2));
			assert_eq!(first_repl_data.data, vec!["Apples".to_string()]);

			assert_eq!(second_repl_data.confirmations, Some(2));
			assert_eq!(second_repl_data.data, vec!["Papayas".to_string()]);
		});

		for task in vec![task_1, task_2, task_3] {
			task.await.unwrap();
		}
	}

	#[tokio::test]
	async fn confirmations_with_min_peer_consistency_model() {
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

		let peer_id_4 = Keypair::from_protobuf_encoding(&NODE_4_KEYPAIR)
			.unwrap()
			.public()
			.to_peer_id();

		// Ports
		let ports_1: (Port, Port) = (49555, 55503);
		let ports_2: (Port, Port) = (49553, 55501);
		let ports_3: (Port, Port) = (49554, 55502);
		let ports_4: (Port, Port) = (49552, 55504);

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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_1,
				&NODE_1_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::MinPeers(2)),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Send to replica node 2
			node.replicate(vec!["Apples".into()], &REPL_NETWORK_ID)
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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);
			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_2,
				&NODE_2_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::MinPeers(2)),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_3,
				&NODE_3_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::MinPeers(2)),
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			tokio::time::sleep(Duration::from_secs(10)).await;
		});

		// Setup node 4
		let task_4 = tokio::task::spawn(async move {
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
			bootnodes.insert(
				peer_id_3.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_4,
				&NODE_4_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Strong(ConsensusModel::MinPeers(2)),
			)
			.await;

			// Join replica network
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			loop {
				while let Some(data) = node.consume_repl_data(REPL_NETWORK_ID.into()).await {
					assert_eq!(data.confirmations, Some(2));
					assert_eq!(data.data, vec!["Apples".to_string()]);
					return;
				}

				tokio::time::sleep(Duration::from_secs(3)).await;
			}
		});

		for task in vec![task_1, task_2, task_3, task_4] {
			task.await.unwrap();
		}
	}
}

mod eventual_consistency {
	use super::*;
	use libp2p_identity::Keypair;

	#[tokio::test]
	async fn new_node_join_and_sync_works() {
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

		// Ports
		let ports_1: (Port, Port) = (49652, 55603);
		let ports_2: (Port, Port) = (49651, 55606);
		let ports_3: (Port, Port) = (49650, 55602);

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
			let mut node = setup_node(
				ports_1,
				&NODE_1_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

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

			let mut node = setup_node(
				ports_2,
				&NODE_2_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

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

			let mut node = setup_node(
				ports_3,
				&NODE_3_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Sleep to allow network wide synchronization
			tokio::time::sleep(Duration::from_secs(10)).await;

			// Check that the number of messages is equal to the number of items sent by nodes 1 and 2
			let mut number_of_messages = 0;
			while let Some(_) = node.consume_repl_data(REPL_NETWORK_ID.into()).await {
				number_of_messages += 1;
			}

			assert_eq!(number_of_messages, 4);
		});

		for task in vec![task_1, task_2, task_3] {
			task.await.unwrap();
		}
	}

	#[tokio::test]
	async fn test_lamports_clock_ordering() {
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

		let peer_id_4 = Keypair::from_protobuf_encoding(&NODE_4_KEYPAIR)
			.unwrap()
			.public()
			.to_peer_id();

		// Ports
		let ports_1: (Port, Port) = (49752, 55703);
		let ports_2: (Port, Port) = (49753, 55701);
		let ports_3: (Port, Port) = (49754, 55702);
		let ports_4: (Port, Port) = (49755, 55704);

		// Setup async channel to send network state between tasks
		let (mut tx, mut rx) = mpsc::channel::<Vec<(String, u64)>>(5);

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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_1,
				&NODE_1_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Send to replica node 2
			node.replicate(vec!["Apples".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();

			// Send to replica node 2
			node.replicate(vec!["Papayas".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();

			// Keep node running
			tokio::time::sleep(Duration::from_secs(15)).await;
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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);
			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_2,
				&NODE_2_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Publish messages
			node.replicate(vec!["Oranges".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();
			node.replicate(vec!["Kiwis".into()], &REPL_NETWORK_ID)
				.await
				.unwrap();

			// Keep node running
			tokio::time::sleep(Duration::from_secs(15)).await;
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
			bootnodes.insert(
				peer_id_4.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_4.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_3,
				&NODE_3_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

			// Join replica network works
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// Sleep to give time for node 1 and 2 to publish data to the network
			tokio::time::sleep(Duration::from_secs(20)).await;

			// Get replica buffer state
			let mut buffer_state = Vec::new();
			while let Some(data) = node.consume_repl_data(REPL_NETWORK_ID.into()).await {
				buffer_state.push((data.data[0].clone(), data.lamport_clock));
			}

			// Send buffer state to node 4 over mpsc channel
			let _ = tx.send(buffer_state).await;

			// Keep node alive for 10 seconds so the producing end does not close
			tokio::time::sleep(Duration::from_secs(10)).await;
		});

		// Setup node 4
		let task_4 = tokio::task::spawn(async move {
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
			bootnodes.insert(
				peer_id_3.to_base58(),
				format!("/ip4/127.0.0.1/tcp/{}", ports_3.0),
			);

			// Setup node with consistency consistency model
			let mut node = setup_node(
				ports_4,
				&NODE_4_KEYPAIR[..],
				bootnodes,
				ConsistencyModel::Eventual,
			)
			.await;

			// Join replica network
			let _ = node.join_repl_network(REPL_NETWORK_ID.into()).await;

			// We wait for 25 seconds so that node 1, 2 and 3 operations are completed
			tokio::time::sleep(Duration::from_secs(25)).await;

			// Get local buffer state
			let mut local_buffer_state = Vec::new();
			while let Some(data) = node.consume_repl_data(REPL_NETWORK_ID.into()).await {
				local_buffer_state.push((data.data[0].clone(), data.lamport_clock));
			}

			// Get node 3's incoming buffer state
			let incoming_buffer_state = rx.next().await.unwrap();

			// Compare both buffer states and the ordering of their data
			for (local_data, incoming_data) in
				local_buffer_state.iter().zip(incoming_buffer_state.iter())
			{
				assert_eq!(local_data.0, incoming_data.0);
				assert_eq!(local_data.1, incoming_data.1);
			}
		});

		for task in vec![task_1, task_2, task_3, task_4] {
			task.await.unwrap();
		}
	}
}

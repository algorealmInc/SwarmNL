//! Tests for data sharding and forwarding.

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use super::constants::*;
use crate::{
	core::{
		gossipsub_cfg::GossipsubConfig,
		replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig},
		sharding::{ShardStorage, Sharding},
		tests::replication::REPL_NETWORK_ID,
		ByteVector, Core, CoreBuilder, NetworkEvent, RpcConfig,
	},
	setup::BootstrapConfig,
	MultiaddrString, PeerIdString, Port,
};
use libp2p::{gossipsub::MessageId, PeerId};
use libp2p_identity::Keypair;
use std::{
	collections::{btree_map::Range, BTreeMap, HashMap, VecDeque},
	sync::Arc,
	time::Duration,
};
use tokio::sync::Mutex;

/// The constant that represents the id of the sharded network. Should be kept as a secret.
pub const NETWORK_SHARDING_ID: &'static str = "sharding_xx";

/// Handle incoming RPC.
fn rpc_incoming_message_handler(data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
	// Just return incoming data
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

/// The shard local storage.
#[derive(Debug)]
struct LocalStorage {
	buffer: VecDeque<String>,
}

// Implement the `ShardStorage` trait for our local storage
impl ShardStorage for LocalStorage {
	fn fetch_data(&self, key: ByteVector) -> ByteVector {
		// Convert the key to a UTF-8 string
		let key_str = String::from_utf8_lossy(&key[0]);

		// Iterate through the buffer to find a matching entry
		for data in self.buffer.iter() {
			if data.starts_with(key_str.as_ref()) {
				// If a match is found, take the latter part
				let data = data.split("-->").collect::<Vec<&str>>();
				return vec![data[1].as_bytes().to_vec()];
			}
		}

		// If no match is found, return an empty ByteVector
		Default::default()
	}
}

/// Implement the `Sharding` trait
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

	/// Locate the shard corresponding to the given key.
	fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId> {
		// Find the first range whose upper bound is greater than or equal to the key.
		self.ranges
			.iter()
			.find(|(&upper_bound, _)| key <= &upper_bound)
			.map(|(_, shard_id)| shard_id.clone())
	}
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

// Range-based sharding

#[tokio::test]
async fn join_and_exit_shard_network() {
	// Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;
	let shard_id_3 = 3;

	// Define shard ranges (Key ranges => Shard id)
	let mut ranges = BTreeMap::new();
	ranges.insert(100, shard_id_1);
	ranges.insert(200, shard_id_2);
	ranges.insert(300, shard_id_3);

	// Initialize the range-based sharding policy
	let shard_exec = Arc::new(Mutex::new(RangeSharding::new(ranges)));

	// Local shard storage
	let local_storage_buffer = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

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
	let ports_1: (Port, Port) = (48152, 54193);
	let ports_2: (Port, Port) = (32153, 32101);
	let ports_3: (Port, Port) = (48154, 54102);

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

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

		let node = setup_node(ports_1, &NODE_1_KEYPAIR[..], bootnodes, local_storage).await;

		// Join first shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		// Sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// Exit shard network
		let _ = sharding_executor
			.lock()
			.await
			.exit_network(node.clone(), &shard_id_1)
			.await;
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

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

		let node = setup_node(ports_2, &NODE_2_KEYPAIR[..], bootnodes, local_storage).await;

		// Join second shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_2)
			.await;

		// Sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// Exit shard network
		let _ = sharding_executor
			.lock()
			.await
			.exit_network(node.clone(), &shard_id_2)
			.await;
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

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

		let node = setup_node(ports_3, &NODE_3_KEYPAIR[..], bootnodes, local_storage).await;

		// Join shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_3)
			.await;

		// Assert there are 3 shards containing one node each
		let shard_network_state =
			<RangeSharding<String> as Sharding>::network_state(node.clone()).await;
		assert_eq!(shard_network_state.len(), 3);

		for shard in &shard_network_state {
			assert_eq!(shard.1.len(), 1);
		}

		// Sleep to allow nodes leave the network
		tokio::time::sleep(Duration::from_secs(12)).await;

		let shard_network_state =
			<RangeSharding<String> as Sharding>::network_state(node.clone()).await;

		assert_eq!(shard_network_state.len(), 1);

		// Check that the first shard contains one node (node 3)
		assert_eq!(shard_network_state.iter().nth(0).unwrap().1.len(), 1);
	});

	for task in vec![task_1, task_2, task_3] {
		task.await.unwrap();
	}
}

#[tokio::test]
async fn shard_data_forwarding() {
	// Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;

	// Define shard ranges (Key ranges => Shard id)
	let mut ranges = BTreeMap::new();
	ranges.insert(100, shard_id_1);
	ranges.insert(200, shard_id_2);

	// Initialize the range-based sharding policy
	let shard_exec = Arc::new(Mutex::new(RangeSharding::new(ranges)));

	// Local shard storage
	let local_storage_buffer = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

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
	let ports_1: (Port, Port) = (40105, 54201);
	let ports_2: (Port, Port) = (40103, 54109);

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 1
	let task_1 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_2.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
		);

		let node = setup_node(ports_1, &NODE_1_KEYPAIR[..], bootnodes, local_storage).await;

		// Join first shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		// Sleep for 2 seconds to allow node 2 to setup and join its own shard
		tokio::time::sleep(Duration::from_secs(2)).await;

		// Store some data on the network with a shard key that points to the second shard which
		// contains node 2
		let shard_key = 150;
		if let Ok(response) = sharding_executor
			.lock()
			.await
			.shard(node.clone(), &shard_key, vec!["sharding works".into()])
			.await
		{
			// Confirm that response is None because data is being forwarded
			assert_eq!(response, None);
		}

		// Keep the node alive for 10 seconds
		tokio::time::sleep(Duration::from_secs(10)).await;
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 2
	let task_2 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		let mut node = setup_node(ports_2, &NODE_2_KEYPAIR[..], bootnodes, local_storage).await;

		// Join second shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_2)
			.await;

		tokio::time::sleep(Duration::from_secs(5)).await;

		while let Some(event) = node.next_event().await {
			match event {
				NetworkEvent::IncomingForwardedData { data, source } => {
					println!(
						"recieved forwarded data: {:?} from peer: {}",
						data,
						source.to_base58()
					);
					// Assert that the data forwarded by node 1 is what we received
					assert_eq!(data, vec!["sharding works".to_string()]);
				},
				_ => {},
			}
		}
	});

	for task in vec![task_1, task_2] {
		task.await.unwrap();
	}
}

// When key falls in our own shard we store it locally and replicate it to our shard peers.
#[tokio::test]
async fn shard_local_storage_and_replication() {
	// Shard Id's
	let shard_id_1 = 1;

	// Define shard ranges (Key ranges => Shard id)
	let mut ranges = BTreeMap::new();
	ranges.insert(100, shard_id_1);

	// Initialize the range-based sharding policy
	let shard_exec = Arc::new(Mutex::new(RangeSharding::new(ranges)));

	// Local shard storage
	let local_storage_buffer = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

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
	let ports_1: (Port, Port) = (40155, 54200);
	let ports_2: (Port, Port) = (40153, 54100);

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 1
	let task_1 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_2.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
		);

		let node = setup_node(ports_1, &NODE_1_KEYPAIR[..], bootnodes, local_storage).await;

		// Join first shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		// Sleep for 2 seconds to allow node 2 to setup and join its own shard
		tokio::time::sleep(Duration::from_secs(2)).await;

		// Store some data that falls into the range of the current node so it can be stored locally
		let shard_key = 28;
		if let Ok(response) = sharding_executor
			.lock()
			.await
			.shard(node.clone(), &shard_key, vec!["sharding works".into()])
			.await
		{
			// Check to see if response is None or contains Some data
			// If it contains some data it means that the data should be stored locally and
			// replicated among peers in the shard containing our node
			assert_eq!(response, Some(vec!["sharding works".into()]));
		}

		// Keep the node alive for 10 seconds
		tokio::time::sleep(Duration::from_secs(10)).await;
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 2
	let task_2 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		let mut node = setup_node(ports_2, &NODE_2_KEYPAIR[..], bootnodes, local_storage).await;

		// Join second shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		tokio::time::sleep(Duration::from_secs(5)).await;

		while let Some(event) = node.next_event().await {
			match event {
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
						// Assert that the data forwarded by node 1 is what we received (forwarded
						// from node 2)
						assert_eq!(repl_data.data, vec!["sharding works".to_string()]);
					}
				},
				_ => {},
			}
		}
	});

	for task in vec![task_1, task_2] {
		task.await.unwrap();
	}
}

#[tokio::test]
async fn data_forwarding_replication() {
	// Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;

	// Define shard ranges (Key ranges => Shard id)
	let mut ranges = BTreeMap::new();
	ranges.insert(100, shard_id_1);
	ranges.insert(200, shard_id_2);

	// Initialize the range-based sharding policy
	let shard_exec = Arc::new(Mutex::new(RangeSharding::new(ranges)));

	// Local shard storage
	let local_storage_buffer = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

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
	let ports_1: (Port, Port) = (48135, 54303);
	let ports_2: (Port, Port) = (48133, 54301);
	let ports_3: (Port, Port) = (48134, 54302);

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

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

		let node = setup_node(ports_1, &NODE_1_KEYPAIR[..], bootnodes, local_storage).await;

		// Join first shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		// Sleep for 2 seconds to allow node 2 to setup and join its own shard
		tokio::time::sleep(Duration::from_secs(2)).await;

		// Store some data on the network with a shard key that points to the second shard which
		// contains node 2
		let shard_key = 150;
		if let Ok(response) = sharding_executor
			.lock()
			.await
			.shard(node.clone(), &shard_key, vec!["sharding works".into()])
			.await
		{
			// Confirm that response is None because data is being forwarded
			assert_eq!(response, None);
		}

		// Keep the node alive for 10 seconds
		tokio::time::sleep(Duration::from_secs(15)).await;
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 2
	let task_2 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		let mut node = setup_node(ports_2, &NODE_2_KEYPAIR[..], bootnodes, local_storage).await;

		// Join second shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_2)
			.await;

		tokio::time::sleep(Duration::from_secs(15)).await;

		while let Some(event) = node.next_event().await {
			match event {
				NetworkEvent::IncomingForwardedData { data, source } => {
					println!(
						"recieved forwarded data: {:?} from peer: {}",
						data,
						source.to_base58()
					);
					// Assert that the data forwarded by node 1 is what we received
					assert_eq!(data, vec!["sharding works".to_string()]);
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
						// Assert that the data forwarded by node 1 is what we received (forwarded
						// from node 3)
						assert_eq!(repl_data.data, vec!["sharding works".to_string()]);
					}
				},
				_ => {},
			}
		}
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

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

		let mut node = setup_node(ports_3, &NODE_3_KEYPAIR[..], bootnodes, local_storage).await;

		// Join shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_2)
			.await;

		tokio::time::sleep(Duration::from_secs(15)).await;

		while let Some(event) = node.next_event().await {
			match event {
				NetworkEvent::IncomingForwardedData { data, source } => {
					println!(
						"recieved forwarded data: {:?} from peer: {}",
						data,
						source.to_base58()
					);
					// Assert that the data forwarded by node 1 is what we received
					assert_eq!(data, vec!["sharding works".to_string()]);
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
						// Assert that the data forwarded by node 1 is what we received (forwarded
						// from node 2)
						assert_eq!(repl_data.data, vec!["sharding works".to_string()]);
					}
				},
				_ => {},
			}
		}
	});

	for task in vec![task_1, task_2, task_3] {
		task.await.unwrap();
	}
}

/// Test fetching data from the a sharded network
#[tokio::test]
async fn fetching_sharded_data() {
	// Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;

	// Key that will determine where our data is placed.
	let shard_key = 15;

	// Define shard ranges (Key ranges => Shard id)
	let mut ranges = BTreeMap::new();
	ranges.insert(100, shard_id_1);
	ranges.insert(200, shard_id_2);

	// Initialize the range-based sharding policy
	let shard_exec = Arc::new(Mutex::new(RangeSharding::new(ranges)));

	// Local shard storage
	let local_storage_buffer = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

	// Get Peer Id's
	let peer_id_1 = Keypair::from_protobuf_encoding(&NODE_1_KEYPAIR[..])
		.unwrap()
		.public()
		.to_peer_id();

	let peer_id_2 = Keypair::from_protobuf_encoding(&NODE_2_KEYPAIR[..])
		.unwrap()
		.public()
		.to_peer_id();

	// Ports
	let ports_1: (Port, Port) = (48155, 54103);
	let ports_2: (Port, Port) = (48153, 54101);

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 1
	let task_1 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_2.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_2.0),
		);

		let node = setup_node(
			ports_1,
			&NODE_1_KEYPAIR[..],
			bootnodes,
			local_storage.clone(),
		)
		.await;

		// Join first shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_1)
			.await;

		// Sleep for 2 seconds to allow node 2 to setup and join its own shard
		tokio::time::sleep(Duration::from_secs(2)).await;

		// Store some data that falls into the range of the current node so it can be stored
		// locally
		if let Ok(response) = sharding_executor
			.lock()
			.await
			.shard(node.clone(), &shard_key, vec!["name-->Jesus".into()])
			.await
		{
			// Check to see if response is None or contains Some data
			// If it contains some data it means that the data should be stored locally and
			// replicated among peers in the shard containing our node
			assert_eq!(response, Some(vec!["name-->Jesus".into()]));

			let res_data = response.unwrap()[0].clone();
			let data = String::from_utf8_lossy(&res_data);

			// Store the data locally
			local_storage.lock().await.buffer.push_back(data.into());
		}
	});

	// Clone the sharding executor
	let sharding_executor = shard_exec.clone();

	// Clone the local storage
	let local_storage = local_storage_buffer.clone();

	// Setup node 2
	let task_2 = tokio::task::spawn(async move {
		// Bootnodes
		let mut bootnodes = HashMap::new();

		bootnodes.insert(
			peer_id_1.to_base58(),
			format!("/ip4/127.0.0.1/tcp/{}", ports_1.0),
		);

		let node = setup_node(
			ports_2,
			&NODE_2_KEYPAIR[..],
			bootnodes,
			local_storage.clone(),
		)
		.await;

		// Join second shard network
		let _ = sharding_executor
			.lock()
			.await
			.join_network(node.clone(), &shard_id_2)
			.await;

		// Sleep for 5 seconds to allow Node 1 forward the data we need.
		tokio::time::sleep(Duration::from_secs(5)).await;

		// Request data from the sharded network. This data is stored in shard 1 (Node 1).
		match sharding_executor
			.lock()
			.await
			.fetch(node.clone(), &shard_key, vec!["name".into()])
			.await
		{
			Ok(response) => match response {
				Some(data) => {
					// Parse the data returned into the name we want
					let name = String::from_utf8_lossy(&data[0]);

					if !data[0].is_empty() {
						println!("The response data is '{}'", name);

						// Make assertions
						assert_eq!(name.as_ref(), "Jesus");
					} else {
						println!("The remote node does not have the data stored.");
					}

					println!("Successfully pulled data from the network.");
				},
				None => println!("Data exists locally on node."),
			},
			Err(e) => println!("Fetching failed: {}", e.to_string()),
		}
	});

	for task in vec![task_1, task_2] {
		task.await.unwrap();
	}
}

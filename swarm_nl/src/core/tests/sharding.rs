use std::{collections::{btree_map::Range, BTreeMap, HashMap, VecDeque}, sync::Arc, time::Duration};
use libp2p::{gossipsub::MessageId, PeerId};
use libp2p_identity::Keypair;
use tokio::sync::Mutex;
use crate::{core::{gossipsub_cfg::GossipsubConfig, replication::{ConsensusModel, ConsistencyModel, ReplNetworkConfig}, sharding::{ShardStorage, Sharding}, tests::replication::REPL_NETWORK_ID, ByteVector, Core, CoreBuilder, RpcConfig}, setup::BootstrapConfig, MultiaddrString, PeerIdString, Port};


/// The constant that represents the id of the sharding network. Should be kept as a secret.
pub const NETWORK_SHARDING_ID: &'static str = "sharding_xx";

/// The time to wait for events, if necessary.
pub const WAIT_TIME: u64 = 2;

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

#[tokio::test]
async fn join_and_exit_shard_network() {
    
    // Shard Id's
	let shard_id_1 = 1;
	let shard_id_2 = 2;
    let shard_id_3 = 3;

    let mut ranges = BTreeMap::new();

	// Define shard ranges (Key ranges => Shard id)
	ranges.insert(100, shard_id_1);
	let mut ranges = BTreeMap::new();
	ranges.insert(200, shard_id_2);
	ranges.insert(300, shard_id_3);

	// Initialize the range-based sharding policy
	let shard_executor = RangeSharding::new(ranges);

    // Local shard storage
	let local_storage = Arc::new(Mutex::new(LocalStorage {
		buffer: Default::default(),
	}));

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
	let ports_1: (Port, Port) = (48155, 54103);
	let ports_2: (Port, Port) = (48153, 54101);
	let ports_3: (Port, Port) = (48154, 54102);

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
			&node_1_keypair[..],
			bootnodes,
            local_storage
            
		)
		.await;

		// Join first shard network
        let _ = shard_executor.join_network(node.clone(), &shard_id_1).await;

		// Sleep for 3 seconds
		tokio::time::sleep(Duration::from_secs(3)).await;

		// Exit shard network
		let _ = shard_executor.exit_network(node.clone(), &shard_id_1).await;
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
			&node_2_keypair[..],
			bootnodes,
            local_storage
		)
		.await;

        // Join second shard network
        let _ = shard_executor.join_network(node.clone(), &shard_id_2).await;

        // Sleep for 3 seconds
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Exit shard network
        let _ = shard_executor.exit_network(node.clone(), &shard_id_2).await;
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
			&node_3_keypair[..],
			bootnodes,
            local_storage
		)
		.await;

		// Join shard network
		let _ = shard_executor.join_network(node.clone(), &shard_id_3).await;

		// Assert there are 3 shards containing one node each
        let shard_network_state = <RangeSharding<String> as Sharding>::network_state(node.clone()).await;
        assert_eq!(shard_network_state.len(), 3);

        for shard in &shard_network_state {
            assert_eq!(shard.1.len(), 1);
        }
        
        // Sleep
        tokio::time::sleep(Duration::from_secs(5)).await;

        let shard_network_state = <RangeSharding<String> as Sharding>::network_state(node.clone()).await;
        assert_eq!(shard_network_state.len(), 3);

        for shard in &shard_network_state {
            assert_eq!(shard.1.len(), 1);
        }

	});

	for task in vec![task_1, task_2, task_3] {
		task.await.unwrap();
	}
    
}


// Join the network of shards and tell others nodes you’ve arrived. This ensures that the state is consistent.Query network state: peerid of hashset of peers.
// Check that nodes have received the data correctly
// Make sure that replication works as expected 


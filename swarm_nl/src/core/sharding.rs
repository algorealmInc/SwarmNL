// Copyright 2024 Algorealm, Inc.
// Apache 2.0 License

//! Module that contains important data structures to manage [`Sharding`] operations on the
//! network.
use std::fmt::Debug;

use super::*;
use async_trait::async_trait;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

/// Trait that interfaces with the storage layer of a node in a shard. It is important for handling
/// forwarded data requests. This is a mechanism to trap into the application storage layer to read
/// sharded data.
pub trait ShardStorage: Send + Sync + Debug {
	fn fetch_data(&self, key: ByteVector) -> ByteVector;
}

/// Important data for the operation of the sharding protocol.
#[derive(Debug, Clone)]
pub struct ShardingInfo {
	/// The id of the entire sharding network.
	pub id: String,
	/// Shard local storage.
	pub local_storage: Arc<Mutex<dyn ShardStorage>>,
	/// The shards and the various nodes they contain.
	pub state: Arc<Mutex<HashMap<ShardId, HashSet<PeerId>>>>,
}

/// Default shard storage to respond to forwarded data requests.
#[derive(Debug)]
pub(super) struct DefaultShardStorage;

impl ShardStorage for DefaultShardStorage {
	fn fetch_data(&self, key: ByteVector) -> ByteVector {
		// Simply echo incoming data request
		key
	}
}

/// Trait that specifies sharding logic and behaviour of shards.
#[async_trait]
pub trait Sharding
where
	Self::Key: Send + Sync,
	Self::ShardId: ToString + Send + Sync,
{
	/// The type of the shard key e.g hash, range etc.
	type Key: ?Sized;
	/// The identifier pointing to a specific group of shards.
	type ShardId;

	/// Map a key to a shard.
	fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId>;

	/// Join a shard network.
	async fn join_network(&self, mut core: Core, shard_id: &Self::ShardId) -> NetworkResult<()> {
		// Ensure the network sharding ID is set.
		let network_shard_id: Vec<u8> = match &core.network_info.sharding.id {
			id if !id.is_empty() => id.clone().into(),
			_ => return Err(NetworkError::MissingShardingNetworkIdError),
		};
		let network_sharding_id = String::from_utf8_lossy(&network_shard_id).to_string();

		// Join the generic shard (gossip) network
		let gossip_request = AppData::GossipsubJoinNetwork(network_sharding_id.clone());
		let _ = core.query_network(gossip_request).await?;

		// Update the local shard state
		let mut shard_state = core.network_info.sharding.state.lock().await;
		shard_state
			.entry(shard_id.to_string())
			.or_insert_with(Default::default)
			.insert(core.peer_id());

		// Free `Core`
		drop(shard_state);

		// Join the shard network
		let gossip_request = AppData::GossipsubJoinNetwork(shard_id.to_string());
		let _ = core.query_network(gossip_request).await?;

		// Inform the entire network about out decision
		let message = vec![
			Core::SHARD_GOSSIP_JOIN_FLAG.as_bytes().to_vec(), // Flag for join event.
			core.peer_id().to_string().into_bytes(),          // Our peer ID.
			shard_id.to_string().into_bytes(),                // Shard we're joining
		];

		let gossip_request = AppData::GossipsubBroadcastMessage {
			topic: network_sharding_id,
			message,
		};

		// Gossip the join event to all nodes.
		core.query_network(gossip_request).await?;

		Ok(())
	}

	/// Exit a shard network.
	async fn exit_network(&self, mut core: Core, shard_id: &Self::ShardId) -> NetworkResult<()> {
		// First, we remove ourself from the network state
		let mut shard_state = core.network_info.sharding.state.lock().await;
		let shard_entry = shard_state
			.entry(shard_id.to_string())
			.or_insert(Default::default());

		shard_entry.retain(|entry| entry != &core.peer_id());

		// Release `core`
		drop(shard_state);

		// Then, we make a broadcast
		let message = vec![
			Core::SHARD_GOSSIP_EXIT_FLAG.to_string().into(), // Appropriate flag
			core.peer_id().to_base58().into(),               // Our peerId
			shard_id.to_string().into(),                     // Network we're leaving
		];

		// Prepare a gossip request
		let gossip_request = AppData::GossipsubBroadcastMessage {
			topic: core.network_info.sharding.id.clone(),
			message,
		};

		let _ = core.query_network(gossip_request).await?;

		// Check if we're in any shard
		let shard_state = core.network_info.sharding.state.lock().await;
		if !shard_state
			.iter()
			.any(|(_, peers)| peers.contains(&core.peer_id()))
		{
			// Release `core`
			drop(shard_state);

			// Leave the underlying sharding (gossip) network
			let gossip_request =
				AppData::GossipsubJoinNetwork(core.network_info.sharding.id.clone());
			core.query_network(gossip_request).await?;
		}

		Ok(())
	}

	/// Send data to peers in the appropriate logical shard. It returns the data if the node is a
	/// member of the shard after replicating it to fellow nodes in the same shard.
	async fn shard(
		&self,
		mut core: Core,
		key: &Self::Key,
		data: ByteVector,
	) -> NetworkResult<Option<ByteVector>> {
		// Locate the shard that would store the key.
		let shard_id = match self.locate_shard(key) {
			Some(shard_id) => shard_id,
			None => return Err(NetworkError::ShardNotFound),
		};

		// Retrieve the nodes in the logical shard.
		let nodes = {
			let shard_state = core.network_info.sharding.state.lock().await;
			shard_state.get(&shard_id.to_string()).cloned()
		};

		// If no nodes exist for the shard, return an error.
		let mut nodes = match nodes {
			Some(nodes) => nodes,
			None => return Err(NetworkError::MissingShardNodesError),
		};

		// Check if the current node is part of the shard.
		if nodes.contains(&core.peer_id()) {
			// Replicate the data to nodes in the shard.
			let _ = core.replicate(data.clone(), &shard_id.to_string()).await;
			return Ok(Some(data)); // Return the data to the caller.
		}

		// Prepare the message for data forwarding.
		let mut message = vec![
			Core::RPC_DATA_FORWARDING_FLAG.as_bytes().to_vec(), /* Flag to indicate data
			                                                     * forwarding. */
			shard_id.to_string().into_bytes(),
		];
		message.extend(data); // Append the data payload.

		// Shuffle nodes so their order of query is randomized
		let mut rng = StdRng::from_entropy();
		let mut nodes = nodes.iter().cloned().collect::<Vec<_>>();
		
		nodes.shuffle(&mut rng);

		// Attempt to forward the data to peers.
		for peer in nodes {
			let rpc_request = AppData::SendRpc {
				keys: message.clone(),
				peer: peer.clone(),
			};

			// Query the network and return success on the first successful response.
			// The recieving node will then replicate it to other nodes in the shard.
			if core.query_network(rpc_request).await.is_ok() {
				return Ok(None); // Forwarding succeeded.
			}
		}

		// If all peers fail, return an error.
		Err(NetworkError::DataForwardingError)
	}

	/// Return the state of the shard network
	async fn shard_state(core: Core) -> HashMap<String, HashSet<PeerId>> {
		core.network_info.sharding.state.lock().await.clone()
	}

	/// Fetch data from the shard network.
	async fn fetch(
		&self,
		mut core: Core,
		key: &Self::Key,
		mut data: ByteVector,
	) -> NetworkResult<Option<ByteVector>> {
		// Locate the shard that would store the key.
		let shard_id = match self.locate_shard(key) {
			Some(shard_id) => shard_id,
			None => return Err(NetworkError::ShardingFailureError),
		};

		// Retrieve the nodes in the logical shard.
		let nodes = {
			let shard_state = core.network_info.sharding.state.lock().await;
			shard_state.get(&shard_id.to_string()).cloned()
		};

		// If no nodes exist for the shard, return an error.
		let nodes = match nodes {
			Some(nodes) => nodes,
			None => return Err(NetworkError::ShardingFetchError),
		};

		// Check if the current node is part of the shard.
		if nodes.contains(&core.peer_id()) {
			// Return `None`
			return Ok(None);
		}

		// Shuffle the peers.
		let mut rng = StdRng::from_entropy();
		let mut nodes = nodes.iter().cloned().collect::<Vec<_>>();

		nodes.shuffle(&mut rng);

		// Prepare an RPC to ask for the data from nodes in the shard.
		let mut message = vec![
			Core::SHARD_RPC_REQUEST_FLAG.as_bytes().to_vec(), /* Flag to indicate shard data
			                                                   * request */
		];

		message.append(&mut data);

		// Attempt to forward the data to peers.
		for peer in nodes {
			let rpc_request = AppData::SendRpc {
				keys: message.clone(),
				peer: peer.clone(),
			};

			// Query the network and return the response on the first successful response.
			if let Ok(response) = core.query_network(rpc_request).await {
				if let AppResponse::SendRpc(data) = response {
					return Ok(Some(data));
				}
			}
		}

		// Fetch Failed
		Err(NetworkError::ShardingFetchError)
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[test]
	fn test_initial_shard_node_state() {
		tokio::runtime::Runtime::new().unwrap().block_on(async {
			// Initialize the shared state
			let state = Arc::new(Mutex::new(HashMap::new()));
			let config = ShardingCfg {
				callback: |_rpc| RpcData::default(),
			};
			let sharding_info = ShardingInfo {
				id: "test-network".to_string(),
				config,
				state: state.clone(),
			};

			// Simulate a shard node initialization
			let shard_id = "shard-1".to_string();

			{
				let mut shard_state = state.lock().await;
				shard_state.insert(shard_id.clone(), vec![]);
			}

			// Check the initial state
			let shard_state = state.lock().await;
			assert!(
				shard_state.contains_key(&shard_id),
				"Shard ID should exist in the state"
			);
			assert!(
				shard_state.get(&shard_id).unwrap().is_empty(),
				"Shard state for shard-1 should be empty"
			);

			// Validate network ID
			assert_eq!(
				sharding_info.id, "test-network",
				"Sharding network ID should be set correctly"
			);
		});
	}
}

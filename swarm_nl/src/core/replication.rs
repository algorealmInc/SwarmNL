//! Module that contains important data structures to manage `Replication` operations on the
//! network.

use super::*;
use std::{cmp::Ordering, collections::BTreeMap, sync::Arc, time::SystemTime};

/// Struct respresenting data for configuring node replication.
#[derive(Clone, Default, Debug)]
pub struct ReplConfigData {
	/// Lamport's clock for synchronization.
	pub lamport_clock: Nonce,
	/// Replica nodes described by their addresses.
	pub nodes: HashMap<String, String>,
}

/// Struct containing important information for replication.
#[derive(Clone)]
pub struct ReplInfo {
	/// Internal state for replication.
	pub state: Arc<Mutex<HashMap<String, ReplConfigData>>>,
}

/// The consistency models supported.
///
/// This is important as is determines the behaviour of the node in handling and delivering
/// replicated data to the application layer. There are also trade-offs to be considered
/// before choosing any model. You must choose the model that aligns and suits your exact
/// usecase and objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsistencyModel {
	/// Eventual consistency
	Eventual,
	/// Strong consistency
	Strong(ConsensusModel),
}

/// This enum dictates how many nodes need to come to an agreement for consensus to be held
/// during the impl of a strong consistency sync model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusModel {
	/// All nodes in the network must contribute to consensus
	All,
	/// Just a subset of the network are needed for consensus
	MinPeers(u64),
}

/// Enum containing configurations for replication.
#[derive(Clone)]
pub enum ReplNetworkConfig {
	/// A custom configuration.
	///
	/// # Fields
	///
	/// - `queue_length`: Max capacity for transient storage.
	/// - `expiry_time`: Expiry time of data in the buffer if the buffer is full. If a `NoExpiry`
	///   behaviour is preferred, `expiry_time` should be set to `None`.
	/// - `sync_wait_time`: Epoch to wait before attempting the next network synchronization of
	///   data in the buffer.
	/// - `consistency_model`: The data consistency model to be supported by the node. This must be
	///   uniform across all nodes to prevent undefined behaviour.
	/// - `data_wait_period`: When data has arrived and is saved into the buffer, the time to wait
	///   for it to get to other peers after which it can be picked for synchronization.
	Custom {
		queue_length: u64,
		expiry_time: Option<Seconds>,
		sync_wait_time: Seconds,
		consistency_model: ConsistencyModel,
		data_aging_period: Seconds,
	},
	/// A default configuration: `queue_length` = 100, `expiry_time` = 60 seconds,
	/// `sync_wait_time` = 5 seconds, `consistency_model`: `Eventual`, `data_wait_period` = 5
	/// seconds
	Default,
}

/// Important data to marshall from incoming relication payload and store in the transient
/// buffer.
#[derive(Clone, Debug)]
pub struct ReplBufferData {
	/// Raw incoming data.
	pub data: StringVector,
	/// Lamports clock for synchronization.
	pub lamport_clock: Nonce,
	/// Timestamp at which the message left the sending node.
	pub outgoing_timestamp: Seconds,
	/// Timestamp at which the message arrived.
	pub incoming_timestamp: Seconds,
	/// Message ID to prevent deduplication. It is usually a hash of the incoming message.
	pub message_id: String,
	/// Sender PeerId.
	pub sender: PeerId,
	/// Number of confirmations. This is to help the nodes using the strong consistency
	/// synchronization data model to come to an agreement
	pub confirmations: Option<Nonce>,
}

/// Implement Ord.
impl Ord for ReplBufferData {
	fn cmp(&self, other: &Self) -> Ordering {
		self.lamport_clock
			.cmp(&other.lamport_clock) // Compare by lamport_clock first
			.then_with(|| self.message_id.cmp(&other.message_id)) // Then compare by message_id
	}
}

/// Implement PartialOrd.
impl PartialOrd for ReplBufferData {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

/// Implement Eq.
impl Eq for ReplBufferData {}

/// Implement PartialEq.
impl PartialEq for ReplBufferData {
	fn eq(&self, other: &Self) -> bool {
		self.lamport_clock == other.lamport_clock && self.message_id == other.message_id
	}
}

/// Transient buffer queue where incoming replicated data are stored temporarily.
pub(crate) struct ReplicaBufferQueue {
	/// Configuration for replication and general synchronization.
	config: ReplNetworkConfig,
	/// In the case of a strong consistency model, this is where data is buffered
	/// initially before it is agreed upon by the majority of the network. After which
	/// it is then moved to the queue exposed to the application layer.
	temporary_queue: Mutex<BTreeMap<String, BTreeMap<String, ReplBufferData>>>,
	/// Internal buffer containing replicated data to be consumed by the application layer.
	queue: Mutex<BTreeMap<String, BTreeSet<ReplBufferData>>>,
}

impl ReplicaBufferQueue {
	/// The default max capacity of the buffer.
	const MAX_CAPACITY: u64 = 150;

	/// The default expiry time of data in the buffer, when the buffer becomes full.
	const EXPIRY_TIME: Seconds = 60;

	/// The default epoch to wait before attempting the next network synchronization.
	const SYNC_WAIT_TIME: Seconds = 5;

	/// The default aging period after which the data can be synchronized across the network.
	const DATA_AGING_PERIOD: Seconds = 5;

	/// Create a new instance of [ReplicaBufferQueue].
	pub fn new(config: ReplNetworkConfig) -> Self {
		Self {
			config,
			temporary_queue: Mutex::new(Default::default()),
			queue: Mutex::new(Default::default()),
		}
	}

	/// Return the configured [`ConsistencyModel`] for data synchronization.
	pub fn consistency_model(&self) -> ConsistencyModel {
		match self.config {
			// Default config always supports eventual consistency
			ReplNetworkConfig::Default => ConsistencyModel::Eventual,
			ReplNetworkConfig::Custom {
				consistency_model, ..
			} => consistency_model,
		}
	}

	/// Push a new [ReplBufferData] item into the buffer.
	pub async fn push(&self, mut core: Core, replica_network: String, data: ReplBufferData) {
		// Different behaviours based on configurations
		match self.config {
			// Default implementation supports expiry of buffer items
			ReplNetworkConfig::Default => {
				// Lock the queue to modify it
				let mut queue = self.queue.lock().await;

				// Filter into replica network the data belongs to.
				// If it doesn't exist, create new
				let queue = queue.entry(replica_network).or_default();

				// If the queue is full, remove expired data first
				while queue.len() as u64 >= Self::MAX_CAPACITY {
					// Check and remove expired data
					let current_time = SystemTime::now()
						.duration_since(SystemTime::UNIX_EPOCH)
						.unwrap()
						.as_secs();
					let mut expired_items = Vec::new();

					// Identify expired items and collect them for removal
					for entry in queue.iter() {
						if current_time - entry.outgoing_timestamp >= Self::EXPIRY_TIME {
							expired_items.push(entry.clone());
						}
					}

					// Remove expired items
					for expired in expired_items {
						queue.remove(&expired);
					}

					// If no expired items were removed, pop the front (oldest) item
					if queue.len() as u64 >= Self::MAX_CAPACITY {
						if let Some(first) = queue.iter().next().cloned() {
							queue.remove(&first);
						}
					}
				}

				// Insert data right into the final queue
				queue.insert(data);
			},
			// Here decay applies in addition to removal of excess buffer content
			ReplNetworkConfig::Custom {
				queue_length,
				expiry_time,
				consistency_model,
				..
			} => {
				// Which buffer the incoming data will interact with initially is determined by
				// the supported data consistency model
				match consistency_model {
					// For eventual consistency, data is written straight into the final queue
					// for consumption
					ConsistencyModel::Eventual => {
						// Lock the queue to modify it
						let mut queue = self.queue.lock().await;

						// Filter into replica network the data belongs to.
						// If it doesn't exist, create new
						let queue = queue.entry(replica_network).or_default();

						// If the queue is full, remove expired data first
						while queue.len() as u64 >= queue_length {
							// Remove only when data expiration is supported
							if let Some(expiry_time) = expiry_time {
								// Check and remove expired data
								let current_time = SystemTime::now()
									.duration_since(SystemTime::UNIX_EPOCH)
									.unwrap()
									.as_secs();
								let mut expired_items = Vec::new();

								// Identify expired items and collect them for removal
								for entry in queue.iter() {
									if current_time - entry.outgoing_timestamp >= expiry_time {
										expired_items.push(entry.clone());
									}
								}

								// Remove expired items
								for expired in expired_items {
									queue.remove(&expired);
								}
							}

							// If no expired items were removed, pop the front (oldest) item
							if queue.len() as u64 >= queue_length {
								if let Some(first) = queue.iter().next().cloned() {
									queue.remove(&first);
								}
							}
						}

						// Insert data right into the final queue
						queue.insert(data);
					},
					// Here data is written into the temporary buffer first, for finalization to
					// occur. It is then moved into the final queue after favourable consensus
					// has been reached.
					ConsistencyModel::Strong(_) => {
						// Lock the queue to modify it
						let mut temp_queue = self.temporary_queue.lock().await;

						// Filter into replica network the data belongs to.
						// If it doesn't exist, create new
						let temp_queue = temp_queue.entry(replica_network.clone()).or_default();

						// Remove the first item from the queue. No decay applies here
						if temp_queue.len() as u64 >= Self::MAX_CAPACITY {
							if let Some(first_key) = temp_queue.keys().next().cloned() {
								temp_queue.remove(&first_key);
							}
						}

						// Get message ID
						let message_id = data.message_id.clone();

						// Insert data into queue. Confirmation count is already 1
						temp_queue.insert(data.message_id.clone(), data);

						// Start strong consistency synchronization algorithm:
						// Broadcast just recieved message to peers to increase the
						// confirmation. It is just the message ID that will be broadcast
						let message = vec![
							Core::STRONG_CONSISTENCY_FLAG.as_bytes().to_vec(), /* Strong Consistency Sync Gossip Flag */
							replica_network.clone().into(),                    /* Replica network */
							message_id.as_bytes().into(),                      /* Message id */
						];

						// Prepare a gossip request
						let gossip_request = AppData::GossipsubBroadcastMessage {
							topic: replica_network.into(),
							message,
						};

						// Gossip data to replica nodes
						let _ = core.query_network(gossip_request).await;
					},
				}
			},
		}
	}

	// Pop the front (earliest data) from the queue
	pub async fn pop_front(&self, replica_network: &str) -> Option<ReplBufferData> {
		let mut queue = self.queue.lock().await;

		// Filter into replica network the data belongs to
		if let Some(queue) = queue.get_mut(replica_network) {
			if let Some(first) = queue.iter().next().cloned() {
				// Remove the front element
				queue.remove(&first);
				return Some(first);
			}
		}
		None
	}

	pub async fn handle_data_confirmation(
		&self,
		mut query_sender: Sender<String>,
		data_receiver: &mut Receiver<u64>,
		replica_network: String,
		message_id: String,
	) {
		// Determine the number of peers required for consensus
		let peers_count = match self.config {
			ReplNetworkConfig::Custom {
				consistency_model, ..
			} => match consistency_model {
				ConsistencyModel::Eventual => 0,
				ConsistencyModel::Strong(consensus_model) => match consensus_model {
					ConsensusModel::All => {
						// Query for real-time peer count
						if query_sender.send(replica_network.clone()).await.is_ok() {
							if let Some(peers_count) = data_receiver.next().await {
								peers_count.saturating_sub(1) // Exclude self
							} else {
								0
							}
						} else {
							0
						}
					},
					ConsensusModel::MinPeers(required_peers) => required_peers,
				},
			},
			ReplNetworkConfig::Default => 0,
		};

		// Update confirmation count while holding the lock minimally
		let is_fully_confirmed = {
			let mut flag = false;
			let mut temporary_queue = self.temporary_queue.lock().await;
			if let Some(temp_queue) = temporary_queue.get_mut(&replica_network) {
				if let Some(data_entry) = temp_queue.get_mut(&message_id) {
					// Increment confirmation count
					data_entry.confirmations = Some(data_entry.confirmations.unwrap_or(1) + 1);
					// Check if confirmations meet required peers
					flag = peers_count != 0 && data_entry.confirmations == Some(peers_count);
				}
			}

			flag
		};

		// If fully confirmed, move data to the public queue
		if is_fully_confirmed {
			let mut public_queue = self.queue.lock().await;
			let public_queue = public_queue
				.entry(replica_network.clone())
				.or_insert_with(BTreeSet::new);

			// Cleanup expired or excessive entries
			if let ReplNetworkConfig::Custom {
				queue_length,
				expiry_time,
				..
			} = self.config
			{
				let current_time = SystemTime::now()
					.duration_since(SystemTime::UNIX_EPOCH)
					.unwrap()
					.as_secs();

				// Remove expired items
				if let Some(expiry_time) = expiry_time {
					public_queue
						.retain(|entry| current_time - entry.outgoing_timestamp < expiry_time);
				}

				// Remove oldest items if queue exceeds capacity
				while public_queue.len() as u64 >= queue_length {
					if let Some(first) = public_queue.iter().next().cloned() {
						public_queue.remove(&first);
					}
				}
			}

			// Move confirmed entry to public queue
			let mut temporary_queue = self.temporary_queue.lock().await;
			if let Some(temp_queue) = temporary_queue.get_mut(&replica_network) {
				if let Some(data_entry) = temp_queue.remove(&message_id) {
					public_queue.insert(data_entry);
				}
			}
		}
	}

	/// Synchronize the data in the buffer queue using eventual consistency.
	pub async fn sync_with_eventual_consistency(&self, core: Core, repl_network: String) {
		// The oldest clock of the previous sync
		let mut prev_clock = 0;

		loop {
			let repl_network = repl_network.clone();
			let mut core = core.clone();

			// Get configured aging period
			let data_aging_time = match self.config {
				ReplNetworkConfig::Default => Self::DATA_AGING_PERIOD,
				ReplNetworkConfig::Custom {
					data_aging_period, ..
				} => data_aging_period,
			};

			// Fetch local data state while holding the lock minimally
			let local_data_state = {
				let queue = self.queue.lock().await;
				queue.get(&repl_network).cloned()
			};

			if let Some(local_data_state) = local_data_state {
				// Filter data outside the lock
				let local_data = local_data_state
					.iter()
					.filter(|&d| {
						util::get_unix_timestamp() - d.incoming_timestamp > data_aging_time
					})
					.cloned()
					.collect::<BTreeSet<_>>();

				// Extract the bounding Lamport clocks
				let (min_clock, max_clock) =
					if let (Some(first), Some(last)) = (local_data.first(), local_data.last()) {
						(first.lamport_clock, last.lamport_clock)
					} else {
						// Default values if no data is present
						(0, 0)
					};

				// Only sync if the max clock is greater than the previous clock
				if max_clock > prev_clock {
					// Extract message IDs for synchronization
					let mut message_ids = local_data
						.iter()
						.map(|data| data.message_id.clone().into())
						.collect::<ByteVector>();

					// Prepare gossip message
					let mut message = vec![
						// Strong Consistency Sync Gossip Flag
						Core::EVENTUAL_CONSISTENCY_FLAG.as_bytes().to_vec(),
						// Node's Peer ID
						core.peer_id().into(),
						repl_network.clone().into(),
						min_clock.to_string().into(),
						max_clock.to_string().into(),
					];

					// Append the message IDs
					message.append(&mut message_ids);

					// Broadcast gossip request
					let gossip_request = AppData::GossipsubBroadcastMessage {
						topic: repl_network.into(),
						message,
					};

					let _ = core.query_network(gossip_request).await;

					// Update the previous clock
					prev_clock = max_clock;
				}
			}

			// Wait for a defined duration before the next sync
			#[cfg(feature = "tokio-runtime")]
			tokio::time::sleep(Duration::from_secs(Self::SYNC_WAIT_TIME)).await;

			#[cfg(feature = "async-std-runtime")]
			async_std::task::sleep(Duration::from_secs(Self::SYNC_WAIT_TIME)).await;
		}
	}

	/// Synchronize incoming buffer image from a replica node with the local buffer image.
	pub async fn sync_buffer_image(
		&self,
		mut core: Core,
		repl_peer_id: PeerIdString,
		repl_network: String,
		lamports_clock_bound: (u64, u64),
		replica_data_state: StringVector,
	) {
		// Convert replica data state into a set outside the mutex lock.
		// Filter replica buffer too so it doesn't contain the data that we published.
		// This is done using the messageId since by gossipsub, messageId = (Publishing peerId +
		// Nonce)

		let replica_buffer_state = replica_data_state
			.into_iter()
			.filter(|id| id.contains(&core.peer_id().to_base58()))
			.collect::<BTreeSet<_>>();

		// Extract local buffer state and filter it while keeping the mutex lock duration
		// minimal
		let mut missing_msgs = {
			let mut queue = self.queue.lock().await;
			if let Some(local_state) = queue.get_mut(&repl_network) {
				let local_buffer_state = local_state
					.iter()
					.filter(|data| {
						data.lamport_clock >= lamports_clock_bound.0
							&& data.lamport_clock <= lamports_clock_bound.1
					})
					.map(|data| data.message_id.clone())
					.collect::<BTreeSet<_>>();

				// Extract messages missing from our local buffer
				replica_buffer_state
					.difference(&local_buffer_state)
					.cloned()
					.map(|id| id.into())
					.collect::<ByteVector>()
			} else {
				return; // If the network state doesn't exist, exit early
			}
		};

		// Prepare an RPC fetch request for missing messages
		if let Ok(repl_peer_id) = repl_peer_id.parse::<PeerId>() {
			let mut rpc_data: ByteVector = vec![
				Core::RPC_SYNC_PULL_FLAG.into(), // RPC sync pull flag
				repl_network.clone().into(),     // Replica network
			];

			// Append the missing message ids to the request data
			rpc_data.append(&mut missing_msgs);

			// Prepare an RPC to ask the replica node for missing data
			let fetch_request = AppData::FetchData {
				keys: missing_msgs.clone(),
				peer: repl_peer_id,
			};

			// Send the fetch request
			if let Ok(response) = core.query_network(fetch_request).await {
				if let AppResponse::FetchData(messages) = response {
					// Parse response
					let response = util::unmarshal_messages(messages);

					// Re-lock the mutex only for inserting new messages
					let mut queue = self.queue.lock().await;
					if let Some(local_state) = queue.get_mut(&repl_network) {
						for missing_msg in response {
							local_state.insert(missing_msg);
						}
					}
				}
			}
		}
	}

	/// Pull and return missing data requested by a replica node.
	pub async fn pull_missing_data(
		&self,
		repl_network: String,
		message_ids: &[Vec<u8>],
	) -> ByteVector {
		// Fetch the local state from the queue with a minimal lock
		let local_state = {
			let queue = self.queue.lock().await;
			queue.get(&repl_network).cloned()
		};

		// If the local state exists, process the message retrieval
		if let Some(local_state) = local_state {
			// Retrieve messages that match the requested message IDs
			let requested_msgs = local_state
				.iter()
				.filter(|&data| message_ids.contains(&data.message_id.as_bytes().to_vec()))
				.collect::<Vec<_>>();

			// Prepare the result buffer
			let mut result = Vec::new();

			for msg in requested_msgs {
				// Serialize the `data` field (Vec<String>) into a single string, separated by
				// `$$`
				let joined_data = msg.data.join(Core::DATA_DELIMITER);

				// Serialize individual fields, excluding `confirmations`
				let mut entry = Vec::new();
				entry.extend_from_slice(joined_data.as_bytes());
				entry.extend_from_slice(Core::FIELD_DELIMITER.to_string().as_bytes());
				entry.extend_from_slice(msg.lamport_clock.to_string().as_bytes());
				entry.extend_from_slice(Core::FIELD_DELIMITER.to_string().as_bytes());
				entry.extend_from_slice(msg.outgoing_timestamp.to_string().as_bytes());
				entry.extend_from_slice(Core::FIELD_DELIMITER.to_string().as_bytes());
				entry.extend_from_slice(msg.incoming_timestamp.to_string().as_bytes());
				entry.extend_from_slice(Core::FIELD_DELIMITER.to_string().as_bytes());
				entry.extend_from_slice(msg.message_id.as_bytes());
				entry.extend_from_slice(Core::FIELD_DELIMITER.to_string().as_bytes());
				entry.extend_from_slice(msg.sender.to_base58().as_bytes());

				// Append the entry to the result, separated by `ENTRY_DELIMITER`
				if !result.is_empty() {
					result.extend_from_slice(Core::ENTRY_DELIMITER.to_string().as_bytes());
				}
				result.extend(entry);
			}

			return vec![result];
		}

		// Default empty result if no local state is found
		Default::default()
	}

	/// Replicate and populate buffer with replica's state.
	pub async fn replicate_buffer(
		&self,
		mut core: Core,
		repl_network: String,
		replica_node: PeerId,
	) -> Result<(), NetworkError> {
		// Send an RPC to the node to retreive it's buffer image
		let rpc_data: ByteVector = vec![
			// RPC buffer copy flag. It is the samething as the sync pull flag with an empty
			// message id vector
			Core::RPC_SYNC_PULL_FLAG.into(),
			repl_network.clone().into(), // Replica network
			vec![],                      // Empty vector indicating a total PULL
		];

		// Prepare an RPC to ask the replica node for missing data
		let fetch_request = AppData::FetchData {
			keys: rpc_data,
			peer: replica_node,
		};

		// Try to query the replica node and insert data gotten into buffer
		let mut queue = self.queue.lock().await;
		match queue.get_mut(&repl_network) {
			Some(local_state) => {
				// Send the fetch request
				match core.query_network(fetch_request).await? {
					AppResponse::FetchData(messages) => {
						// Parse response
						let response = util::unmarshal_messages(messages);
						// Insert into data buffer queue
						for missing_msg in response {
							local_state.insert(missing_msg);
						}

						Ok(())
					},
					AppResponse::Error(err) => Err(err),
					_ => Err(NetworkError::RpcDataFetchError),
				}
			},
			None => Err(NetworkError::MissingReplNetwork),
		}
	}
}

#[cfg(test)]
mod tests {

	// use libp2p::dns::tokio;
	use super::*;

	// Define custom ports for testing
	const CUSTOM_TCP_PORT: Port = 49666;
	const CUSTOM_UDP_PORT: Port = 49852;

	// Setup a node using default config
	pub async fn setup_node(ports: (Port, Port)) -> Core {
		let config = BootstrapConfig::default()
			.with_tcp(ports.0)
			.with_udp(ports.1);

		// Set up network
		CoreBuilder::with_config(config).build().await.unwrap()
	}

	#[test]
	fn test_initialization_with_default_config() {
		let buffer = ReplicaBufferQueue::new(ReplNetworkConfig::Default);

		match buffer.consistency_model() {
			ConsistencyModel::Eventual => assert!(true),
			_ => panic!("Consistency model not initialized correctly"),
		}
	}

	#[test]
	fn test_initialization_with_custom_config() {
		let config = ReplNetworkConfig::Custom {
			queue_length: 200,
			expiry_time: Some(120),
			sync_wait_time: 10,
			consistency_model: ConsistencyModel::Strong(ConsensusModel::All),
			data_aging_period: 15,
		};
		let buffer = ReplicaBufferQueue::new(config);

		match buffer.consistency_model() {
			ConsistencyModel::Strong(ConsensusModel::All) => assert!(true),
			_ => panic!("Consistency model not initialized correctly"),
		}

		// Verify queue length
		match buffer.config {
			ReplNetworkConfig::Custom { queue_length, .. } => {
				assert_eq!(queue_length, 200);
			},
			_ => panic!("Queue length not initialized correctly"),
		}

		// Verify expiry time
		match buffer.config {
			ReplNetworkConfig::Custom { expiry_time, .. } => {
				assert_eq!(expiry_time, Some(120));
			},
			_ => panic!("Expiry time not initialized correctly"),
		}

		// Verify sync wait time
		match buffer.config {
			ReplNetworkConfig::Custom { sync_wait_time, .. } => {
				assert_eq!(sync_wait_time, 10);
			},
			_ => panic!("Sync wait time not initialized correctly"),
		}

		// Verify data aging period
		match buffer.config {
			ReplNetworkConfig::Custom {
				data_aging_period, ..
			} => {
				assert_eq!(data_aging_period, 15);
			},
			_ => panic!("Data aging period not initialized correctly"),
		}
	}

	// -- Buffer Queue Tests --

	#[test]
	fn test_buffer_overflow_expiry_behavior() {
		tokio::runtime::Runtime::new().unwrap().block_on(async {
			let expiry_period: u64 = 2;

			let config = ReplNetworkConfig::Custom {
				queue_length: 4,
				expiry_time: Some(expiry_period), // Set very short expiry for testing
				sync_wait_time: 10,
				consistency_model: ConsistencyModel::Eventual,
				data_aging_period: 10,
			};

			let network = setup_node((CUSTOM_TCP_PORT, CUSTOM_UDP_PORT)).await;
			let buffer = ReplicaBufferQueue::new(config);

			for clock in 1..5 {
				let data = ReplBufferData {
					data: vec!["Data 1".into()],
					lamport_clock: clock,
					outgoing_timestamp: util::get_unix_timestamp(),
					incoming_timestamp: util::get_unix_timestamp(),
					message_id: "msg1".into(),
					sender: PeerId::random(),
					confirmations: None,
				};

				buffer
					.push(network.clone(), "network1".into(), data.clone())
					.await;
			}

			// Check that the first data lamport is 1
			assert_eq!(buffer.pop_front("network1").await.unwrap().lamport_clock, 1);

			tokio::time::sleep(std::time::Duration::from_secs(expiry_period)).await; // Wait for expiry

			// Fill up buffer
			buffer
				.push(
					network.clone(),
					"network1".into(),
					ReplBufferData {
						data: vec!["Data 1".into()],
						lamport_clock: 6,
						outgoing_timestamp: util::get_unix_timestamp(),
						incoming_timestamp: util::get_unix_timestamp(),
						message_id: "msg1".into(),
						sender: PeerId::random(),
						confirmations: None,
					},
				)
				.await;

			// Overflow buffer
			buffer
				.push(
					network.clone(),
					"network1".into(),
					ReplBufferData {
						data: vec!["Data 1".into()],
						lamport_clock: 42,
						outgoing_timestamp: util::get_unix_timestamp(),
						incoming_timestamp: util::get_unix_timestamp(),
						message_id: "msg1".into(),
						sender: PeerId::random(),
						confirmations: None,
					},
				)
				.await;

			// We expect that 6 is the first element and 42 is the second as they have not aged out
			assert_eq!(buffer.pop_front("network1").await.unwrap().lamport_clock, 6);
			assert_eq!(
				buffer.pop_front("network1").await.unwrap().lamport_clock,
				42
			);
		});
	}

	#[test]
	fn test_buffer_overflow_no_expiry_behavior() {
		tokio::runtime::Runtime::new().unwrap().block_on(async {
			let config = ReplNetworkConfig::Custom {
				queue_length: 4,
				expiry_time: None, // Disable aging
				sync_wait_time: 10,
				consistency_model: ConsistencyModel::Eventual,
				data_aging_period: 10,
			};

			let network = setup_node((15555, 6666)).await;
			let buffer = ReplicaBufferQueue::new(config);

			for clock in 1..5 {
				let data = ReplBufferData {
					data: vec!["Data 1".into()],
					lamport_clock: clock,
					outgoing_timestamp: util::get_unix_timestamp(),
					incoming_timestamp: util::get_unix_timestamp(),
					message_id: "msg1".into(),
					sender: PeerId::random(),
					confirmations: None,
				};

				buffer
					.push(network.clone(), "network1".into(), data.clone())
					.await;
			}

			// Check that the first data lamport is 1
			assert_eq!(buffer.pop_front("network1").await.unwrap().lamport_clock, 1);

			buffer
				.push(
					network.clone(),
					"network1".into(),
					ReplBufferData {
						data: vec!["Data 1".into()],
						lamport_clock: 6,
						outgoing_timestamp: util::get_unix_timestamp(),
						incoming_timestamp: util::get_unix_timestamp(),
						message_id: "msg1".into(),
						sender: PeerId::random(),
						confirmations: None,
					},
				)
				.await;

			// Check that the data lamports are 2 and 3 as expected
			assert_eq!(buffer.pop_front("network1").await.unwrap().lamport_clock, 2);
			assert_eq!(buffer.pop_front("network1").await.unwrap().lamport_clock, 3);
		});
	}

	#[test]
	fn test_pop_from_empty_buffer() {
		tokio::runtime::Runtime::new().unwrap().block_on(async {
			let config = ReplNetworkConfig::Default;
			let buffer = ReplicaBufferQueue::new(config);

			let result = buffer.pop_front("network1").await;
			assert!(result.is_none(), "Buffer should be empty");
		});
	}
}

# SwarmNL: a library to build custom networking layers for decentralized and distributed applications

SwarmNL addresses two critical concerns in distributed systems: **fault tolerance** and **scaling**. This document provides a technical overview of the design decisions of how these are implemented in the library.

## Table of contents

- [Fault Tolerance](#fault-tolerance)
   - [Replication](#replication)
     - [Replication Data Structure](#replication-data-structure)
     - [Configurations](#configurations)
     - [Default Configuration](#default-configuration)
     - [Consistency Model](#consistency-model)
       - [Strong Consistency](#strong-consistency)
       - [Eventual Consistency](#eventual-consistency)
- [Scaling](#scaling)
   - [The `Sharding` Trait](#the-sharding-trait)
     - [Data Forwarding](#data-forwarding)
   - [The `ShardStorage` Trait](#the-shardstorage-trait)
     - [Handling Incoming Requests](#handling-incoming-requests)
   - [Shards and Replication](#shards-and-replication)
- [Reference](#reference)

## Fault tolerance

Fault tolerance in SwarmNL is primarily achieved through **Redundancy**, which ensures that other nodes in the network remain operational to service incoming requests, even in the event of failures. SwarmNL handles redundancy using **Replication**.

### Replication

SwarmNL simplifies data replication across nodes, ensuring consistency and reliability in distributed systems. Below is the structure of an entry in the replication buffer, along with the purpose of each field:

#### Replication data structure

```rust
   /// Important data to marshall from incoming payload and store in the replication
   /// buffer.
   #[derive(Clone, Debug)]
   pub struct ReplBufferData {
      /// The raw incoming data from the replica peer.
      pub data: StringVector,
      /// Lamport clock for ordering and synchronization.
      pub lamport_clock: Nonce,
      /// Timestamp indicating when the message was sent.
      pub outgoing_timestamp: Seconds,
      /// Timestamp indicating when the message was received.
      pub incoming_timestamp: Seconds,
      /// A unique identifier for the message, typically a hash of the payload.
      pub message_id: String,
      /// The peer ID of the source node that sent the data.
      pub source: PeerId,
      /// Number of confirmations for strong consistency models.
      pub confirmations: Option<Nonce>,
   }
```

- **`data`**  
  The raw data received from a replica peer. This field contains a `StringVector`, which is a vector of strings representing the replicated payload.

- **`lamport_clock`**  
  A critical synchronization and ordering primitive in distributed systems. The Lamport clock is used internally in the replication buffer queue to order messages and data across the replica network. The clock is incremented whenever a node receives a message or sends data for replication. Each node maintains its own Lamport clock, updating it with the highest value received in messages. The replication buffer is implemented as a `BTreeSet`, ordered by this clock.

```rust
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
```

- **`outgoing_timestamp`**  
  Records the time a message leaves the source node for replication across the network.

- **`incoming_timestamp`**  
  Captures the time when replicated data is received on a node.

- **`message_id`**  
  A unique identifier, typically a hash of the message payload. It ensures the uniqueness of messages in the replication buffer and prevents duplication. In `eventual consistency` sychronization models, only `message_id`s are exchanged for comparison before data is pulled on demand.

- **`source`**  
  The peer ID of the node that sent the replicated data across the network.

- **`confirmations`**  
  Relevant for `strong consistency` synchronization models, this field tracks the number of confirmations received from nodes in the network. Data is only moved from the transient replication buffer to the primary public buffer for application consumption once the configured confirmation threshold is met.

#### Configurations

Replication is governed by key primitives that define the behavior of individual nodes and the network as a whole. These settings are detailed below:

```rust
   /// Enum containing configurations for replication.
   #[derive(Clone, Debug)]
   pub enum ReplNetworkConfig {
      /// A custom configuration.
      Custom {
         /// Max capacity for transient storage.
         queue_length: u64,
         /// Expiry time of data in the buffer if the buffer is full. Set to `None` for no expiry.
         expiry_time: Option<Seconds>,
         /// Epoch to wait before attempting the next network synchronization of data in the buffer.
         sync_wait_time: Seconds,
         /// The data consistency model to be supported by the node. This must be uniform across all
         /// nodes to prevent undefined behaviour.
         consistency_model: ConsistencyModel,
         /// When data has arrived and is saved into the buffer, the time to wait for it to get to
         /// other peers after which it can be picked for synchronization.
         data_aging_period: Seconds,
      },
      /// A default configuration: `queue_length` = 100, `expiry_time` = 60 seconds,
      /// `sync_wait_time` = 5 seconds, `consistency_model`: `Eventual`, `data_wait_period` = 5
      /// seconds.
      Default,
   }`
```

- **`queue_length`**  
  The maximum number of entries the transient storage buffer can hold. Once the buffer exceeds this limit, new data may overwrite older entries depending on the configuration.

- **`expiry_time`**  
  Specifies how long data remains in the buffer before it expires if the buffer becomes full. Setting it to `None` disables expiry, allowing data to persist indefinitely until explicitly removed.

- **`sync_wait_time`**  
  The interval (in seconds) between synchronization attempts for data in the buffer. This ensures efficient utilization of network resources while maintaining data freshness.

- **`consistency_model`**  
  Defines the level of consistency required for data replication and the behaviour to ensure it. This must be uniform across all nodes in the replica network to prevent inconsistent or undefined behavior.

- **`data_aging_period`**  
  The waiting period (in seconds) after data is saved into the buffer before it is eligible for synchronization. This allows for additional processing or validations if needed.

#### Default configuration

If no custom configuration is provided, the library uses a default setup with:

- **`queue_length`**: 100
- **`expiry_time`**: 60 seconds
- **`sync_wait_time`**: 5 seconds
- **`consistency_model`**: Eventual consistency
- **`data_aging_period`**: 5 seconds

#### Consistency model

Replication is greatly influenced by the configured **consistency model**, which ensures that all nodes have a consistent view of the data in the network. SwarmNL supports two consistency models:

```rust
   /// The consistency models supported.
   ///
   /// This is important as is determines the behaviour of the node in handling and delivering
   /// replicated data to the application layer. There are also trade-offs to be considered
   /// before choosing any model. You must choose the model that aligns and suits your exact
   /// usecase and objectives.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum ConsistencyModel {
      /// Eventual consistency.
      Eventual,
      /// Strong consistency.
      Strong(ConsensusModel),
   }

   /// This enum dictates how many nodes need to come to an agreement for consensus to be held
   /// during the impl of a strong consistency sync model.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum ConsensusModel {
      /// All nodes in the network must contribute to consensus.
      All,
      /// Just a subset of the network are needed for consensus.
      MinPeers(u64),
   }
```

**Strong Consistency**

In the **Strong Consistency** model, replicated data is temporarily stored in a transient buffer and is only committed to the public buffer after ensuring synchronization across all nodes. The process involves the following steps:

- Receiving data:

  - When replicated data arrives at a node, it includes a flag (`confirmations`) initialized to `1`, indicating the originating node already has the data.
  - This data is stored in the **temporary buffer** of the receiving node.

- Broadcasting data:

  - The receiving node immediately broadcasts the data to its replica peers.
  - Each peer increments the `confirmations` fields of the data upon receiving the broadcast.

- Confirming Consistency:
  - When the `confirmations` reaches `node_count - 1` (e.g., 2 for a 3-node network), the data is deemed consistent.
  - The data is then moved from the temporary buffer to the primary (public) buffer, making it accessible to the application layer.

This model guarantees that data is fully synchronized across all replicas before it becomes available to the application layer.

**Eventual Consistency**

In the **Eventual Consistency** model, replicated data is immediately stored in the **public buffer**. Consistency is achieved over time through a periodic synchronization task. The process works as follows:

- Buffer queue:

  - The public buffer uses a `BTreeSet` to organize replicated data based on a **Lamport clock**.

- Synchronization task:

  - A background task periodically broadcasts the `MessageId`s of data in the queue to all replica peers.
  - Peers compare the received `MessageId`s with their local buffer to identify missing data.

- Retrieving missing data:

  - Peers send an RPC request to retrieve missing data and add it to their buffers.
  - The system trusts that, over time, all nodes will achieve eventual consistency as data propagates and synchronizes across the network.

- Buffer aging and eviction:

  The buffer has a **maximum size** and supports an **aging mechanism**:
  - Each data item has an associated lifespan `max_age` calculated as the current unix timestamp minus the `incoming_timestamp` of the data item.
  - If the buffer is full, items exceeding their lifespan are lazily removed during the next data insertion.
  - This ensures data remains accessible for sufficient time while optimizing buffer space.

In the eventual consistency model, the application layer operates with the expectation that data becomes consistent over time, even if it has been consumed from the buffer.

## **Scaling**

Scaling the network is primarily achieved through **replication** and **sharding**. Replication has already been discussed in the context of fault tolerance. Scaling enables improved read and write performance by partitioning the network into logical `shards`, each responsible for a subset of the data. A `shard` may span multiple nodes, and each shard manages its own data storage and operations.

### The `Sharding` trait

SwarmNL provides a trait called `Sharding` to implement sharding. To maintain flexibility and configurability, developers are required to implement the `locate_shard()` function of the trait. This function maps a key or data item to a logical shard, allowing developers to define sharding strategies tailored to their application's needs.

```rust
   /// Trait that specifies sharding logic and behaviour of shards.
   #[async_trait]
   pub trait Sharding
   where
      Self::Key: Send + Sync,
      Self::ShardId: ToString + Send + Sync,
   {
      /// The type of the shard key e.g hash, range etc.
      type Key: ?Sized;
      /// The identifier pointing to a specific shard.
      type ShardId;

      /// Map a key to a shard.
      fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId>;

      /// Return the state of the shard network.
      async fn network_state(core: Core) -> HashMap<String, HashSet<PeerId>> {
         core.network_info.sharding.state.lock().await.clone()
      }

      /// Join a shard network.
      async fn join_network(&self, mut core: Core, shard_id: &Self::ShardId) -> NetworkResult<()> { .. }

      /// Exit a shard network.
	   async fn exit_network(&self, mut core: Core, shard_id: &Self::ShardId) -> NetworkResult<()> { .. }

      /// Send data to peers in the appropriate logical shard. It returns the data if the node is a
      /// member of the shard, after replicating it to fellow nodes in the same shard.
      async fn shard(
         &self,
         mut core: Core,
         key: &Self::Key,
         data: ByteVector,
      ) -> NetworkResult<Option<ByteVector>> { .. }

      /// Fetch data from the shard network. It returns `None` if the node is a member of the shard the stores the data, meaning the node should read it locally.
      async fn fetch(
         &self,
         mut core: Core,
         key: &Self::Key,
         mut data: ByteVector,
      ) -> NetworkResult<Option<ByteVector>> { .. }
   }
```

The `Sharding` trait also includes generic functions for:

- Adding nodes to a shard.
- Joining or exiting a shard.
- Fetching data over the network.
- Storing data in the appropriate shard.
- **Data Forwarding**, explained below.

### Data forwarding

Data Forwarding occurs when a node receives data it isn’t responsible for due to its shard configuration. In such cases, the node locates the appropriate shard and forwards the data to the relevant nodes within that shard.

#### How it works

1. The node takes the data's key and uses the `locate_shard()` function to determine which shard the data should go to.
2. After identifying the target shard, the node finds the nodes in that shard and attempts to forward the data to them using an RPC mechanism.
3. The forwarding process continues in a loop until one of the nodes responds with `Ok()`, indicating that the data has been successfully received. At this point, the loop breaks, completing the operation.
4. Once the data is received by a node in the shard, replication happens quickly and efficiently, saving bandwidth for the sending node.

This is why replication should be configured for sharding to ensure smooth data forwarding and efficient network usage.

```rust
   //! Forward data to peers in shard

   // ...
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

   //...
```

### The `ShardStorage` trait

The `ShardStorage` trait allows nodes to `trap` into their application logic and environment to answer the sharded network requests.

```rust
/// Trait that interfaces with the storage layer of a node in a shard. It is important for handling
/// forwarded data requests. This is a mechanism to trap into the application storage layer to read
/// sharded data.
pub trait ShardStorage: Send + Sync + Debug {
	fn fetch_data(&self, key: ByteVector) -> ByteVector;
}
```

### Handling incoming requests

Incoming requests can be easily managed by calling the `fetch_data()` function on the storage object, which returns the requested data to the asking node in the sharded network.

SwarmNL remains storage-agnostic, meaning any storage system can be used with this generic interface e.g memory, filesystem etc. During network configuration, the storage object is passed via an atomic reference counter (`Arc`), which wraps an asynchronous `Mutex`. This allows the application to modify the storage state as needed while still enabling safe access to the data across different parts of the system, using async primitives.

This approach ensures flexibility, efficiency, and thread-safe handling of data.

```rust
   //! Implement ShardStorage for reading from the filesystem of data requests

   // ...
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


   // Internally, while handling network requests
   // ...

   // It is an incoming request to ask for data on this node because it is a member of a logical shard
   Core::SHARD_RPC_REQUEST_FLAG => {
      // Pass request data to configured shard request handler
      let response_data = network_info.sharding.local_storage.lock().await.fetch_data(data[1..].into());
      // Send the response
      let _ = swarm.behaviour_mut().request_response.send_response(channel, Rpc::ReqResponse { data: response_data });
   }
   // ...
```

### Shards and replication

All nodes within a shard act as replicas of each other and synchronize their data based on the consistency model configured during replication setup. This tight integration between sharding and replication ensures that the data within each shard is reliable and consistent, as defined by the application's requirements.

By combining replication and sharding, SwarmNL offers a scalable and fault-tolerant framework for managing decentralized networks while giving developers the freedom to design shard configurations that align with their use case.


#### Reference

Andrew Tanenbaum, "Distributed Systems: Principles and Paradigms", 2002.
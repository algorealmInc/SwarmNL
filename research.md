# **SwarmNL: A Library to Build Custom Networking Layers for Decentralized and Distributed Applications**

SwarmNL addresses two critical concerns in distributed systems: **Scaling** and **Fault Tolerance**. This section focuses on how SwarmNL handles **Fault Tolerance** using redundancy.

## **Fault Tolerance**

Fault tolerance in SwarmNL is primarily achieved through **redundancy**, which ensures that other nodes in the network remain operational to service incoming requests, even in the event of failures. SwarmNL handles redundancy using one key technique: **Replication**.

### **Replication**

SwarmNL simplifies data replication across nodes, ensuring consistency and reliability in distributed systems. Below is the structure of an entry in the replication buffer, along with the purpose of each field:

#### Replication data structure

```rust
   /// Important data to marshall from incoming relication payload and store in the transient
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
  A critical synchronization and ordering primitive in distributed systems. The Lamport clock is used internally in the replication buffer queue to order messages and data across the replication network. The clock is incremented whenever a node receives a message or sends data for replication. Each node maintains its own Lamport clock, updating it with the highest value received in messages. The replication buffer is implemented as a `BTreeSet`, ordered by this clock.

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
  Defines the level of consistency required for data replication. This must be uniform across all nodes in the replication network to prevent inconsistent or undefined behavior.

- **`data_aging_period`**  
  The waiting period (in seconds) after data is saved into the buffer before it is eligible for synchronization. This allows for additional processing or validations if needed.

### Default Configuration

If no custom configuration is provided, the library uses a default setup:

- **`queue_length`**: 100
- **`expiry_time`**: 60 seconds
- **`sync_wait_time`**: 5 seconds
- **`consistency_model`**: Eventual consistency
- **`data_aging_period`**: 5 seconds

This replication is governed by a configurable **consistency model**, which ensures that all nodes in the network have a consistent view. SwarmNL supports two consistency models:

```rust
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
```

#### - Strong Consistency

In the **Strong Consistency** model, replicated data is temporarily stored in a transient buffer and is only committed to the public buffer after ensuring synchronization across all nodes. The process involves the following steps:

1. **Receiving Data**:

   - When replicated data arrives at a node, it includes a flag (`confirmations`) initialized to `1`, indicating the originating node already has the data.
   - This data is stored in the **temporary buffer** of the receiving node.

2. **Broadcasting Data**:

   - The receiving node immediately broadcasts the data to its replica peers.
   - Each peer increments the `confirmations` fields of the data upon receiving the broadcast.

3. **Confirming Consistency**:
   - When the `confirmations` reaches `node_count - 1` (e.g., 2 for a 3-node network), the data is deemed consistent.
   - The data is then moved from the temporary buffer to the primary (public) buffer, making it accessible to the application layer.

This model guarantees that data is fully synchronized across all replicas before it becomes available to the application layer.

---

#### - Eventual Consistency\*\*

In the **Eventual Consistency** model, replicated data is immediately stored in the **public buffer**. Consistency is achieved over time through a periodic synchronization task. The process works as follows:

1. **Buffer Queue**:

   - The public buffer uses a `BTreeSet` to organize replicated data based on a **Lamport clock**.

2. **Synchronization Task**:

   - A background task periodically broadcasts the `MessageId`s of data in the queue to all replica peers.
   - Peers compare the received `MessageId`s with their local buffer to identify missing data.

3. **Retrieving Missing Data**:
   - Peers send an RPC request to retrieve missing data and add it to their buffers.
   - The system trusts that, over time, all nodes will achieve eventual consistency as data propagates and synchronizes across the network.

#### **Buffer Aging and Eviction**

- The buffer has a **maximum size** and supports an **aging mechanism**:
  - Each data item has an associated lifespan (`max age`).
  - If the buffer is full, items exceeding their lifespan are lazily removed during the next data insertion.
  - This ensures data remains accessible for sufficient time while optimizing buffer space.

In the eventual consistency model, the application layer operates with the expectation that data becomes consistent over time, even if it has been consumed from the buffer.

## **Scaling**

Scaling the network is primarily achieved through **replication** and **sharding**. Replication has already been discussed in the context of fault tolerance. Scaling enables improved read and write performance by partitioning the network into logical `shards`, each responsible for a subset of the data. A `shard` may span multiple nodes, and each shard manages its own data storage and operations.

### **Sharding in SwarmNL**

SwarmNL provides a trait called `Sharding` to implement sharding. To maintain flexibility and configurability, developers are required to implement the `locate_shard()` function within the trait. This function maps a key or data item to a logical shard, allowing developers to define sharding strategies tailored to their application's needs.

The `Sharding` trait also includes generic functions for:

- Adding nodes to a shard.
- Joining or exiting a shard.
- Fetching data over the network.
- Storing data in the appropriate shard.
- **Data forwarding**, explained below.

### **Data Forwarding**

Data forwarding occurs when a node receives data it is not configured to store or process due to the shard's configuration. In such cases, the node identifies the appropriate shard and forwards the data to the corresponding nodes within that shard.

### **Shards and Replication**

All nodes within a shard act as replicas of each other and synchronize their data based on the consistency model configured during replication setup. This tight integration between sharding and replication ensures that the data within each shard is reliable and consistent, as defined by the application's requirements.

By combining replication and sharding, SwarmNL offers a scalable and fault-tolerant framework for managing decentralized networks while giving developers the freedom to design shard configurations that align with their use case.

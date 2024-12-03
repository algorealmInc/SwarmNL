# **SwarmNL: A Library to Build Custom Networking Layers for Decentralized Applications**

SwarmNL addresses two critical concerns in distributed systems: **Scaling** and **Fault Tolerance**. This section focuses on how SwarmNL handles **Fault Tolerance** using redundancy.

## **Fault Tolerance**

Fault tolerance in SwarmNL is primarily achieved through **redundancy**, which ensures that other nodes in the network remain operational to service incoming requests, even in the event of failures. SwarmNL handles redundancy using one key technique: **Replication**.

### **Replication**

SwarmNL facilitates seamless data replication among configured nodes in the network. This replication is governed by a configurable **consistency model**, which ensures synchronized data across nodes. SwarmNL supports two consistency models:

---

### **1. Strong Consistency**

In the **Strong Consistency** model, replicated data is temporarily stored in a buffer and is only committed to the public buffer after ensuring synchronization across all nodes. The process involves the following steps:

1. **Receiving Data**:

   - When replicated data arrives at a node, it includes a flag (`confirmation count`) initialized to `1`, indicating the originating node already has the data.
   - This data is stored in the **temporary buffer** of the receiving node.

2. **Broadcasting Data**:

   - The receiving node immediately broadcasts the data to its replica peers.
   - Each peer increments the `confirmation count` upon receiving the broadcast.

3. **Confirming Consistency**:
   - When the `confirmation count` reaches `node_count - 1` (e.g., 2 for a 3-node network), the data is deemed consistent.
   - The data is then moved from the temporary buffer to the primary (public) buffer, making it accessible to the application layer.

This model guarantees that data is fully synchronized across all replicas before it becomes available to the application layer.

---

### **2. Eventual Consistency**

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

---

By supporting both strong and eventual consistency, SwarmNL provides flexibility for developers to balance consistency guarantees with system performance based on their application's needs.

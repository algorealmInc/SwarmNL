//! A doc-only module explaining how to run core library tests.
//!
//! > **Note**: the library is compatible with both `tokio` and `async-std` runtimes, however all
//! > tests are written to use the `tokio` executor.
//! > Therefore, to run the tests you must specify the runtime feature flag e.g. `cargo test
//! > --features=tokio-runtime` unless it is already set as the default runtime in Cargo.toml.
//!
//! Tests are organised into the following modules:
//!
//! - `node_behaviour` tests for single node setup and behaviour.
//! - `layer_communication` tests involving the synchronization between two nodes.
//! - `replication` tests for integration tests involving replication configuration and behavior.
//! - `sharding` tests for integration tests involving sharding configuration and behavior.
//!
//!
//! # Node behaviour testing
//!
//! These are simple unit tests that check the behaviour of a single node. To run these tests,
//! simply run the following command:
//!
//! ```bash
//! cargo test node_ --features=tokio-runtime
//! ```
//!
//! # Layer communication testing
//!
//! In order to create tests for communication between two nodes, we used the Rust conditional
//! compilation feature to be able to setup different nodes and test their communication.
//! All commands for running these tests should be run with `-- --nocapture` to verify the expected
//! results.
//!
//! For these tests, we've created two test nodes: `node1` and `node2`.
//!
//! - Node 1 is setup by calling the `setup_node_1` function which uses a pre-configured
//!   cryptographic keypair and the `setup_core_builder_1` function to configure a default node.
//! This keeps its identity consistent across tests.
//!
//! - Node 2 is setup by calling the `setup_node_2` function which creates a new node identity every
//!   time it is called.
//! It then adds Node 1 as its bootnode and establishes a connection by dialing Node 1.
//!
//! ### Peer dialing tests
//!
//! The peer dialing tests checks if a node can dial another node by using a `listening` node and a
//! `dialing` node. To run these tests, start the listening node by running the following command in
//! one terminal:
//!
//! ```bash
//! cargo test dialing_peer_works --features=test-listening-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! Then, in another terminal run the dialing node:
//!
//! ```bash
//! cargo test dialing_peer_works --features=test-dialing-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! The application event handler will log the dialing node's peer id and the listening node's peer
//! id.  
//! ## Fetching tests
//!
//! The fetching test checks if a node can fetch a value from another node.
//! These tests use a `server` node and a `client` node.
//!
//! To run these tests first start the server node in one terminal:
//!
//! ```bash
//! cargo test rpc_fetch_works --features=test-server-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! And in another terminal, run the client node:
//!
//! ```bash
//! cargo test rpc_fetch_works --features=test-client-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! Then you can check that the server node prints out a _"Recvd incoming RPC:"_ message with the
//! data sent by the client node.
//!
//! ## Kademlia tests
//!
//! For Kademlia tests, we have a `reading` node and a `writing` node.
//! We use a time delay to simulate the reading node "sleeping" so as to allow the writing node to
//! make changes to the DHT.
//!
//! When the reading node "wakes up" it then tries to read the value from the DHT. If the value is
//! what it expects, the tests passes successfully.
//!
//! To run this test, run the following command in one terminal to launch the "reading" node:
//!
//! ```bash
//! cargo test kademlia_record_store_itest_works --features=test-reading-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! And then run the following command in another terminal to launch the "writing node":
//!
//! ```bash
//! cargo test kademlia_record_store_itest_works --features=test-writing-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! ### Record providers tests
//!
//! To run the providers tests, we have a `reading` node and a `writing` node.
//!
//! We first run the "writing" node to store a record in the DHT. Then we run a "reading" node to
//! fetch the list of providers of the record that's been written.
//!
//! Then we simply assert that node 1 is a provider of the record.
//!
//! To run this test, first run the "writing" node:
//!
//! ```bash
//! cargo test kademlia_provider_records_itest_works --features=test-writing-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! Then, in another terminal, run the "reading" node:
//!
//! ```bash
//! cargo test kademlia_provider_records_itest_works --features=test-reading-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! ### Gossipsub tests
//!
//! **Join/Exit tests**
//!
//! For Gossipsub tests, we have a `subscribe` node and a `query` node.
//!
//! When the "subscribe" node is set up, it joins a mesh network. Then node 2 is setup and connects
//! to node 1, sleeps for a while (to allow propagtion of data from node 1) and then joins the
//! network. After joining, it then queries the network layer for gossipping information. This
//! information contains topics the node is currently subscribed to such as the peers that node 2
//! knows (which is node 1) and the network they are a part of. The peers that have been blacklisted
//! are also returned.
//!
//! In this test, we test that node 1 is a part of the mesh network that node 2 is subscribed to.
//!
//! To run this test, first run the "subscribe" node:
//!
//! ```bash
//! cargo test gossipsub_join_exit_itest_works --features=test-subscribe-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! Then, in another terminal, run the "query" node:
//!
//! ```bash
//! cargo test gossipsub_join_exit_itest_works --features=test-query-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! **Publish/Subscribe tests**
//!
//! For this test we have a `listening` node and a `broadcast` node. The first node is setup which
//! joins a mesh network. Then, node 2 is setup and connects to node 1, sleeps for a few seconds (to
//! allow propagtion of data from node 1) and then joins the network. It then joins the network that
//! node 1 was already a part of and sends a broadcast message to every peer in the mesh network.
//!
//! The indicator of the success of this test is revealed in the application's event handler
//! function which logs the message received from node 2.
//!
//! To run this test, first run the "listening" node in one terminal:
//!
//! ```bash
//! cargo test gossipsub_message_itest_works --features=test-listening-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! Then run the "broadcast" node in another terminal:
//!
//! ```bash
//! cargo test gossipsub_message_itest_works --features=test-broadcast-node --features=tokio-runtime -- --nocapture
//! ```
//!
//! ## Replication tests
//!
//! For each Replication test, we setup nodes as separate async tasks that dial each other to form a replica network.
//!
//! The `setup_node` function builds each node with replication configured.
//!
//! For basic replication tests we test the network behaves as expected for:
//! - Joining and exiting the network
//! - Replicating and fetching data from the network
//! - Fully replicating a node from the replica network
//!
//! For Strong Consistency tests, we test the network behaves as expected for:
//! - Number of confirmations are correct in a network with only 2 nodes
//! - Number of confirmations are correct in a network with 3 nodes
//! - Number of confirmations are correct in a network with `MinPeers` set to 2 nodes
//!
//! For Eventual Consistency tests, we test the network behaves as expected for:
//! - A node is updated upon newly joining a replica network
//! - Lamport ordering
//! - Replicating a value across the network
//!
//! ## Sharding tests
//!
//! To setup the testing environment for Sharding, we implement `ShardStorage` to define the behavior of `fetch_data()` 
//! which fetches data separated by `-->`. We also implement the `Sharding` trait for range-based sharding to test the behavior of the network.
//!
//! In each test, we setup nodes as separate async tasks that forms the sharded network.
//! The `setup_node` function builds each node with replication and sharding configured.
//!
//! For Sharding, we test the network behaves as expected for:
//!
//! - Joining and exiting a sharded network
//! - Data forwarding between shards
//! - Replication between nodes in a shard
//! - Sharding and fetching from local storage
//! - Fetching sharded data from the network
//!  

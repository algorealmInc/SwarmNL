<!-- <img src="https://github.com/algorealmInc/SwarmNL/blob/c3fe530350ec37755c64b47cba06361d39b3b095/SwarmNL.png" alt="SwarmNL" style="border-radius: 15px !important;"> -->

# SwarmNL

**A library to build custom networking layers for decentralized and distributed applications**

SwarmNL is a library designed for P2P networking in distributed systems. It's lightweight, scalable, and easy to configure, making it perfect for decentralized applications. Powered by [libp2p](https://docs.libp2p.io/), SwarmNL simplifies networking so developers can focus on building.

<!-- TOC start (generated with https://github.com/derlin/bitdowntoc) -->

- [Why SwarmNL?](#why-swarmnl)
- [examples](#examples)
- [Documentation](#documentation)
- [Features and examples](#features-and-examples)
  - [Node configuration](#node-configuration)
  - [Node communication](#node-communication)

<!-- TOC end -->

## Why SwarmNL?

SwarmNL makes buiding a peer-to-peer decentralized and distributed networking stack for your application a breeze. With SwarmNL, you can effortlessly configure nodes, tailor network conditions, and fine-tune behaviors specific to your project's needs, allowing you to dive into networking without any hassle.

Say goodbye to the complexities of networking and hello to simplicity. With SwarmNL, all the hard work is done for you, leaving you to focus on simple configurations and your application logic.

## Examples

Have a look at some examples that demonstrate the use of SwarmNl in various contexts:

- [Echo server tutorial](https://github.com/algorealmInc/SwarmNL/tree/dev/examples/echo_server): demonstrates a simple use case of setting up a node and querying the network layer.
- [File sharing application tutorial](https://github.com/algorealmInc/SwarmNL/tree/dev/examples/file_sharing_app): demonstrates interacting with the DHT and sending/recieving RPCs from peers.
- [Simple game tutorial](https://github.com/algorealmInc/SwarmNL/tree/dev/examples/simple_game): demonstrates communicating with peers over the network through gossiping.
- [The sharding tutorial](https://github.com/algorealmInc/SwarmNL/tree/dev/examples/sharding): demonstrates splitting a network into shards for scaling and handling communication between various nodes in a shard and across the network.
- [The replication tutorial](https://github.com/algorealmInc/SwarmNL/tree/dev/examples/replication): demonstrates the replication of data across nodes specially configured to provide redundancy to the network.

Visit the examples folder [here](https://github.com/algorealmInc/SwarmNL/tree/dev/examples) to understand more on how to use the library. The examples also contains integration with IPFS, HTTP servers etc.

## Documentation

Visit the deployed Rust docs [here](https://algorealminc.github.io/SwarmNL/swarm_nl/index.html).

## Features and examples

### Node configuration

SwarmNL provides a simple interface to configure a node and specify parameters to dictate its behaviour. This includes:

- Selection and configuration of the transport layers to be supported by the node
- Selection of cryptographic keypairs (ed25519, RSA, secp256k1, ecdsa)
- Storage and retrieval of keypair locally
- PeerID and multiaddress generation
- Protocol specification and handlers
- Event handlers for network events and logging

#### Example

```rust
    #![cfg_attr(not(doctest))]
      //! Using the default node setup configuration

      // Default config
      let config = BootstrapConfig::default();
      // Build node or network core
      let node = CoreBuilder::with_config(config)
          .build()
          .await
          .unwrap();

      //! Using a custom node setup configuration

      // Custom configuration
      // a. Using config from an `.ini` file
      let config = BootstrapConfig::from_file("bootstrap_config.ini");

      // b. Using config methods
      let mut bootnode = HashMap::new();  // Bootnodes
      let ports = (1509, 2710);  // TCP, UDP ports

      bootnode.insert(
          PeerId::random(),
          "/ip4/x.x.x.x/tcp/1509".to_string()
      );

      let config = BootstrapConfig::new()
          .with_bootnodes(bootnode)
          .with_tcp(ports.0)
          .with_udp(ports.1);

      // Build node or network core
      let node = CoreBuilder::with_config(config)
          .build()
          .await
          .unwrap();

```

Please look at a template `.ini` file [here](https://github.com/algorealmInc/SwarmNL/blob/dev/swarm_nl/bootstrap_config.ini) for configuring a node in the network.<br><br>

### Event Handling

During network operations, various events are generated. These events help us track the activities in the network layer. When generated, they are stored in an internal buffer until they are explicitly polled and consumed, or until the queue is full. It is important to consume critical events promptly to prevent loss if the buffer becomes full.

```rust
    #![cfg_attr(not(doctest))]
    //! Consuming the events by retrieving it as a iterator

   // Default config
    let config = BootstrapConfig::default();
    // Build node or network core
    let node = CoreBuilder::with_config(config)
        .build()
        .await
        .unwrap();

	// Read all currently buffered network events
	let events = node.events().await;

	let _ = events
		.map(|e| {
			match e {
				NetworkEvent::NewListenAddr {
					local_peer_id,
					listener_id: _,
					address,
				} => {
					// Announce interfaces we're listening on
					println!("Peer id: {}", local_peer_id);
					println!("We're listening on the {}", address);
				},
				NetworkEvent::ConnectionEstablished {
					peer_id,
					connection_id: _,
					endpoint: _,
					num_established: _,
					established_in: _,
				} => {
					println!("Connection established with peer: {:?}", peer_id);
				},
				_ => {},
			}
		})
		.collect::<Vec<_>>();


    //! Consume the immediate next events in the internal event buffer

    // Read events generated at setup
	while let Some(event) = node.next_event().await {
		match event {
			NetworkEvent::NewListenAddr {
				local_peer_id,
				listener_id: _,
				address,
			} => {
				// announce interfaces we're listening on
				println!("Peer id: {}", local_peer_id);
				println!("We're listening on the {}", address);
			},
			NetworkEvent::ConnectionEstablished {
				peer_id,
				connection_id: _,
				endpoint: _,
				num_established: _,
				established_in: _,
			} => {
				println!("Connection established with peer: {:?}", peer_id);
			},
			_ => {},
		}
	}
```

### Node communication

For communication, SwarmNL leverages the powerful capabilities of libp2p. These includes:

- The Kadmlia DHT: Developers can use the DHT to store infomation and leverage the capabilities of the DHT to build powerful applications, easily.
- A simple RPC mechanism to exchange data quickly between peers.
- Gossiping: SwarmNL uses the Gossipsub 1.1 protocol, specified by the [libp2p spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md).

#### Communicate with the network layer

```rust
    #![cfg_attr(not(doctest))]
      //! Communicate with remote nodes using the simple and familiar async-await paradigm.

      // Build node or network core
      let node = CoreBuilder::with_config(config, state)
          .build()
          .await
          .unwrap();

      // Communication interfaces
      // a. Kademlia DHT e.g

      // Prepare an kademlia `store_record` request to send to the network layer
      let (key, value, expiration_time, explicit_peers) = (
          KADEMLIA_TEST_KEY.as_bytes().to_vec(),
          KADEMLIA_TEST_VALUE.as_bytes().to_vec(),
          None,
          None,
      );

      let kad_request = AppData::KademliaStoreRecord {
          key: key.clone(),
          value,
          expiration_time,
          explicit_peers,
      };

      // Send request
      if let Ok(result) = node.query_network(kad_request).await {
          assert_eq!(KademliaStoreRecordSuccess,result);
	    }

      // b. RPC (request-response) e.g

      // Prepare a RPC fetch request
      let fetch_key = vec!["SomeFetchKey".as_bytes().to_vec()];

      let fetch_request = AppData::SendRpc {
          keys: fetch_key.clone(),
          peer: node4_peer_id,
      };

      // Get a stream id to track the request
      let stream_id = node.send_to_network(fetch_request).await.unwrap();

      // Poll for the result
      if let Ok(result) = node.recv_from_network(stream_id).await {
          // Here, the request data was simply echoed by the remote peer
          assert_eq!(AppResponse::SendRpc(fetch_key), result);
      }

      // c. Gossiping e.g

      // Prepare gossip request
      let gossip_request = AppData::GossipsubBroadcastMessage {
          topic: GOSSIP_NETWORK.to_string(),
          message: vec!["Daniel".to_string(), "Deborah".to_string()],
      };

      if let Ok(result) = node.query_network(gossip_request).await {
          assert_eq!(AppResponse::GossipsubBroadcastSuccess, result);
      }
```

## Replication

**SwarmNL** makes fault tolerance through redundancy simple and easy to integrate into your application. With replication built into SwarmNL, you can achieve robust and scalable systems effortlessly.

### Key Features

- **Consistency Models**: Choose from a variety of consistency models, including strong consistency with customizable parameters.
- **Dynamic Node Management**: Nodes can seamlessly join and leave replica networks without disrupting operations. Events are quickly propagated to all nodes.
- **Ease of Use**: Minimal setup is required to add replication to your system, ensuring quick integration and deployment.

### Example: Configuring and Using Replication

Here’s how you can set up and use SwarmNL's replication capabilities:

#### Configuring a Node for Replication

```rust
    //! Configure the node for replication with a strong consistency model

    // Define the replica network ID
    const REPL_NETWORK_ID: &str = "replica_xx";

    // Configure replication settings
    let repl_config = ReplNetworkConfig::Custom {
        queue_length: 150,
        expiry_time: Some(10),
        sync_wait_time: 5,
        consistency_model: ConsistencyModel::Strong(ConsensusModel::All),
        data_aging_period: 2,
    };

    // Build the node with replication enabled
    let node = builder.with_replication(repl_config).build().await.unwrap();

    // Join a replica network
    node.join_repl_network(REPL_NETWORK_ID.into()).await;

    // Replicate data across the network
    node.replicate(payload, REPL_NETWORK_ID).await;
```

#### Handling Replication Events

SwarmNL exposes network events to your application, allowing you to process incoming replica data effectively.

```rust
    // Listen for replication events
    loop {
        // Check for incoming data events
        if let Some(event) = node.next_event().await {
            if let NetworkEvent::ReplicaDataIncoming { source, .. } = event {
                println!("Received incoming replica data from {}", source.to_base58());
            }
        }

        // Try to consume data from the replication buffer
        if let Some(repl_data) = node.consume_repl_data(REPL_NETWORK_ID).await {
            println!(
                "Data received from replica: {} ({} confirmations)",
                repl_data.data[0],
                repl_data.confirmations.unwrap()
            );
        }
    }

```

### Why Use SwarmNL for Replication?

- **Reliability**: Ensures data integrity across multiple nodes with customizable consistency guarantees.
- **Scalability**: Handles dynamic node changes with ease, making it suitable for large distributed systems.
- **Flexibility**: Provides a range of replication configurations to meet diverse application needs.

## Sharding

Sharding is a capability in distributed systems that enables networks to scale efficiently. SwarmNL provides a generic sharding functionality, allowing applications to easily partition their network and configure it for sharding.

### Key Features

- **Customizable Sharding Algorithms**: SwarmNL supports generic interfaces that let you specify your own sharding algorithm, such as hash-based or range-based, while leveraging the full capabilities of the network.
- **Replication-Driven Sharding**: Sharding in SwarmNL is built on its replication capabilities, ensuring the library remains lightweight and highly functional.
- **Data Forwarding**: SwarmNL implements data-forwarding, allowing any node to handle requests for data stored on other nodes within any shard. Data is forwarded to the appropriate node for storage, and a network search algorithm enables retrieval from any node in any shard.
- **Integrated Application Layer Traps**: To maintain flexibility, SwarmNL permits nodes storing data to `trap` into the application layer when handling data requests. This ensures practicality and usability in real-world scenarios.

### Example: Configuring a sharded network

Here’s how you can set up and use SwarmNL's sharding capabilities:

#### Configuring a node for sharding

```rust

    //! Configure a node for sharding operations

    /// The constant id of the sharded network. Should be kept as a secret.
    pub const NETWORK_SHARDING_ID: &'static str = "sharding_xx";

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

    /// Hash-based sharding implementation.
    pub struct HashSharding;

    impl HashSharding {
        /// Compute a simple hash for the key.
        fn hash_key(&self, key: &str) -> u64 {
            // Convert the key to bytes
            let key_bytes = key.as_bytes();

            // Generate a hash from the first byte
            if let Some(&first_byte) = key_bytes.get(0) {
                key_bytes.iter().fold(first_byte as u64, |acc, &byte| {
                    acc.wrapping_add(byte as u64)
                })
            } else {
                0
            }
        }
    }

    /// Implement the `Sharding` trait.
    impl Sharding for HashSharding {
        type Key = str;
        type ShardId = String;

        /// Locate the shard corresponding to the given key.
        fn locate_shard(&self, key: &Self::Key) -> Option<Self::ShardId> {
            // Calculate and return hash
            Some(self.hash_key(key).to_string())
        }
    }

    // Local shard storage
    let local_storage = Arc::new(Mutex::new(LocalStorage));

    // Configure node for replication, we will be using an eventual consistency model here.
    // If we will be sharding data, our consistency model MUST be set to EVENTUAL.
    let repl_config = ReplNetworkConfig::Custom {
        queue_length: 150,
        expiry_time: Some(10),
        sync_wait_time: 5,
        consistency_model: ConsistencyModel::Eventual,
        data_aging_period: 2,
    };

    let node = builder
        .with_replication(repl_config)
        .with_sharding(NETWORK_SHARDING_ID.into(), shard_storage)
        .build()
        .await
        .unwrap();
```

#### Choosing a sharding algorithm and storing data on the network

```rust
    //! Select a sharding algorithm and assign nodes to their respective shards

    // Initialize the hash-based sharding policy
    let shard_executor = HashSharding;

    // Get shard IDs using the configured location algorithm.
    let shard_id_1 = shard_executor.locate_shard("earth").unwrap();
    let shard_id_2 = shard_executor.locate_shard("mars").unwrap();

    // Nodes join their respective shards
    // Node 2 and Node 3 will join the same shard, enabling replication to maintain
    // a consistent shard network state across nodes.
    match name {
        "Node 1" => {
            if shard_executor
                .join_network(node.clone(), &shard_id_1)
                .await
                .is_ok()
            {
                println!("Successfully joined shard: {}", shard_id_1);
            }
        },
        "Node 2" => {
            if shard_executor
                .join_network(node.clone(), &shard_id_2)
                .await
                .is_ok()
            {
                println!("Successfully joined shard: {}", shard_id_2);
            }
        },
        "Node 3" => {
            if shard_executor
                .join_network(node.clone(), &shard_id_2)
                .await
                .is_ok()
            {
                println!("Successfully joined shard: {}", shard_id_2);
            }
        },
        _ => {}
    }

    let shard_key = "mars".to_string();

    // Store data across the network in the shard pointed to by the key
    match shard_executor
        .shard(
            node.clone(),
            &shard_key,
            payload,
        )
        .await;
```

#### Handling Sharding Events

A node can receive data either through forwarding from a node in another shard or via replication from a peer node in the same shard. Below is an example demonstrating how to listen for and handle both types of events.

```rust
    loop {
        // Check for incoming data events
        if let Some(event) = node.next_event().await {
            // Handle incoming data events
            match event {
                NetworkEvent::IncomingForwardedData { data, source } => {
                    println!(
                        "Received forwarded data: {:?} from peer: {}",
                        data,
                        source.to_base58()
                    );

                    // Split the contents of the incoming data
                    let data_vec = data[0].split(" ").collect::<Vec<_>>();

                    // Extract file name and content
                    if let [file_name, content] = &data_vec[..] {
                        let _ = append_to_file(file_name, content).await;
                    }
                },
                NetworkEvent::ReplicaDataIncoming {
                    data,
                    network,
                    source,
                    ..
                } => {
                    println!(
                        "Received replica data: {:?} from shard peer: {}",
                        data,
                        source.to_base58()
                    );

                    if let Some(repl_data) = node.consume_repl_data(&network).await {
                        // Split the contents of the incoming data
                        let data = repl_data.data[0].split(" ").collect::<Vec<_>>();

                        // Extract file name and content
                        if let [file_name, content] = &data[..] {
                            let _ = append_to_file(file_name, content).await;
                        }
                    } else {
                        println!("Error: No message in replica buffer");
                    }
                },
                _ => {}
            }
        }
    }
```

### Why Use SwarmNL for Sharding?

SwarmNL integrates the networking and storage layers to deliver a seamless sharding experience. This approach enables nodes to interact directly with the application layer and local environment, providing a robust and flexible solution for scalable distributed systems.

### _Moving forward 👷🏼_
_In future iterations, we will be working on:_
- _Extending support for more transport layers._
- _Optimization of network algorithms._

<br>
In essence, SwarmNL is designed to simplify networking so you can focus on building that world-changing application of yours! Cheers! 🥂

With ❤️ from [Deji](https://github.com/thewoodfish) and [Sacha](https://github.com/sacha-l).

<p align="center">
<img src="https://github.com/algorealmInc/SwarmNL/blob/9c00c692857c6981faae2680cc785a430e378ecf/web3%20foundation_grants_badge_black.png"  alt="Web3 Foundation Grants Badge" width="300"> 
</p>

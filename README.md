<!-- <img src="https://github.com/algorealmInc/SwarmNL/blob/c3fe530350ec37755c64b47cba06361d39b3b095/SwarmNL.png" alt="SwarmNL" style="border-radius: 15px !important;"> -->

# SwarmNL

**A library to build custom networking layers for decentralized applications**

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

## examples

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

_In Development 👷_:

- _Node failure handling involving reconnection strategies, failover mechanisms etc_.
- _Scaling involving techniques like sharding, data forwarding etc_.
- _IPFS upload and download interfaces_.

In essence, SwarmNL is designed to simplify networking so you can focus on building that world-changing application of yours! Cheers! 🥂

With ❤️ from [Deji](https://github.com/thewoodfish) and [Sacha](https://github.com/sacha-l).

<p align="center">
<img src="https://github.com/algorealmInc/SwarmNL/blob/9c00c692857c6981faae2680cc785a430e378ecf/web3%20foundation_grants_badge_black.png"  alt="Web3 Foundation Grants Badge" width="300"> 
</p>

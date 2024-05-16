<!-- <img src="https://github.com/algorealmInc/SwarmNL/blob/c3fe530350ec37755c64b47cba06361d39b3b095/SwarmNL.png" alt="SwarmNL" style="border-radius: 15px !important;"> -->

# SwarmNL
**A library to build custom networking layers for decentralized applications**

SwarmNL is a library designed for P2P networking in distributed systems. It's lightweight, scalable, and easy to configure, making it perfect for decentralized applications. Powered by [libp2p](https://docs.libp2p.io/), SwarmNL simplifies networking so developers can focus on building.

## Why SwarmNL?
SwarmNL makes buiding a peer-to-peer decentralized and distributed networking stack for your application a breeze. With SwarmNL, you can effortlessly configure nodes, tailor network conditions, and fine-tune behaviors specific to your project's needs, allowing you to dive into networking without any hassle.

Say goodbye to the complexities of networking and hello to simplicity. With SwarmNL, all the hard work is done for you, leaving you to focus on simple configurations and your application logic.

## Tutorials

Have a look at some tutorials that demonstrate the use of SwarmNl in various contexts:

- [Echo server]()
- [File sharing app]()
- [Simple game]()

## Documentation

Visit the deployed Rust docs [here](https://algorealminc.github.io/SwarmNL/swarm_nl/index.html).

## Features

- **Node Configuration**: SwarmNL provides a simple interface to configure a node and specify parameters to dictate its behaviour. This includes:

  - Selection and configuration of the transport layers to be supported by the node
  - Selection of cryptographic keypairs (ed25519, RSA, secp256k1, ecdsa)
  - Storage and retrieval of keypair locally
  - PeerID and multiaddress generation
  - Protocol specification and handlers
  - Event handlers for network events and logging

  - **Node Communication**: For communication, SwarmNL leverages the powerful capabilities of libp2p. These includes:

  - The Kadmlia DHT: Developers can use the DHT to store infomation and leverage the capabilities of the DHT to build powerful applications, easily.
  - A simple RPC mechanism to exchange data quickly between peers.
  - Gossiping: SwarmNL uses the Gossipsub 1.1 protocol, specified by the [libp2p spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md).

  ## Example
  
  ### Setup a node

  ```rust
        //! Using the default node setup configuration and the default network event handler

        // Default config
        let config = BootstrapConfig::default();
        // Default network handler
        let handler = DefaultHandler;
        // Build node or network core
        let node = CoreBuilder::with_config(config, handler)
            .build()
            .await
            .unwrap();


        //! Using a custom node setup configuration and a custom network event handler

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

        // Custom event handler
        use swarm_nl::core::EventHandler;

        #[derive(Clone)]
        struct ApplicationState{
            name: String,
            version: i8,
        }

        // Define custom behaviour to respond to network events
        impl EventHandler for AppState {
            fn new_listen_addr(
                &mut self,
                local_peer_id: PeerId,
                listener_id: ListenerId,
                addr: Multiaddr,
            ) {
                // Announce interfaces we're listening on
                println!("Peer id: {}", local_peer_id);
                println!("We're listening on the {}", addr);
            }

            // Echo data recieved from a RPC
            fn rpc_handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
                println!("Recvd incoming RPC: {:?}", data);
                data
            }

            // Handle the incoming gossip message
            fn gossipsub_handle_incoming_message(&mut self, source: PeerId, data: Vec<String>) {
                println!("Recvd incoming gossip: {:?}", data);
            }
        }

        // Define custom event handler
        let state = ApplicationState {
            name: String::from("SwarmNL"),
            version: 0.1
        }

        // Build node or network core
        let node = CoreBuilder::with_config(config, state)
            .build()
            .await
            .unwrap();

  ```
  Please look at a template `.ini` file [here](https://github.com/algorealmInc/SwarmNL/blob/dev/swarm_nl/bootstrap_config.ini) for configuring a node in the network.<br><br>

  ### Application state change
    SwarmNL exposes two interfaces to make application state changes:
    - Event handlers: Registered event handlers can make state changes as a response to occurrences in the network.
    - Network core or the node: The node construct is the canonical way of changing internal applicatio state. The application state is exposed by the `state` field of the node or network core.
    This behaviour can only mean one thing: synchronization. Employement of really good synchronization pimitives. The occurrences of network events are non-deterministic meaning they could change state at any point in time. We solve this problem by using an `Mutex` to lock the state and make changes.

    #### Event Handlers
    When an event handler method id run, we can be sure that it has already acquired the application state `Mutex` and the entire length of its lifetime **is a critical section**. This is important to know as it is not advicable to make long computations or delay the release of the `Mutex` by prolonging the time the function completes its task and returns. It should be relatively simple.
    ```rust
        // A simple event handler method that modifies application state
        // ...
            /// Event that announces the arrival of a gossip message.
            fn gossipsub_incoming_message_handled(&mut self, _source: PeerId, data: Vec<String>) {
                println!(
                    "[[Node {}]] >> incoming data from peer -> {}: {}",
                    self.node, data[0], data[1]
                );

                // Parse our data
                match data[0].as_str() {
                    "guess" => {
                        // Our remote peer has made a guess
                        let remote_peer_guess = data[1].parse::<i32>().unwrap();

                        // Compare
                        if self.current_guess > remote_peer_guess {
                            self.score += 1;
                        }
                    },
                    "win" => {
                        // Set our score to -1
                        // Game over
                        self.score = -1;
                    },
                    _ => {},
                }

                if self.score != -1 && self.score != HIGH_SCORE {
                    println!(
                        "[[Node {}]] >> Node ({}) score: {}",
                        self.node, self.node, self.score
                    );
                }
            }
        // ...
    ```

    #### Network Core
    The state of the application is exposed to the network core (or node) through the `state` field. The field is protected by a mutex which prevents reace conditions in case the network event handlers are fired. So in order to access the application state or make changes to it, the mutex must first be acquired.
    ```rust
        // Check score
        // If the remote has won, our handler will set our score to -1
        if node_2.state.lock().await.score == HIGH_SCORE {
            // We've won!
            println!(
                "[[Node {}]] >> Congratulations! Node 2 is the winner.",
                node_2.state.lock().await.node
            );

            // Inform Node 1

            // Prepare a gossip request
            let gossip_request = AppData::GossipsubBroadcastMessage {
                topic: GOSSIP_NETWORK.to_string(),
                message: vec!["win".to_string(), random_u32.to_string()],
            };

            // Gossip our random value to our peers
            let _ = node_2.query_network(gossip_request).await;

            break;
        } else if node_2.state.lock().await.score == -1 {
            // We lost :(
            println!(
                "[[Node {}]] >> Game Over! Node 1 is the winner.",
                node_2.state.lock().await.node
            );
            break;
        }
    ```

- **Node Communication**: For communication, SwarmNL leverages the powerful capabilities of libp2p. These includes:

  - The Kadmlia DHT: Developers can use the DHT to store infomation and leverage the capabilities of the DHT to build powerful applications, easily.
  - A simple RPC mechanism to exchange data quickly between peers.
  - Gossiping: SwarmNL uses the Gossipsub 1.1 protocol, specified by the [libp2p spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md).

  #### Communicate with the network layer

  ```rust
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

        let fetch_request = AppData::FetchData {
            keys: fetch_key.clone(),
            peer: node4_peer_id,
        };

        // Get a stream id to track the request
        let stream_id = node.send_to_network(fetch_request).await.unwrap();

        // Poll for the result
        if let Ok(result) = node.recv_from_network(stream_id).await {
            // Here, the request data was simply echoed by the remote peer
            assert_eq!(AppResponse::FetchData(fetch_key), result);
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

- _In Development 👷_:
  - _Node failure handling involving reconnection strategies, failover mechanisms etc_.
  - _Scaling involving techniques like sharding, data forwarding etc_.
  - _IPFS upload and download interfaces_.

In essence, SwarmNL is designed to simplify networking so you can focus on building that world-changing application of yours! Cheers! 🥂 

With ❤️ from [Deji](https://github.com/thewoodfish) and [Sacha](https://github.com/sacha-l).


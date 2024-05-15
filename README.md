<img src="https://github.com/algorealmInc/SwarmNL/blob/c3fe530350ec37755c64b47cba06361d39b3b095/SwarmNl.png" alt="SwarmNl" style="border-radius: 15px !important;">

SwarmNL is a Rust library that provides a highly configurable P2P networking layer to be used in distributed system architectures that require data transfer solutions.
It is designed to offer developers a lightweight, scalable and configurable networking stack, easy to integrate with any decentralized application.<br>
It is built on libp2p.

## Why SwarmNl?
SwarmNl helps you set up a p2p decentralized and distributed networking stack for your application quickly and with great ease. You can easily configure nodes, set custom network conditions and behaviour perculiar to your problem scope, and begin networking!<br>
All the hassles and fun of networking has been taken care of for you. You only need to worry about simple configurations. That easy!

## Features
- **Node Configuration**: SwarmNL provides a simple interface to configure a node and specify parameters to dictate its behaviour. This includes:
    - Selection and configuration of the transport layers to be supported by the node.
    - Selection of cryptographic keypairs (ed25519, RSA, secp256k1, ecdsa)
    - Storage and retrieval of keypair locally.
    - PeerID and multiaddress generation.
    - Protocol specification and handlers.
    - Event handlers for network events and logging.

    ### Example
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
        
        // Custom configuration:
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

        /// Simple as ABC!
    ```

- **Node Communication**: For communication, SwarmNl leverages the powerful capabilities of libp2p. These includes:  
    - The Kadmlia DHT: Developers can use the DHT to store infomation and leverage the capabilities of the DHT to build powerful applications, easily.
    - A simple RPC mechanism to exchange data quickly between peers.
    - Gossiping: SwarmNL uses the Gossipsub 1.1 protocol, specified by the [libp2p spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md).

- *In Development 👷*:
    - *Node failure handling involving reconnection strategies, failover mechanisms etc*.
    - *Scaling involving techniques like sharding, data forwarding etc*.
    - *IPFS upload and download interfaces*.

## License

Apache 2.0

<img src="https://github.com/algorealmInc/SwarmNL/blob/c3fe530350ec37755c64b47cba06361d39b3b095/SwarmNl.png" alt="SwarmNl" style="border-radius: 15px !important;">

SwarmNL is a Rust library that provides a highly configurable P2P networking layer to be used in distributed system architectures that require data transfer solutions.
It is designed to offer developers a lightweight, scalable and configurable networking stack, easy to integrate with any decentralized application.

## Why SwarmNl?
SwarmNl helps you set up a p2p decentralized and distributed network stack for your application quickly and with great ease. You can easily configure nodes, set custom network conditions and behaviour perculiar to your problem scope, and begin networking!<br>
All the hassles and fun of networking has been taken care of for you. You only need to worry about simple configurations. That easy!

## Features

**Node Configuration**

SwarmNL provides a simple interface to configure a node and specify parameters to dictate its behaviour. This includes:

- Selection and configuration of the transport layers to be supported by the node.
- Selection of the cryptographic keypairs (ed25519, RSA, secp256k1, ecdsa)
- Storage and retrieval of keypair locally.
- PeerID and multiaddress generation.
- Protocol specification and handlers.
- Event handlers for network events and logging.

**Node Communication**

SwarmNL uses the Gossipsub 1.1 protocol, specified by the [libp2p spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md).

**Node Failure Handling**

SwarmNL provides customizable options for developers to define reconnection strategies, automatic peer discovery, and failover mechanisms. This ensures that the network can gracefully adapt to failures without compromising overall system performance.

**Scaling**

Here's how SwarmNL handles a growing (or shrinking) number of nodes while maintaining performance and reliability (_note: this is currently under development👷_):

- *Sharding* -- a flexible generic sharding protocol that allows application specify configurations like sharding hash functions and locations for shards.
- *Data Forwarding* -- definition of a protocol for forwarding messages between nodes in different shards and establishment of efficient routing mechanisms for inter-shard communication.
- *Fault Tolerance* -- implementation of fault-tolerant mechanisms for detecting (and recovering from) node failures. This might involve redundancy, node replication, erasure encoding/decoding or re-routing strategies.

**IPFS**

- *Upload* -- provision of interfaces to upload to IPFS, pin on current node and post arbitrary data to remote servers. Encryption is also easily pluggable and will be provided.
- *Download* -- retrieval and possible decryption of data from the IPFS network.

## License

Apache 2.0

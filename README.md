<div style="text-align: center;" align="center">

# SwarmNL
## a library to build custom networking layers for decentralized web applications

![swarmnl](/swarm-img.png)

SwarmNL is a Rust library that provides a highly configurable P2P networking layer to be used in distributed system architectures that require data transfer solutions for off-chain communication.
It is designed to offer developers a lightweight, scalable and configurable networking stack, easy to integrate to any decentralized application.

</div>

## Features

**Node Configuration**

SwarmNL provides a simple interface to configure a node and specify parameters to dictate its behaviour. This includes:

- Selection and configuration of the transport layers to be supported by the node.
- Selection of the cryptographic keypair to use for identity generation e.g Edwards.
- Storage and retrieval of keypair locally.
- PeerID and multiaddress generation.
- Protocol specification and handlers.
- Event handlers for network events and logging.

**Node Communication**

SwarmNL uses the Gossipsub 1.1 protocol, specified by the libp2p spec.

**Node Failure Handling**

SwarmNL provides customizable options for developers to define reconnection strategies, automatic peer discovery, and failover mechanisms. This ensures that the network can gracefully adapt to failures without compromising overall system performance.

**Scaling**

SwarmNL needs to efficiently handle a growing (or shrinking) number of nodes while maintaining performance and reliability. Here's what we plan to implement to this effect:

- *Sharding* -- implementation of a flexible generic sharding protocol that allows application specify configurations like sharding hash functions and locations for shards.
- *Data Forwarding* -- definition of a protocol for forwarding messages between nodes in different shards and establishment of efficient routing mechanisms for inter-shard communication.
- *Fault Tolerance* -- implementation of fault-tolerant mechanisms for detecting (and recovering from) node failures. This might involve redundancy, node replication, erasure encoding/decoding or re-routing strategies.

**IPFS**

- *Upload* -- provision of interfaces to upload to IPFS, pin on current node and post arbitrary data to remote servers. Encryption is also easily pluggable and will be provided.
- *Download* -- retrieval and possible decryption of data from the IPFS network.

## Technology Stack

- [Libp2p](https://libp2p.io/)
- [Rust](https://www.rust-lang.org/)

## License

Apache 2.0
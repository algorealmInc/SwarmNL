<div style="text-align: center;" align="center">

# SwarmNL
## a library to build custom networking layers for decentralized applications

SwarmNL is a Rust library that provides a highly configurable P2P networking layer to be used in distributed system architectures that require data transfer solutions.
It is designed to offer developers a lightweight, scalable and configurable networking stack, easy to integrate to any decentralized application.

</div>

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
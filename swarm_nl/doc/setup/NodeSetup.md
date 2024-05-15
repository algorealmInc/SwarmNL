# Node setup

To set up a node, you'll need to configure either a single peer or a group of peers that can kickstart the network. This involves:
- Reading a `.ini` file with bootstrap configuration data
- Or configuring parameters like bootstrap nodes, TCP/UDP ports, and cryptographic settings directly

These configurations takes place on a [`BootstrapConfig`] config object and they affect the node and the network at large.
[`BootstrapConfig`] can be configured in two ways:
- Using configuration methods:
```rust
    let mut bootnode = HashMap::new();  // Bootnodes
    let ports = (1509, 2710);  // TCP, UDP ports

    bootnode.insert(
        PeerId::random(),
        "/ip4/x.x.x.x/tcp/1509".to_string()
    );

    // Cryptographic keypair for message signing and identity generation    
    let mut ed25519_serialized_keypair =    
        Keypair::generate_ed25519().to_protobuf_encoding().unwrap();

    // Build config
    BootstrapConfig::new()
        .with_bootnodes(bootnode)
        .with_tcp(ports.0)
        .with_udp(ports.1)
        .generate_keypair_from_protobuf(key_type_str, &mut ed25519_serialized_keypair);
```

- Reading the config values from a `.ini` config file:
```rust 
    // Build config
    BootstrapConfig::from_file("bootstrap_config.ini")
        // You can combine methods that override the values in bootstrap_config
        .with_tcp(1509);     
```

When setting up a new network, you won't need to specify any bootnodes initially since you're the only one in the network. However, if you're joining an existing network, you'll need to obtain a peer's `peerId` and `multiaddress` to configure it as your bootnode and connect to them.

An example `.ini` file could look like this:

```ini
# example .ini file
[ports]
; TCP/IP port to listen on
tcp=3000
; UDP port to listen on
udp=4000

[auth]
; Type of keypair to generate for node identity and message auth e.g RSA, EDSA, Ed25519
crypto=Ed25519
; The protobuf serialized format of the node's cryptographic keypair
protobuf_keypair=[]

[bootstrap]
; The boostrap nodes to connect to immediately after start up
boot_nodes=[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/x.x.x.x/tcp/1509, QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt:/x.x.x.x/tcp/1509]

[blacklist]
; The list of blacklisted peers we don't want to have anything to do with
blacklist=[]
```

## Fallback behaviour

Node setup will fallback to default behavior if: 
* The `ini` file doesn't contain any `[ports]` section, the fallback ports for TCP is [`MIN_PORT`] and for UDP is [`MAX_PORT`].
* No key type is specified, it will default to `Ed25519`.

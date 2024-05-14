# Node setup

For any node setup, you need a valid `.ini` file to create the bootstrap config object and a [`core::EventHandler`] object to specify what events you would like to listen to and how you want to handle them. 

When we say "node setup" we mean the requirements to launch a single or a set of peers that can bootstrap the network. This requires passing in an `.ini` file with bootstrap configuration data such as bootstrap nodes, TCP/UDP ports and cryptographic types for keypair generation.

If you're setting up a new network for the first time, you don't need to pass in any bootnodes. If you're joining an exisiting network, you need to ask someone for their bootnode addresses to connect to. The [`BootstrapConfig`] object will handle reading the `.ini` file to build a configuration for setting up the core network.

## Example

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
```

## Fallback behaviour

Node setup will fallback to default behavior if: 
* The `ini` file doesn't contain any `[ports]` section, the fallback ports for TCP is [`MIN_PORT`] and for UDP is [`MAX_PORT`].
* No key type is specified, the default is will fallback to `Ed25519`
* No bootnodes are passed in, an empty hashmap is created

## Panics
* If no `.ini` file is supplied
* If the `.ini` file does not contain a valid keypair
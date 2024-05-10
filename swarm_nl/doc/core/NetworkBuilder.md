# Network builder

The [`CoreBuilder::with_config`] method takes in one parameter for the bootstrap config done by [`setup::BootstrapConfig`] and another parameter to pass in an [`EventHandler`] to handle network events. With this you can build a swarm like this:

```rust
    let config = BootstrapConfig::default();
    let handler = DefaultHandler; // from the core library
    let mut network = swarm_nl::core::CoreBuilder::with_config(config, complex_handler).build().await.unwrap();
```

## Event Handlers

You could always just use the [`DefaultHandler`] provided by the library. But if you wanted to add any custom logic around how you want to handle network events, you must implement the methods you want from [`EventHandler`]. For example:

```rust
use swarm_nl::core::EventHandler;

struct ComplexHandler;

impl EventHandler for ComplexHandler {
	fn new_listen_addr(&mut self, _listener_id: ListenerId, addr: Multiaddr) {
		// Log the address we begin listening on
		println!("We're now listening on: {}", addr);
	}
}
```

## Custom configurations

You can specify custom transport layers for TCP and QUIC (in the future we will be able to handle other transport options such as WebRTC).

# Implementing your own protocols

For now, the protocols we've implemented are Ping, Kademlia and Identify. You could always introduce your own custom protocol, for example:

TODO
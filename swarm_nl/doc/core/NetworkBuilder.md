# Network builder

To build a swarm you first need to create a [`CoreBuilder`] object using the `with_config` method to build a bootstrap node, then you can simply call `build` to set up the network.

The [`CoreBuilder::with_config`] method takes two parameters:
1. [`BootstrapConfig`] to pass in a bootstrap node configuration
2. [`EventHandler`] to handle network events.  

Here's how you would build a bootstrap node with the default library settings and set up the network:

```rust
let config = BootstrapConfig::default();
let handler = DefaultHandler;
let mut network = CoreBuilder::with_config(config, handler)
	.build()
	.await
	.unwrap()
```

To add any custom logic around how you want to handle network events, you must implement the methods you want from [`EventHandler`]. For example:

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

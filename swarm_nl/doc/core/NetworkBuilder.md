# Network builder

To build a network or a node, you first need to create a [`CoreBuilder`] object using the [`CoreBuilder::with_config`] method to create a bootstrap node, then you can simply call [`CoreBuilder::build`] to set up the network. This will create a [`Core`] struct with methods you can use to send and receive data to/from the network.

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

### Custom event handler

To add any custom logic around how you want to handle network events, you must implement the methods from [`EventHandler`]. The role of this implementation override is to respond to your custom network's state and instruct the network to behave in a pre-configured way. For example:

```rust
use swarm_nl::core::EventHandler;

#[derive(Clone)]
struct ApplicationHandler{
	name: String,
	version: u8,
}

impl EventHandler for ApplicationHandler {
	async fn new_listen_addr(
		&mut self,
		local_peer_id: PeerId,
		_listener_id: swarm_nl::ListenerId,
		addr: swarm_nl::Multiaddr,
	) {
		// announce interfaces we're listening on
		println!("Peer id: {}", local_peer_id);
		println!("We're listening on the {}", addr);
		println!(
			"Connected to {}, current version: {} ",
			self.name, self.version
		);
	}
}
```

## Overriding the default network configuration

You can explicitly change the default values of [`CoreBuilder::with_config`] by calling the following methods before building the network:

- [`CoreBuilder::with_transports`]: a custom transport to use, specified in [`TcpConfig::Custom`].
- [`CoreBuilder::with_network_id`] : the network ID (e.g. `/your-protocol-name/1.0`).
- [`CoreBuilder::listen_on`] : the IP address to listen on (e.g. 127.0.0.1).
- [`CoreBuilder::with_idle_connection_timeout`]: a timeout for keeping a connection alive.

For example: TODO make it docified.

```rust
		// default node setup 
		let config = BootstrapConfig::default();
		let handler = DefaultHandler;

		// return default network core builder
		let default_node = CoreBuilder::with_config(config, handler)

		// custom node configuration
		let mut custom_network_id = "/custom-protocol/1.0".to_string();
		let mut custom_transport = TransportOpts::TcpQuic {
			tcp_config: TcpConfig::Custom {
				ttl: 10,
				nodelay: true,
				backlog: 10,
			},
		};
		let mut custom_keep_alive_duration = 20;
		let mut custom_ip_address = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));

		// pass in your custom node configuration
		let custom_node = default_node
			.with_network_id(custom_network_id.clone())
			.with_transports(custom_transport.clone())
			.with_idle_connection_timeout(custom_keep_alive_duration.clone())
			.listen_on(custom_ip_address.clone());
```
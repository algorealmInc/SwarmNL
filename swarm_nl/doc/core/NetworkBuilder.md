# Network builder

To build a network or a node, you first need to create a [`CoreBuilder`] object using the [`CoreBuilder::with_config`] method to create a bootstrap node, then you can simply call [`CoreBuilder::build`] to set up the network. This will create a [`Core`] struct with methods you can use to send and receive data to/from the network.

The [`CoreBuilder::with_config`] method takes two parameters:
1. [`BootstrapConfig`] to pass in a bootstrap node configuration.
2. [`EventHandler`] to respond to network events.  

### Default setup

Here's how you would build a bootstrap node with the default library settings, using a [`DefaultHandler`] struct to respond to network events:

```rust
// Default config
let config = BootstrapConfig::default();
// Default Handler
let handler = DefaultHandler;
let mut network = CoreBuilder::with_config(config, handler)
	.build()
	.await
	.unwrap();
```

### Custom event handler

To customize how your application handles network events, you'll need to implement the methods from [`EventHandler`]. It's best to implement [`EventHandler`] on your application's state. This allows you to:
- make critical state changes in response to network events.
- log state data at different point during network event changes.

```rust
use swarm_nl::core::EventHandler;

#[derive(Clone)]
struct ApplicationState {
	name: String,
	version: u8,
}

impl EventHandler for ApplicationState {
	async fn new_listen_addr(
		&mut self,
		local_peer_id: PeerId,
		listener_id: swarm_nl::ListenerId,
		addr: swarm_nl::Multiaddr,
	) {
		// Announce interfaces we're listening on
		println!("Peer id: {}", local_peer_id);
		println!("We're listening on the {}", addr);
		println!(
			"Connected to {}, current version: {} ",
			self.name, self.version
		);
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
```

## Overriding the default network configuration

You can explicitly overrride the default values of [`CoreBuilder::with_config`] by calling the methods like the following before building the network:

- [`CoreBuilder::with_transports`]: Configures a custom transport to use, specified in [`TransportOpts`].
- [`CoreBuilder::with_network_id`] : Configures the network ID or name e.g. `/your-protocol-name/1.0`.
- [`CoreBuilder::listen_on`] : Configures the IP address to listen on e.g. IPv4(127.0.0.1).
- [`CoreBuilder::with_idle_connection_timeout`]: Configures a timeout for keeping a connection alive.
- etc.

For example: 

```rust
		// Default config
		let config = BootstrapConfig::default();
		// Default handler
		let handler = DefaultHandler;

		// Create a default network core builder
		let default_node = CoreBuilder::with_config(config, handler);

		// Override default with custom configurations
		// Network Id
		let mut custom_network_id = "/custom-protocol/1.0".to_string();		
		// Transport
		let mut custom_transport = TransportOpts::TcpQuic {		
			tcp_config: TcpConfig::Custom {
				ttl: 10,
				nodelay: true,
				backlog: 10,
			},
		};
		// Keep-alive	
		let mut custom_keep_alive_duration = 20;	
		// IP address
		let mut custom_ip_address = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));	

		// Build a custom configured node
		let custom_node = default_node
			.with_network_id(custom_network_id.clone())
			.with_transports(custom_transport.clone())
			.with_idle_connection_timeout(custom_keep_alive_duration.clone())
			.listen_on(custom_ip_address.clone())
			.build()
			.await.
			.unwrap();
```
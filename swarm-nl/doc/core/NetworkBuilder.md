# Network builder

To build a network or a node, you first need to create a [`CoreBuilder`] object using the [`CoreBuilder::with_config`] method to create a bootstrap node, then you can simply call [`CoreBuilder::build`] to set up the network. This will create a [`Core`] struct which serves as the interface between an application and the network. It contains several methods used to interact and perform activities on the network.

The [`CoreBuilder::with_config`] method takes a parameter - [`BootstrapConfig`], to pass in a node's bootstrap configuration.

### Default setup

Here's how you would build a bootstrap node with the default library settings

```rust
// Default config
let config = BootstrapConfig::default();
let mut network = CoreBuilder::with_config(config)
	.build()
	.await
	.unwrap();
```

## Overriding the default network configuration

You can explicitly overrride the default values of [`CoreBuilder::with_config`] by calling the methods like the following before building the network:

- [`CoreBuilder::with_transports`]: Configures a custom transport to use, specified in [`TransportOpts`].
- [`CoreBuilder::with_network_id`] : Configures the network ID or name e.g. `/your-protocol-name/1.0`.
- [`CoreBuilder::listen_on`] : Configures the IP address to listen on e.g. IPv4(127.0.0.1).
- [`CoreBuilder::with_idle_connection_timeout`]: Configures a timeout for keeping a connection alive.
- [`CoreBuilder::with_replication`]: Configures the node for replication.
- etc.

For example: 

```rust
		// Default config
		let config = BootstrapConfig::default();

		// Create a default network core builder
		let default_node = CoreBuilder::with_config(config);

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
			.with_replication(ReplNetworkConfig::Default)
			.build()
			.await.
			.unwrap();
```
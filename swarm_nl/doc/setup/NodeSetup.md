# Node setup

When we say "node setup" we mean the requirements to launch a single or a set of peers that can bootstrap the network. This requires passing in an `.ini` file with bootstrap configuration data such as bootstrap nodes, TCP/UDP ports and cryptographic types for keypair generation. Have a look at the template you can use which outlines the different fields you can include.

If you're setting up a new network for the first time, you don't need to pass in any bootnodes. If you're joining an exisiting network, you need to ask someone for their bootnode addresses to connect to. The [`BootstrapConfig`] object will handle reading the `.ini` file to build a configuration. Then the [`CoreBuilder`] object launches the network with that config.

Once the configuration is setup, a stream can be polled.

## Examples

For any node setup, you need a valid `.ini` file to create the bootstrap config object and a `network_handler` object to specify what events you would like to listen to and how you want to handle them.

1. In this example, we'll setup a new network that uses `Ed25519` for keypair generation and uses the [`DefaultHandler`] from the core library. 

Put this `.ini` file at the root of your project:

```ini
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

Run the following:

```rust
use swarm_nl::core::DefaultHandler;

#[tokio::main]
async fn main() {

    // network handler object
    let handler = DefaultHandler; 

    // use the default setup, TCP
	let default_config = swarm_nl::setup::BootstrapConfig::default();

    // set up network core
	let mut network = swarm_nl::core::CoreBuilder::with_config(default_config, handler)
		.build()
		.await
		.unwrap();

    // read first (ready) message
    if let Some(StreamData::Ready) = network.application_receiver.next().await {
        println!("Database is online");

        // begin listening
        loop {
            if let Some(data) = network.application_receiver.next().await {
                println!("{:?}", data);
            }
        }
    }
}
```

1. In this example, we'll implement our own event handler to override the default handler bahavior and explicitly connect to known bootnodes:

```rust
use swarm_nl::core::DefaultHandler;

struct ComplexHandler;

impl EventHandler for ComplexHandler {
	fn new_listen_addr(&mut self, _listener_id: ListenerId, addr: Multiaddr) {
		// Log the address we begin listening on
		println!("We're now listening on: {}", addr);
	}
}

#[tokio::main]
async fn main() {
	// handler for events happening in the network layer (majorly for technical use)
	// use default handler
	let handler = DefaultHandler;
	let complex_handler = ComplexHandler;

	// set up node
	let mut bootnodes: HashMap<PeerIdString, MultiaddrString> = HashMap::new();
	bootnodes.insert(
		"12D3KooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string(),
		"/ip4/127.0.0.1/tcp/63307".to_string(),
	);

	// configure default data
	let config = swarm_nl::setup::BootstrapConfig::new().with_bootnodes(bootnodes);

	// set up network core
	let mut network = swarm_nl::core::CoreBuilder::with_config(config, complex_handler)
		.build()
		.await
		.unwrap();

	// read first (ready) message
	if let Some(StreamData::Ready) = network.application_receiver.next().await {
		println!("Database is online");

		// begin listening
		loop {
			if let Some(data) = network.application_receiver.next().await {
				println!("{:?}", data);
			}
		}
	}
}
```


## Fallback behaviour

Node setup will fallback to default behavior if: 
* The `ini` file doesn't contain any `[ports]` section, the fallback ports for TCP is [`MIN_PORT`] and for UDP is [`MAX_PORT`].
* No key type is specified, the default is always `Ed25519`
* No bootnodes are passed in, an empty hashmap is created

## Panics
* If no `.ini` file is supplied
* If the `.ini` file does not contain a valid keypair
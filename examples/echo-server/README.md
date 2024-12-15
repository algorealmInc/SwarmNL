# Echo server example

To run this example, cd into the root of this directory and run:

```bash
cargo run
```
 
Then submit an input into the terminal and watch your input get echoed back to you.

## Run with Docker

Build:

```bash
docker build -t echo-server .
```

Run:

```bash
docker run -it echo-server
```

Then submit an input into the terminal and watch your input get echoed back to you.

Hit `Ctrl+D` to exit.

## Tutorial

1. Create an async function to set up a node using the default configuration provided by SwarmNl and specify the ports your want to use.

```rust
/// Setup first node using default config.
pub async fn setup_node(ports: (Port, Port)) -> Core {
	// Use the default config parameters and override a few configurations e.g ports, keypair
	let config = BootstrapConfig::default()
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network
	CoreBuilder::with_config(config).build().await.unwrap()
}
```

2. Create a main function to run the server and echo lines read from stdin over the network.

```rust
#[tokio::main]
async fn main() {
	let stdin = io::stdin();
	let mut handle = stdin.lock();

	// 3a. Create node
	let mut node = setup_node((55000, 46000)).await;

	println!("Welcome to the Echo-Server SwarmNl example.");
	println!("Type into the terminal and watch it get echoed back to you.");

	println!("Enter your input (Ctrl+D to end):");

	// Create a buffer to store each line
	let mut buffer = String::new();

	// Loop to read lines from stdin
	while let Ok(bytes_read) = handle.read_line(&mut buffer) {
		// If no bytes were read, we've reached EOF
		if bytes_read == 0 {
			break;
		}

		let input = buffer.trim();

		// 3b. Prepare an Echo request to send to the network
		let echo_request = AppData::Echo(input.to_string());

		// 3c. Send request to the network layer and retrieve response
		if let Ok(result) = node.query_network(echo_request).await {
			// Echo to stdout
			if let AppResponse::Echo(output) = result {
				println!("--> {}", output);
			}
		}

		// Clear the buffer for the next line
		buffer.clear();
	}
}
```

The server simply creates an Echo request using [`AppData`](https://algorealminc.github.io/SwarmNL/swarm-nl/core/enum.AppData.html#variant.Echo), sends it to the network using [`query_network`](https://algorealminc.github.io/SwarmNL/swarm-nl/core/struct.Core.html#method.query_network) and prints the string received from [`AppResponse`](https://algorealminc.github.io/SwarmNL/swarm-nl/core/enum.AppResponse.html#variant.Echo) to the terminal.
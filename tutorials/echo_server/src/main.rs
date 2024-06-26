// Copyright 2024 Algorealm

/// This crate demonstrates how to use SwarmNl. Here, we build a simple echo server that
/// recieves inout from stdin, writes it to the network layer and then recieves it
/// back from the network.

use swarm_nl::core::{AppData, AppResponse, Core, CoreBuilder, EventHandler};
use swarm_nl::setup::BootstrapConfig;
use swarm_nl::{PeerId, Port};
use std::io::{self, BufRead};

/// Our application state.
#[derive(Clone)]
struct EchoServer;

/// Define custom handler for application state.
impl EventHandler for EchoServer {
	// We're just echoing the data back
	fn rpc_incoming_message_handled(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		println!("Recvd incoming RPC: {:?}", data);
		data
	}

	// Handle the incoming gossip message
	fn gossipsub_incoming_message_handled(&mut self, _source: PeerId, data: Vec<String>) {
		println!("Recvd incoming gossip: {:?}", data);
	}
}

/// Setup first node using default config.
pub async fn setup_node(ports: (Port, Port)) -> Core<EchoServer> {
	// Application state
	let state = EchoServer;

	// Use the default config parameters and override a few configurations e.g ports, keypair
	let config = BootstrapConfig::default()
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network
	CoreBuilder::with_config(config, state)
		.build()
		.await
		.unwrap()
}

// Run server
#[tokio::main]
async fn main() {
	let stdin = io::stdin();
	let mut handle = stdin.lock();

	// Create node
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

		// Prepare an Echo request to send to the network
		let echo_request = AppData::Echo(input.to_string());

		// Send request to the network layer and retrieve response
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

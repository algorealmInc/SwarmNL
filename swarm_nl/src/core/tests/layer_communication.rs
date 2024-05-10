//! Tests for the communication between the layers of the application.

use super::*;
use libp2p::{
	core::{ConnectedPoint, Multiaddr},
	PeerId,
};

/// Sate of the Application
#[derive(Clone)]
pub struct AppState;

impl EventHandler for AppState {
	fn new_listen_addr(
		&mut self,
		local_peer_id: PeerId,
		_listener_id: ListenerId,
		addr: Multiaddr,
	) {
		// announce interfaces we're listening on
		println!("Peer id: {}", local_peer_id);
		println!("We're listening on the {}", addr);
	}

	fn connection_established(
		&mut self,
		peer_id: PeerId,
		_connection_id: ConnectionId,
		_endpoint: &ConnectedPoint,
		_num_established: NonZeroU32,
		_established_in: Duration,
	) {
		println!("Connection established with peer: {}", peer_id);
	}

	fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		data
	}
}

/// Used to create a detereministic node.
pub async fn setup_node_1() -> Core<AppState> {
	// Our test keypair for the first node
	let mut protobuf = vec![
		8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27,
		202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85,
		72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164,
		148, 159, 36, 170, 109, 178,
	];
	// Ports for the first node
	let ports = (49600, 49501);

	// The PeerId of the first node
	let peer_id = Keypair::from_protobuf_encoding(&protobuf)
		.unwrap()
		.public()
		.to_peer_id();

	setup_node(&mut protobuf[..], ports).await
}

pub async fn setup_node(buffer: &mut [u8], ports: (u16, u16)) -> Core<AppState> {
	let app_state = AppState;

	// First, we want to configure our node with the bootstrap config file on disk
	let config = BootstrapConfig::default()
		.generate_keypair_from_protobuf("ed25519", buffer)
		.with_tcp(ports.0)
		.with_udp(ports.1);

	println!("First node here!");

	// Set up network
	CoreBuilder::with_config(config, app_state)
		.build()
		.await
		.unwrap()
}

#[test]
fn echo_for_node_1_fetch_from_network() {
	// Prepare an echo request
	let echo_string = "Sacha rocks!".to_string();
    let data_request = AppData::Echo(echo_string.clone());

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1()
			.await
			.fetch_from_network(data_request)
			.await
		{
			if let AppResponse::Echo(echoed_response) = result {
				// Assert that what was sent was gotten back
				assert_eq!(echo_string, echoed_response);
			}
		}
	});
}

#[test]
fn echo_for_node_1_send_and_receive(){
    // Prepare an echo request
	let echo_string = "Sacha rocks!".to_string();
    let data_request = AppData::Echo(echo_string.clone());

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let stream_id = setup_node_1()
			.await
			.send_to_network(data_request)
            .await
            .unwrap();

            if let Ok(result) = setup_node_1()
			.await
			.recv_from_network(stream_id)
			.await
		{
			if let AppResponse::Echo(echoed_response) = result {
				// Assert that what was sent was gotten back
				assert_eq!(echo_string, echoed_response);
			}
		}
	});
}

#[test]
fn dial_peer_failure_works() {
    // What we're dialing
	let peer_id = PeerId::random();
    let multi_addr = "/ip4/192.168.1.205/tcp/1509".to_string();

    let dial_request = AppData::DailPeer(peer_id, multi_addr.clone());

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let stream_id = setup_node_1()
			.await
			.send_to_network(dial_request)
            .await
            .unwrap();

            if let Ok(result) = setup_node_1()
			.await
			.recv_from_network(stream_id)
			.await
		{
            assert_eq!(AppResponse::Error(NetworkError::DailPeerError), result);
		}
	});

}
//! Tests for the communication between the layers of the application.
//!
//! To run the dialing test you need dialing_peer_works with listening_node feature enabled first:
//! ```bash
//! cargo test dialing_peer_works --features=tokio-runtime --features=listening-node
//! ```
//! Then, you can test that the dialing_peer_works with the dialing-node feature enabled:
//! ```bash
//! cargo test dialing_peer_works --features=tokio-runtime --features=dialing-node
//! ```

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
		println!("Connection established with peer: {:?}", peer_id);
	}

	fn handle_incoming_message(&mut self, data: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		data
	}
}

/// Used to create a detereministic node.
pub async fn setup_node_1(ports: (Port, Port)) -> Core<AppState> {
	// Our test keypair for the first node
	let mut protobuf = vec![
		8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27,
		202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85,
		72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164,
		148, 159, 36, 170, 109, 178,
	];

	// The PeerId of the first node
	let peer_id = Keypair::from_protobuf_encoding(&protobuf)
		.unwrap()
		.public()
		.to_peer_id();

	setup_core_builder_1(&mut protobuf[..], ports).await
}

/// Used to create a node to peer with node_1.
pub async fn setup_node_2(
	node_1_ports: (Port, Port),
	ports: (Port, Port),
) -> (Core<AppState>, PeerId) {
	let app_state = AppState;

	// Our test keypair for the node_1
	let mut protobuf = vec![
		8, 1, 18, 64, 34, 116, 25, 74, 122, 174, 130, 2, 98, 221, 17, 247, 176, 102, 205, 3, 27,
		202, 193, 27, 6, 104, 216, 158, 235, 38, 141, 58, 64, 81, 157, 155, 36, 193, 50, 147, 85,
		72, 64, 174, 65, 132, 232, 78, 231, 224, 88, 38, 55, 78, 178, 65, 42, 97, 39, 152, 42, 164,
		148, 159, 36, 170, 109, 178,
	];

	// The PeerId of the first node
	let peer_id = Keypair::from_protobuf_encoding(&protobuf)
		.unwrap()
		.public()
		.to_peer_id();

	// Set up bootnode to query node 1
	let mut bootnode = HashMap::new();
	bootnode.insert(
		peer_id.to_base58(),
		format!("/ip4/127.0.0.1/tcp/{}", node_1_ports.0),
	);

	println!("Second node here!");

	// First, we want to configure our node
	let config = BootstrapConfig::new()
		.with_bootnodes(bootnode)
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network
	(
		CoreBuilder::with_config(config, app_state)
			.build()
			.await
			.unwrap(),
		peer_id,
	)
}

pub async fn setup_core_builder_1(buffer: &mut [u8], ports: (u16, u16)) -> Core<AppState> {
	let app_state = AppState;

	// First, we want to configure our node with the bootstrap config file on disk
	let config = BootstrapConfig::default()
		.generate_keypair_from_protobuf("ed25519", buffer)
		.with_tcp(ports.0)
		.with_udp(ports.1);

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
		if let Ok(result) = setup_node_1((49600, 49601))
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
fn echo_for_node_1_send_and_receive() {
	// Prepare an echo request
	let echo_string = "Sacha rocks!".to_string();
	let data_request = AppData::Echo(echo_string.clone());

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let stream_id = setup_node_1((49500, 49501))
			.await
			.send_to_network(data_request)
			.await
			.unwrap();

		if let Ok(result) = setup_node_1((49400, 49401))
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
		let stream_id = setup_node_1((49600, 49601))
			.await
			.send_to_network(dial_request)
			.await
			.unwrap();

		if let Ok(result) = setup_node_1((49500, 49501))
			.await
			.recv_from_network(stream_id)
			.await
		{
			assert_eq!(AppResponse::Error(NetworkError::DailPeerError), result);
		}
	});
}

#[cfg(feature = "listening-node")]
#[test]
fn dialing_peer_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// set up the node that will be dialled
		setup_node_1((49666, 49606)).await;
		// loop for the listening node to keep running
		loop {}
	});
}

#[cfg(feature = "dialing-node")]
#[test]
fn dialing_peer_works() {
	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((49666, 49606), (49667, 49607)).await;

		// what we're dialing
		let multi_addr = format!("/ip4/127.0.0.1/tcp/{}", 49666);

		let dial_request = AppData::DailPeer(node_1_peer_id, multi_addr.clone());
		let stream_id = node_2.send_to_network(dial_request).await.unwrap();

		if let Ok(result) = node_2.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::DailPeerSuccess(multi_addr), result);
		}
	});
}

#[test]
fn kademlia_store_records_works() {
	// Prepare an kademlia request to send to the network layer
	let (key, value, expiration_time, explicit_peers) = (
		"Deji".as_bytes().to_vec(),
		"1000".as_bytes().to_vec(),
		None,
		None,
	);

	let kad_request = AppData::KademliaStoreRecord {
		key,
		value,
		expiration_time,
		explicit_peers,
	};

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49100, 49101))
			.await
			.fetch_from_network(kad_request)
			.await
		{
			assert_eq!(AppResponse::KademliaStoreRecordSuccess, result);
		}
	});
}

#[test]
fn kademlia_lookup_record_works() {
	// Prepare an kademlia request to send to the network layer
	let (key, value, expiration_time, explicit_peers) = (
		"Deji".as_bytes().to_vec(),
		"1000".as_bytes().to_vec(),
		None,
		None,
	);

	let kad_request = AppData::KademliaStoreRecord {
		key: key.clone(),
		value,
		expiration_time,
		explicit_peers,
	};

	// use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let mut node = setup_node_1((49155, 49222)).await;

		if let Ok(result) = node.clone().fetch_from_network(kad_request).await {
			let kad_request = AppData::KademliaLookupRecord { key };

			if let Ok(result) = node.fetch_from_network(kad_request).await {
				if let AppResponse::KademliaLookupSuccess(value) = result {
					assert_eq!("1000".as_bytes().to_vec(), value);
				}
			}
		}
	});
}

#[test]
fn kademlia_get_providers_works() {
	// Note: we can only test for the error case here, an integration test is needed to actually check that the providers can be fetched

	// Prepare an kademlia request to send to the network layer
	let req_key = "Deji".as_bytes().to_vec();

	let kad_request = AppData::KademliaGetProviders {
		key: req_key.clone(),
	};

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49988, 64544))
			.await
			.fetch_from_network(kad_request)
			.await
		{
			assert_eq!(AppResponse::KademliaNoProvidersFound, result);
		}
	});
}

// KademliaSTopProviding and KademliaDeleteRecord will alwys succeed.
// The right function to use is sent_to_network() which will not return a Some(StreamId) but will always return None.
// This is because it always succedds and doesnt need to be tracked internally.
// DO not use fetch_from_network() to send the command, if you do, it will succeed but you will get a wrong error.
// The wrong error will be NetworkError::StreamBufferOverflow, (which is wrong)

// /// Port ranges
// pub const MIN_PORT: u16 = 49152;
// pub const MAX_PORT: u16 = 65535;
// KademliaGetProviders { key: Vec<u8> },
// 	/// Stop providing a record on the network
// 	KademliaStopProviding { key: Vec<u8> },
// 	/// Remove record from local store
// 	KademliaDeleteRecord { key: Vec<u8> },
// 	/// Return important information about the local routing table
// 	KademliaGetRoutingTableInfo,
// 	/// Fetch data(s) quickly from a peer over the network
// 	FetchData { keys: Vec<Vec<u8>>, peer: PeerId },
// 	/// Get network information about the node
// 	GetNetworkInfo,
// 	// Send message to gossip peers in a mesh network
// 	GossipsubBroadcastMessage {
// 		/// Topic to send messages to
// 		topic: String,
// 		message: Vec<String>,
// 		/// Explicit peers to gossip to
// 		peers: Option<Vec<PeerId>>,
// 	},
// 	/// Join a mesh network
// 	GossipsubJoinNetwork(String),
// 	/// Get gossip information about node
// 	GossipsubGetInfo,
// 	/// Leave a network we are a part of
// 	GossipsubExitNetwork(String),
// 	/// Blacklist a peer explicitly
// 	GossipsubBlacklistPeer(PeerId),
// 	/// Remove a peer from the blacklist
// 	GossipsubFilterBlacklist(PeerId),
// }

// TODO:
// - check that DHT record store between nodes work, using a storing peer and a looking up
// and all other tests for AppData
// -

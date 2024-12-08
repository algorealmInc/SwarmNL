//! Tests for the communication between the layers of the application.

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;

use crate::{core::{AppData, AppResponse, Core, CoreBuilder, DataQueue, NetworkError, NetworkEvent}, setup::BootstrapConfig, Port, DEFAULT_NETWORK_ID};

use super::*;
use libp2p::{
	core::{transport::ListenerId, ConnectedPoint, Multiaddr},
	PeerId,
};
use libp2p_identity::Keypair;

/// Time to wait for the other peer to act, during integration tests (in seconds).
pub const ITEST_WAIT_TIME: u64 = 7;
/// The key to test the Kademlia DHT.
pub const KADEMLIA_TEST_KEY: &str = "GOAT";
/// The value to test the Kademlia DHT.
pub const KADEMLIA_TEST_VALUE: &str = "Steve Jobs";
/// The test network we join for our mesh.
pub const GOSSIP_NETWORK: &str = "avada";

/// Sate of the Application.
#[derive(Clone)]
pub struct AppState;

/// Used to create a detereministic node.
async fn setup_node_1(ports: (Port, Port)) -> Core {
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
async fn setup_node_2(node_1_ports: (Port, Port), ports: (Port, Port)) -> (Core, PeerId) {
	let app_state = AppState;

	// Our test keypair for the node_1
	let protobuf = vec![
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
		CoreBuilder::with_config(config).build().await.unwrap(),
		peer_id,
	)
}

async fn setup_core_builder_1(buffer: &mut [u8], ports: (u16, u16)) -> Core {
	let app_state = AppState;

	// First, we want to configure our node with the bootstrap config file on disk
	let config = BootstrapConfig::default()
		.generate_keypair_from_protobuf("ed25519", buffer)
		.with_tcp(ports.0)
		.with_udp(ports.1);

	// Set up network
	CoreBuilder::with_config(config).build().await.unwrap()
}

/// Sample network event for testing.
fn sample_event() -> NetworkEvent {
	NetworkEvent::NewListenAddr {
		local_peer_id: PeerId::random(),
		listener_id: ListenerId::next(),
		address: Multiaddr::empty(),
	}
}

/// Helper function to populate a queue.
async fn populate_queue(queue: &DataQueue<NetworkEvent>, count: usize) {
	for _ in 0..count {
		queue.push(sample_event()).await;
	}
}

#[test]
fn echo_for_node1_query_network() {
	// Prepare an echo request
	let echo_string = "Sacha rocks!".to_string();
	let data_request = AppData::Echo(echo_string.clone());

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49600, 49623))
			.await
			.query_network(data_request)
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
fn echo_for_node1_send_and_receive() {
	// Prepare an echo request
	let echo_string = "Sacha rocks!".to_string();
	let data_request = AppData::Echo(echo_string.clone());

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

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let stream_id = setup_node_1((49611, 49601))
			.await
			.send_to_network(dial_request)
			.await
			.unwrap();

		if let Ok(result) = setup_node_1((49507, 49508))
			.await
			.recv_from_network(stream_id)
			.await
		{
			assert_eq!(AppResponse::Error(NetworkError::DailPeerError), result);
		}
	});
}

#[test]
fn kademlia_store_records_works() {
	// Prepare an kademlia request to send to the network layer
	let (key, value, expiration_time, explicit_peers) = (
		KADEMLIA_TEST_KEY.as_bytes().to_vec(),
		KADEMLIA_TEST_VALUE.as_bytes().to_vec(),
		None,
		None,
	);

	let kad_request = AppData::KademliaStoreRecord {
		key,
		value,
		expiration_time,
		explicit_peers,
	};

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49100, 49101))
			.await
			.query_network(kad_request)
			.await
		{
			println!("----> {:?}", result);
			assert_eq!(AppResponse::KademliaStoreRecordSuccess, result);
		} else {
			// TODO: do something  to ensure this test works
		}
	});
}

#[test]
fn kademlia_lookup_record_works() {
	// Prepare a kademlia request to send to the network layer
	let (key, value, expiration_time, explicit_peers) = (
		KADEMLIA_TEST_KEY.as_bytes().to_vec(),
		KADEMLIA_TEST_VALUE.as_bytes().to_vec(),
		None,
		None,
	);

	let kad_request = AppData::KademliaStoreRecord {
		key: key.clone(),
		value,
		expiration_time,
		explicit_peers,
	};

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let mut node = setup_node_1((49155, 49222)).await;

		if let Ok(_) = node.clone().query_network(kad_request).await {
			let kad_request = AppData::KademliaLookupRecord { key };

			if let Ok(result) = node.query_network(kad_request).await {
				if let AppResponse::KademliaLookupSuccess(value) = result {
					assert_eq!(KADEMLIA_TEST_VALUE.as_bytes().to_vec(), value);
				}
			}
		}
	});
}

#[test]
fn kademlia_get_providers_works() {
	// Note: we can only test for the error case here, an integration test is needed to actually
	// check that the providers can be fetched

	// Prepare a kademlia request to send to the network layer
	let req_key = KADEMLIA_TEST_KEY.as_bytes().to_vec();

	let kad_request = AppData::KademliaGetProviders {
		key: req_key.clone(),
	};

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49988, 64544))
			.await
			.query_network(kad_request)
			.await
		{
			assert_eq!(AppResponse::KademliaNoProvidersFound, result);
		}
	});
}

#[test]
fn kademlia_get_routing_table_info_works() {
	// Prepare an kademlia request to send to the network layer
	let kad_request = AppData::KademliaGetRoutingTableInfo;

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		if let Ok(result) = setup_node_1((49999, 64555))
			.await
			.query_network(kad_request)
			.await
		{
			assert_eq!(
				AppResponse::KademliaGetRoutingTableInfo {
					protocol_id: DEFAULT_NETWORK_ID.to_string()
				},
				result
			);
		}
	});
}

#[test]
fn get_network_info_works() {
	// Prepare an info request to send to the network layer
	let kad_request = AppData::GetNetworkInfo;

	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let mut node = setup_node_1((59999, 54555)).await;

		if let Ok(result) = node.query_network(kad_request).await {
			// We'll use the peer id returned to validate the network information recieved
			if let AppResponse::GetNetworkInfo {
				peer_id,
				connected_peers,
				external_addresses,
			} = result
			{
				println!("Connected peers: {:?}", connected_peers);
				println!("External Addresses: {:?}", external_addresses);
				assert_eq!(peer_id, node.peer_id());
			}
		}
	});
}

#[test]
fn gossipsub_join_and_exit_network_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((49655, 49609)).await;

		let network = "Testers";

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(network.to_string());

		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::GossipsubJoinSuccess, result);
		}

		// exit a network (unsubscribe to a topic)
		let gossip_request = AppData::GossipsubExitNetwork(network.to_string());

		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		// test for exit
		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::GossipsubExitSuccess, result);
		}
	});
}

#[test]
fn gossipsub_blacklist_and_remove_blacklist_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((49695, 49699)).await;

		// Random peer id
		let peer_id = PeerId::random();

		// Blacklist
		let gossip_request = AppData::GossipsubBlacklistPeer(peer_id);
		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::GossipsubBlacklistSuccess, result);
		}

		// Remove blacklist
		let gossip_request = AppData::GossipsubFilterBlacklist(peer_id);
		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::GossipsubBlacklistSuccess, result);
		}
	});
}

#[test]
fn gossipsub_info_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((49395, 43699)).await;

		// Random peer id
		let peer_id = PeerId::random();
		let network = "Blackbeard".to_string();

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(network.clone());
		node_1.query_network(gossip_request).await.unwrap();

		// Blacklist a random peer
		let gossip_request = AppData::GossipsubBlacklistPeer(peer_id);
		node_1.query_network(gossip_request).await.unwrap();

		// Prepare request
		let gossip_request = AppData::GossipsubGetInfo;
		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			// break up the response info
			if let AppResponse::GossipsubGetInfo {
				topics, blacklist, ..
			} = result
			{
				// make assertions for the topic joined
				assert_eq!(network, topics[0].clone());

				// make assertions for the peers blacklisted
				assert_eq!(peer_id, *blacklist.get(&peer_id).unwrap());
			}
		}
	});
}

// -- Event queue tests --

const MAX_QUEUE_ELEMENTS: usize = 300;

#[test]
fn test_event_queue_flood() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let queue = DataQueue::new();

		// Flood the queue
		populate_queue(&queue, MAX_QUEUE_ELEMENTS + 10).await;

		let inner = queue.into_inner().await;
		assert_eq!(
			inner.len(),
			MAX_QUEUE_ELEMENTS,
			"Queue size should not exceed MAX_QUEUE_ELEMENTS"
		);
	});
}

#[test]
fn test_event_queue_pop_and_match() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let queue = DataQueue::new();

		// Push a known event
		let event = sample_event();
		queue.push(event.clone()).await;

		// Pop the event
		if let Some(popped_event) = queue.pop().await {
			assert_eq!(
				popped_event, event,
				"Popped event should match the pushed event"
			);
		} else {
			panic!("Queue should not be empty");
		}
	});
}

#[test]
fn test_event_queue_max_size() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let queue = DataQueue::new();

		// Push events up to the max size
		populate_queue(&queue, MAX_QUEUE_ELEMENTS).await;

		// Push one more event to exceed the size
		queue.push(sample_event()).await;

		let inner = queue.into_inner().await;
		assert_eq!(
			inner.len(),
			MAX_QUEUE_ELEMENTS,
			"Queue should maintain its max size"
		);

		// Oldest item should have been removed
		assert_ne!(
			Some(inner.front()),
			Some(Some(&sample_event())),
			"Oldest event should have been removed"
		);
	});
}

#[test]
fn test_event_handler_functions() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		let queue = DataQueue::new();

		// Test with no events
		assert_eq!(
			queue.pop().await,
			None,
			"Popping from empty queue should return None"
		);

		// Add an event and test handler function
		let event = sample_event();
		queue.push(event.clone()).await;

		if let Some(popped_event) = queue.pop().await {
			assert_eq!(popped_event, event, "Handler should consume the next event");
		} else {
			panic!("Queue should not be empty");
		}

		// Ensure queue is empty after consumption
		assert_eq!(
			queue.pop().await,
			None,
			"Queue should be empty after event is handled"
		);
	});
}

// -- Dialing and fetch tests --
// See: `swarm_nl::testing_guide` for information on how to run these tests.

#[cfg(feature = "test-listening-node")]
#[test]
fn dialing_peer_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		setup_node_1((49666, 49606)).await;
		// Loop for the listening node to keep running
		loop {}
	});
}

#[cfg(feature = "test-dialing-node")]
#[test]
fn dialing_peer_works() {
	// Use tokio runtime to test async function
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((49666, 49606), (49667, 49607)).await;

		// What we're dialing
		let multi_addr = format!("/ip4/127.0.0.1/tcp/{}", 49666);

		let dial_request = AppData::DailPeer(node_1_peer_id, multi_addr.clone());
		let stream_id = node_2.send_to_network(dial_request).await.unwrap();

		if let Ok(result) = node_2.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::DailPeerSuccess(multi_addr), result);
		}
	});
}

#[cfg(feature = "test-server-node")]
#[test]
fn rpc_fetch_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		setup_node_1((49666, 49606)).await;

		println!("This is the server node for RPC testing");
		// Loop for the listening node to keep running
		loop {}
	});
}

#[cfg(feature = "test-client-node")]
#[test]
fn rpc_fetch_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((49666, 49606), (49667, 49607)).await;

		println!("This is the client node for RPC testing");

		let fetch_key = vec!["SomeFetchKey".as_bytes().to_vec()];

		// What we're dialing
		let multi_addr = format!("/ip4/127.0.0.1/tcp/{}", 49666);

		// Prepare fetch request
		let fetch_request = AppData::FetchData {
			keys: fetch_key.clone(),
			peer: node_1_peer_id,
		};

		let stream_id = node_2.send_to_network(fetch_request).await.unwrap();

		if let Ok(result) = node_2.recv_from_network(stream_id).await {
			assert_eq!(AppResponse::FetchData(fetch_key), result);
		}
	});
}

// -- Tests for kademlia --
// Two nodes will interact with each other using the commands to the DHT.
// See: `swarm_nl::testing_guide` for information on how to run these tests.

#[cfg(feature = "test-reading-node")]
#[test]
fn kademlia_record_store_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((51666, 51606)).await;

		// Wait for a few seconds before trying to read the DHT
		#[cfg(feature = "tokio-runtime")]
		tokio::time::sleep(Duration::from_secs(ITEST_WAIT_TIME)).await;

		// Now poll for the kademlia record
		let kad_request = AppData::KademliaLookupRecord {
			key: KADEMLIA_TEST_KEY.as_bytes().to_vec(),
		};
		if let Ok(result) = node_1.query_network(kad_request).await {
			if let AppResponse::KademliaLookupSuccess(value) = result {
				assert_eq!(KADEMLIA_TEST_VALUE.as_bytes().to_vec(), value);
			}
		} else {
			println!("No record found");
		}
	});
}

#[cfg(feature = "test-writing-node")]
#[test]
fn kademlia_record_store_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((51666, 51606), (51667, 51607)).await;

		let (key, value, expiration_time, explicit_peers) = (
			KADEMLIA_TEST_KEY.as_bytes().to_vec(),
			KADEMLIA_TEST_VALUE.as_bytes().to_vec(),
			None,
			None,
		);

		let kad_request = AppData::KademliaStoreRecord {
			key,
			value,
			expiration_time,
			explicit_peers,
		};

		// Submit query
		let res = node_2.query_network(kad_request).await;

		loop {}
	});
}

// Note: KademliaStopProviding and KademliaDeleteRecord will alwys succeed.
// The right function to use is sent_to_network() which will not return a Some(StreamId) but will
// always return None. This is because it always succeeds and doesn't need to be tracked internally. 
// Do not use query_network() to send the command, if you do, it will succeed but you will get a 
// wrong error. The wrong error will be NetworkError::StreamBufferOverflow, (which is not correct).

// -- Tests for providers --
// See: `swarm_nl::testing_guide` for information on how to run these tests.

#[cfg(feature = "test-writing-node")]
#[test]
fn kademlia_provider_records_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((51066, 51006)).await;

		// create a Kademlia request
		let (key, value, expiration_time, explicit_peers) = (
			KADEMLIA_TEST_KEY.as_bytes().to_vec(),
			KADEMLIA_TEST_VALUE.as_bytes().to_vec(),
			None,
			None,
		);

		let kad_request = AppData::KademliaStoreRecord {
			key,
			value,
			expiration_time,
			explicit_peers,
		};

		// submit request
		let res = node_1.query_network(kad_request).await;

		loop {}
	});
}

#[cfg(feature = "test-reading-node")]
#[test]
fn kademlia_provider_records_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((51066, 51006), (51067, 51007)).await;

		// Wait for a few seconds before trying to read the DHT
		#[cfg(feature = "tokio-runtime")]
		tokio::time::sleep(Duration::from_secs(ITEST_WAIT_TIME)).await;

		// Now poll for the kademlia record provider
		let kad_request = AppData::KademliaGetProviders {
			key: KADEMLIA_TEST_KEY.as_bytes().to_vec(),
		};
		// Submit query and assert that the provider is the node 1
		if let Ok(result) = node_2.query_network(kad_request).await {
			if let AppResponse::KademliaGetProviders { key, providers } = result {
				assert_eq!(providers[0], node_1_peer_id.to_base58());
			}
		} else {
			println!("No record found");
		}
	});
}

// -- Gossipsub tests --
// See: `swarm_nl::testing_guide` for information on how to run these tests.

#[cfg(feature = "test-subscribe-node")]
#[test]
fn gossipsub_join_exit_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((49775, 49779)).await;

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			println!("Subscription successfull");
			assert_eq!(AppResponse::GossipsubJoinSuccess, result);
		}

		loop {}
	});
}

#[cfg(feature = "test-query-node")]
#[test]
fn gossipsub_join_exit_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, node_1_peer_id) = setup_node_2((49775, 49779), (51767, 51707)).await;

		// Wait for a few seconds for propagation
		#[cfg(feature = "tokio-runtime")]
		tokio::time::sleep(Duration::from_secs(ITEST_WAIT_TIME)).await;

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

		if let Ok(_) = node_2.query_network(gossip_request).await {
			println!("Subscription successfull");
			// Query the network to confirm subscription of peer
			let gossip_request = AppData::GossipsubGetInfo;
			if let Ok(result) = node_2.query_network(gossip_request).await {
				if let AppResponse::GossipsubGetInfo { mesh_peers, .. } = result {
					assert_eq!(mesh_peers[0].0, node_1_peer_id);
					assert_eq!(mesh_peers[0].1[0], GOSSIP_NETWORK);
				}
			}
		}
	});
}

#[cfg(feature = "test-listening-node")]
#[test]
fn gossipsub_message_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the node that will be dialled
		let mut node_1 = setup_node_1((49885, 49889)).await;

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

		let stream_id = node_1.send_to_network(gossip_request).await.unwrap();

		if let Ok(result) = node_1.recv_from_network(stream_id).await {
			println!("Subscription successfull");
			assert_eq!(AppResponse::GossipsubJoinSuccess, result);
		}

		loop {}
	});
}

#[cfg(feature = "test-broadcast-node")]
#[test]
fn gossipsub_message_itest_works() {
	tokio::runtime::Runtime::new().unwrap().block_on(async {
		// Set up the second node that will dial
		let (mut node_2, _) = setup_node_2((49885, 49889), (51887, 51888)).await;

		// Join a network (subscribe to a topic)
		let gossip_request = AppData::GossipsubJoinNetwork(GOSSIP_NETWORK.to_string());

		if let Ok(_) = node_2.query_network(gossip_request).await {
			println!("Subscription successfull");

			// Prepare broadcast query
			let gossip_request = AppData::GossipsubBroadcastMessage {
				topic: GOSSIP_NETWORK.to_string(),
				message: vec!["Apple".to_string().into(), "nike".to_string().into()],
			};

			if let Ok(result) = node_2.query_network(gossip_request).await {
				println!("{:?}", result);
			}
		}
	});
}

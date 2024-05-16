//! Node setup and behavor tests.

use super::*;
use futures::TryFutureExt;
use ini::Ini;
use std::fs;
use std::fs::File;
use std::net::{Ipv4Addr, Ipv6Addr};

// set up a default node helper
pub fn setup_core_builder() -> CoreBuilder<DefaultHandler> {
	let config = BootstrapConfig::default();
	let handler = DefaultHandler;

	// return default network core builder
	CoreBuilder::with_config(config, handler)
}

// define custom ports for testing
const CUSTOM_TCP_PORT: Port = 49666;
const CUSTOM_UDP_PORT: Port = 49852;

// used to test saving keypair to file
fn create_test_ini_file(file_path: &str) {
	let mut config = Ini::new();
	config
		.with_section(Some("ports"))
		.set("tcp", CUSTOM_TCP_PORT.to_string())
		.set("udp", CUSTOM_UDP_PORT.to_string());

	config.with_section(Some("bootstrap")).set(
		"boot_nodes",
		"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
	);
	// write config to a new INI file
	config.write_to_file(file_path).unwrap_or_default();
}

#[test]
fn node_default_behavior_works() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// assert that the default network id is '/swarmnl/1.0'
	assert_eq!(default_node.network_id, DEFAULT_NETWORK_ID);

	// default transport is TCP/QUIC
	assert_eq!(
		default_node.transport,
		TransportOpts::TcpQuic {
			tcp_config: TcpConfig::Default
		}
	);

	// default keep alive duration is 60 seconds
	assert_eq!(default_node.keep_alive_duration, 60);

	// default listen on is 0:0:0:0
	assert_eq!(
		default_node.ip_address,
		IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
	);

	// default tcp/udp port is MIN_PORT and MAX_PORT
	assert_eq!(default_node.tcp_udp_port, (MIN_PORT, MAX_PORT));
}

#[test]
fn node_custom_setup_works() {
	// build a node with the default network id
	let default_node = setup_core_builder();

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

	// pass in the custom node configuration and assert it works as expected
	let custom_node = default_node
		.with_network_id(custom_network_id.clone())
		.with_transports(custom_transport.clone())
		.with_idle_connection_timeout(custom_keep_alive_duration.clone())
		.listen_on(custom_ip_address.clone());

	// assert that the custom network id is '/custom/protocol/1.0'
	assert_eq!(custom_node.network_id(), custom_network_id);

	// assert that the custom transport is 'TcpQuic'
	assert_eq!(custom_node.transport, custom_transport);

	// assert that the custom keep alive duration is 20
	assert_eq!(custom_node.keep_alive_duration, custom_keep_alive_duration);
}

#[test]
fn node_custom_behavior_with_network_id_works() {
	// setup a node with the default config builder
	let custom_builder = setup_core_builder();

	// configure builder with custom protocol and assert it works as expected
	let custom_protocol: &str = "/custom-protocol/1.0";
	let custom_builder = custom_builder.with_network_id(custom_protocol.to_string());

	// cannot be less than MIN_NETWORK_ID_LENGTH
	assert_eq!(
		custom_builder.network_id().len() >= MIN_NETWORK_ID_LENGTH.into(),
		true
	);

	// must start with a forward slash
	assert!(custom_builder.network_id().starts_with("/"));

	// assert that the custom network id is '/custom/protocol/1.0'
	assert_eq!(custom_builder.network_id(), custom_protocol.to_string());
}

#[test]
#[should_panic(expected = "Could not parse provided network id")]
fn node_custom_behavior_with_network_id_fails() {
	// build a node with the default network id
	let custom_builder = setup_core_builder();

	// pass in an invalid network ID: network ID length is less than MIN_NETWORK_ID_LENGTH
	let invalid_protocol_1 = "/1.0".to_string();
	let custom_builder = custom_builder.with_network_id(invalid_protocol_1);

	// pass in an invalid network ID: network ID must start with a forward slash
	let invalid_protocol_2 = "1.0".to_string();
	custom_builder.with_network_id(invalid_protocol_2);
}

#[cfg(feature = "tokio-runtime")]
#[test]
fn node_save_keypair_offline_works_tokio() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// use tokio runtime to test async function
	let result = tokio::runtime::Runtime::new().unwrap().block_on(
		default_node
			.build()
			.unwrap_or_else(|_| panic!("Could not build node")),
	);

	// create a saved_keys.ini file
	let file_path_1 = "saved_keys.ini";
	create_test_ini_file(file_path_1);

	// save the keypair to existing file
	let saved_1 = result.save_keypair_offline(&file_path_1);

	// assert that the keypair was saved successfully
	assert_eq!(saved_1, true);

	// test if it works for a file name that does not exist
	let file_path_2 = "test.ini";
	let saved_2 = result.save_keypair_offline(file_path_2);
	assert_eq!(saved_2, true);

	// // clean up
	fs::remove_file(file_path_1).unwrap_or_default();
	fs::remove_file(file_path_2).unwrap_or_default();
}

#[cfg(feature = "async-std-runtime")]
#[test]
fn node_save_keypair_offline_works_async_std() {
	// build a node with the default network id
	let default_node = setup_core_builder();

	// use tokio runtime to test async function
	let result = async_std::task::block_on(
		default_node
			.build()
			.unwrap_or_else(|_| panic!("Could not build node")),
	);

	// make a saved_keys.ini file
	let file_path_1 = "saved_keys.ini";
	create_test_ini_file(file_path_1);

	// save the keypair to existing file
	let saved_1 = result.save_keypair_offline(file_path_1);

	// assert that the keypair was saved successfully
	assert_eq!(saved_1, true);

	// now test if it works for a file name that does not exist
	let file_path_2 = "test.txt";
	let saved_2 = result.save_keypair_offline(file_path_2);

	// assert that the keypair was saved successfully
	assert_eq!(saved_2, true);

	// clean up
	fs::remove_file(file_path_1).unwrap_or_default();
	fs::remove_file(file_path_2).unwrap_or_default();
}

//! Copyright 2024 Algorealm, Inc.
//! Apache 2.0 License

//! Utility helper functions for reading from and writing to `.ini` config files.

use crate::{
	core::{
		replica_cfg::{ReplBufferData, ReplConfigData},
		Core,
	},
	prelude::*,
	setup::BootstrapConfig,
};
use base58::FromBase58;
use ini::Ini;
use libp2p_identity::PeerId;
use rand::{distributions::Alphanumeric, Rng};
use std::{
	collections::HashMap,
	path::Path,
	str::FromStr,
	time::{SystemTime, UNIX_EPOCH},
};

/// Read an INI file containing bootstrap config information.
pub fn read_ini_file(file_path: &str) -> SwarmNlResult<BootstrapConfig> {
	// Read the file from disk
	if let Ok(config) = Ini::load_from_file(file_path) {
		// Get TCP port & UDP port
		let (tcp_port, udp_port) = if let Some(section) = config.section(Some("ports")) {
			(
				section
					.get("tcp")
					.unwrap_or_default()
					.parse::<Port>()
					.unwrap_or_default(),
				section
					.get("udp")
					.unwrap_or_default()
					.parse::<Port>()
					.unwrap_or_default(),
			)
		} else {
			// Fallback to default ports
			(MIN_PORT, MAX_PORT)
		};

		// Try to read the serialized keypair
		// auth section
		let (key_type, mut serialized_keypair) = if let Some(section) = config.section(Some("auth"))
		{
			(
				// Get the preferred key type
				section.get("crypto").unwrap_or_default(),
				// Get serialized keypair
				string_to_vec::<u8>(section.get("protobuf_keypair").unwrap_or_default()),
			)
		} else {
			Default::default()
		};

		// Now, move onto reading the bootnodes if any
		let section = config
			.section(Some("bootstrap"))
			.ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

		// Get the provided bootnodes
		let boot_nodes = string_to_hashmap(section.get("boot_nodes").unwrap_or_default());

		// Now, move onto reading the blacklist if any
		let blacklist = if let Some(section) = config.section(Some("blacklist")) {
			string_to_vec(section.get("blacklist").unwrap_or_default())
		} else {
			Default::default()
		};

		// Now, read replication config data if any
		let replica_network_data = if let Some(section) = config.section(Some("repl")) {
			// Get the configured replica nodes
			parse_replication_data(section.get("replica_nodes").unwrap_or_default())
		} else {
			Default::default()
		};

		let mut bootstrap_data = BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type, &mut serialized_keypair)
			.with_bootnodes(boot_nodes)
			.with_blacklist(blacklist)
			.with_tcp(tcp_port)
			.with_udp(udp_port);

		// Loop to configure replication data if any
		for (network_key, repl_data) in replica_network_data {
			bootstrap_data = bootstrap_data.with_replication(network_key, repl_data);
		}

		Ok(bootstrap_data)
	} else {
		// Return error
		Err(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))
	}
}

/// Write value into config file.
pub fn write_config<T: AsRef<Path> + ?Sized>(
	section: &str,
	key: &str,
	new_value: &str,
	file_path: &T,
) -> bool {
	if let Ok(mut conf) = Ini::load_from_file(file_path) {
		// Set a value:
		conf.set_to(Some(section), key.into(), new_value.into());
		if let Ok(_) = conf.write_to_file(file_path) {
			return true;
		}
	}
	false
}

/// Parse string into a vector.
fn string_to_vec<T: FromStr>(input: &str) -> Vec<T> {
	input
		.trim_matches(|c| c == '[' || c == ']')
		.split(',')
		.filter_map(|s| s.trim().parse::<T>().ok())
		.fold(Vec::new(), |mut acc, item| {
			acc.push(item);
			acc
		})
}

/// Parse string into a hashmap.
fn string_to_hashmap(input: &str) -> HashMap<String, String> {
	input
		.trim_matches(|c| c == '[' || c == ']')
		.split(',')
		.filter(|s| s.contains(':'))
		.fold(HashMap::new(), |mut acc, s| {
			let mut parts = s.trim().splitn(2, ':');
			if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
				if key.len() > 1 {
					acc.insert(key.trim().to_owned(), value.trim().to_owned());
				}
			}
			acc
		})
}

/// Parse replica nodes specified in the `bootstrap_config.ini` config file
fn parse_replication_data(input: &str) -> Vec<(String, ReplConfigData)> {
	let mut result = Vec::new();

	// Remove brackets and split by '@'
	let data = input.trim_matches(|c| c == '[' || c == ']').split('@');

	for section in data {
		if section.is_empty() {
			continue;
		}

		// Split outer identifier and the rest
		if let Some((outer_id, inner_data)) = section.split_once(':') {
			let mut inner_map = HashMap::new();

			// Split each key-value pair
			for entry in inner_data.trim_matches(|c| c == '[' || c == ']').split(',') {
				if let Some((key, value)) = entry.trim().split_once(':') {
					inner_map.insert(key.to_string(), value.to_string());
				}
			}

			// Set up replica network config data
			let cfg = ReplConfigData {
				lamport_clock: 0, // Set clock to 0
				nodes: inner_map, // Replica nodes
			};

			result.push((outer_id.trim().to_string(), cfg));
		}
	}

	result
}

/// Convert a peer ID string to [`PeerId`].
pub fn string_to_peer_id(peer_id_string: &str) -> Option<PeerId> {
	PeerId::from_bytes(&peer_id_string.from_base58().unwrap_or_default()).ok()
}

/// Generate a random string of variable length
pub fn generate_random_string(length: usize) -> String {
	let mut rng = rand::thread_rng();
	(0..length)
		.map(|_| rng.sample(Alphanumeric) as char)
		.collect()
}

/// Unmarshall data recieved as RPC during the execution of the eventual consistency algorithm to
/// fill in missing messages in the node's buffer
pub fn unmarshal_messages(data: Vec<Vec<u8>>) -> Vec<ReplBufferData> {
	let mut result = Vec::new();

	for entry in data {
		let serialized = String::from_utf8_lossy(&entry).to_string();
		let entries: Vec<&str> = serialized.split(Core::ENTRY_DELIMITER).collect();

		for entry in entries {
			let fields: Vec<&str> = entry.split(Core::FIELD_DELIMITER).collect();
			if fields.len() < 6 {
				continue; // Skip malformed entries
			}

			let data_field: Vec<String> = fields[0]
				.split(Core::DATA_DELIMITER)
				.map(|s| s.to_string())
				.collect();
			let lamport_clock = fields[1].parse().unwrap_or(0);
			let outgoing_timestamp = fields[2].parse().unwrap_or(0);
			let incoming_timestamp = fields[3].parse().unwrap_or(0);
			let message_id = fields[4].to_string();
			let sender = fields[5];

			// Parse peerId
			if let Ok(peer_id) = sender.parse::<PeerId>() {
				result.push(ReplBufferData {
					data: data_field,
					lamport_clock,
					outgoing_timestamp,
					incoming_timestamp,
					message_id,
					sender: peer_id,
					confirmations: None, // Since eventual consistency
				});
			}
		}
	}

	result
}

/// Get unix timestamp as string
pub fn get_unix_timestamp() -> Seconds {
	// Get the current system time
	let now = SystemTime::now();
	// Calculate the duration since the Unix epoch
	let duration_since_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
	// Return the Unix timestamp in seconds as a string
	duration_since_epoch.as_secs()
}

#[cfg(test)]
mod tests {

	use libp2p_identity::{KeyType, Keypair};

	use super::*;
	use std::fs;

	// Define custom ports for testing
	const CUSTOM_TCP_PORT: Port = 49666;
	const CUSTOM_UDP_PORT: Port = 49852;

	// Helper to create an INI file without a keypair and a valid range for ports.
	fn create_test_ini_file_without_keypair(file_path: &str) {
		let mut config = Ini::new();
		config
			.with_section(Some("ports"))
			.set("tcp", CUSTOM_TCP_PORT.to_string())
			.set("udp", CUSTOM_UDP_PORT.to_string());

		config.with_section(Some("bootstrap")).set(
			"boot_nodes",
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
		);
		config
			.with_section(Some("blacklist"))
			.set("blacklist", "[]");
		// Write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// Helper to create an INI file with keypair
	fn create_test_ini_file_with_keypair(file_path: &str, key_type: KeyType) {
		let mut config = Ini::new();

		match key_type {
			KeyType::Ed25519 => {
				let keypair_ed25519 = Keypair::generate_ed25519().to_protobuf_encoding().unwrap();
				config
					.with_section(Some("auth"))
					.set("crypto", "ed25519")
					.set("protobuf_keypair", &format!("{:?}", keypair_ed25519));
			},
			KeyType::Secp256k1 => {
				let keypair_secp256k1 = Keypair::generate_secp256k1()
					.to_protobuf_encoding()
					.unwrap();
				config
					.with_section(Some("auth"))
					.set("crypto", "secp256k1")
					.set("protobuf_keypair", &format!("{:?}", keypair_secp256k1));
			},
			KeyType::Ecdsa => {
				let keypair_ecdsa = Keypair::generate_ecdsa().to_protobuf_encoding().unwrap();
				config
					.with_section(Some("auth"))
					.set("crypto", "ecdsa")
					.set("protobuf_keypair", &format!("{:?}", keypair_ecdsa));
			},
			_ => {},
		}

		config.with_section(Some("bootstrap")).set(
			"boot_nodes",
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
		);

		config
			.with_section(Some("blacklist"))
			.set("blacklist", "[]");

		// Write config to the new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// Helper to clean up temp file
	fn clean_up_temp_file(file_path: &str) {
		fs::remove_file(file_path).unwrap_or_default();
	}

	#[test]
	fn file_does_not_exist() {
		// Try to read a non-existent file should panic
		assert_eq!(read_ini_file("non_existent_file.ini").is_err(), true);
	}

	#[test]
	fn write_config_works() {
		// Create temp INI file
		let file_path = "temp_test_write_ini_file.ini";

		// Create INI file without keypair for simplicity
		create_test_ini_file_without_keypair(file_path);

		// Try to write some keypair to the INI file
		let add_keypair = write_config(
			"auth",
			"serialized_keypair",
			&format!(
				"{:?}",
				vec![
					8, 1, 18, 64, 116, 193, 199, 84, 83, 25, 220, 116, 119, 194, 155, 173, 2, 241,
					82, 0, 130, 225, 121, 9, 232, 244, 8, 253, 170, 13, 100, 24, 195, 179, 60, 133,
					128, 221, 43, 214, 180, 33, 61, 73, 124, 161, 127, 119, 40, 146, 226, 50, 65,
					35, 97, 188, 159, 169, 250, 241, 98, 36, 146, 9, 139, 98, 114, 224
				]
			),
			file_path,
		);

		assert_eq!(add_keypair, true);

		// Delete temp file
		clean_up_temp_file(file_path);
	}

	// Read without keypair file
	#[test]
	fn read_ini_file_with_custom_setup_works() {
		// Create temp INI file
		let file_path = "temp_test_read_ini_file_custom.ini";

		// We've set our ports to tcp=49666 and upd=49852
		create_test_ini_file_without_keypair(file_path);

		let ini_file_result: BootstrapConfig = read_ini_file(file_path).unwrap();

		assert_eq!(ini_file_result.ports().0, CUSTOM_TCP_PORT);
		assert_eq!(ini_file_result.ports().1, CUSTOM_UDP_PORT);

		// Checking for the default keypair that's generated (ED25519) if none are provided
		assert_eq!(ini_file_result.keypair().key_type(), KeyType::Ed25519);

		// Delete temp file
		clean_up_temp_file(file_path);
	}

	#[test]
	fn read_ini_file_with_default_setup_works() {
		// Create INI file
		let file_path = "temp_test_ini_file_default.ini";
		create_test_ini_file_with_keypair(file_path, KeyType::Ecdsa);

		// Assert that the content has no [port] section
		// let ini_file_content = fs::read_to_string(file_path).unwrap();
		// assert!(!ini_file_content.contains("[port]"));

		// But when we call read_ini_file it generates a BootstrapConfig with random ports
		let ini_file_result = read_ini_file(file_path).unwrap();

		// assert_eq!(ini_file_result.ports().0, MIN_PORT);
		// assert_eq!(ini_file_result.ports().1, MAX_PORT);

		// Checking that the default keypair matches the configured keytype
		assert_eq!(ini_file_result.keypair().key_type(), KeyType::Ecdsa);

		// Delete temp file
		clean_up_temp_file(file_path);
	}

	#[test]
	fn string_to_vec_works() {
		// Define test input
		let test_input_1 = "[1, 2, 3]";
		let test_input_2 = "[]";

		// Call the function
		let result: Vec<i32> = string_to_vec(test_input_1);
		let result_2: Vec<i32> = string_to_vec(test_input_2);

		// Assert that the result is as expected
		assert_eq!(result, vec![1, 2, 3]);
		assert_eq!(result_2, vec![]);
	}

	#[test]
	fn string_to_hashmap_works() {
		// Define test input
		let input =
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]";

		// Call the function
		let result = string_to_hashmap(input);

		// Assert that the result is as expected
		let mut expected = HashMap::new();
		expected.insert(
			"12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq".to_string(),
			"/ip4/192.168.1.205/tcp/1509".to_string(),
		);

		assert_eq!(result, expected);
	}

	#[test]
	fn bootstrap_config_blacklist_works() {
		let file_path = "bootstrap_config_blacklist_test.ini";

		// Create a new INI file with a blacklist
		let mut config = Ini::new();
		config
			.with_section(Some("ports"))
			.set("tcp", CUSTOM_TCP_PORT.to_string())
			.set("udp", CUSTOM_UDP_PORT.to_string());

		config.with_section(Some("bootstrap")).set(
			"boot_nodes",
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
		);

		let blacklist_peer_id: PeerId = PeerId::random();
		let black_list_peer_id_string = format!("[{}]", blacklist_peer_id.to_base58());

		config
			.with_section(Some("blacklist"))
			.set("blacklist", black_list_peer_id_string);

		// Write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();

		// Read the new file
		let ini_file_result: BootstrapConfig = read_ini_file(file_path).unwrap();

		assert_eq!(ini_file_result.blacklist().list.len(), 1);
		assert!(ini_file_result
			.blacklist()
			.list
			.contains(&blacklist_peer_id));

		fs::remove_file(file_path).unwrap_or_default();
	}
}

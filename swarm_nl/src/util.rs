/// Copyright (c) 2024 Algorealm
///
/// This file is part of the SwarmNl library.
use crate::{prelude::*, setup::BootstrapConfig};
use base58::FromBase58;
use ini::Ini;
use libp2p_identity::PeerId;
use std::{collections::HashMap, str::FromStr};

/// Read an INI file containing bootstrap config information.
pub fn read_ini_file(file_path: &str) -> SwarmNlResult<BootstrapConfig> {
	// read the file from disk
	if let Ok(config) = Ini::load_from_file(file_path) {
		// ports section
		// get TCP port & UDP port
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
			// fallback to default ports
			(MIN_PORT, MAX_PORT)
		};

		// try to read the serialized keypair
		// auth section
		let (key_type, mut serialized_keypair) = if let Some(section) = config.section(Some("auth"))
		{
			(
				// get the preferred key type
				section.get("crypto").unwrap_or_default(),
				// get serialized keypair
				string_to_vec::<u8>(section.get("protobuf_keypair").unwrap_or_default()),
			)
		} else {
			Default::default()
		};

		// now, move onto reading the bootnodes if any
		let section = config
			.section(Some("bootstrap"))
			.ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

		// get the provided bootnodes
		let boot_nodes = string_to_hashmap(section.get("boot_nodes").unwrap_or_default());

		// now, move onto reading the blacklist if any
		let section = config
			.section(Some("blacklist"))
			.ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

		// blacklist
		let blacklist = string_to_vec(section.get("blacklist").unwrap_or_default());

		Ok(BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type, &mut serialized_keypair)
			.with_bootnodes(boot_nodes)
			.with_blacklist(blacklist)
			.with_tcp(tcp_port)
			.with_udp(udp_port))
	} else {
		// return error
		Err(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))
	}
}

/// write value into config file
pub fn write_config(section: &str, key: &str, new_value: &str, file_path: &str) -> bool {
	if let Ok(mut conf) = Ini::load_from_file(file_path) {
		// Set a value:
		conf.set_to(Some(section), key.into(), new_value.into());
		if let Ok(_) = conf.write_to_file(file_path) {
			return true;
		}
	}
	false
}

/// Parse string into a vector
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

/// Parse string into a hashmap
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

/// Convert PeerId string to peerId
pub fn string_to_peer_id(peer_id_string: &str) -> Option<PeerId> {
	PeerId::from_bytes(&peer_id_string.from_base58().unwrap_or_default()).ok()
}

#[cfg(test)]
mod tests {

	use libp2p_identity::{KeyType, Keypair};

	use super::*;
	use crate::prelude::{MAX_PORT, MIN_PORT};
	use std::fs;

	// define custom ports for testing
	const CUSTOM_TCP_PORT: Port = 49666;
	const CUSTOM_UDP_PORT: Port = 49852;

	// helper to create an INI file without a static keypair
	// here we specify a valid range for ports
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
		// write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// helper to create an INI file with keypair
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

		// write config to the new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// helper to clean up temp file
	fn clean_up_temp_file(file_path: &str) {
		fs::remove_file(file_path).unwrap_or_default();
	}

	#[test]
	fn file_does_not_exist() {
		// try to read a non-existent file should panic
		assert_eq!(read_ini_file("non_existent_file.ini").is_err(), true);
	}

	#[test]
	fn write_config_works() {
		// create temp INI file
		let file_path = "temp_test_write_ini_file.ini";

		// create INI file without keypair for simplicity
		create_test_ini_file_without_keypair(file_path);

		// try to write some keypair to the INI file
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

		// delete temp file
		clean_up_temp_file(file_path);
	}

	// read without keypair file
	#[test]
	fn read_ini_file_with_custom_setup_works() {
		// create temp INI file
		let file_path = "temp_test_read_ini_file_custom.ini";

		// we've set our ports to tcp=49666 and upd=49852
		create_test_ini_file_without_keypair(file_path);

		let ini_file_result: BootstrapConfig = read_ini_file(file_path).unwrap();

		assert_eq!(ini_file_result.ports().0, CUSTOM_TCP_PORT);
		assert_eq!(ini_file_result.ports().1, CUSTOM_UDP_PORT);

		// checking for the default keypair that's generated (ED25519) if none are provided
		assert_eq!(ini_file_result.keypair().key_type(), KeyType::Ed25519);

		// delete temp file
		clean_up_temp_file(file_path);
	}

	#[test]
	fn read_ini_file_with_default_setup_works() {
		// create INI file
		let file_path = "temp_test_ini_file_default.ini";
		create_test_ini_file_with_keypair(file_path, KeyType::Ecdsa);

		// assert that the content has no [port] section
		let ini_file_content = fs::read_to_string(file_path).unwrap();
		assert!(!ini_file_content.contains("[port]"));

		// but when we call read_ini_file it generates a BootstrapConfig with default ports from
		// crate::prelude::{MIN_PORT, MAX_PORT}
		let ini_file_result = read_ini_file(file_path).unwrap();

		assert_eq!(ini_file_result.ports().0, MIN_PORT);
		assert_eq!(ini_file_result.ports().1, MAX_PORT);

		// checking that the default keypair matches the configured keytype
		assert_eq!(ini_file_result.keypair().key_type(), KeyType::Ecdsa);

		// delete temp file
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

		let mut config = Ini::new();
		config
			.with_section(Some("ports"))
			.set("tcp", CUSTOM_TCP_PORT.to_string())
			.set("udp", CUSTOM_UDP_PORT.to_string());

		config.with_section(Some("bootstrap")).set(
			"boot_nodes",
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
		);

		let black_list_node = "[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq]";

		let black_list_node_trimmed = black_list_node.trim_matches(|c| c == '[' || c == ']');

		let str_to_peer_id = string_to_peer_id(black_list_node_trimmed).unwrap();

		config
			.with_section(Some("blacklist"))
			.set("blacklist", black_list_node.to_string());

		// write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();

		// read the new file
		let ini_file_result: BootstrapConfig = read_ini_file(file_path).unwrap();

		assert_eq!(ini_file_result.blacklist().list.len(), 1);
		assert!(ini_file_result.blacklist().list.contains(&str_to_peer_id));

		fs::remove_file(file_path).unwrap_or_default();
	}
}

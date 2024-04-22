/// Copyright (c) 2024 Algorealm
///
/// This file is part of the SwarmNl library.
use crate::{prelude::*, setup::BootstrapConfig};
use ini::Ini;
use std::{collections::HashMap, str::FromStr};

/// Read an .ini file containing bootstrap config information.
pub fn read_ini_file(file_path: &str) -> SwarmNlResult<BootstrapConfig> {
	// read the file from disk
	if let Ok(config) = Ini::load_from_file(file_path) {
		// ports section
		let section = config
			.section(Some("ports"))
			.ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

		// get TCP port
		let tcp_port = section
			.get("tcp")
			.unwrap_or_default()
			.parse::<Port>()
			.unwrap_or_default();

		// get UDP port
		let udp_port = section
			.get("udp")
			.unwrap_or_default()
			.parse::<Port>()
			.unwrap_or_default();

        // try to read the serialized keypair
        // auth section
		let (key_type, mut serialized_keypair) = if let Some(section) = config.section(Some("auth"))
		{
			// get the preferred key type
			let key_type = section.get("crypto").unwrap_or_default();

			// get serialized keypair
			let serialized_keypair =
				string_to_vec::<u8>(section.get("protobuf_keypair").unwrap_or_default());
			// get serialized keypair
			let serialized_keypair =
				string_to_vec::<u8>(section.get("protobuf_keypair").unwrap_or_default());

			(key_type, serialized_keypair)
		} else {
			Default::default()
		};

			(key_type, serialized_keypair)
		} else {
			Default::default()
		};

		// Now, move on the read bootnodes if any
		let section = config
			.section(Some("Bootstrap"))
			.ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

		// get the provided bootnodes
		let boot_nodes = string_to_hashmap(section.get("boot_nodes").unwrap_or_default());

		Ok(BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type, &mut serialized_keypair)
			.with_bootnodes(boot_nodes)
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

#[cfg(test)]
mod tests {

	use libp2p_identity::{KeyType, Keypair};

	use super::*;
	use std::fs;

	// Function to create INI file without a static keypair
	fn create_test_ini_file_without_keypair(file_path: &str) {
		let mut config = Ini::new();
		config
			.with_section(Some("ports"))
			.set("tcp", "3500")
			.set("udp", "4300");

		config.with_section(Some("bootstrap")).set(
			"boot_nodes",
			"[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]",
		);
		// write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// Function to create INI file without a static keypair
	fn create_test_ini_file_with_keypair(file_path: &str, key_type: KeyType) {
		let mut config = Ini::new();
		config
			.with_section(Some("ports"))
			.set("tcp", "3500")
			.set("udp", "4300");

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

		// write config to a new INI file
		config.write_to_file(file_path).unwrap_or_default();
	}

	// Function to clean up temp file
	fn clean_up_temp_file(file_path: &str) {
		fs::remove_file(file_path).unwrap_or_default();
	}

	#[test]
	fn file_does_not_exist() {
		assert_eq!(read_ini_file("non_existent_file.ini").is_err(), true);
	}

	#[test]
	fn write_config_works() {
		// create temp INI file
		let mut file_path = "temp_test_ini_file.ini";

		// without keypair
		create_test_ini_file_without_keypair(file_path);

		let add_keypair = write_config(
			"auth",
			"serialized_keypair",
			&format!(
				"{:?}",
				vec![12, 234, 45, 34, 54, 34, 43, 34, 43, 23, 43, 43, 34, 67, 98]
			),
			file_path,
		);

		assert_eq!(add_keypair, true);

		//clean up
		clean_up_temp_file(file_path);

		// with keypair (this also tests generating the keypair from protobuf)
		create_test_ini_file_with_keypair(file_path, KeyType::Ed25519);

		assert_eq!(read_ini_file(file_path).is_ok(), true);
	}

	// TODO: why is this test failing?
	#[test]
	fn read_ini_file_works() {
		// create temp INI file
		let temp_file_path = "temp_test_ini_file.ini";
		create_test_ini_file_without_keypair(temp_file_path);

		let result: BootstrapConfig = read_ini_file(temp_file_path).unwrap();

		assert_eq!(result.ports().0, 3500);
		assert_eq!(result.ports().1, 4300);

		// checking for the default keypair that's generated (ED25519) if none are provided
		assert_eq!(result.keypair().into_inner().unwrap().key_type(), KeyType::Ed25519);

		// // cleanup temp file
		// clean_up_temp_file(temp_file_path);
	}

	#[test]
	fn string_to_vec_works() {
		// Define test input
		let input = "[1, 2, 3]";
		let input_2 = "[]";

		// Call the function
		let result: Vec<i32> = string_to_vec(input);
		let result_2: Vec<i32> = string_to_vec(input_2);

		// Assert that the result is as expected
		assert_eq!(result, vec![1, 2, 3]);
		assert_eq!(result_2, vec![]);
	}

	#[test]
	fn string_to_hashmap_works() {
	
		// Define test input
		let input = "[12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq:/ip4/192.168.1.205/tcp/1509]";

		// Call the function
		let result = string_to_hashmap(input);

		// Assert that the result is as expected
		let mut expected = HashMap::new();
		expected.insert("12D3KooWGfbL6ZNGWqS11MoptH2A7DB1DG6u85FhXBUPXPVkVVRq".to_string(), "/ip4/192.168.1.205/tcp/1509".to_string());
		
		assert_eq!(result, expected);
	}
}

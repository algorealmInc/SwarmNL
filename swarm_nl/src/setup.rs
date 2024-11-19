// Copyright 2024 Algorealm, Inc.
// Apache 2.0 License

//! Data structures and functions to setup a node and configure it for networking.

#![doc = include_str!("../doc/setup/NodeSetup.md")]
use std::rc::Rc;

use crate::core::gossipsub_cfg::Blacklist;
pub use crate::prelude::*;
pub use libp2p_identity::{rsa::Keypair as RsaKeypair, KeyType, Keypair, PeerId};

/// Import the contents of the exported modules into this module.
use super::*;

/// Configuration data required for node bootstrap.
#[derive(Debug)]
pub struct BootstrapConfig {
	/// The port to listen on if using the TCP/IP protocol.
	tcp_port: Port,
	/// The port to listen on if using the UDP or QUIC protocol.
	udp_port: Port,
	/// The Cryptographic Keypair for node identification and message auth.
	keypair: Keypair,
	/// Bootstrap peers.
	boot_nodes: Nodes,
	/// Blacklisted peers
	blacklist: Blacklist,
	/// Configuration data for replication
	replication_cfg: Rc<ReplConfigData>,
}

impl BootstrapConfig {
	/// Read from a bootstrap config file on disk.
	///
	/// # Panics
	///
	/// This function will panic if the file is not found at the specified path.
	pub fn from_file(file_path: &str) -> Self {
		util::read_ini_file(file_path).unwrap()
	}

	/// Return a new `BootstrapConfig` struct populated by default (empty) values.
	///
	/// Must be called first if the config is to be explicitly built without reading `.ini` file
	/// from disk.
	pub fn new() -> Self {
		BootstrapConfig {
			// Default TCP/IP port if not specified.
			tcp_port: MIN_PORT,
			// Default UDP port if not specified.
			udp_port: MAX_PORT,
			// Default node keypair type i.e Ed25519.
			keypair: Keypair::generate_ed25519(),
			boot_nodes: Default::default(),
			// List of blacklisted peers
			blacklist: Default::default(),
			// List containing replication nodes
			replication_cfg: Default::default(),
		}
	}

	/// Configure available bootnodes.
	pub fn with_bootnodes(mut self, boot_nodes: Nodes) -> Self {
		// Additive operation
		self.boot_nodes.extend(boot_nodes.into_iter());
		self
	}

	/// Configure a list of peers to add to blacklist.
	pub fn with_blacklist(mut self, list: Vec<PeerId>) -> Self {
		// additive operation
		self.blacklist.list.extend(list.into_iter());
		self
	}

	/// Configure the TCP/IP port.
	///
	/// Note: Port must range between [`MIN_PORT`] and [`MAX_PORT`].
	pub fn with_tcp(self, tcp_port: Port) -> Self {
		if tcp_port > MIN_PORT && tcp_port < MAX_PORT {
			BootstrapConfig { tcp_port, ..self }
		} else {
			self
		}
	}

	/// Configure the UDP port.
	///
	/// Note: Port must range between [`MIN_PORT`] and [`MAX_PORT`]
	pub fn with_udp(self, udp_port: Port) -> Self {
		if udp_port > MIN_PORT && udp_port < MAX_PORT {
			BootstrapConfig { udp_port, ..self }
		} else {
			self
		}
	}

	/// Configure nodes for replication and add them to bootnodes for early connection
	pub fn with_replication(self, cfg_data: ReplConfigData) -> Self {
		// A connection request must be sent to the replica nodes on startup, so we will add it to
		// our list of bootnodes

		let bootnodes = cfg_data
			.clone()
			.into_iter()
			.flat_map(|outer_map| {
				outer_map
					.into_iter()
					.flat_map(|(_, inner_map)| inner_map.into_iter())
			})
			.collect();

		let node = self.with_bootnodes(bootnodes);

		Self {
			replication_cfg: Rc::new(cfg_data),
			..node
		}
	}

	/// Generate a Cryptographic Keypair for node identity creation and message signing.
	///
	/// An RSA keypair cannot be generated on-the-fly. It has to be generated from a `.pk8` file.
	/// Hence the `rsa_pk8_filepath` parameter must always be set to `None` except in the case of
	/// RSA. Please note that calling this function overrides whatever might have been read from the
	/// `.ini` file
	///
	/// # Panics
	///
	/// This function will panic if:
	///
	/// 1. The RSA key type is specified and the `rsa_pk8_filepath` is set to `None`.
	/// 2. If the file contains invalid data and an RSA keypair cannot be generated from it.
	pub fn generate_keypair(self, key_type: KeyType, rsa_pk8_filepath: Option<&str>) -> Self {
		if rsa_pk8_filepath.is_none() && key_type == KeyType::RSA {
			panic!("RSA keypair specified without a .pk8 file");
		}

		let keypair = match key_type {
			// Generate a Ed25519 Keypair
			KeyType::Ed25519 => Keypair::generate_ed25519(),
			KeyType::RSA => {
				let mut bytes = std::fs::read(rsa_pk8_filepath.unwrap()).unwrap_or_default();
				// Return RSA keypair generated from a .pk8 binary file
				Keypair::rsa_from_pkcs8(&mut bytes).unwrap()
			},
			KeyType::Secp256k1 => Keypair::generate_secp256k1(),
			KeyType::Ecdsa => Keypair::generate_ecdsa(),
		};

		BootstrapConfig { keypair, ..self }
	}

	/// Generate a Cryptographic Keypair from a protobuf format.
	///
	/// This will override any already set keypair.
	///
	/// # Panics
	///
	/// This function will panic if the `u8` buffer is not parsable into the specified key type.
	/// This could be because one of two reasons:
	///
	/// 1. If the key type is valid, but the keypair data is not valid for that key type.
	/// 2. If the key type is invalid.
	pub fn generate_keypair_from_protobuf(self, key_type_str: &str, bytes: &mut [u8]) -> Self {
		// Parse the key type
		if let Some(key_type) = <KeyType as CustomFrom>::from(key_type_str) {
			let raw_keypair = Keypair::from_protobuf_encoding(bytes)
				.expect("Invalid keypair: protobuf bytes not parsable into keypair");

			let keypair = match key_type {
				// Generate a Ed25519 Keypair
				KeyType::Ed25519 => Keypair::try_into_ed25519(raw_keypair).unwrap().into(),
				// Generate a RSA Keypair
				KeyType::RSA => Keypair::rsa_from_pkcs8(bytes).unwrap(),
				// Generate a Secp256k1 Keypair
				KeyType::Secp256k1 => Keypair::try_into_secp256k1(raw_keypair).unwrap().into(),
				// Generate a Ecdsa Keypair
				KeyType::Ecdsa => Keypair::try_into_ecdsa(raw_keypair).unwrap().into(),
			};

			BootstrapConfig { keypair, ..self }
		} else {
			// Generate a default Ed25519 keypair
			BootstrapConfig {
				keypair: Keypair::generate_ed25519(),
				..self
			}
		}
	}

	/// Return a node's cryptographic keypair.
	pub fn keypair(&self) -> Keypair {
		self.keypair.clone()
	}

	/// Return the configured ports in a tuple i.e (TCP Port, UDP port).
	pub fn ports(&self) -> (Port, Port) {
		(self.tcp_port, self.udp_port)
	}

	/// Return the configured bootnodes for the network.
	pub fn bootnodes(&self) -> Nodes {
		self.boot_nodes.clone()
	}

	/// Return the 	`PeerId`'s of nodes that are to be blacklisted.
	pub fn blacklist(&self) -> Blacklist {
		self.blacklist.clone()
	}

	/// Return the configuration data for replication
	pub fn repl_cfg(&self) -> Rc<ReplConfigData> {
		self.replication_cfg.clone()
	}
}

/// [`Default`] implementation for [`BootstrapConfig`].
impl Default for BootstrapConfig {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;
	use std::fs;
	use std::panic;
	use std::process::Command;

	// helper to generate RSA keypair files
	// commands taken from https://docs.rs/libp2p-identity/0.2.8/libp2p_identity/struct.Keypair.html#example-generating-rsa-keys-with-openssl
	fn generate_rsa_keypair_files() {
		// Generate RSA private key
		let genrsa_output = Command::new("openssl")
			.args(&["genrsa", "-out", "private.pem", "2048"])
			.output()
			.expect("Failed to execute openssl genrsa command");

		// Convert private key to PKCS8 format
		let pkcs8_output = Command::new("openssl")
			.args(&[
				"pkcs8",
				"-in",
				"private.pem",
				"-inform",
				"PEM",
				"-topk8",
				"-out",
				"private.pk8",
				"-outform",
				"DER",
				"-nocrypt",
			])
			.output()
			.expect("Failed to execute openssl pkcs8 command");

		// Check command outputs for success or failure
		if genrsa_output.status.success() {
			println!("RSA private key generated successfully");
		} else {
			eprintln!(
				"Failed to generate RSA private key:\n{}",
				String::from_utf8_lossy(&genrsa_output.stderr)
			);
		}

		if pkcs8_output.status.success() {
			println!("RSA private key converted to PKCS8 format successfully");
		} else {
			eprintln!(
				"Failed to convert RSA private key to PKCS8 format:\n{}",
				String::from_utf8_lossy(&pkcs8_output.stderr)
			);
		}
	}

	#[test]
	fn file_read_should_panic() {
		let result = panic::catch_unwind(|| {
			BootstrapConfig::from_file("non_existent_file.ini");
		});
		assert!(result.is_err());
	}

	#[test]
	fn default_config_works() {
		let bootstrap_config = BootstrapConfig::default();

		// Default port values
		assert_eq!(bootstrap_config.tcp_port, MIN_PORT);
		assert_eq!(bootstrap_config.udp_port, MAX_PORT);

		// .. and we know that the default is Ed25519
		let keypair = bootstrap_config.keypair;
		assert_eq!(keypair.key_type(), KeyType::Ed25519);

		// Bootnodes aren't configured by default so we expect an empty HashMap
		assert_eq!(bootstrap_config.boot_nodes, HashMap::new());
	}

	#[test]
	fn new_config_with_bootnodes_works() {
		// Setup test data
		let mut bootnodes: Nodes = HashMap::new();
		let key_1 = "12D3KooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string();
		let val_1 = "/ip4/192.168.1.205/tcp/1509".to_string();
		let key_2 = "12A0ZooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string();
		let val_2 = "/ip4/192.168.1.205/tcp/1588".to_string();
		bootnodes.insert(key_1.clone(), val_1.clone());
		bootnodes.insert(key_2.clone(), val_2.clone());

		// We've inserted two bootnodes
		let bootstrap_config = BootstrapConfig::new().with_bootnodes(bootnodes);
		assert_eq!(bootstrap_config.bootnodes().len(), 2);

		// we can also check that the bootnodes method returns the correct values
		let bootnodes = bootstrap_config.bootnodes();
		assert_eq!(bootnodes.get_key_value(&key_1), Some((&key_1, &val_1)));
		assert_eq!(bootnodes.get_key_value(&key_2), Some((&key_2, &val_2)));
	}

	#[test]
	fn new_config_with_tcp_port_works() {
		// First assert that the default is MIN_PORT
		let bootstrap_config = BootstrapConfig::default();
		assert_eq!(bootstrap_config.ports().0, MIN_PORT);

		// Now set a custom port
		let bootstrap_config_with_tcp = bootstrap_config.with_tcp(49666);
		assert_eq!(bootstrap_config_with_tcp.ports().0, 49666);

		// Now set an invalid port and check it falls back to the default tcp port value
		// Note: MAX_PORT+1 would overflow the u16 type
		let bootstrap_config_invalid_tcp_port = BootstrapConfig::new().with_tcp(MIN_PORT - 42);

		// TCP will always be reset to MIN_PORT if out of bounds
		assert_eq!(bootstrap_config_invalid_tcp_port.ports().0, MIN_PORT);
	}

	#[test]
	fn new_config_with_udp_port_works() {
		// Default should be MAX_PORT
		let bootstrap_config = BootstrapConfig::default();
		assert_eq!(bootstrap_config.ports().1, MAX_PORT);

		// Now set a custom port
		let bootstrap_config_with_udp = bootstrap_config.with_udp(55555);
		assert_eq!(bootstrap_config_with_udp.ports().1, 55555);

		// Now set an invalid port and check it falls back to the default udp port value
		let bootstrap_config_invalid_udp_port = BootstrapConfig::new().with_udp(MIN_PORT - 42);
		assert_eq!(bootstrap_config_invalid_udp_port.ports().1, MAX_PORT);
	}

	#[test]
	fn key_type_is_invalid() {
		// Invalid keytype
		let invalid_keytype = "SomeMagicCryptoType";

		// Valid keypair
		let mut ed25519_serialized_keypair =
			Keypair::generate_ed25519().to_protobuf_encoding().unwrap();

		// Should not panic but default to ed25519
		let result = panic::catch_unwind(move || {
			let bootstrap_config = BootstrapConfig::default()
				.generate_keypair_from_protobuf(invalid_keytype, &mut ed25519_serialized_keypair);

			assert_eq!(bootstrap_config.keypair().key_type(), KeyType::Ed25519);
		});

		assert!(result.is_ok());
	}

	#[test]
	#[should_panic(expected = "Invalid keypair: protobuf bytes not parsable into keypair")]
	fn key_pair_is_invalid() {
		let valid_key_types = ["Ed25519", "RSA", "Secp256k1", "Ecdsa"];
		let mut invalid_keypair: [u8; 2] = [0; 2];

		// Keypair is invalid for each valid key type
		let _ = BootstrapConfig::default()
			.generate_keypair_from_protobuf(valid_key_types[0], &mut invalid_keypair);
		let _ = BootstrapConfig::default()
			.generate_keypair_from_protobuf(valid_key_types[1], &mut invalid_keypair);
		let _ = BootstrapConfig::default()
			.generate_keypair_from_protobuf(valid_key_types[2], &mut invalid_keypair);
		let _ = BootstrapConfig::default()
			.generate_keypair_from_protobuf(valid_key_types[3], &mut invalid_keypair);
	}

	#[test]
	#[should_panic(expected = "RSA keypair specified without a .pk8 file")]
	fn rsa_specified_without_filepath_panics() {
		let bootstrap_config = BootstrapConfig::default();
		let _ = bootstrap_config.generate_keypair(KeyType::RSA, None);
	}

	#[test]
	#[should_panic]
	fn rsa_specified_with_nonexistant_file() {
		let bootstrap_config = BootstrapConfig::default();
		let _ = bootstrap_config.generate_keypair(KeyType::RSA, Some("invalid_rsa_file.pk8"));
	}

	#[test]
	fn rsa_with_invalid_contents_should_panic() {
		// Create an RSA file with invalid contents
		let file_path = "invalid_rsa_keypair_temp_file.pk8";
		let invalid_keypair: [u8; 64] = [0; 64];
		std::fs::write(file_path, invalid_keypair).unwrap();

		let result = panic::catch_unwind(|| {
			// Should panic when parsing invalid RSA file
			let _ = BootstrapConfig::default().generate_keypair(KeyType::RSA, Some(file_path));
		});

		// This will return an error
		assert!(result.is_err());

		// Clean-up invalid_rsa_keypair_temp_file.pk8
		fs::remove_file(file_path).unwrap_or_default();
	}

	#[test]
	fn rsa_from_valid_file_works() {
		// Create a valid private.pk8 file
		generate_rsa_keypair_files();

		let bootstrap_config =
			BootstrapConfig::new().generate_keypair(KeyType::RSA, Some("private.pk8"));

		assert_eq!(bootstrap_config.keypair().key_type(), KeyType::RSA);

		// Clean-up RSA files
		fs::remove_file("private.pk8").unwrap_or_default();
		fs::remove_file("private.pem").unwrap_or_default();
	}

	#[test]
	fn generate_keypair_from_protobuf_ed25519_works() {
		// Generate a valid keypair for ed25519
		let key_type_str = "Ed25519";
		let mut ed25519_serialized_keypair =
			Keypair::generate_ed25519().to_protobuf_encoding().unwrap();

		// Add to bootstrap config from protobuf
		let bootstrap_config = BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type_str, &mut ed25519_serialized_keypair);

		assert_eq!(bootstrap_config.keypair().key_type(), KeyType::Ed25519);
	}

	#[test]
	fn generate_keypair_from_protobuf_ecdsa_works() {
		// Generate a valid keypair for ecdsa
		let key_type_str = "Ecdsa";
		let mut ecdsa_serialized_keypair =
			Keypair::generate_ecdsa().to_protobuf_encoding().unwrap();

		// Add to bootstrap config from protobuf
		let bootstrap_config = BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type_str, &mut ecdsa_serialized_keypair);

		assert_eq!(bootstrap_config.keypair().key_type(), KeyType::Ecdsa);
	}

	#[test]
	fn generate_keypair_from_protobuf_secp256k1_works() {
		// Generate a valid keypair for Secp256k1
		let key_type_str = "Secp256k1";
		let mut secp256k1_serialized_keypair = Keypair::generate_secp256k1()
			.to_protobuf_encoding()
			.unwrap();

		// Add to bootstrap config from protobuf
		let bootstrap_config = BootstrapConfig::new()
			.generate_keypair_from_protobuf(key_type_str, &mut secp256k1_serialized_keypair);

		assert_eq!(bootstrap_config.keypair().key_type(), KeyType::Secp256k1);
	}
}

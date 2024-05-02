/// Copyright (c) 2024 Algorealm

// The module containing the data structures and functions to setup a node identity and
/// configure it for networking.
///
/// This file is part of the SwarmNl library.
use std::collections::HashMap;

/// Import the contents of the exported modules into this module
use super::*;

/// Configuration data required for node bootstrap
#[derive(Debug)]
pub struct BootstrapConfig {
	/// The port to listen on if using the TCP/IP protocol
	tcp_port: Port,
	/// The port to listen on if using the UDP or QUIC protocol
	udp_port: Port,
	/// The Cryptographic Keypair for node identification and message auth
	keypair: Keypair,
	/// Bootstrap peers
	boot_nodes: HashMap<PeerIdString, MultiaddrString>,
}

impl BootstrapConfig {
	/// Read from a bootstrap config file on disk
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
	/// from disk
	pub fn new() -> Self {
		BootstrapConfig {
			// Default TCP/IP port if not specified
			tcp_port: MIN_PORT,
			// Default UDP port if not specified
			udp_port: MAX_PORT,
			// Default node keypair type i.e Ed25519
			keypair: Keypair::generate_ed25519(),
			boot_nodes: Default::default(),
		}
	}

	/// Configure available bootnodes
	pub fn with_bootnodes(mut self, boot_nodes: HashMap<PeerIdString, MultiaddrString>) -> Self {
		// additive operation
		self.boot_nodes.extend(boot_nodes.into_iter());
		self
	}

	/// Configure the TCP/IP port
	/// Port must range between [`MIN_PORT`] and [`MAX_PORT`]
	pub fn with_tcp(self, tcp_port: Port) -> Self {
		if tcp_port > MIN_PORT && tcp_port < MAX_PORT {
			BootstrapConfig { tcp_port, ..self }
		} else {
			self
		}
	}

	/// Configure the UDP port
	/// Port must range between [`MIN_PORT`] and [`MAX_PORT`]
	pub fn with_udp(self, udp_port: Port) -> Self {
		if udp_port > MIN_PORT && udp_port < MAX_PORT {
			BootstrapConfig { udp_port, ..self }
		} else {
			self
		}
	}

	/// Generate a Cryptographic Keypair.
	/// An RSA keypair cannot be generated on-the-fly. It has to be generated from a `.pk8` file.
	/// Hence the `Option` parameter is always `None` except in the case of RSA.
	/// Please note that calling this function overrides whatever might have been read from the
	/// `.ini` file
	///
	/// # Panics (Only applies to the RSA keypair instance)
	///
	/// This function will panic if:
	/// 1. The RSA key type is specified and the `rsa_pk8_filepath` is set to `None`.
	/// 2. If the file contains invalid data and an RSA keypair cannot be generated from it.
	pub fn generate_keypair(self, key_type: KeyType, rsa_pk8_filepath: Option<&str>) -> Self {
		let keypair = match key_type {
			// Generate a Ed25519 Keypair
			KeyType::Ed25519 => Keypair::generate_ed25519(),
			KeyType::RSA => {
				let mut bytes = std::fs::read(rsa_pk8_filepath.unwrap()).unwrap_or_default();
				// return RSA keypair generated from a .pk8 binary file
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
	/// This could be for one of two reasons:
	/// 1. If the key type is valid, but the keypair data is not valid for that key type.
	/// 2. If the key type is invalid.
	pub fn generate_keypair_from_protobuf(self, key_type_str: &str, bytes: &mut [u8]) -> Self {

		// Parse the key type
		if let Some(key_type) = <KeyType as CustomFrom>::from(key_type_str) {	
			let raw_keypair = Keypair::from_protobuf_encoding(bytes).expect("Invalid keypair");

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
			// generate a default Ed25519 keypair
			BootstrapConfig { keypair : Keypair::generate_ed25519(), ..self }
		}
	}

	/// Return a node's cryptographic keypair
	pub fn keypair(&self) -> Keypair {
		self.keypair.clone()
	}

	/// Return the configured ports in a tuple i.e (TCP Port, UDP port)
	pub fn ports(&self) -> (Port, Port) {
		(self.tcp_port, self.udp_port)
	}

	/// Return the configured bootnodes for the network
	pub fn bootnodes(&self) -> HashMap<PeerIdString, MultiaddrString> {
		self.boot_nodes.clone()
	}
}

/// Implement [`Default`] for [`BootstrapConfig`]
impl Default for BootstrapConfig {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use libp2p_identity::ed25519;

	use super::*;
	use std::panic;

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

		// default port values
		assert_eq!(bootstrap_config.tcp_port, MIN_PORT);
		assert_eq!(bootstrap_config.udp_port, MAX_PORT);

		// and we know that the default is Ed25519
		let keypair = bootstrap_config.keypair;
		assert_eq!(keypair.key_type(), KeyType::Ed25519);

		// bootnodes aren't configured by default so we expect an empty HashMap
		assert_eq!(bootstrap_config.boot_nodes, HashMap::new());
	}

	#[test]
	fn new_config_with_bootnodes_works() {
		// setup test data
		let mut bootnodes: HashMap<PeerIdString, MultiaddrString> = HashMap::new();
		let mut key_1 = "12D3KooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string();
		let mut val_1 = "/ip4/192.168.1.205/tcp/1509".to_string();
		let mut key_2 = "12A0ZooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string();
		let mut val_2 = "/ip4/192.168.1.205/tcp/1588".to_string();
		bootnodes.insert(key_1.clone(), val_1.clone());
		bootnodes.insert(key_2.clone(), val_2.clone());

		// we've inserted two bootnodes
		let bootstrap_config = BootstrapConfig::new().with_bootnodes(bootnodes);
		assert_eq!(bootstrap_config.bootnodes().len(), 2);

		// we can also check that the bootnodes method returns the correct values
		let bootnodes = bootstrap_config.bootnodes();
		assert_eq!(bootnodes.get_key_value(&key_1), Some((&key_1, &val_1)));
		assert_eq!(bootnodes.get_key_value(&key_2), Some((&key_2, &val_2)));
	}

	#[test]
	fn new_config_with_tcp_port_works() {
		// first assert that the default is MIN_PORT
		let bootstrap_config = BootstrapConfig::default();
		assert_eq!(bootstrap_config.ports().0, MIN_PORT);

		// now set a custom port
		let bootstrap_config_with_tcp = bootstrap_config.with_tcp(49666);
		assert_eq!(bootstrap_config_with_tcp.ports().0, 49666);

		// now set an invalid port and check it falls back to the default tcp port value
		// Note: MAX_PORT+1 would overflow the u16 type
		let bootstrap_config_invalid_tcp_port = BootstrapConfig::new().with_tcp(MIN_PORT - 42);

		// TCP will always be reset to MIN_PORT if out of bounds
		assert_eq!(bootstrap_config_invalid_tcp_port.ports().0, MIN_PORT);
	}

	#[test]
	fn new_config_with_udp_port_works() {
		// default should be MAX_PORT
		let bootstrap_config = BootstrapConfig::default();
		assert_eq!(bootstrap_config.ports().1, MAX_PORT);

		// now set a custom port
		let bootstrap_config_with_udp = bootstrap_config.with_udp(55555);
		assert_eq!(bootstrap_config_with_udp.ports().1, 55555);

		// now set an invalid port and check it falls back to the default udp port value
		let bootstrap_config_invalid_udp_port = BootstrapConfig::new().with_udp(MIN_PORT - 42);
		assert_eq!(bootstrap_config_invalid_udp_port.ports().1, MAX_PORT);
	}

	#[test]
	fn key_type_is_invalid() {
		let bootstrap_config = BootstrapConfig::default();
		// valid keypair
		let mut ed25519_serialized_keypair = Keypair::generate_ed25519().to_protobuf_encoding().unwrap();

		// should not panic but default to ed25519
		let result = panic::catch_unwind(move || {
			let _ = bootstrap_config
				.generate_keypair_from_protobuf("DejisMagicCryptoType", &mut ed25519_serialized_keypair);
		});

		assert!(result.is_ok());
	}

	#[test]
	#[should_panic(expected = "Invalid keypair")]
	// TODO fix how panic is handled!
	fn key_pair_is_invalid() {
		let valid_key_types = ["Ed25519", "RSA", "Secp256k1", "Ecdsa"];

		valid_key_types
			.iter()
			.map(|key_type| {
				let mut invalid_keypair: [u8; 8] = [0; 8];

				let bootstrap_config = BootstrapConfig::default();

				// should panic with invalid keypair
				let _ = bootstrap_config
					.generate_keypair_from_protobuf(key_type, &mut invalid_keypair);
			});
	}

	#[test]
	fn rsa_should_panic() {

		// TODO
		// rsa_pk8_filepath is set to None
		// - read from the file
		// - filepath is set to None

		// invalid RSA cryptographic file
		// - read from the file
		// -RSA keypair cannot be generated from it
	}

	// #[test]
	// fn test_generate_keypair_from_protobuf_should_fail() {
	// 	// Initialize your test data
	// 	let key_type_str = "Ed25519"; // TODO make this an array with different keytypes to test all
	// 	let mut bytes: [u8; 64] = [0; 64]; // example protobuf bytes

	// 	// create default bootstrap config to test against
	// 	// we know that the default is Ed25519
	// 	let bootstrap_config = BootstrapConfig::new();

	// 	let bootstrap_with_generated_ed25519 =
	// 		bootstrap_config.generate_keypair_from_protobuf(key_type_str, &bytes);

	// 	assert_eq!(generate_ed25519.keypair().key_type(), KeyType::Ed25519);
	// }

	// #[test]
	// #[should_panic(expected = "specified key type")]
	// fn test_generate_keypair_from_protobuf_panic() {
	// 	let key_type_str = "InvalidKeyType";
	// 	let mut bytes: [u8; 64] = [0; 64];
	// 	let bootstrap_config = BootstrapConfig::new();

	// 	// this should panic
	// 	bootstrap_config.generate_keypair_from_protobuf(key_type_str, &mut bytes);
	// }
}

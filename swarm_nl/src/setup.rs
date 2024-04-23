/// The module containing the data structures and functions to setup a node identity and configure
/// it for networking.
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
	/// Must be called first if the config is to be explicitly built without reading `.ini` file from disk
	pub fn new() -> Self {
		BootstrapConfig {
			// Default TCP/IP port if not specified
			tcp_port: 49352,
			// Default UDP port if not specified
			udp_port: 49852,
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
	/// Please note that calling this function overrides whatever might have been read from the `.ini` file
	///
	/// # Panics (Only applies to the RSA keypair instance)
	///
	/// This function will panic if the RSA key type is specified and the `rsa_pk8_filepath` is set to `None`.
	/// It will panic if the file contains invalid data and an RSA keypair cannot be generated from it.
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
	/// This function will panic if the `u8` buffer is not parsable into the specified key type
	pub fn generate_keypair_from_protobuf(self, key_type_str: &str, bytes: &mut [u8]) -> Self {
		// Parse the key type
		let key_type = <KeyType as CustomFrom>::from(key_type_str)
			.ok_or(SwarmNlError::BoostrapDataParseError(
				key_type_str.to_owned(),
			))
			.unwrap();

		let raw_keypair = Keypair::from_protobuf_encoding(bytes).unwrap();
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

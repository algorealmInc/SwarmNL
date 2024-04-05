/// Copyright (c) 2024 Algorealm
use crate::{prelude::*, setup::BootstrapConfig};
use ini::Ini;

/// Read an .ini file containing bootstrap config information
pub fn read_ini_file(file_path: &str) -> SwarmNlResult<BootstrapConfig> {
    // read the file from disk;
    // does not panic but returns a Result<> type because of the `safe` metavariable
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
        let section = config
            .section(Some("auth"))
            .ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

        // get the preferred key type
        let key_type = section.get("crypto").unwrap_or_default();

        // get serialized keypair
        let mut serialized_keypair =
            string_to_vec(section.get("protobuf_keypair").unwrap_or_default());

        Ok(BootstrapConfig::new()
            .generate_keypair_from_protobuf(key_type, &mut serialized_keypair)
            .with_tcp(tcp_port)
            .with_udp(udp_port))
    } else {
        // return error
        Err(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))
    }
}

/// write value into config file
fn write_config(section: &str, key: &str, new_value: &str, file_path: &str) -> bool {
    if let Ok(mut conf) = Ini::load_from_file(file_path) {
        // Set a value:
        conf.set_to(Some(section), key.into(), new_value.into());
        if let Ok(_) = conf.write_to_file(file_path) {
            return true;
        }
    }
    false
}

/// Parse string into a u8 vector
fn string_to_vec(input: &str) -> Vec<u8> {
    input
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

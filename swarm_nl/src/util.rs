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
        let section = config
            .section(Some("auth"))
            .ok_or(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))?;

        // get the preferred key type
        let key_type = section.get("crypto").unwrap_or_default();

        // get serialized keypair
        let mut serialized_keypair =
            string_to_vec::<u8>(section.get("protobuf_keypair").unwrap_or_default());

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
        .trim_matches(|c| c == '{' || c == '}')
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

/// Copyright (c) 2024 Algorealm
use crate::{prelude::*, setup::BootstrapConfig};
use ini::ini;

/// Read an .ini file containing bootstrap config information
pub fn read_ini_file(file_path: &str) -> SwarmNlResult<BootstrapConfig> {
    // read the file from disk;
    // does not panic but returns a Result<> type because of the `safe` metavariable
    if let Ok(config) = ini!(safe file_path) {
        // get TCP port
        let tcp_port = config["ports"]["tcp"]
            .clone()
            .unwrap_or_default()
            .parse::<Port>()
            .unwrap_or_default();

        // get UDP port
        let udp_port = config["ports"]["udp"]
            .clone()
            .unwrap_or_default()
            .parse::<Port>()
            .unwrap_or_default();

        Ok(BootstrapConfig::new().with_tcp(tcp_port).with_udp(udp_port))
    } else {
        // return error
        Err(SwarmNlError::BoostrapFileReadError(file_path.to_owned()))
    }
}

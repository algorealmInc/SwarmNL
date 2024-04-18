/// Copyright (c) 2024 Algorealm

/// This crate is simply for quick-testing the swarmNL library APIs and assisting in developement. It is not the default or de-facto test crate/module, as it is only used
/// in-dev and will be removed subsequently.


use std::collections::HashMap;
use swarm_nl::{PeerIdString, MultiaddrString, StreamData, core::{EventHandler, DefaultHandler}, ListenerId, Multiaddr, StreamExt};

pub static CONFIG_FILE_PATH: &str = "test_config.ini";

/// Complex Event Handler
struct ComplexHandler;

impl EventHandler for ComplexHandler {
    fn new_listen_addr(&mut self, _listener_id: ListenerId, addr: Multiaddr) {
        // Log the address we begin listening on
        println!("We're now listening on: {}", addr);
    }
}

#[tokio::main]
async fn main() {
    // handler for events happening in the network layer (majorly for technical use)
    // use default handler
    let handler = DefaultHandler;
    let complex_handler = ComplexHandler;

    // set up node
    let mut bootnodes: HashMap<PeerIdString, MultiaddrString> = HashMap::new();
    bootnodes.insert("12D3KooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string(), "/ip4/127.0.0.1/tcp/63307".to_string());

    // configure default data
    let config = swarm_nl::setup::BootstrapConfig::new().with_bootnodes(bootnodes);

    // set up network core
    let mut network = swarm_nl::core::CoreBuilder::with_config(config, complex_handler).build().await.unwrap();

    // read first (ready) message
    if let Some(StreamData::Ready) = network.application_receiver.next().await {
        println!("Database is online");

        // begin listening
        loop {
            if let Some(data) = network.application_receiver.next().await {
                println!("{:?}", data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ini::Ini;
    use std::borrow::Cow;

    use crate::CONFIG_FILE_PATH;

    /// try to read/write a byte vector to config file
    #[test]
    fn write_to_ini_file() {
        let test_vec = vec![12, 234, 45, 34, 54, 34, 43, 34, 43, 23, 43, 43, 34, 67, 98];

        // try vec to `.ini` file
        assert!(write_config(
            "auth",
            "serialized_keypair",
            &format!("{:?}", test_vec)
        ));
        assert_eq!(
            read_config("auth", "serialized_keypair"),
            format!("{:?}", test_vec)
        );

        // test that we can read something after it
        assert_eq!(read_config("bio", "name"), "@thewoodfish");
    }

    #[test]
    fn test_conversion_fn() {
        let test_vec = vec![12, 234, 45, 34, 54, 34, 43, 34, 43, 23, 43, 43, 34, 67, 98];

        let vec_string = "[12, 234, 45, 34, 54, 34, 43, 34, 43, 23, 43, 43, 34, 67, 98,]";
        assert_eq!(string_to_vec(vec_string), test_vec);
    }

    /// read value from config file
    fn read_config(section: &str, key: &str) -> Cow<'static, str> {
        if let Ok(conf) = Ini::load_from_file(CONFIG_FILE_PATH) {
            if let Some(section) = conf.section(Some(section)) {
                if let Some(value) = section.get(key) {
                    return Cow::Owned(value.to_owned());
                }
            }
        }

        "".into()
    }

    fn string_to_vec(input: &str) -> Vec<u8> {
        input
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    }

    /// write value into config file
    fn write_config(section: &str, key: &str, new_value: &str) -> bool {
        if let Ok(mut conf) = Ini::load_from_file(CONFIG_FILE_PATH) {
            // Set a value:
            conf.set_to(Some(section), key.into(), new_value.into());
            if let Ok(_) = conf.write_to_file(CONFIG_FILE_PATH) {
                return true;
            }
        }
        false
    }
}

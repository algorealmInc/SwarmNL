/// Copyright (c) 2024 Algorealm

/// This crate is simply for quick-testing the swarmNL library APIs and assisting in
/// developement. It is not the default or de-facto test crate/module, as it is only used
/// in-dev and will be removed subsequently.
use std::collections::HashMap;
use swarm_nl::{
	core::{DefaultHandler, EventHandler},
	ListenerId, Multiaddr, MultiaddrString, PeerIdString, StreamData, StreamExt,
};

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
	bootnodes.insert(
		"12D3KooWBmwXN3rsVfnLsZKbXeBrSLfczHxZHwVjPrbKwpLfYm3t".to_string(),
		"/ip4/127.0.0.1/tcp/63307".to_string(),
	);

	// configure default data
	let config = swarm_nl::setup::BootstrapConfig::new().with_bootnodes(bootnodes);

	// set up network core
	let mut network = swarm_nl::core::CoreBuilder::with_config(config, complex_handler)
		.build()
		.await
		.unwrap();

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

mod age_of_empires {
    use std::{num::NonZeroU32, time::Duration};
    use swarm_nl::{core::EventHandler, PeerId, ConnectionId, ConnectedPoint, Multiaddr, StreamData, Sender, AppData};

    // Rename Sender during re-export to something more custom for its function
    // pub use Sender as StreamComm

	/// The state of the game
	struct Empire {
		soldiers: u32,
		farmers: u32,
		blacksmith: u32,
		land_mass: u32,
        gold_reserve: u32
	}

	/// implement `EventHander` for `Empire` to reponse to network events and make state changes
    impl EventHandler for Empire {
        fn connection_established(
            &mut self,
            peer_id: PeerId,
            _connection_id: ConnectionId,
            _endpoint: &ConnectedPoint,
            _num_established: NonZeroU32,
            _concurrent_dial_errors: Option<Vec<(Multiaddr, TransportError<Error>)>>,
            _established_in: Duration,
            mut application_sender: Sender<StreamData>
        ) {
            // We want to only ally with empires that are richer than us.
            // We we connect, we ask for their reserve and make sure they are richer,
            // Then we collect a fee of 200 gold coins

            // construct keys to send
            let keys = vec!["get_coins".to_string()];

            // Ask the network to get the gold reserve of the empire seeking partnership
            let total_coins_from_peer = application_sender.try_send(StreamData::Application(AppData::FetchData { keys , peer: peer_id }));
            

            // if total_coins_from_peer > self.gold_reserve {
            //     // ask for alignment fee  ReqRes::AskForMergingFee
            //     let m_fee = 100;
                
            //     // add to our gold reserve
            //     self.gold_reserve += m_fee;

            //     // deal complete
            // } else {
            //     // we dont ally with broke empires
            //     // disconnect (This is a network operation)
            // }

        }
    }

	/// Function to run game
	fn start_game() {}

	/// Setup network
	async fn setup_network() {
		// set up a default bootrap config
		let config = swarm_nl::setup::BootstrapConfig::new();

        let spartan_empire = Empire {
            soldiers: 1000,
            farmers: 1000,
            blacksmith: 1000,
            land_mass: 1000,
            gold_reserve: 1000,
        };

		// set up network core
		let mut network = swarm_nl::core::CoreBuilder::with_config(config, spartan_empire)
			.build()
			.await
			.unwrap();

        network.application_sender.try_send(StreamData::ReqRes("gettotalcoins".as_bytes().to_vec())).await;

        let data = network.send_to_network_layer(get_coins(89)).await; //-> 3 seconds

        // Do many task

        let coin = network.recv_from_network_layer(stream_id).await;    // no waiting

        // use coin


        let data = fetch_form_network_layer(Coins).await;

        let coin = fetch_from_network_layer = {
            let data = network.send_to_network_layer(get_coins(89)).await; // -> 3 seconds
            let coin = network.recv_from_network_layer(stream_id).await; // -> 45 seconds
            
            coin
        };


        // TODO! SPECICIFY APPDATA TYPE AND ITS RESPONSE!

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

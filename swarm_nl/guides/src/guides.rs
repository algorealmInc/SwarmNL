//! # Basic Node Setup
//! 
#![doc = docify::embed!("../library/src/setup.rs", default_config_works)]

//! # Gossipsub integration tests
//! 
//! In order to run these tests do xyz
//! ```bash
//! cargo test --features gossipsub
//! ```
#![doc = docify::embed!("../library/src/core/tests/layer_communication.rs", gossipsub_message_itest_works)]


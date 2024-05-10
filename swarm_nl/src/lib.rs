// Copyright 2024 Algorealm
// Apache 2.0 License

#![doc = include_str!("../../README.md")]

/// Re-exports
pub use crate::prelude::*;
pub use futures::{
	channel::mpsc::{self, Receiver, Sender},
	SinkExt, StreamExt,
};
pub use libp2p::{
	core::{transport::ListenerId, ConnectedPoint, Multiaddr},
	swarm::ConnectionId,
};
pub use libp2p_identity::{rsa::Keypair as RsaKeypair, KeyType, Keypair, PeerId};
pub use async_trait::async_trait;

pub mod core;
mod prelude;
pub mod setup;
pub mod util;

/// Copyright (c) 2024 Algorealm

/// This module contains an implementation of the [fetch protocol](https://github.com/libp2p/specs/tree/master/fetch) specified in the original libp2p spec.
/// The fetch protocol is a means for a libp2p node to get data from another, based on a given
/// key. It generally fulfills the contract: `Fetch(key) (value, statusCode)`

/// The fetch protocol is important and useful in performing a simple retrieval. It can be used
/// to augment other protocols e.g `Kademlia`.
/// This module contains an implementation of the libp2p fetch protocol.


// Use re-exports from the crate root
use super::*;

mod handler;
mod behaviour;



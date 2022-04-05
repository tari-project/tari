// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Tari Comms
//!
//! The Tari network messaging library.
//!
//! See [CommsBuilder] for more information on using this library.
//!
//! [CommsBuilder]: ./builder/index.html
#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;

mod builder;
pub use builder::{CommsBuilder, CommsBuilderError, CommsNode, UnspawnedCommsNode};

pub mod connection_manager;
pub use connection_manager::{validate_peer_addresses, PeerConnection, PeerConnectionError};

pub mod connectivity;

pub mod peer_manager;
pub use peer_manager::{NodeIdentity, OrNotFound, PeerManager};

pub mod framing;

pub mod rate_limit;

mod multiplexing;
pub use multiplexing::Substream;

mod noise;
mod proto;
mod stream_id;

pub mod backoff;
pub mod bounded_executor;
pub mod memsocket;
pub mod protocol;
pub mod runtime;
#[macro_use]
pub mod message;
pub mod net_address;
pub mod pipeline;
pub mod socks;
pub mod tor;
pub mod transports;
pub mod types;
#[macro_use]
pub mod utils;

// TODO: Test utils should be part of a `tari_comms_test` crate
// #[cfg(test)]
pub mod test_utils;

//---------------------------------- Re-exports --------------------------------------------//
// Rather than requiring dependent crates to import dependencies for use with `tari_comms` we re-export them here.

pub mod multiaddr {
    // Re-export so that client code does not have to have multiaddr as a dependency
    pub use ::multiaddr::{Error, Multiaddr, Protocol};
}

pub use async_trait::async_trait;
pub use bytes::{Bytes, BytesMut};
#[cfg(feature = "rpc")]
pub use tower::make::MakeService;

//! # Tari Comms
//!
//! The Tari network messaging library.
//!
//! See [CommsBuilder] for more information on using this library.
//!
//! [CommsBuilder]: ./builder/index.html
// Recursion limit for futures::select!
#![recursion_limit = "512"]
// Allow `type Future = impl Future`
#![feature(type_alias_impl_trait)]
// Required to use `Ip4Addr::is_global`. Stabilisation imminent https://github.com/rust-lang/rust/issues/27709
#![feature(ip)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;

pub mod connection_manager;
pub use connection_manager::{validate_peer_addresses, ConnectionManagerEvent, PeerConnection, PeerConnectionError};

pub mod connectivity;

pub mod peer_manager;
pub use peer_manager::{NodeIdentity, PeerManager};

pub mod framing;

mod common;
mod consts;

mod multiplexing;
pub use multiplexing::Substream;

mod noise;
mod proto;
mod runtime;

pub mod backoff;
pub mod bounded_executor;
pub mod compat;
pub mod memsocket;
pub mod protocol;
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

mod builder;
pub use builder::{BuiltCommsNode, CommsBuilder, CommsBuilderError, CommsNode};

// TODO: Test utils should be part of a `tari_comms_test` crate
// #[cfg(test)]
pub mod test_utils;

//---------------------------------- Re-exports --------------------------------------------//
// Rather than requiring dependant crates to import dependencies for use with `tari_comms` we re-export them here.

pub mod multiaddr {
    // Re-export so that client code does not have to have multiaddr as a dependency
    pub use ::multiaddr::{Error, Multiaddr, Protocol};
}

pub use bytes::{Bytes, BytesMut};

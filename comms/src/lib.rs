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

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;

mod connection_manager;
mod consts;
mod multiplexing;
mod noise;
mod proto;
mod protocol;

pub mod backoff;
pub mod bounded_executor;
pub mod compat;
pub mod memsocket;
pub mod peer_manager;
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

// Re-exports
pub use bytes::Bytes;

#[cfg(test)]
pub(crate) mod test_utils;

pub mod multiaddr {
    // Re-export so that client code does not have to have multiaddr as a dependency
    pub use ::multiaddr::{Error, Multiaddr, Protocol};
}

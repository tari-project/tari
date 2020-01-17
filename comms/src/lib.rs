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
#[macro_use]
pub mod message;
#[macro_use]
pub mod utils;

pub mod builder;
#[macro_use]
pub mod connection;
pub mod backoff;
pub mod condvar_shim;
pub mod connection_manager;
mod consts;
pub mod control_service;
pub mod inbound_message_service;
pub mod outbound_message_service;
pub mod peer_manager;
mod proto;
pub mod types;

cfg_next! {
    mod multiplexing;
    mod noise;
    mod socks;
    pub mod memsocket;

    pub mod transports;
}

#[cfg(test)]
pub(crate) mod test_utils;

pub use self::builder::CommsBuilder;

pub mod multiaddr {
    // Re-export so that client code does not have to have multiaddr as a dependency
    pub use ::multiaddr::{Error, Multiaddr, Protocol};
}

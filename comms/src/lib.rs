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
// Required to use `CondVar::wait_timeout_until`
#![feature(wait_timeout_until)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;
#[macro_use]
pub mod message;

pub mod builder;
#[macro_use]
pub mod connection;
pub mod connection_manager;
mod consts;
pub mod control_service;
pub mod inbound_message_service;
mod multiplexing;
mod noise;
pub mod outbound_message_service;
pub mod peer_manager;
mod proto;
mod socks;
pub mod transports;
pub mod types;
pub mod utils;

#[cfg(test)]
pub(crate) mod test_utils;

pub use self::builder::CommsBuilder;

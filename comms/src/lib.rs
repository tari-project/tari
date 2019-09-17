//! # Tari Comms
//!
//! The Tari network messaging library.
//!
//! See [CommsBuilder] for more information on using this library.
//!
//! [CommsBuilder]: ./builder/index.html
// Recursion limit for futures::select!
#![recursion_limit = "256"]
// Allow `type Future = impl Future`
#![feature(type_alias_impl_trait)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;

pub mod builder;
#[macro_use]
pub mod connection;
pub mod connection_manager;
mod consts;
pub mod control_service;
pub mod inbound_message_pipeline;
pub mod message;
pub mod outbound_message_service;
pub mod peer_manager;
pub mod pub_sub_channel;
pub mod reply_channel;
pub mod types;
mod utils;

#[cfg(test)]
pub(crate) mod test_utils;

pub use self::builder::CommsBuilder;

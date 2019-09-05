//! # Tari Comms
//!
//! The Tari network messaging library.
//!
//! See [CommsBuilder] for more information on using this library.
//!
//! [CommsBuilder]: ./builder/index.html
#![feature(checked_duration_since)]

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
pub mod domain_subscriber;
pub mod inbound_message_pipeline;
pub mod message;
pub mod outbound_message_service;
pub mod peer_manager;
pub mod pub_sub_channel;
pub mod types;
mod utils;

pub use self::builder::CommsBuilder;

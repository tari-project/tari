#[macro_use]
extern crate lazy_static;

#[macro_use]
mod macros;

pub mod builder;
#[macro_use]
pub mod connection;
pub mod connection_manager;
pub mod control_service;
pub mod dispatcher;
pub mod domain_connector;
pub mod inbound_message_service;
pub mod message;
pub mod outbound_message_service;
pub mod peer_manager;
pub mod types;
mod utils;

pub use self::{builder::CommsBuilder, domain_connector::Connector};

#![recursion_limit = "256"]
#![feature(drain_filter)]
#![feature(type_alias_impl_trait)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

#[macro_use]
mod macros;
pub mod output_manager_service;
pub mod schema;
pub mod text_message_service;
pub mod transaction_service;
pub mod types;
pub mod wallet;

pub use wallet::Wallet;

#![recursion_limit = "512"]
#![feature(drain_filter)]
#![feature(type_alias_impl_trait)]

#[macro_use]
mod macros;
pub mod contacts_service;
pub mod error;
pub mod output_manager_service;
pub mod storage;
pub mod transaction_service;
pub mod types;
pub mod wallet;

#[cfg(feature = "test_harness")]
pub mod testnet_utils;

pub use wallet::Wallet;

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
pub mod schema;
// pub mod text_message_service;

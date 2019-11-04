#![recursion_limit = "256"]
#![feature(drain_filter)]
#![feature(type_alias_impl_trait)]

#[macro_use]
mod macros;
// pub mod ffi;
pub mod contacts_service;
pub mod output_manager_service;
pub mod transaction_service;
pub mod types;
pub mod wallet;
pub use wallet::Wallet;

// TODO: Put back after MVP
//#[macro_use]
// extern crate diesel;
//#[macro_use]
// extern crate diesel_migrations;
// pub mod schema;
// pub mod text_message_service;

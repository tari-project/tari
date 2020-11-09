#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
mod key_val_store;
pub mod lmdb_store;

pub use key_val_store::{
    key_val_store::IterationResult,
    lmdb_database::LMDBWrapper,
    HashmapDatabase,
    KeyValStoreError,
    KeyValueStore,
};

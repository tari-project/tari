#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
pub mod cipher_seed;
pub mod diacritics;
pub mod error;
pub mod key_manager;
pub mod mnemonic;
pub mod mnemonic_wordlists;
pub mod wasm;

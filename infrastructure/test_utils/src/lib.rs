#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
//! # Tari Test Utilities
//!
//! This crate contains some commonly useful test utilities for testing Tari codebase.
//!
//! ## Modules
//!
//! - `futures` - Contains utilities which make testing future-based code easier
//! - `paths` - Contains utilities which return and create paths which are useful for tests involving files
//! - `random` - Contains utilities to making generating random values easier

pub mod enums;
pub mod futures;
pub mod paths;
pub mod random;
#[macro_use]
pub mod streams;
pub mod runtime;

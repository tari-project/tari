//! # Tari Test Utilities
//!
//! This crate contains some commonly useful test utilities for testing Tari codebase.
//!
//! ## Modules
//!
//! - `futures` - Contains utilities which make testing future-based code easier
//! - `paths` - Contains utilities which return and create paths which are useful for tests involving files
//! - `random` - Contains utilities to making generating random values easier

#[macro_use]
extern crate lazy_static;

pub mod address;
pub mod futures;
pub mod paths;
pub mod random;
#[macro_use]
pub mod streams;
pub mod runtime;

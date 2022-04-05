// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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

// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Lightweight wallet libraries for Tari
//!
//! This crate provides lightweight wallet functionality for the Tari blockchain,
//! including UTXO management, transaction validation, and key management.

pub mod data_structures;
pub mod errors;
pub mod hex_utils;
pub mod validation;
pub mod extraction;
pub mod key_management;

pub use data_structures::*;
pub use errors::*;
pub use hex_utils::*;
pub use validation::*;
pub use extraction::*;
pub use key_management::*; 
// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Lightweight wallet libraries for Tari
//!
//! This crate provides lightweight wallet functionality for the Tari blockchain,
//! including UTXO management, transaction validation, and key management.

pub mod crypto;
pub mod data_structures;
pub mod errors;
pub mod hex_utils;
pub mod validation;
pub mod extraction;
pub mod key_management;
pub mod scanning;
pub mod wallet;

// Include generated GRPC code when the feature is enabled
#[cfg(feature = "grpc")]
pub mod tari_rpc {
    tonic::include_proto!("tari.rpc");
}

pub use errors::*;
pub use hex_utils::*;
pub use validation::*;
pub use extraction::*;
pub use key_management::*;
pub use scanning::*;
pub use wallet::*; 
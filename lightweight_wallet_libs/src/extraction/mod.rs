// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! UTXO extraction and key recovery module for lightweight wallets
//!
//! This module provides functionality to extract and decrypt UTXO data
//! using provided keys, recover wallet outputs from transaction outputs,
//! and handle various payment ID types.

pub mod encrypted_data_decryption;

pub use encrypted_data_decryption::{
    EncryptedDataDecryptor,
    DecryptionResult,
    DecryptionOptions,
}; 
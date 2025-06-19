// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! UTXO validation module for lightweight wallets
//! 
//! This module provides lightweight validation functionality for UTXOs,
//! including range proof validation, signature verification, and commitment integrity checks.

pub mod range_proofs;

pub use range_proofs::*; 
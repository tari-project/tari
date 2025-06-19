// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Validation modules for lightweight wallets
//! 
//! This module provides lightweight validation for various cryptographic components
//! without requiring the full Tari crypto stack.

pub mod range_proofs;
pub mod metadata_signature;
pub mod script_signature;
pub mod commitment;

pub use range_proofs::{
    LightweightBulletProofPlusValidator,
    LightweightRevealedValueValidator,
    RangeProofStatement,
    RangeProofValidationResult,
};

pub use metadata_signature::{
    LightweightMetadataSignatureValidator,
    MetadataSignatureValidationResult,
};

pub use script_signature::{
    LightweightScriptSignatureValidator,
    ScriptSignatureValidationResult,
};

pub use commitment::LightweightCommitmentValidator; 
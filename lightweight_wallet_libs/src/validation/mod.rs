// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Validation module for lightweight wallet functionality
//! 
//! This module provides lightweight validation for UTXOs and transactions
//! without requiring the full Tari crypto stack.

pub mod batch;
pub mod commitment;
pub mod encrypted_data;
pub mod metadata_signature;
pub mod minimum_value_promise;
pub mod range_proofs;
pub mod script_signature;

pub use batch::{
    validate_output_batch,
    BatchValidationResult,
    BatchValidationOptions,
    OutputValidationResult,
    BatchValidationSummary,
};

#[cfg(feature = "parallel")]
pub use batch::validate_output_batch_parallel;

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

pub use encrypted_data::{
    LightweightEncryptedDataValidator,
    EncryptedDataValidationResult,
    validate_encrypted_data_comprehensive,
};

pub use minimum_value_promise::{
    LightweightMinimumValuePromiseValidator,
    MinimumValuePromiseValidationOptions,
    MinimumValuePromiseValidationResult,
}; 
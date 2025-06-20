// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! UTXO extraction and key recovery module for lightweight wallets
//!
//! This module provides functionality to extract and decrypt UTXO data
//! using provided keys, recover wallet outputs from transaction outputs,
//! handle various payment ID types, recover stealth address keys,
//! extract and validate range proofs, and handle special outputs like
//! coinbase and burn outputs appropriately.

pub mod encrypted_data_decryption;
pub mod payment_id_extraction;
pub mod wallet_output_reconstruction;
pub mod stealth_address_key_recovery;
pub mod range_proof_extraction;
pub mod special_output_handling;
pub mod corruption_detection;

pub use encrypted_data_decryption::{
    EncryptedDataDecryptor,
    DecryptionResult,
    DecryptionOptions,
};

pub use payment_id_extraction::{
    PaymentIdExtractor,
    PaymentIdExtractionResult,
    PaymentIdMetadata,
    PaymentIdType,
};

pub use wallet_output_reconstruction::{
    WalletOutputReconstructor,
    WalletOutputReconstructionResult,
    WalletOutputReconstructionOptions,
    WalletOutputReconstructionError,
};

pub use stealth_address_key_recovery::{
    StealthKeyRecoveryManager,
    StealthKeyRecoveryResult,
    StealthKeyRecoveryOptions,
    StealthKeyRecoveryError,
};

pub use range_proof_extraction::{
    RangeProofExtractor,
    RangeProofExtractionResult,
    RangeProofType,
};

pub use special_output_handling::{
    SpecialOutputHandler,
    SpecialOutputHandlingResult,
    SpecialOutputType,
}; 
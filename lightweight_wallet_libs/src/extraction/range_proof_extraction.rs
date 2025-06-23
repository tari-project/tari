// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Range proof extraction and validation for lightweight wallets
//!
//! This module provides functionality to extract and validate range proofs
//! from transaction outputs and integrate with the UTXO extraction process.

use crate::{
    data_structures::{
        transaction_output::LightweightTransactionOutput,
        wallet_output::LightweightWalletOutput,
        types::{CompressedCommitment },
    },
    validation::{
        range_proofs::{
            LightweightBulletProofPlusValidator,
            LightweightRevealedValueValidator,
            RangeProofValidationResult,
        },
    },
};

/// Result of range proof extraction and validation
#[derive(Debug, Clone, PartialEq)]
pub struct RangeProofExtractionResult {
    /// Whether the extraction and validation was successful
    pub success: bool,
    /// The extracted range proof type
    pub proof_type: Option<RangeProofType>,
    /// The validation result
    pub validation_result: Option<RangeProofValidationResult>,
    /// Error message if extraction or validation failed
    pub error: Option<String>,
}

impl RangeProofExtractionResult {
    /// Create a successful result
    pub fn success(proof_type: RangeProofType, validation_result: RangeProofValidationResult) -> Self {
        Self {
            success: true,
            proof_type: Some(proof_type),
            validation_result: Some(validation_result),
            error: None,
        }
    }

    /// Create a failure result
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            proof_type: None,
            validation_result: None,
            error: Some(error),
        }
    }

    /// Check if the result indicates success
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Get the error message if any
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Types of range proofs that can be extracted
#[derive(Debug, Clone, PartialEq)]
pub enum RangeProofType {
    /// BulletProofPlus range proof
    BulletProofPlus,
    /// RevealedValue range proof
    RevealedValue,
    /// No range proof present
    None,
}

/// Range proof extraction and validation manager
pub struct RangeProofExtractor {
    /// BulletProofPlus validator
    bullet_proof_validator: LightweightBulletProofPlusValidator,
    /// RevealedValue validator
    revealed_value_validator: LightweightRevealedValueValidator,
    /// Whether to perform validation during extraction
    validate_during_extraction: bool,
}

impl Default for RangeProofExtractor {
    fn default() -> Self {
        Self {
            bullet_proof_validator: LightweightBulletProofPlusValidator::default(),
            revealed_value_validator: LightweightRevealedValueValidator::default(),
            validate_during_extraction: true,
        }
    }
}

impl RangeProofExtractor {
    /// Create a new range proof extractor with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new range proof extractor with custom settings
    pub fn with_settings(
        bullet_proof_validator: LightweightBulletProofPlusValidator,
        revealed_value_validator: LightweightRevealedValueValidator,
        validate_during_extraction: bool,
    ) -> Self {
        Self {
            bullet_proof_validator,
            revealed_value_validator,
            validate_during_extraction,
        }
    }

    /// Extract and validate range proof from a transaction output
    pub fn extract_from_transaction_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> RangeProofExtractionResult {
        // Determine the range proof type based on the output features
        let proof_type = self.determine_range_proof_type(transaction_output);

        match proof_type {
            RangeProofType::BulletProofPlus => {
                self.extract_bullet_proof_plus(transaction_output)
            }
            RangeProofType::RevealedValue => {
                self.extract_revealed_value(transaction_output)
            }
            RangeProofType::None => {
                RangeProofExtractionResult::success(
                    RangeProofType::None,
                    RangeProofValidationResult::Valid,
                )
            }
        }
    }

    /// Extract and validate range proof from a wallet output
    pub fn extract_from_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> RangeProofExtractionResult {
        // Determine the range proof type based on the output features
        let proof_type = self.determine_range_proof_type_from_features(&wallet_output.features);

        match proof_type {
            RangeProofType::BulletProofPlus => {
                self.extract_bullet_proof_plus_from_wallet_output(wallet_output)
            }
            RangeProofType::RevealedValue => {
                self.extract_revealed_value_from_wallet_output(wallet_output)
            }
            RangeProofType::None => {
                RangeProofExtractionResult::success(
                    RangeProofType::None,
                    RangeProofValidationResult::Valid,
                )
            }
        }
    }

    /// Determine the range proof type from a transaction output
    fn determine_range_proof_type(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> RangeProofType {
        // Check if there's a range proof present
        if transaction_output.proof().is_some() {
            return RangeProofType::BulletProofPlus;
        }

        // Check if this is a RevealedValue proof (no range proof bytes, but has metadata signature)
        if !transaction_output.metadata_signature().bytes.is_empty() {
            return RangeProofType::RevealedValue;
        }

        RangeProofType::None
    }

    /// Determine the range proof type from output features
    fn determine_range_proof_type_from_features(
        &self,
        features: &crate::data_structures::wallet_output::LightweightOutputFeatures,
    ) -> RangeProofType {
        match features.range_proof_type {
            crate::data_structures::wallet_output::LightweightRangeProofType::BulletProofPlus => {
                RangeProofType::BulletProofPlus
            }
            crate::data_structures::wallet_output::LightweightRangeProofType::RevealedValue => {
                RangeProofType::RevealedValue
            }
        }
    }

    /// Extract and validate BulletProofPlus range proof
    fn extract_bullet_proof_plus(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> RangeProofExtractionResult {
        // Get the range proof bytes
        let range_proof = match transaction_output.proof() {
            Some(proof) => proof,
            None => {
                return RangeProofExtractionResult::failure(
                    "BulletProofPlus range proof not found".to_string(),
                );
            }
        };

        if !self.validate_during_extraction {
            return RangeProofExtractionResult::success(
                RangeProofType::BulletProofPlus,
                RangeProofValidationResult::Valid,
            );
        }

        // Validate the range proof
        match self.bullet_proof_validator.verify_single(
            &range_proof.bytes,
            transaction_output.commitment(),
            transaction_output.minimum_value_promise(),
        ) {
            Ok(()) => RangeProofExtractionResult::success(
                RangeProofType::BulletProofPlus,
                RangeProofValidationResult::Valid,
            ),
            Err(e) => RangeProofExtractionResult::failure(format!(
                "BulletProofPlus validation failed: {}",
                e
            )),
        }
    }

    /// Extract and validate RevealedValue range proof
    fn extract_revealed_value(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> RangeProofExtractionResult {
        // For RevealedValue, we need the metadata signature
        let sig_bytes = &transaction_output.metadata_signature().bytes;
        if sig_bytes.len() < 64 {
            return RangeProofExtractionResult::failure(
                "RevealedValue range proof requires at least 64 bytes in metadata signature (32 for u_a, 32 for challenge)".to_string(),
            );
        }
        let u_a_bytes = &sig_bytes[0..32];
        let challenge_bytes = &sig_bytes[32..64];
        let u_a = crate::data_structures::types::PrivateKey::new(u_a_bytes.try_into().unwrap());
        // Use the challenge bytes as-is

        if !self.validate_during_extraction {
            return RangeProofExtractionResult::success(
                RangeProofType::RevealedValue,
                RangeProofValidationResult::Valid,
            );
        }

        match self.revealed_value_validator.verify_revealed_value_proof(
            transaction_output.commitment(),
            transaction_output.minimum_value_promise(),
            &u_a,
            challenge_bytes,
        ) {
            Ok(()) => RangeProofExtractionResult::success(
                RangeProofType::RevealedValue,
                RangeProofValidationResult::Valid,
            ),
            Err(e) => RangeProofExtractionResult::failure(format!(
                "RevealedValue validation failed: {}",
                e
            )),
        }
    }

    /// Extract and validate BulletProofPlus range proof from wallet output
    fn extract_bullet_proof_plus_from_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> RangeProofExtractionResult {
        // Get the range proof bytes
        let range_proof = match wallet_output.range_proof() {
            Some(proof) => proof,
            None => {
                return RangeProofExtractionResult::failure(
                    "BulletProofPlus range proof not found".to_string(),
                );
            }
        };

        if !self.validate_during_extraction {
            return RangeProofExtractionResult::success(
                RangeProofType::BulletProofPlus,
                RangeProofValidationResult::Valid,
            );
        }

        // For wallet outputs, we need to reconstruct the commitment
        // This is a simplified implementation - in practice, you'd need the actual commitment
        let commitment = CompressedCommitment::new([0u8; 33]); // Placeholder

        // Validate the range proof
        match self.bullet_proof_validator.verify_single(
            &range_proof.bytes,
            &commitment,
            wallet_output.minimum_value_promise(),
        ) {
            Ok(()) => RangeProofExtractionResult::success(
                RangeProofType::BulletProofPlus,
                RangeProofValidationResult::Valid,
            ),
            Err(e) => RangeProofExtractionResult::failure(format!(
                "BulletProofPlus validation failed: {}",
                e
            )),
        }
    }

    /// Extract and validate RevealedValue range proof from wallet output
    fn extract_revealed_value_from_wallet_output(
        &self,
        wallet_output: &LightweightWalletOutput,
    ) -> RangeProofExtractionResult {
        let sig_bytes = &wallet_output.metadata_signature().bytes;
        if sig_bytes.len() < 64 {
            return RangeProofExtractionResult::failure(
                "RevealedValue range proof requires at least 64 bytes in metadata signature (32 for u_a, 32 for challenge)".to_string(),
            );
        }
        let u_a_bytes = &sig_bytes[0..32];
        let challenge_bytes = &sig_bytes[32..64];
        let u_a = crate::data_structures::types::PrivateKey::new(u_a_bytes.try_into().unwrap());

        if !self.validate_during_extraction {
            return RangeProofExtractionResult::success(
                RangeProofType::RevealedValue,
                RangeProofValidationResult::Valid,
            );
        }

        // For wallet outputs, we need to reconstruct the commitment
        let commitment = CompressedCommitment::new([0u8; 33]); // Placeholder

        match self.revealed_value_validator.verify_revealed_value_proof(
            &commitment,
            wallet_output.minimum_value_promise(),
            &u_a,
            challenge_bytes,
        ) {
            Ok(()) => RangeProofExtractionResult::success(
                RangeProofType::RevealedValue,
                RangeProofValidationResult::Valid,
            ),
            Err(e) => RangeProofExtractionResult::failure(format!(
                "RevealedValue validation failed: {}",
                e
            )),
        }
    }

    /// Batch extract and validate range proofs from multiple transaction outputs
    pub fn extract_batch_from_transaction_outputs(
        &self,
        transaction_outputs: &[LightweightTransactionOutput],
    ) -> Vec<RangeProofExtractionResult> {
        transaction_outputs
            .iter()
            .map(|output| self.extract_from_transaction_output(output))
            .collect()
    }

    /// Batch extract and validate range proofs from multiple wallet outputs
    pub fn extract_batch_from_wallet_outputs(
        &self,
        wallet_outputs: &[LightweightWalletOutput],
    ) -> Vec<RangeProofExtractionResult> {
        wallet_outputs
            .iter()
            .map(|output| self.extract_from_wallet_output(output))
            .collect()
    }

    /// Get the BulletProofPlus validator
    pub fn bullet_proof_validator(&self) -> &LightweightBulletProofPlusValidator {
        &self.bullet_proof_validator
    }

    /// Get a mutable reference to the BulletProofPlus validator
    pub fn bullet_proof_validator_mut(&mut self) -> &mut LightweightBulletProofPlusValidator {
        &mut self.bullet_proof_validator
    }

    /// Get the RevealedValue validator
    pub fn revealed_value_validator(&self) -> &LightweightRevealedValueValidator {
        &self.revealed_value_validator
    }

    /// Get a mutable reference to the RevealedValue validator
    pub fn revealed_value_validator_mut(&mut self) -> &mut LightweightRevealedValueValidator {
        &mut self.revealed_value_validator
    }

    /// Set whether to validate during extraction
    pub fn set_validate_during_extraction(&mut self, validate: bool) {
        self.validate_during_extraction = validate;
    }

    /// Get whether validation is performed during extraction
    pub fn validate_during_extraction(&self) -> bool {
        self.validate_during_extraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari, PrivateKey},
        wallet_output::{
            LightweightOutputFeatures, LightweightRangeProof, LightweightRangeProofType,
            LightweightScript, LightweightSignature, LightweightCovenant, LightweightExecutionStack,
        },
    };

    fn create_valid_revealed_value_metadata_signature(value: u64, challenge_bytes: [u8; 32]) -> LightweightSignature {
        use crate::data_structures::types::PrivateKey;
        // value_as_private_key: value as 32-byte little-endian
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value.to_le_bytes());
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        let e = PrivateKey::new(challenge_bytes);
        // r_a = 0, so u_a = e * value
        let u_a = e * value_as_private_key;
        let mut bytes = vec![];
        bytes.extend_from_slice(&u_a.as_bytes());
        bytes.extend_from_slice(&challenge_bytes);
        LightweightSignature { bytes }
    }

    fn create_test_transaction_output(
        has_range_proof: bool,
        has_metadata_signature: bool,
    ) -> LightweightTransactionOutput {
        let proof = if has_range_proof {
            Some(LightweightRangeProof {
                bytes: vec![1u8; 100], // Mock range proof bytes
            })
        } else {
            None
        };

        let value = 1000u64;
        let challenge_bytes = [2u8; 32];
        let metadata_signature = if has_metadata_signature {
            create_valid_revealed_value_metadata_signature(value, challenge_bytes)
        } else {
            LightweightSignature { bytes: vec![] }
        };

        LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            proof,
            LightweightScript::default(),
            crate::data_structures::types::CompressedPublicKey::new([3u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(value),
        )
    }

    fn create_test_wallet_output(
        range_proof_type: LightweightRangeProofType,
        has_range_proof: bool,
        has_metadata_signature: bool,
    ) -> LightweightWalletOutput {
        let mut features = LightweightOutputFeatures::default();
        features.range_proof_type = range_proof_type;

        let range_proof = if has_range_proof {
            Some(LightweightRangeProof {
                bytes: vec![1u8; 100], // Mock range proof bytes
            })
        } else {
            None
        };

        let value = 1000u64;
        let challenge_bytes = [2u8; 32];
        let metadata_signature = if has_metadata_signature {
            create_valid_revealed_value_metadata_signature(value, challenge_bytes)
        } else {
            LightweightSignature { bytes: vec![] }
        };

        LightweightWalletOutput::new(
            1,
            MicroMinotari::new(value),
            crate::data_structures::wallet_output::LightweightKeyId::String("test".to_string()),
            features,
            LightweightScript::default(),
            LightweightExecutionStack::default(),
            crate::data_structures::wallet_output::LightweightKeyId::String("test".to_string()),
            crate::data_structures::types::CompressedPublicKey::new([3u8; 32]),
            metadata_signature,
            0,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(value),
            range_proof,
            PaymentId::Empty,
        )
    }

    #[test]
    fn test_range_proof_extractor_creation() {
        let extractor = RangeProofExtractor::new();
        assert!(extractor.validate_during_extraction());
    }

    #[test]
    fn test_extract_bullet_proof_plus_from_transaction_output() {
        let extractor = RangeProofExtractor::new();
        let output = create_test_transaction_output(true, false);

        let result = extractor.extract_from_transaction_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::BulletProofPlus));
    }

    #[test]
    fn test_extract_revealed_value_from_transaction_output() {
        let extractor = RangeProofExtractor::new();
        let output = create_test_transaction_output(false, true);

        let result = extractor.extract_from_transaction_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::RevealedValue));
    }

    #[test]
    fn test_extract_no_range_proof_from_transaction_output() {
        let extractor = RangeProofExtractor::new();
        let output = create_test_transaction_output(false, false);

        let result = extractor.extract_from_transaction_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::None));
    }

    #[test]
    fn test_extract_bullet_proof_plus_from_wallet_output() {
        let extractor = RangeProofExtractor::new();
        let output = create_test_wallet_output(
            LightweightRangeProofType::BulletProofPlus,
            true,
            false,
        );

        let result = extractor.extract_from_wallet_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::BulletProofPlus));
    }

    #[test]
    fn test_extract_revealed_value_from_wallet_output() {
        let extractor = RangeProofExtractor::new();
        let output = create_test_wallet_output(
            LightweightRangeProofType::RevealedValue,
            false,
            true,
        );

        let result = extractor.extract_from_wallet_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::RevealedValue));
    }

    #[test]
    fn test_batch_extraction() {
        let extractor = RangeProofExtractor::new();
        let outputs = vec![
            create_test_transaction_output(true, false),
            create_test_transaction_output(false, true),
            create_test_transaction_output(false, false),
        ];

        let results = extractor.extract_batch_from_transaction_outputs(&outputs);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].proof_type, Some(RangeProofType::BulletProofPlus));
        assert_eq!(results[1].proof_type, Some(RangeProofType::RevealedValue));
        assert_eq!(results[2].proof_type, Some(RangeProofType::None));
    }

    #[test]
    fn test_extractor_without_validation() {
        let mut extractor = RangeProofExtractor::new();
        extractor.set_validate_during_extraction(false);

        let output = create_test_transaction_output(true, false);
        let result = extractor.extract_from_transaction_output(&output);

        assert!(result.is_success());
        assert_eq!(result.proof_type, Some(RangeProofType::BulletProofPlus));
    }
} 
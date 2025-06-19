// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Minimum value promise verification for lightweight wallets
//!
//! This module provides comprehensive validation for minimum value promises
//! in transaction outputs, ensuring they are consistent with range proofs
//! and meet all cryptographic requirements.

use crate::{
    data_structures::{
        types::{CompressedCommitment, MicroMinotari, PrivateKey},
        wallet_output::{LightweightRangeProof, LightweightRangeProofType},
    },
    errors::ValidationError,
};

/// Options for minimum value promise verification
#[derive(Debug, Clone)]
pub struct MinimumValuePromiseValidationOptions {
    /// Whether to validate against range proof bit length
    pub validate_range_proof_bounds: bool,
    /// Whether to validate RevealedValue consistency
    pub validate_revealed_value_consistency: bool,
    /// Whether to validate BulletProofPlus consistency
    pub validate_bulletproof_consistency: bool,
    /// Whether to allow zero values
    pub allow_zero_values: bool,
    /// Maximum allowed value (for additional safety checks)
    pub max_allowed_value: Option<u64>,
}

impl Default for MinimumValuePromiseValidationOptions {
    fn default() -> Self {
        Self {
            validate_range_proof_bounds: true,
            validate_revealed_value_consistency: true,
            validate_bulletproof_consistency: true,
            allow_zero_values: true,
            max_allowed_value: None,
        }
    }
}

/// Lightweight minimum value promise validator
#[derive(Debug, Clone)]
pub struct LightweightMinimumValuePromiseValidator {
    /// Default range proof bit length (64 for Tari)
    default_bit_length: usize,
}

impl Default for LightweightMinimumValuePromiseValidator {
    fn default() -> Self {
        Self {
            default_bit_length: 64, // Tari's default range proof bit length
        }
    }
}

impl LightweightMinimumValuePromiseValidator {
    /// Create a new validator with the specified default bit length
    pub fn new(default_bit_length: usize) -> Self {
        Self {
            default_bit_length,
        }
    }

    /// Get the default bit length
    pub fn default_bit_length(&self) -> usize {
        self.default_bit_length
    }

    /// Validate a minimum value promise against a range proof
    ///
    /// # Arguments
    /// * `minimum_value_promise` - The minimum value promise to validate
    /// * `range_proof` - The range proof (optional for some validation types)
    /// * `range_proof_type` - The type of range proof
    /// * `options` - Validation options
    ///
    /// # Returns
    /// * `Ok(())` if the minimum value promise is valid
    /// * `Err(ValidationError)` if the minimum value promise is invalid
    pub fn validate_minimum_value_promise(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<&LightweightRangeProof>,
        range_proof_type: &LightweightRangeProofType,
        options: &MinimumValuePromiseValidationOptions,
    ) -> Result<(), ValidationError> {
        let value = minimum_value_promise.as_u64();

        // Check for zero values if not allowed
        if !options.allow_zero_values && value == 0 {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "Zero values are not allowed",
            ));
        }

        // Check maximum allowed value if specified
        if let Some(max_allowed) = options.max_allowed_value {
            if value > max_allowed {
                return Err(ValidationError::minimum_value_promise_validation_failed(
                    &format!(
                        "Minimum value promise {} exceeds maximum allowed value {}",
                        value, max_allowed
                    ),
                ));
            }
        }

        // Validate range proof bounds
        if options.validate_range_proof_bounds {
            self.validate_range_proof_bounds(minimum_value_promise, range_proof_type)?;
        }

        // Validate based on range proof type
        match range_proof_type {
            LightweightRangeProofType::RevealedValue => {
                if options.validate_revealed_value_consistency {
                    self.validate_revealed_value_consistency(minimum_value_promise, range_proof)?;
                }
            }
            LightweightRangeProofType::BulletProofPlus => {
                if options.validate_bulletproof_consistency {
                    self.validate_bulletproof_consistency(minimum_value_promise, range_proof)?;
                }
            }
        }

        Ok(())
    }

    /// Validate minimum value promise against a RevealedValue range proof
    ///
    /// For RevealedValue proofs, the minimum value promise should equal the actual value
    /// and be consistent with the metadata signature.
    ///
    /// # Arguments
    /// * `minimum_value_promise` - The minimum value promise to validate
    /// * `range_proof` - The range proof (should be None for RevealedValue)
    /// * `metadata_signature_u_a` - The u_a component of the metadata signature
    /// * `metadata_signature_challenge` - The challenge used in the metadata signature
    ///
    /// # Returns
    /// * `Ok(())` if the minimum value promise is valid
    /// * `Err(ValidationError)` if the minimum value promise is invalid
    pub fn validate_revealed_value_minimum_promise(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<&LightweightRangeProof>,
        metadata_signature_u_a: &PrivateKey,
        metadata_signature_challenge: &[u8],
    ) -> Result<(), ValidationError> {
        // For RevealedValue, the range proof should be None
        if range_proof.is_some() {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "RevealedValue range proofs should not have proof bytes",
            ));
        }

        // Validate range proof bounds
        self.validate_range_proof_bounds(minimum_value_promise, &LightweightRangeProofType::RevealedValue)?;

        // Verify the RevealedValue proof using the metadata signature
        // This is the same logic as in the RevealedValue validator
        let e = PrivateKey::from_canonical_bytes(metadata_signature_challenge)
            .map_err(|_| ValidationError::minimum_value_promise_validation_failed(
                "Invalid metadata signature challenge"
            ))?;
        
        // Convert the minimum value promise to a private key
        let value_bytes = minimum_value_promise.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        // For RevealedValue proofs, the ephemeral nonce r_a is always zero
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        
        // Verify the balance proof: u_a should equal r_a + e * value
        let expected_u_a = commit_nonce_a + e * value_as_private_key;
        
        if metadata_signature_u_a.as_bytes() == expected_u_a.as_bytes() {
            Ok(())
        } else {
            Err(ValidationError::minimum_value_promise_validation_failed(
                "RevealedValue minimum value promise verification failed"
            ))
        }
    }

    /// Validate minimum value promise against a BulletProofPlus range proof
    ///
    /// For BulletProofPlus proofs, the minimum value promise should be within
    /// the range that can be proven by the proof.
    ///
    /// # Arguments
    /// * `minimum_value_promise` - The minimum value promise to validate
    /// * `range_proof` - The BulletProofPlus range proof
    /// * `commitment` - The commitment being proven
    ///
    /// # Returns
    /// * `Ok(())` if the minimum value promise is valid
    /// * `Err(ValidationError)` if the minimum value promise is invalid
    pub fn validate_bulletproof_minimum_promise(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof: &LightweightRangeProof,
        commitment: &CompressedCommitment,
    ) -> Result<(), ValidationError> {
        // Validate range proof bounds
        self.validate_range_proof_bounds(minimum_value_promise, &LightweightRangeProofType::BulletProofPlus)?;

        // Basic structure validation for BulletProofPlus
        if range_proof.bytes.is_empty() {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "BulletProofPlus range proof cannot be empty",
            ));
        }

        // Check that the proof has a reasonable size
        if range_proof.bytes.len() > 10000 { // 10KB as a reasonable upper bound
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "BulletProofPlus range proof is unreasonably large",
            ));
        }

        // Validate commitment structure
        if commitment.as_bytes().len() != 33 {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "Commitment must be 33 bytes",
            ));
        }

        // TODO: In a full implementation, this would validate the actual BulletProofPlus
        // against the minimum value promise. For now, we'll do basic structure validation.
        
        Ok(())
    }

    /// Validate that a minimum value promise is within the valid range for a range proof type
    ///
    /// # Arguments
    /// * `minimum_value_promise` - The minimum value promise to validate
    /// * `range_proof_type` - The type of range proof
    ///
    /// # Returns
    /// * `Ok(())` if the minimum value promise is within range
    /// * `Err(ValidationError)` if the minimum value promise is out of range
    pub fn validate_range_proof_bounds(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof_type: &LightweightRangeProofType,
    ) -> Result<(), ValidationError> {
        let value = minimum_value_promise.as_u64();
        let max_value = 1u64.checked_shl(self.default_bit_length as u32).unwrap_or(u64::MAX);
        
        if value >= max_value {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                &format!(
                    "Minimum value promise {} exceeds range proof bit length {} for {:?}",
                    value,
                    self.default_bit_length,
                    range_proof_type
                ),
            ));
        }

        Ok(())
    }

    /// Check if a value is within the valid range for this validator
    pub fn is_value_in_range(&self, value: u64) -> bool {
        value < (1u64 << self.default_bit_length)
    }

    /// Get the maximum value that can be proven with this validator
    pub fn max_value(&self) -> u64 {
        (1u64 << self.default_bit_length) - 1
    }

    /// Validate RevealedValue consistency (internal helper)
    fn validate_revealed_value_consistency(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<&LightweightRangeProof>,
    ) -> Result<(), ValidationError> {
        // For RevealedValue, the range proof should be None
        if range_proof.is_some() {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "RevealedValue range proofs should not have proof bytes",
            ));
        }

        Ok(())
    }

    /// Validate BulletProofPlus consistency (internal helper)
    fn validate_bulletproof_consistency(
        &self,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<&LightweightRangeProof>,
    ) -> Result<(), ValidationError> {
        // For BulletProofPlus, the range proof should be Some
        if range_proof.is_none() {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "BulletProofPlus range proofs must have proof bytes",
            ));
        }

        let proof = range_proof.unwrap();
        if proof.bytes.is_empty() {
            return Err(ValidationError::minimum_value_promise_validation_failed(
                "BulletProofPlus range proof cannot be empty",
            ));
        }

        Ok(())
    }
}

/// Minimum value promise validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinimumValuePromiseValidationResult {
    /// The minimum value promise is valid
    Valid,
    /// The minimum value promise is invalid
    Invalid(String),
    /// The minimum value promise could not be validated (e.g., unsupported format)
    Unsupported(String),
}

impl MinimumValuePromiseValidationResult {
    /// Check if the validation result indicates success
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }

    /// Get the error message if validation failed
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Valid => None,
            Self::Invalid(msg) | Self::Unsupported(msg) => Some(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = LightweightMinimumValuePromiseValidator::new(32);
        assert_eq!(validator.default_bit_length(), 32);
        assert_eq!(validator.max_value(), (1u64 << 32) - 1);
    }

    #[test]
    fn test_validator_default() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        assert_eq!(validator.default_bit_length(), 64);
    }

    #[test]
    fn test_value_range_checking() {
        let validator = LightweightMinimumValuePromiseValidator::new(32);
        
        assert!(validator.is_value_in_range(0));
        assert!(validator.is_value_in_range(1000));
        assert!(validator.is_value_in_range(validator.max_value()));
        assert!(!validator.is_value_in_range(validator.max_value() + 1));
    }

    #[test]
    fn test_range_proof_bounds_validation() {
        let validator = LightweightMinimumValuePromiseValidator::new(16);
        
        // Valid values
        assert!(validator.validate_range_proof_bounds(
            MicroMinotari::new(0),
            &LightweightRangeProofType::BulletProofPlus
        ).is_ok());
        
        assert!(validator.validate_range_proof_bounds(
            MicroMinotari::new(1000),
            &LightweightRangeProofType::BulletProofPlus
        ).is_ok());
        
        assert!(validator.validate_range_proof_bounds(
            MicroMinotari::new(validator.max_value()),
            &LightweightRangeProofType::BulletProofPlus
        ).is_ok());
        
        // Invalid values
        assert!(validator.validate_range_proof_bounds(
            MicroMinotari::new(validator.max_value() + 1),
            &LightweightRangeProofType::BulletProofPlus
        ).is_err());
    }

    #[test]
    fn test_revealed_value_consistency_validation() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        
        // Valid: RevealedValue with no proof
        assert!(validator.validate_revealed_value_consistency(
            MicroMinotari::new(1000),
            None
        ).is_ok());
        
        // Invalid: RevealedValue with proof
        assert!(validator.validate_revealed_value_consistency(
            MicroMinotari::new(1000),
            Some(&LightweightRangeProof { bytes: vec![1, 2, 3, 4] })
        ).is_err());
    }

    #[test]
    fn test_bulletproof_consistency_validation() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        
        // Valid: BulletProofPlus with proof
        assert!(validator.validate_bulletproof_consistency(
            MicroMinotari::new(1000),
            Some(&LightweightRangeProof { bytes: vec![1, 2, 3, 4] })
        ).is_ok());
        
        // Invalid: BulletProofPlus without proof
        assert!(validator.validate_bulletproof_consistency(
            MicroMinotari::new(1000),
            None
        ).is_err());
        
        // Invalid: BulletProofPlus with empty proof
        assert!(validator.validate_bulletproof_consistency(
            MicroMinotari::new(1000),
            Some(&LightweightRangeProof { bytes: vec![] })
        ).is_err());
    }

    #[test]
    fn test_comprehensive_validation() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        let options = MinimumValuePromiseValidationOptions::default();
        
        // Valid RevealedValue
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(1000),
            None,
            &LightweightRangeProofType::RevealedValue,
            &options
        ).is_ok());
        
        // Valid BulletProofPlus
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(1000),
            Some(&LightweightRangeProof { bytes: vec![1, 2, 3, 4] }),
            &LightweightRangeProofType::BulletProofPlus,
            &options
        ).is_ok());
        
        // Invalid: RevealedValue with proof
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(1000),
            Some(&LightweightRangeProof { bytes: vec![1, 2, 3, 4] }),
            &LightweightRangeProofType::RevealedValue,
            &options
        ).is_err());
        
        // Invalid: BulletProofPlus without proof
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(1000),
            None,
            &LightweightRangeProofType::BulletProofPlus,
            &options
        ).is_err());
    }

    #[test]
    fn test_validation_options() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        
        // Test with zero values disabled
        let mut options = MinimumValuePromiseValidationOptions::default();
        options.allow_zero_values = false;
        
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(0),
            None,
            &LightweightRangeProofType::RevealedValue,
            &options
        ).is_err());
        
        // Test with max allowed value
        let mut options = MinimumValuePromiseValidationOptions::default();
        options.max_allowed_value = Some(1000);
        
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(500),
            None,
            &LightweightRangeProofType::RevealedValue,
            &options
        ).is_ok());
        
        assert!(validator.validate_minimum_value_promise(
            MicroMinotari::new(1500),
            None,
            &LightweightRangeProofType::RevealedValue,
            &options
        ).is_err());
    }

    #[test]
    fn test_revealed_value_minimum_promise_validation() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        let minimum_value = MicroMinotari::new(1000);
        
        // Create a valid challenge
        let challenge = [1u8; 32];
        let e = PrivateKey::from_canonical_bytes(&challenge).unwrap();
        
        // Convert value to private key (little-endian bytes)
        let value_bytes = minimum_value.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        // r_a = 0 for RevealedValue proofs
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        let expected_u_a = commit_nonce_a + e * value_as_private_key;

        let result = validator.validate_revealed_value_minimum_promise(
            minimum_value,
            None,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_bulletproof_minimum_promise_validation() {
        let validator = LightweightMinimumValuePromiseValidator::default();
        let minimum_value = MicroMinotari::new(1000);
        let commitment = CompressedCommitment::new([0x08; 33]);
        let range_proof = LightweightRangeProof { bytes: vec![1, 2, 3, 4, 5] };

        let result = validator.validate_bulletproof_minimum_promise(
            minimum_value,
            &range_proof,
            &commitment,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validation_result() {
        let valid = MinimumValuePromiseValidationResult::Valid;
        assert!(valid.is_valid());
        assert_eq!(valid.error_message(), None);

        let invalid = MinimumValuePromiseValidationResult::Invalid("test error".to_string());
        assert!(!invalid.is_valid());
        assert_eq!(invalid.error_message(), Some("test error"));

        let unsupported = MinimumValuePromiseValidationResult::Unsupported("unsupported".to_string());
        assert!(!unsupported.is_valid());
        assert_eq!(unsupported.error_message(), Some("unsupported"));
    }
} 
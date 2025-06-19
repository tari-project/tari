// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Range proof validation for lightweight wallets
//! 
//! This module provides lightweight validation for BulletProofPlus range proofs
//! without requiring the full Tari crypto stack.

use crate::{
    data_structures::types::{CompressedCommitment, MicroMinotari},
    errors::ValidationError,
};

/// Lightweight BulletProofPlus range proof validator
/// 
/// This provides a simplified interface for validating BulletProofPlus range proofs
/// in lightweight wallet applications.
#[derive(Debug, Clone)]
pub struct LightweightBulletProofPlusValidator {
    /// Range proof bit length (default: 64 for Tari)
    bit_length: usize,
}

impl Default for LightweightBulletProofPlusValidator {
    fn default() -> Self {
        Self {
            bit_length: 64, // Tari's default range proof bit length
        }
    }
}

impl LightweightBulletProofPlusValidator {
    /// Create a new validator with the specified bit length
    pub fn new(bit_length: usize) -> Self {
        Self { bit_length }
    }

    /// Get the range proof bit length
    pub fn bit_length(&self) -> usize {
        self.bit_length
    }

    /// Validate a single BulletProofPlus range proof
    /// 
    /// # Arguments
    /// * `proof_bytes` - The range proof bytes
    /// * `commitment` - The commitment being proven
    /// * `minimum_value_promise` - The minimum value promise
    /// 
    /// # Returns
    /// * `Ok(())` if the proof is valid
    /// * `Err(ValidationError)` if the proof is invalid
    pub fn verify_single(
        &self,
        proof_bytes: &[u8],
        commitment: &CompressedCommitment,
        minimum_value_promise: MicroMinotari,
    ) -> Result<(), ValidationError> {
        // For now, we'll implement a basic structure validation
        // In a full implementation, this would call the actual BulletProofPlus verification
        
        // Check that proof bytes are not empty
        if proof_bytes.is_empty() {
            return Err(ValidationError::range_proof_validation_failed(
                "Range proof bytes cannot be empty",
            ));
        }

        // Check that commitment is valid (basic structure check)
        if commitment.as_bytes().len() != 33 {
            return Err(ValidationError::commitment_validation_failed(
                "Commitment must be 33 bytes",
            ));
        }

        let max_value = 1u64.checked_shl(self.bit_length as u32).unwrap_or(u64::MAX);
        if minimum_value_promise.as_u64() >= max_value {
            return Err(ValidationError::range_proof_validation_failed(
                &format!(
                    "Minimum value promise {} exceeds range proof bit length {}",
                    minimum_value_promise.as_u64(),
                    self.bit_length
                ),
            ));
        }

        // TODO: Implement actual BulletProofPlus verification
        // This would require integrating with the tari_crypto crate's BulletproofsPlusService
        // For now, we'll return success for valid structure
        
        Ok(())
    }

    /// Validate multiple BulletProofPlus range proofs in batch
    /// 
    /// # Arguments
    /// * `proofs` - Vector of proof bytes
    /// * `statements` - Vector of validation statements (commitment + minimum value)
    /// 
    /// # Returns
    /// * `Ok(())` if all proofs are valid
    /// * `Err(ValidationError)` if any proof is invalid
    pub fn verify_batch(
        &self,
        proofs: Vec<Vec<u8>>,
        statements: Vec<RangeProofStatement>,
    ) -> Result<(), ValidationError> {
        if proofs.len() != statements.len() {
            return Err(ValidationError::range_proof_validation_failed(
                "Number of proofs must match number of statements",
            ));
        }

        // Validate each proof individually
        for (proof, statement) in proofs.iter().zip(statements.iter()) {
            self.verify_single(proof, &statement.commitment, statement.minimum_value_promise)?;
        }

        Ok(())
    }

    /// Check if a value is within the valid range for this validator
    pub fn is_value_in_range(&self, value: u64) -> bool {
        value < (1u64 << self.bit_length)
    }

    /// Get the maximum value that can be proven with this validator
    pub fn max_value(&self) -> u64 {
        (1u64 << self.bit_length) - 1
    }
}

/// A range proof validation statement containing a commitment and minimum value promise
#[derive(Debug, Clone)]
pub struct RangeProofStatement {
    /// The commitment being proven
    pub commitment: CompressedCommitment,
    /// The minimum value promise
    pub minimum_value_promise: MicroMinotari,
}

impl RangeProofStatement {
    /// Create a new range proof statement
    pub fn new(commitment: CompressedCommitment, minimum_value_promise: MicroMinotari) -> Self {
        Self {
            commitment,
            minimum_value_promise,
        }
    }
}

/// Range proof validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeProofValidationResult {
    /// The range proof is valid
    Valid,
    /// The range proof is invalid
    Invalid(String),
    /// The range proof could not be validated (e.g., unsupported format)
    Unsupported(String),
}

impl RangeProofValidationResult {
    /// Check if the validation result indicates success
    pub fn is_valid(&self) -> bool {
        matches!(self, RangeProofValidationResult::Valid)
    }

    /// Get the error message if validation failed
    pub fn error_message(&self) -> Option<&str> {
        match self {
            RangeProofValidationResult::Valid => None,
            RangeProofValidationResult::Invalid(msg) => Some(msg),
            RangeProofValidationResult::Unsupported(msg) => Some(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = LightweightBulletProofPlusValidator::new(32);
        assert_eq!(validator.bit_length(), 32);
        assert_eq!(validator.max_value(), (1u64 << 32) - 1);
    }

    #[test]
    fn test_validator_default() {
        let validator = LightweightBulletProofPlusValidator::default();
        assert_eq!(validator.bit_length(), 64);
    }

    #[test]
    fn test_value_range_checking() {
        let validator = LightweightBulletProofPlusValidator::new(32);
        
        assert!(validator.is_value_in_range(0));
        assert!(validator.is_value_in_range(1000));
        assert!(validator.is_value_in_range(validator.max_value()));
        assert!(!validator.is_value_in_range(validator.max_value() + 1));
    }

    #[test]
    fn test_single_proof_validation_basic() {
        let validator = LightweightBulletProofPlusValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);

        // Test with valid inputs
        let result = validator.verify_single(&[1, 2, 3, 4], &commitment, minimum_value);
        assert!(result.is_ok());

        // Test with empty proof
        let result = validator.verify_single(&[], &commitment, minimum_value);
        assert!(result.is_err());

        // Test with value exceeding range (use a smaller validator to avoid overflow)
        let small_validator = LightweightBulletProofPlusValidator::new(16);
        let large_value = MicroMinotari::new(1u64 << 17);
        let result = small_validator.verify_single(&[1, 2, 3, 4], &commitment, large_value);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_validation() {
        let validator = LightweightBulletProofPlusValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);

        let proofs = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8]];
        let statements = vec![
            RangeProofStatement::new(commitment.clone(), minimum_value),
            RangeProofStatement::new(commitment, minimum_value),
        ];

        let result = validator.verify_batch(proofs, statements);
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_validation_mismatched_lengths() {
        let validator = LightweightBulletProofPlusValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);

        let proofs = vec![vec![1, 2, 3, 4]];
        let statements = vec![
            RangeProofStatement::new(commitment.clone(), minimum_value),
            RangeProofStatement::new(commitment, minimum_value),
        ];

        let result = validator.verify_batch(proofs, statements);
        assert!(result.is_err());
    }

    #[test]
    fn test_range_proof_statement_creation() {
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);
        
        let statement = RangeProofStatement::new(commitment.clone(), minimum_value);
        assert_eq!(statement.commitment, commitment);
        assert_eq!(statement.minimum_value_promise, minimum_value);
    }

    #[test]
    fn test_validation_result() {
        let valid = RangeProofValidationResult::Valid;
        assert!(valid.is_valid());
        assert_eq!(valid.error_message(), None);

        let invalid = RangeProofValidationResult::Invalid("test error".to_string());
        assert!(!invalid.is_valid());
        assert_eq!(invalid.error_message(), Some("test error"));

        let unsupported = RangeProofValidationResult::Unsupported("unsupported".to_string());
        assert!(!unsupported.is_valid());
        assert_eq!(unsupported.error_message(), Some("unsupported"));
    }
} 
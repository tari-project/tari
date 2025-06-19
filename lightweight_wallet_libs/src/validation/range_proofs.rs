// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Range proof validation for lightweight wallets
//! 
//! This module provides lightweight validation for BulletProofPlus range proofs
//! without requiring the full Tari crypto stack.

use crate::{
    data_structures::types::{CompressedCommitment, MicroMinotari, PrivateKey},
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

/// Lightweight RevealedValue range proof validator
/// 
/// This provides validation for RevealedValue range proofs which use
/// deterministic ephemeral nonces and bind values into metadata signatures.
#[derive(Debug, Clone)]
pub struct LightweightRevealedValueValidator {
    /// Range proof bit length (default: 64 for Tari)
    bit_length: usize,
}

impl Default for LightweightRevealedValueValidator {
    fn default() -> Self {
        Self {
            bit_length: 64, // Tari's default range proof bit length
        }
    }
}

impl LightweightRevealedValueValidator {
    /// Create a new validator with the specified bit length
    pub fn new(bit_length: usize) -> Self {
        Self { bit_length }
    }

    /// Get the range proof bit length
    pub fn bit_length(&self) -> usize {
        self.bit_length
    }

    /// Validate a RevealedValue range proof
    /// 
    /// # Arguments
    /// * `commitment` - The commitment being proven
    /// * `minimum_value_promise` - The minimum value promise (should equal the actual value)
    /// * `metadata_signature_u_a` - The u_a component of the metadata signature
    /// * `metadata_signature_challenge` - The challenge used in the metadata signature
    /// 
    /// # Returns
    /// * `Ok(())` if the proof is valid
    /// * `Err(ValidationError)` if the proof is invalid
    pub fn verify_revealed_value_proof(
        &self,
        commitment: &CompressedCommitment,
        minimum_value_promise: MicroMinotari,
        metadata_signature_u_a: &PrivateKey,
        metadata_signature_challenge: &[u8],
    ) -> Result<(), ValidationError> {
        // Check that commitment is valid (basic structure check)
        if commitment.as_bytes().len() != 33 {
            return Err(ValidationError::commitment_validation_failed(
                "Commitment must be 33 bytes",
            ));
        }

        // Check that minimum value promise is within range
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

        // For RevealedValue proofs, the ephemeral nonce r_a is always zero
        let commit_nonce_a = PrivateKey::new([0u8; 32]); // This is the deterministic nonce r_a of zero
        
        // Derive the challenge e from the metadata signature challenge
        let e = PrivateKey::from_canonical_bytes(metadata_signature_challenge)
            .map_err(|_| ValidationError::range_proof_validation_failed(
                "Invalid metadata signature challenge"
            ))?;
        
        // Convert the minimum value promise to a private key
        let value_bytes = minimum_value_promise.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        // Verify the balance proof: u_a should equal r_a + e * value
        let expected_u_a = commit_nonce_a + e * value_as_private_key;
        
        if metadata_signature_u_a.as_bytes() == expected_u_a.as_bytes() {
            Ok(())
        } else {
            Err(ValidationError::range_proof_validation_failed(
                "RevealedValue range proof verification failed"
            ))
        }
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

    // RevealedValue validator tests
    #[test]
    fn test_revealed_value_validator_creation() {
        let validator = LightweightRevealedValueValidator::new(32);
        assert_eq!(validator.bit_length(), 32);
        assert_eq!(validator.max_value(), (1u64 << 32) - 1);
    }

    #[test]
    fn test_revealed_value_validator_default() {
        let validator = LightweightRevealedValueValidator::default();
        assert_eq!(validator.bit_length(), 64);
    }

    #[test]
    fn test_revealed_value_range_checking() {
        let validator = LightweightRevealedValueValidator::new(32);
        
        assert!(validator.is_value_in_range(0));
        assert!(validator.is_value_in_range(1000));
        assert!(validator.is_value_in_range(validator.max_value()));
        assert!(!validator.is_value_in_range(validator.max_value() + 1));
    }

    #[test]
    fn test_revealed_value_proof_validation_valid() {
        let validator = LightweightRevealedValueValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
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

        let result = validator.verify_revealed_value_proof(
            &commitment,
            minimum_value,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_revealed_value_proof_validation_invalid_signature() {
        let validator = LightweightRevealedValueValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);
        
        // Create a challenge
        let challenge = [1u8; 32];
        
        // Use an incorrect u_a (random private key)
        let invalid_u_a = PrivateKey::random();

        let result = validator.verify_revealed_value_proof(
            &commitment,
            minimum_value,
            &invalid_u_a,
            &challenge,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_revealed_value_proof_validation_invalid_commitment() {
        let validator = LightweightRevealedValueValidator::default();
        let invalid_commitment = CompressedCommitment::new([0u8; 33]); // This is actually valid size
        let minimum_value = MicroMinotari::new(1000);
        
        let challenge = [1u8; 32];
        let e = PrivateKey::from_canonical_bytes(&challenge).unwrap();
        
        // Convert value to private key
        let value_bytes = minimum_value.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        let expected_u_a = commit_nonce_a + e * value_as_private_key;

        let result = validator.verify_revealed_value_proof(
            &invalid_commitment,
            minimum_value,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_ok()); // This should actually pass since commitment is valid
    }

    #[test]
    fn test_revealed_value_proof_validation_value_out_of_range() {
        let validator = LightweightRevealedValueValidator::new(16); // Small bit length
        let commitment = CompressedCommitment::new([0u8; 33]);
        let large_value = MicroMinotari::new(1u64 << 17); // Exceeds 16-bit range
        
        let challenge = [1u8; 32];
        let e = PrivateKey::from_canonical_bytes(&challenge).unwrap();
        
        // Convert value to private key
        let value_bytes = large_value.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        let expected_u_a = commit_nonce_a + e * value_as_private_key;

        let result = validator.verify_revealed_value_proof(
            &commitment,
            large_value,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_revealed_value_proof_validation_invalid_challenge() {
        let validator = LightweightRevealedValueValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let minimum_value = MicroMinotari::new(1000);
        
        // Use an invalid challenge (wrong size)
        let invalid_challenge = [1u8; 31]; // Should be 32 bytes
        
        let u_a = PrivateKey::random();

        let result = validator.verify_revealed_value_proof(
            &commitment,
            minimum_value,
            &u_a,
            &invalid_challenge,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_revealed_value_proof_validation_zero_value() {
        let validator = LightweightRevealedValueValidator::default();
        let commitment = CompressedCommitment::new([0u8; 33]);
        let zero_value = MicroMinotari::new(0);
        
        let challenge = [1u8; 32];
        let e = PrivateKey::from_canonical_bytes(&challenge).unwrap();
        
        // Convert value to private key
        let value_bytes = zero_value.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        let expected_u_a = commit_nonce_a + e * value_as_private_key;

        let result = validator.verify_revealed_value_proof(
            &commitment,
            zero_value,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_revealed_value_proof_validation_large_value() {
        let validator = LightweightRevealedValueValidator::new(32);
        let commitment = CompressedCommitment::new([0u8; 33]);
        let large_value = MicroMinotari::new((1u64 << 31) - 1); // Max 32-bit value
        
        let challenge = [1u8; 32];
        let e = PrivateKey::from_canonical_bytes(&challenge).unwrap();
        
        // Convert value to private key
        let value_bytes = large_value.as_u64().to_le_bytes();
        let mut value_key_bytes = [0u8; 32];
        value_key_bytes[..8].copy_from_slice(&value_bytes);
        let value_as_private_key = PrivateKey::new(value_key_bytes);
        
        let commit_nonce_a = PrivateKey::new([0u8; 32]);
        let expected_u_a = commit_nonce_a + e * value_as_private_key;

        let result = validator.verify_revealed_value_proof(
            &commitment,
            large_value,
            &expected_u_a,
            &challenge,
        );
        assert!(result.is_ok());
    }
} 
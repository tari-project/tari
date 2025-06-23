// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Metadata signature verification for lightweight wallets
//! 
//! This module provides lightweight validation for transaction output metadata signatures
//! without requiring the full Tari crypto stack.

use crate::{
    data_structures::{
        types::{ MicroMinotari},
        transaction_output::LightweightTransactionOutput,
        wallet_output::{LightweightCovenant, LightweightOutputFeatures, LightweightScript},
        encrypted_data::EncryptedData,
    },
    errors::ValidationError,
};

/// Lightweight metadata signature validator
/// 
/// This provides a simplified interface for validating metadata signatures
/// in lightweight wallet applications.
#[derive(Debug, Clone)]
pub struct LightweightMetadataSignatureValidator {
    /// Whether to perform full cryptographic verification (requires crypto dependencies)
    full_verification: bool,
}

impl Default for LightweightMetadataSignatureValidator {
    fn default() -> Self {
        Self {
            full_verification: false, // Default to lightweight validation
        }
    }
}

impl LightweightMetadataSignatureValidator {
    /// Create a new validator with the specified verification mode
    pub fn new(full_verification: bool) -> Self {
        Self { full_verification }
    }

    /// Get the verification mode
    pub fn full_verification(&self) -> bool {
        self.full_verification
    }

    /// Validate a metadata signature on a transaction output
    /// 
    /// # Arguments
    /// * `output` - The transaction output to validate
    /// 
    /// # Returns
    /// * `Ok(())` if the signature is valid
    /// * `Err(ValidationError)` if the signature is invalid
    pub fn verify_metadata_signature(
        &self,
        output: &LightweightTransactionOutput,
    ) -> Result<(), ValidationError> {
        // Extract signature components
        let signature_bytes = &output.metadata_signature().bytes;
        if signature_bytes.len() < 5 * 32 {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Metadata signature must be at least 160 bytes (5 * 32)",
            ));
        }

        // Parse signature components (basic structure validation)
        let ephemeral_commitment_bytes = &signature_bytes[0..33];
        let ephemeral_pubkey_bytes = &signature_bytes[33..65];
        let u_a_bytes = &signature_bytes[65..97];
        let u_x_bytes = &signature_bytes[97..129];
        let u_y_bytes = &signature_bytes[129..161];

        // Validate ephemeral commitment structure
        if ephemeral_commitment_bytes[0] != 0x08 && ephemeral_commitment_bytes[0] != 0x09 {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Invalid ephemeral commitment format",
            ));
        }

        // Validate ephemeral pubkey structure
        if ephemeral_pubkey_bytes.len() != 32 {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Invalid ephemeral pubkey length",
            ));
        }

        // Validate signature components are not all zero
        if u_a_bytes.iter().all(|&b| b == 0) || u_x_bytes.iter().all(|&b| b == 0) || u_y_bytes.iter().all(|&b| b == 0) {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Signature components cannot be all zero",
            ));
        }

        // Build the metadata signature challenge
        let challenge = self.build_metadata_signature_challenge(output)?;

        // For lightweight validation, we just check the structure
        // For full verification, we would verify the cryptographic signature
        if self.full_verification {
            // TODO: Implement full cryptographic verification
            // This would require integrating with the tari_crypto crate
            return Err(ValidationError::metadata_signature_validation_failed(
                "Full cryptographic verification not yet implemented",
            ));
        }

        Ok(())
    }

    /// Build the metadata signature challenge for a transaction output
    /// 
    /// # Arguments
    /// * `output` - The transaction output
    /// 
    /// # Returns
    /// * `Ok([u8; 64])` - The challenge bytes
    /// * `Err(ValidationError)` if the challenge cannot be built
    pub fn build_metadata_signature_challenge(
        &self,
        output: &LightweightTransactionOutput,
    ) -> Result<[u8; 64], ValidationError> {
        // Extract signature components
        let signature_bytes = &output.metadata_signature().bytes;
        if signature_bytes.len() < 5 * 32 {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Metadata signature must be at least 160 bytes",
            ));
        }

        let ephemeral_commitment_bytes = &signature_bytes[0..33];
        let ephemeral_pubkey_bytes = &signature_bytes[33..65];

        // Build the metadata message
        let metadata_message = self.build_metadata_signature_message(output)?;

        // For lightweight validation, we'll create a simplified challenge
        // In full implementation, this would use the actual domain-separated hashing
        let mut challenge = [0u8; 64];
        
        // Simple hash-like construction for lightweight validation
        // In practice, this would be: H(ephemeral_pubkey || ephemeral_commitment || sender_offset_pubkey || commitment || message)
        let mut hasher = blake2b_simd::State::new();
        hasher.update(b"metadata_signature");
        hasher.update(ephemeral_pubkey_bytes);
        hasher.update(ephemeral_commitment_bytes);
        hasher.update(&output.sender_offset_public_key().as_bytes());
        hasher.update(output.commitment().as_bytes());
        hasher.update(&metadata_message);
        
        let hash = hasher.finalize();
        challenge.copy_from_slice(&hash.as_bytes()[..64]);

        Ok(challenge)
    }

    /// Build the metadata signature message for a transaction output
    /// 
    /// # Arguments
    /// * `output` - The transaction output
    /// 
    /// # Returns
    /// * `Ok([u8; 32])` - The message bytes
    /// * `Err(ValidationError)` if the message cannot be built
    pub fn build_metadata_signature_message(
        &self,
        output: &LightweightTransactionOutput,
    ) -> Result<[u8; 32], ValidationError> {
        // Build the common message part
        let common_message = self.build_metadata_signature_message_common(
            output.version(),
            output.features(),
            output.covenant(),
            output.encrypted_data(),
            output.minimum_value_promise(),
        )?;

        // Build the full message including script
        let full_message = self.build_metadata_signature_message_from_script_and_common(
            output.script(),
            &common_message,
        )?;

        Ok(full_message)
    }

    /// Build the common part of the metadata signature message
    /// 
    /// # Arguments
    /// * `version` - Output version
    /// * `features` - Output features
    /// * `covenant` - Output covenant
    /// * `encrypted_data` - Encrypted data
    /// * `minimum_value_promise` - Minimum value promise
    /// 
    /// # Returns
    /// * `Ok([u8; 32])` - The common message bytes
    /// * `Err(ValidationError)` if the message cannot be built
    pub fn build_metadata_signature_message_common(
        &self,
        version: u8,
        features: &LightweightOutputFeatures,
        covenant: &LightweightCovenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: MicroMinotari,
    ) -> Result<[u8; 32], ValidationError> {
        // For lightweight validation, we'll create a simplified message
        // In full implementation, this would use the actual domain-separated hashing
        let mut hasher = blake2b_simd::State::new();
        hasher.update(b"metadata_message");
        hasher.update(&[version]);
        hasher.update(&features.bytes());
        hasher.update(&covenant.bytes);
        hasher.update(&encrypted_data.as_bytes());
        hasher.update(&minimum_value_promise.as_u64().to_le_bytes());
        
        let hash = hasher.finalize();
        let mut message = [0u8; 32];
        message.copy_from_slice(&hash.as_bytes()[..32]);

        Ok(message)
    }

    /// Build the full metadata signature message from script and common message
    /// 
    /// # Arguments
    /// * `script` - The script
    /// * `common_message` - The common message part
    /// 
    /// # Returns
    /// * `Ok([u8; 32])` - The full message bytes
    /// * `Err(ValidationError)` if the message cannot be built
    pub fn build_metadata_signature_message_from_script_and_common(
        &self,
        script: &LightweightScript,
        common_message: &[u8; 32],
    ) -> Result<[u8; 32], ValidationError> {
        // For lightweight validation, we'll create a simplified message
        // In full implementation, this would use the actual domain-separated hashing
        let mut hasher = blake2b_simd::State::new();
        hasher.update(b"metadata_message");
        hasher.update(&script.bytes);
        hasher.update(common_message);
        
        let hash = hasher.finalize();
        let mut message = [0u8; 32];
        message.copy_from_slice(&hash.as_bytes()[..32]);

        Ok(message)
    }

    /// Validate multiple metadata signatures in batch
    /// 
    /// # Arguments
    /// * `outputs` - Vector of transaction outputs to validate
    /// 
    /// # Returns
    /// * `Ok(())` if all signatures are valid
    /// * `Err(ValidationError)` if any signature is invalid
    pub fn verify_batch(
        &self,
        outputs: &[LightweightTransactionOutput],
    ) -> Result<(), ValidationError> {
        for (i, output) in outputs.iter().enumerate() {
            self.verify_metadata_signature(output).map_err(|e| {
                ValidationError::metadata_signature_validation_failed(
                    &format!("Output {}: {}", i, e.to_string()),
                )
            })?;
        }

        Ok(())
    }

    /// Extract signature components from metadata signature bytes
    /// 
    /// # Arguments
    /// * `signature_bytes` - The signature bytes
    /// 
    /// # Returns
    /// * `Ok((Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>))` - The signature components
    /// * `Err(ValidationError)` if the signature is malformed
    pub fn extract_signature_components(
        &self,
        signature_bytes: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>), ValidationError> {
        if signature_bytes.len() < 5 * 32 {
            return Err(ValidationError::metadata_signature_validation_failed(
                "Metadata signature must be at least 160 bytes",
            ));
        }

        let ephemeral_commitment = signature_bytes[0..33].to_vec();
        let ephemeral_pubkey = signature_bytes[33..65].to_vec();
        let u_a = signature_bytes[65..97].to_vec();
        let u_x = signature_bytes[97..129].to_vec();
        let u_y = signature_bytes[129..161].to_vec();

        Ok((ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }
}

/// Metadata signature validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataSignatureValidationResult {
    /// The signature is valid
    Valid,
    /// The signature is invalid
    Invalid(String),
    /// The signature could not be validated (e.g., unsupported format)
    Unsupported(String),
}

impl MetadataSignatureValidationResult {
    /// Check if the validation result indicates a valid signature
    pub fn is_valid(&self) -> bool {
        matches!(self, MetadataSignatureValidationResult::Valid)
    }

    /// Get the error message if the validation failed
    pub fn error_message(&self) -> Option<&str> {
        match self {
            MetadataSignatureValidationResult::Valid => None,
            MetadataSignatureValidationResult::Invalid(msg) => Some(msg),
            MetadataSignatureValidationResult::Unsupported(msg) => Some(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        types::{CompressedCommitment, CompressedPublicKey, MicroMinotari},
        wallet_output::{LightweightOutputFeatures, LightweightScript, LightweightSignature, LightweightCovenant},
        encrypted_data::EncryptedData,
    };

    #[test]
    fn test_validator_creation() {
        let validator = LightweightMetadataSignatureValidator::new(true);
        assert!(validator.full_verification());
        
        let validator = LightweightMetadataSignatureValidator::new(false);
        assert!(!validator.full_verification());
    }

    #[test]
    fn test_validator_default() {
        let validator = LightweightMetadataSignatureValidator::default();
        assert!(!validator.full_verification());
    }

    #[test]
    fn test_extract_signature_components_valid() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let result = validator.extract_signature_components(&signature_bytes);
        assert!(result.is_ok());
        
        let (ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y) = result.unwrap();
        assert_eq!(ephemeral_commitment.len(), 33);
        assert_eq!(ephemeral_pubkey.len(), 32);
        assert_eq!(u_a.len(), 32);
        assert_eq!(u_x.len(), 32);
        assert_eq!(u_y.len(), 32);
    }

    #[test]
    fn test_extract_signature_components_invalid_length() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        let signature_bytes = vec![0u8; 100]; // Too short
        
        let result = validator.extract_signature_components(&signature_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_signature_validation_basic() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.verify_metadata_signature(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_metadata_signature_validation_invalid_signature() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create an invalid signature structure (all zeros)
        let signature_bytes = vec![0u8; 161];
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.verify_metadata_signature(&output);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_signature_validation_invalid_commitment_format() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create a signature with invalid commitment format
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x0A; // Invalid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.verify_metadata_signature(&output);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_signature_validation_short_signature() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create a signature that's too short
        let signature_bytes = vec![0u8; 100];
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.verify_metadata_signature(&output);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_validation() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create multiple valid outputs
        let mut outputs = Vec::new();
        for i in 0..3 {
            let mut signature_bytes = vec![0u8; 161];
            signature_bytes[0] = 0x08; // Valid commitment format
            signature_bytes[65] = 0x01; // Non-zero u_a
            signature_bytes[97] = 0x01; // Non-zero u_x
            signature_bytes[129] = 0x01; // Non-zero u_y
            
            let metadata_signature = LightweightSignature { bytes: signature_bytes };
            
            let output = LightweightTransactionOutput::new(
                1,
                LightweightOutputFeatures::default(),
                CompressedCommitment::new([i as u8; 33]),
                None,
                LightweightScript::default(),
                CompressedPublicKey::new([i as u8; 32]),
                metadata_signature,
                LightweightCovenant::default(),
                EncryptedData::default(),
                MicroMinotari::new(1000 + i as u64),
            );
            
            outputs.push(output);
        }
        
        let result = validator.verify_batch(&outputs);
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_validation_with_invalid_signature() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create outputs with one invalid signature
        let mut outputs = Vec::new();
        
        // Valid output
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        outputs.push(output);
        
        // Invalid output (all zeros)
        let signature_bytes = vec![0u8; 161];
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([2u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([3u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(2000),
        );
        
        outputs.push(output);
        
        let result = validator.verify_batch(&outputs);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_result() {
        let valid_result = MetadataSignatureValidationResult::Valid;
        assert!(valid_result.is_valid());
        assert!(valid_result.error_message().is_none());
        
        let invalid_result = MetadataSignatureValidationResult::Invalid("test error".to_string());
        assert!(!invalid_result.is_valid());
        assert_eq!(invalid_result.error_message(), Some("test error"));
        
        let unsupported_result = MetadataSignatureValidationResult::Unsupported("unsupported".to_string());
        assert!(!unsupported_result.is_valid());
        assert_eq!(unsupported_result.error_message(), Some("unsupported"));
    }

    #[test]
    fn test_build_metadata_signature_message() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            LightweightSignature::default(),
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.build_metadata_signature_message(&output);
        assert!(result.is_ok());
        
        let message = result.unwrap();
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_build_metadata_signature_challenge() {
        let validator = LightweightMetadataSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let metadata_signature = LightweightSignature { bytes: signature_bytes };
        
        let output = LightweightTransactionOutput::new(
            1,
            LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            None,
            LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            metadata_signature,
            LightweightCovenant::default(),
            EncryptedData::default(),
            MicroMinotari::new(1000),
        );
        
        let result = validator.build_metadata_signature_challenge(&output);
        assert!(result.is_ok());
        
        let challenge = result.unwrap();
        assert_eq!(challenge.len(), 64);
    }
} 
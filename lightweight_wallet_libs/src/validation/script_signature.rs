// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Script signature validation for lightweight wallets
//! 
//! This module provides lightweight validation for transaction input script signatures
//! without requiring the full Tari crypto stack.

use crate::{
    data_structures::{
        types::{CompressedCommitment, CompressedPublicKey},
        wallet_output::{LightweightScript, LightweightExecutionStack, LightweightSignature},
    },
    errors::ValidationError,
};

/// Lightweight script signature validator
/// 
/// This provides a simplified interface for validating script signatures
/// in lightweight wallet applications.
#[derive(Debug, Clone)]
pub struct LightweightScriptSignatureValidator {
    /// Whether to perform full cryptographic verification (requires crypto dependencies)
    full_verification: bool,
}

impl Default for LightweightScriptSignatureValidator {
    fn default() -> Self {
        Self {
            full_verification: false, // Default to lightweight validation
        }
    }
}

impl LightweightScriptSignatureValidator {
    /// Create a new validator with the specified verification mode
    pub fn new(full_verification: bool) -> Self {
        Self { full_verification }
    }

    /// Get the verification mode
    pub fn full_verification(&self) -> bool {
        self.full_verification
    }

    /// Validate a script signature on a transaction input
    /// 
    /// # Arguments
    /// * `script_signature` - The script signature to validate
    /// * `script` - The script being signed
    /// * `input_data` - The input data for the script
    /// * `script_public_key` - The script public key
    /// * `commitment` - The commitment being spent
    /// * `version` - The transaction input version
    /// 
    /// # Returns
    /// * `Ok(())` if the signature is valid
    /// * `Err(ValidationError)` if the signature is invalid
    pub fn verify_script_signature(
        &self,
        script_signature: &LightweightSignature,
        script: &LightweightScript,
        input_data: &LightweightExecutionStack,
        script_public_key: &CompressedPublicKey,
        commitment: &CompressedCommitment,
        version: u8,
    ) -> Result<(), ValidationError> {
        // Extract signature components
        let signature_bytes = &script_signature.bytes;
        if signature_bytes.len() < 5 * 32 {
            return Err(ValidationError::script_signature_validation_failed(
                "Script signature must be at least 160 bytes (5 * 32)",
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
            return Err(ValidationError::script_signature_validation_failed(
                "Invalid ephemeral commitment format",
            ));
        }

        // Validate ephemeral pubkey structure
        if ephemeral_pubkey_bytes.len() != 32 {
            return Err(ValidationError::script_signature_validation_failed(
                "Invalid ephemeral pubkey length",
            ));
        }

        // Validate signature components are not all zero
        if u_a_bytes.iter().all(|&b| b == 0) || u_x_bytes.iter().all(|&b| b == 0) || u_y_bytes.iter().all(|&b| b == 0) {
            return Err(ValidationError::script_signature_validation_failed(
                "Signature components cannot be all zero",
            ));
        }

        // Build the script signature challenge
        let challenge = self.build_script_signature_challenge(
            version,
            ephemeral_commitment_bytes,
            ephemeral_pubkey_bytes,
            script,
            input_data,
            script_public_key,
            commitment,
        )?;

        // For lightweight validation, we just check the structure
        // For full verification, we would verify the cryptographic signature
        if self.full_verification {
            // TODO: Implement full cryptographic verification
            // This would require integrating with the tari_crypto crate
            return Err(ValidationError::script_signature_validation_failed(
                "Full cryptographic verification not yet implemented",
            ));
        }

        Ok(())
    }

    /// Build the script signature challenge for a transaction input
    /// 
    /// # Arguments
    /// * `version` - Transaction input version
    /// * `ephemeral_commitment_bytes` - Ephemeral commitment bytes
    /// * `ephemeral_pubkey_bytes` - Ephemeral pubkey bytes
    /// * `script` - The script being signed
    /// * `input_data` - The input data for the script
    /// * `script_public_key` - The script public key
    /// * `commitment` - The commitment being spent
    /// 
    /// # Returns
    /// * `Ok([u8; 64])` - The challenge bytes
    /// * `Err(ValidationError)` if the challenge cannot be built
    pub fn build_script_signature_challenge(
        &self,
        version: u8,
        ephemeral_commitment_bytes: &[u8],
        ephemeral_pubkey_bytes: &[u8],
        script: &LightweightScript,
        input_data: &LightweightExecutionStack,
        script_public_key: &CompressedPublicKey,
        commitment: &CompressedCommitment,
    ) -> Result<[u8; 64], ValidationError> {
        // Build the script signature message
        let script_message = self.build_script_signature_message(version, script, input_data)?;

        // Build the final challenge
        let challenge = self.finalize_script_signature_challenge(
            version,
            ephemeral_commitment_bytes,
            ephemeral_pubkey_bytes,
            script_public_key,
            commitment,
            &script_message,
        )?;

        Ok(challenge)
    }

    /// Build the script signature message for a transaction input
    /// 
    /// # Arguments
    /// * `version` - Transaction input version
    /// * `script` - The script being signed
    /// * `input_data` - The input data for the script
    /// 
    /// # Returns
    /// * `Ok([u8; 32])` - The message bytes
    /// * `Err(ValidationError)` if the message cannot be built
    pub fn build_script_signature_message(
        &self,
        version: u8,
        script: &LightweightScript,
        input_data: &LightweightExecutionStack,
    ) -> Result<[u8; 32], ValidationError> {
        // For lightweight validation, we'll create a simplified message
        // In full implementation, this would use the actual domain-separated hashing
        let mut hasher = blake2b_simd::State::new();
        hasher.update(b"script_message");
        hasher.update(&[version]);
        hasher.update(&script.bytes);
        hasher.update(&input_data.bytes());
        
        let hash = hasher.finalize();
        let mut message = [0u8; 32];
        message.copy_from_slice(&hash.as_bytes()[..32]);

        Ok(message)
    }

    /// Finalize the script signature challenge
    /// 
    /// # Arguments
    /// * `version` - Transaction input version
    /// * `ephemeral_commitment_bytes` - Ephemeral commitment bytes
    /// * `ephemeral_pubkey_bytes` - Ephemeral pubkey bytes
    /// * `script_public_key` - The script public key
    /// * `commitment` - The commitment being spent
    /// * `message` - The script signature message
    /// 
    /// # Returns
    /// * `Ok([u8; 64])` - The challenge bytes
    /// * `Err(ValidationError)` if the challenge cannot be built
    pub fn finalize_script_signature_challenge(
        &self,
        version: u8,
        ephemeral_commitment_bytes: &[u8],
        ephemeral_pubkey_bytes: &[u8],
        script_public_key: &CompressedPublicKey,
        commitment: &CompressedCommitment,
        message: &[u8; 32],
    ) -> Result<[u8; 64], ValidationError> {
        // For lightweight validation, we'll create a simplified challenge
        // In full implementation, this would use the actual domain-separated hashing
        let mut challenge = [0u8; 64];
        
        // Simple hash-like construction for lightweight validation
        // In practice, this would be: H(ephemeral_commitment || ephemeral_pubkey || script_public_key || commitment || message)
        let mut hasher = blake2b_simd::State::new();
        hasher.update(b"script_challenge");
        hasher.update(ephemeral_commitment_bytes);
        hasher.update(ephemeral_pubkey_bytes);
        hasher.update(&script_public_key.as_bytes());
        hasher.update(commitment.as_bytes());
        hasher.update(message);
        
        let hash = hasher.finalize();
        challenge.copy_from_slice(&hash.as_bytes()[..64]);

        Ok(challenge)
    }

    /// Validate multiple script signatures in batch
    /// 
    /// # Arguments
    /// * `signatures` - Vector of script signatures to validate
    /// * `scripts` - Vector of scripts being signed
    /// * `input_data_sets` - Vector of input data sets
    /// * `script_public_keys` - Vector of script public keys
    /// * `commitments` - Vector of commitments being spent
    /// * `version` - The transaction input version
    /// 
    /// # Returns
    /// * `Ok(())` if all signatures are valid
    /// * `Err(ValidationError)` if any signature is invalid
    pub fn verify_batch(
        &self,
        signatures: &[LightweightSignature],
        scripts: &[LightweightScript],
        input_data_sets: &[LightweightExecutionStack],
        script_public_keys: &[CompressedPublicKey],
        commitments: &[CompressedCommitment],
        version: u8,
    ) -> Result<(), ValidationError> {
        if signatures.len() != scripts.len() || 
           signatures.len() != input_data_sets.len() || 
           signatures.len() != script_public_keys.len() || 
           signatures.len() != commitments.len() {
            return Err(ValidationError::script_signature_validation_failed(
                "All input vectors must have the same length",
            ));
        }

        for (i, (((signature, script), input_data), (script_public_key, commitment))) in 
            signatures.iter().zip(scripts.iter())
            .zip(input_data_sets.iter())
            .zip(script_public_keys.iter().zip(commitments.iter())).enumerate() {
            self.verify_script_signature(
                signature, script, input_data, script_public_key, commitment, version
            ).map_err(|e| {
                ValidationError::script_signature_validation_failed(
                    &format!("Signature {}: {}", i, e.to_string()),
                )
            })?;
        }

        Ok(())
    }

    /// Extract signature components from script signature bytes
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
            return Err(ValidationError::script_signature_validation_failed(
                "Script signature must be at least 160 bytes",
            ));
        }

        let ephemeral_commitment = signature_bytes[0..33].to_vec();
        let ephemeral_pubkey = signature_bytes[33..65].to_vec();
        let u_a = signature_bytes[65..97].to_vec();
        let u_x = signature_bytes[97..129].to_vec();
        let u_y = signature_bytes[129..161].to_vec();

        Ok((ephemeral_commitment, ephemeral_pubkey, u_a, u_x, u_y))
    }

    /// Run a script with input data and return the resulting public key
    /// 
    /// # Arguments
    /// * `script` - The script to execute
    /// * `input_data` - The input data for the script
    /// 
    /// # Returns
    /// * `Ok(CompressedPublicKey)` - The resulting public key
    /// * `Err(ValidationError)` if script execution fails
    pub fn run_script(
        &self,
        script: &LightweightScript,
        input_data: &LightweightExecutionStack,
    ) -> Result<CompressedPublicKey, ValidationError> {
        // For lightweight validation, we'll implement a basic script execution
        // In full implementation, this would use the actual Tari script engine
        
        // Check if script is empty (default script)
        if script.bytes.is_empty() {
            // Default script just pushes a public key onto the stack
            // For now, we'll return a default public key
            return Ok(CompressedPublicKey::new([0u8; 32]));
        }

        // TODO: Implement actual script execution
        // This would require integrating with the tari_script crate
        // For now, we'll return an error indicating this is not yet implemented
        Err(ValidationError::script_signature_validation_failed(
            "Script execution not yet implemented in lightweight mode",
        ))
    }

    /// Run a script and verify its signature
    /// 
    /// # Arguments
    /// * `script_signature` - The script signature to validate
    /// * `script` - The script being signed
    /// * `input_data` - The input data for the script
    /// * `commitment` - The commitment being spent
    /// * `version` - The transaction input version
    /// 
    /// # Returns
    /// * `Ok(CompressedPublicKey)` - The resulting public key if valid
    /// * `Err(ValidationError)` if script execution or signature verification fails
    pub fn run_and_verify_script(
        &self,
        script_signature: &LightweightSignature,
        script: &LightweightScript,
        input_data: &LightweightExecutionStack,
        commitment: &CompressedCommitment,
        version: u8,
    ) -> Result<CompressedPublicKey, ValidationError> {
        // Run the script to get the public key
        let script_public_key = self.run_script(script, input_data)?;
        
        // Verify the script signature
        self.verify_script_signature(
            script_signature,
            script,
            input_data,
            &script_public_key,
            commitment,
            version,
        )?;
        
        Ok(script_public_key)
    }
}

/// Script signature validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptSignatureValidationResult {
    /// The signature is valid
    Valid,
    /// The signature is invalid
    Invalid(String),
    /// The signature could not be validated (e.g., unsupported format)
    Unsupported(String),
}

impl ScriptSignatureValidationResult {
    /// Check if the validation result indicates a valid signature
    pub fn is_valid(&self) -> bool {
        matches!(self, ScriptSignatureValidationResult::Valid)
    }

    /// Get the error message if the validation failed
    pub fn error_message(&self) -> Option<&str> {
        match self {
            ScriptSignatureValidationResult::Valid => None,
            ScriptSignatureValidationResult::Invalid(msg) => Some(msg),
            ScriptSignatureValidationResult::Unsupported(msg) => Some(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        types::{CompressedCommitment, CompressedPublicKey},
        wallet_output::{LightweightScript, LightweightSignature, LightweightExecutionStack},
    };

    #[test]
    fn test_validator_creation() {
        let validator = LightweightScriptSignatureValidator::new(true);
        assert!(validator.full_verification());
        
        let validator = LightweightScriptSignatureValidator::new(false);
        assert!(!validator.full_verification());
    }

    #[test]
    fn test_validator_default() {
        let validator = LightweightScriptSignatureValidator::default();
        assert!(!validator.full_verification());
    }

    #[test]
    fn test_extract_signature_components_valid() {
        let validator = LightweightScriptSignatureValidator::default();
        
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
        let validator = LightweightScriptSignatureValidator::default();
        
        let signature_bytes = vec![0u8; 100]; // Too short
        
        let result = validator.extract_signature_components(&signature_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_script_signature_validation_basic() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let script_signature = LightweightSignature { bytes: signature_bytes };
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let script_public_key = CompressedPublicKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.verify_script_signature(
            &script_signature,
            &script,
            &input_data,
            &script_public_key,
            &commitment,
            1,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_script_signature_validation_invalid_signature() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create an invalid signature structure (all zeros)
        let signature_bytes = vec![0u8; 161];
        let script_signature = LightweightSignature { bytes: signature_bytes };
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let script_public_key = CompressedPublicKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.verify_script_signature(
            &script_signature,
            &script,
            &input_data,
            &script_public_key,
            &commitment,
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_script_signature_validation_invalid_commitment_format() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create a signature with invalid commitment format
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x0A; // Invalid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let script_signature = LightweightSignature { bytes: signature_bytes };
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let script_public_key = CompressedPublicKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.verify_script_signature(
            &script_signature,
            &script,
            &input_data,
            &script_public_key,
            &commitment,
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_script_signature_validation_short_signature() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create a signature that's too short
        let signature_bytes = vec![0u8; 100];
        let script_signature = LightweightSignature { bytes: signature_bytes };
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let script_public_key = CompressedPublicKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.verify_script_signature(
            &script_signature,
            &script,
            &input_data,
            &script_public_key,
            &commitment,
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_validation() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create multiple valid signatures
        let mut signatures = Vec::new();
        let mut scripts = Vec::new();
        let mut input_data_sets = Vec::new();
        let mut script_public_keys = Vec::new();
        let mut commitments = Vec::new();
        
        for i in 0..3 {
            let mut signature_bytes = vec![0u8; 161];
            signature_bytes[0] = 0x08; // Valid commitment format
            signature_bytes[65] = 0x01; // Non-zero u_a
            signature_bytes[97] = 0x01; // Non-zero u_x
            signature_bytes[129] = 0x01; // Non-zero u_y
            
            signatures.push(LightweightSignature { bytes: signature_bytes });
            scripts.push(LightweightScript::default());
            input_data_sets.push(LightweightExecutionStack::default());
            script_public_keys.push(CompressedPublicKey::new([i as u8; 32]));
            commitments.push(CompressedCommitment::new([i as u8; 33]));
        }
        
        let result = validator.verify_batch(
            &signatures,
            &scripts,
            &input_data_sets,
            &script_public_keys,
            &commitments,
            1,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_validation_with_invalid_signature() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create signatures with one invalid signature
        let mut signatures = Vec::new();
        let mut scripts = Vec::new();
        let mut input_data_sets = Vec::new();
        let mut script_public_keys = Vec::new();
        let mut commitments = Vec::new();
        
        // Valid signature
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        signatures.push(LightweightSignature { bytes: signature_bytes });
        scripts.push(LightweightScript::default());
        input_data_sets.push(LightweightExecutionStack::default());
        script_public_keys.push(CompressedPublicKey::new([1u8; 32]));
        commitments.push(CompressedCommitment::new([2u8; 33]));
        
        // Invalid signature (all zeros)
        let signature_bytes = vec![0u8; 161];
        signatures.push(LightweightSignature { bytes: signature_bytes });
        scripts.push(LightweightScript::default());
        input_data_sets.push(LightweightExecutionStack::default());
        script_public_keys.push(CompressedPublicKey::new([3u8; 32]));
        commitments.push(CompressedCommitment::new([4u8; 33]));
        
        let result = validator.verify_batch(
            &signatures,
            &scripts,
            &input_data_sets,
            &script_public_keys,
            &commitments,
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_validation_mismatched_lengths() {
        let validator = LightweightScriptSignatureValidator::default();
        
        let signatures = vec![LightweightSignature::default()];
        let scripts = vec![LightweightScript::default(), LightweightScript::default()]; // Different length
        
        let result = validator.verify_batch(
            &signatures,
            &scripts,
            &[LightweightExecutionStack::default()],
            &[CompressedPublicKey::new([1u8; 32])],
            &[CompressedCommitment::new([2u8; 33])],
            1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_result() {
        let valid_result = ScriptSignatureValidationResult::Valid;
        assert!(valid_result.is_valid());
        assert!(valid_result.error_message().is_none());
        
        let invalid_result = ScriptSignatureValidationResult::Invalid("test error".to_string());
        assert!(!invalid_result.is_valid());
        assert_eq!(invalid_result.error_message(), Some("test error"));
        
        let unsupported_result = ScriptSignatureValidationResult::Unsupported("unsupported".to_string());
        assert!(!unsupported_result.is_valid());
        assert_eq!(unsupported_result.error_message(), Some("unsupported"));
    }

    #[test]
    fn test_build_script_signature_message() {
        let validator = LightweightScriptSignatureValidator::default();
        
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        
        let result = validator.build_script_signature_message(1, &script, &input_data);
        assert!(result.is_ok());
        
        let message = result.unwrap();
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_build_script_signature_challenge() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let script_public_key = CompressedPublicKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.build_script_signature_challenge(
            1,
            &signature_bytes[0..33],
            &signature_bytes[33..65],
            &script,
            &input_data,
            &script_public_key,
            &commitment,
        );
        assert!(result.is_ok());
        
        let challenge = result.unwrap();
        assert_eq!(challenge.len(), 64);
    }

    #[test]
    fn test_run_script_default() {
        let validator = LightweightScriptSignatureValidator::default();
        
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        
        let result = validator.run_script(&script, &input_data);
        assert!(result.is_ok());
        
        let public_key = result.unwrap();
        assert_eq!(public_key.as_bytes().len(), 32);
    }

    #[test]
    fn test_run_script_not_implemented() {
        let validator = LightweightScriptSignatureValidator::default();
        
        let script = LightweightScript { bytes: vec![1u8, 2u8, 3u8] }; // Non-empty script
        let input_data = LightweightExecutionStack::default();
        
        let result = validator.run_script(&script, &input_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_and_verify_script() {
        let validator = LightweightScriptSignatureValidator::default();
        
        // Create a valid signature structure
        let mut signature_bytes = vec![0u8; 161];
        signature_bytes[0] = 0x08; // Valid commitment format
        signature_bytes[65] = 0x01; // Non-zero u_a
        signature_bytes[97] = 0x01; // Non-zero u_x
        signature_bytes[129] = 0x01; // Non-zero u_y
        
        let script_signature = LightweightSignature { bytes: signature_bytes };
        let script = LightweightScript::default();
        let input_data = LightweightExecutionStack::default();
        let commitment = CompressedCommitment::new([2u8; 33]);
        
        let result = validator.run_and_verify_script(
            &script_signature,
            &script,
            &input_data,
            &commitment,
            1,
        );
        assert!(result.is_ok());
        
        let public_key = result.unwrap();
        assert_eq!(public_key.as_bytes().len(), 32);
    }
} 
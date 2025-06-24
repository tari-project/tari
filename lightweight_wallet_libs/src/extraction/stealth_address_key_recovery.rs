// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Stealth address key recovery for lightweight wallets
//!
//! This module provides functionality to recover private keys for stealth addresses
//! and integrate with the UTXO extraction process.

use crate::{
    data_structures::{
        transaction_output::LightweightTransactionOutput,
        types::{CompressedPublicKey, PrivateKey, MicroMinotari},
    },
    errors::LightweightWalletError,
    key_management::{
        StealthAddress, StealthAddressManager,
        KeyStore,
    },
    extraction::{
        encrypted_data_decryption::EncryptedDataDecryptor,
        payment_id_extraction::PaymentIdExtractor,
    },
};

/// Result of stealth address key recovery
#[derive(Debug, Clone)]
pub struct StealthKeyRecoveryResult {
    /// The recovered stealth private key
    pub stealth_private_key: PrivateKey,
    /// The stealth address that was recovered
    pub stealth_address: StealthAddress,
    /// The key identifier used for recovery
    pub recovery_key_id: String,
    /// Whether the recovery was successful
    pub success: bool,
    /// Error message if recovery failed
    pub error: Option<String>,
}

/// Options for stealth address key recovery
#[derive(Debug, Clone)]
pub struct StealthKeyRecoveryOptions {
    /// Whether to try all available keys
    pub try_all_keys: bool,
    /// Maximum number of keys to try
    pub max_keys_to_try: usize,
    /// Whether to validate the recovered key
    pub validate_recovered_key: bool,
    /// Whether to attempt decryption with recovered keys
    pub attempt_decryption: bool,
    /// Whether to extract payment ID after recovery
    pub extract_payment_id: bool,
}

impl Default for StealthKeyRecoveryOptions {
    fn default() -> Self {
        Self {
            try_all_keys: true,
            max_keys_to_try: 100,
            validate_recovered_key: true,
            attempt_decryption: true,
            extract_payment_id: true,
        }
    }
}

/// Error types for stealth address key recovery
#[derive(Debug, thiserror::Error)]
pub enum StealthKeyRecoveryError {
    #[error("Failed to recover stealth private key: {0}")]
    RecoveryFailed(String),
    
    #[error("No suitable key found for recovery")]
    NoSuitableKey,
    
    #[error("Invalid stealth address: {0}")]
    InvalidStealthAddress(String),
    
    #[error("Key validation failed: {0}")]
    KeyValidationFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(#[from] LightweightWalletError),
}

// TODO: Update StealthKeyRecoveryManager to use new entropy-based key derivation approach
// instead of the removed ConcreteKeyManager
/*
/// Stealth address key recovery manager
pub struct StealthKeyRecoveryManager {
    key_manager: ConcreteKeyManager,
    options: StealthKeyRecoveryOptions,
}
*/

/*
impl StealthKeyRecoveryManager {
    /// Create a new stealth key recovery manager
    pub fn new(key_manager: ConcreteKeyManager) -> Self {
        Self {
            key_manager,
            options: StealthKeyRecoveryOptions::default(),
        }
    }

    /// Create a new stealth key recovery manager with custom options
    pub fn with_options(key_manager: ConcreteKeyManager, options: StealthKeyRecoveryOptions) -> Self {
        Self {
            key_manager,
            options,
        }
    }

    /// Recover stealth private key from ephemeral public key
    pub fn recover_stealth_private_key(
        &self,
        ephemeral_public_key: &CompressedPublicKey,
    ) -> Result<StealthKeyRecoveryResult, StealthKeyRecoveryError> {
        // Try imported keys first
        for imported_key in self.key_manager.get_all_imported_keys() {
            match self.try_recover_with_key(
                &imported_key.private_key,
                ephemeral_public_key,
                &imported_key.label.as_ref().unwrap_or(&"imported".to_string()),
            ) {
                Ok(result) => return Ok(result),
                Err(_) => continue,
            }
        }

        // Try derived keys if enabled
        if self.options.try_all_keys {
            for i in 0..self.options.max_keys_to_try {
                match self.key_manager.derive_key_pair_at_index(i as u64) {
                    Ok(key_pair) => {
                        match self.try_recover_with_key(
                            &key_pair.private_key,
                            ephemeral_public_key,
                            &format!("derived_{}", i),
                        ) {
                            Ok(result) => return Ok(result),
                            Err(_) => continue,
                        }
                    }
                    Err(_) => continue,
                }
            }
        }

        Err(StealthKeyRecoveryError::NoSuitableKey)
    }

    /// Try to recover stealth private key with a specific key
    fn try_recover_with_key(
        &self,
        private_key: &PrivateKey,
        ephemeral_public_key: &CompressedPublicKey,
        key_id: &str,
    ) -> Result<StealthKeyRecoveryResult, StealthKeyRecoveryError> {
        // Recover the stealth private key
        let stealth_private_key = StealthAddressManager::recover_stealth_private_key(
            private_key,
            ephemeral_public_key,
        ).map_err(|e| StealthKeyRecoveryError::RecoveryFailed(e.to_string()))?;

        // Derive the stealth public key from the recovered private key
        let stealth_public_key = CompressedPublicKey::from_private_key(&stealth_private_key);

        // Create the stealth address
        let stealth_address = StealthAddress::new(
            stealth_public_key,
            ephemeral_public_key.clone(),
            None,
        );

        // Validate the recovered key if requested
        if self.options.validate_recovered_key {
            self.validate_recovered_key(&stealth_private_key, &stealth_address)?;
        }

        Ok(StealthKeyRecoveryResult {
            stealth_private_key,
            stealth_address,
            recovery_key_id: key_id.to_string(),
            success: true,
            error: None,
        })
    }

    /// Validate a recovered stealth private key
    fn validate_recovered_key(
        &self,
        stealth_private_key: &PrivateKey,
        stealth_address: &StealthAddress,
    ) -> Result<(), StealthKeyRecoveryError> {
        // Verify that the private key corresponds to the public key
        let derived_public_key = CompressedPublicKey::from_private_key(stealth_private_key);
        if derived_public_key != stealth_address.public_key {
            return Err(StealthKeyRecoveryError::KeyValidationFailed(
                "Recovered private key does not correspond to stealth public key".to_string()
            ));
        }

        Ok(())
    }

    /// Attempt to decrypt transaction output with recovered stealth key
    pub fn decrypt_with_recovered_key(
        &self,
        transaction_output: &LightweightTransactionOutput,
        recovery_result: &StealthKeyRecoveryResult,
    ) -> Result<Option<MicroMinotari>, StealthKeyRecoveryError> {
        if !self.options.attempt_decryption {
            return Ok(None);
        }

        let key_store = KeyStore::new();
        let decryptor = EncryptedDataDecryptor::new(key_store);

        let decryption_result = decryptor.decrypt_with_key(
            transaction_output.encrypted_data(),
            transaction_output.commitment(),
            &recovery_result.stealth_private_key,
            &crate::extraction::encrypted_data_decryption::DecryptionOptions::default(),
        ).map_err(|e| StealthKeyRecoveryError::DecryptionFailed(e))?;

        if decryption_result.is_success() {
            Ok(decryption_result.value)
        } else {
            Ok(None)
        }
    }

    /// Extract payment ID from transaction output using recovered stealth key
    pub fn extract_payment_id_with_recovered_key(
        &self,
        transaction_output: &LightweightTransactionOutput,
        recovery_result: &StealthKeyRecoveryResult,
    ) -> Result<Option<crate::data_structures::payment_id::PaymentId>, StealthKeyRecoveryError> {
        if !self.options.extract_payment_id {
            return Ok(None);
        }

        let extraction_result = PaymentIdExtractor::extract(
            transaction_output.encrypted_data(),
            &recovery_result.stealth_private_key,
            transaction_output.commitment(),
        );

        if extraction_result.is_success() {
            Ok(extraction_result.payment_id)
        } else {
            Ok(None)
        }
    }

    /// Recover stealth key and attempt to decrypt transaction output
    pub fn recover_and_decrypt(
        &self,
        transaction_output: &LightweightTransactionOutput,
        ephemeral_public_key: &CompressedPublicKey,
    ) -> Result<StealthKeyRecoveryResult, StealthKeyRecoveryError> {
        // First, recover the stealth private key
        let recovery_result = self.recover_stealth_private_key(ephemeral_public_key)?;

        // Attempt to decrypt the transaction output
        if let Ok(Some(_value)) = self.decrypt_with_recovered_key(transaction_output, &recovery_result) {
            // If decryption succeeds, we know this is the correct key
            // We could add the value to the result if needed
        }

        Ok(recovery_result)
    }

    /// Batch recover stealth keys for multiple transaction outputs
    pub fn recover_batch(
        &self,
        transaction_outputs: &[(LightweightTransactionOutput, CompressedPublicKey)],
    ) -> Result<Vec<StealthKeyRecoveryResult>, StealthKeyRecoveryError> {
        let mut results = Vec::new();

        for (_transaction_output, ephemeral_public_key) in transaction_outputs {
            match self.recover_stealth_private_key(ephemeral_public_key) {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Log the error but continue with other outputs
                    eprintln!("Failed to recover stealth key: {:?}", e);
                    continue;
                }
            }
        }

        if results.is_empty() {
            return Err(StealthKeyRecoveryError::NoSuitableKey);
        }

        Ok(results)
    }

    /// Check if a transaction output is a stealth address output
    pub fn is_stealth_address_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
    ) -> bool {
        // This is a simplified check - in practice, you'd need to analyze
        // the script and covenant to determine if this is a stealth address output
        // For now, we'll assume any output with a non-zero script is potentially a stealth address
        !transaction_output.script().bytes.is_empty()
    }

    /// Get the key manager
    pub fn key_manager(&self) -> &ConcreteKeyManager {
        &self.key_manager
    }

    /// Get a mutable reference to the key manager
    pub fn key_manager_mut(&mut self) -> &mut ConcreteKeyManager {
        &mut self.key_manager
    }

    /// Set the recovery options
    pub fn set_options(&mut self, options: StealthKeyRecoveryOptions) {
        self.options = options;
    }

    /// Get the current recovery options
    pub fn options(&self) -> &StealthKeyRecoveryOptions {
        &self.options
    }
}
*/

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari},
    };
    use crate::key_management::ImportedPrivateKey;

    #[test]
    fn test_stealth_key_recovery_basic() {
        // Create a key manager with a test key
        let mut key_manager = ConcreteKeyManager::new([1u8; 32]);
        let test_key = PrivateKey::random();
        let imported_key = ImportedPrivateKey::new(
            test_key.clone(),
            Some("test_key".to_string()),
        );
        key_manager.import_private_key(imported_key).unwrap();

        // Generate a stealth address
        let recipient_public_key = CompressedPublicKey::from_private_key(&test_key);
        let stealth_address = StealthAddressManager::generate_stealth_address(
            &PrivateKey::random(),
            &recipient_public_key,
        ).unwrap();

        // Create recovery manager and recover the stealth key
        let recovery_manager = StealthKeyRecoveryManager::new(key_manager);
        let result = recovery_manager.recover_stealth_private_key(
            &stealth_address.ephemeral_public_key,
        ).unwrap();

        // Verify the result
        assert!(result.success);
        assert_eq!(result.recovery_key_id, "test_key");
        assert_eq!(result.stealth_address.public_key, stealth_address.public_key);
    }

    #[test]
    fn test_stealth_key_recovery_with_decryption() {
        // Create a key manager with a test key
        let mut key_manager = ConcreteKeyManager::new([1u8; 32]);
        let test_key = PrivateKey::random();
        let imported_key = ImportedPrivateKey::new(
            test_key.clone(),
            Some("test_key".to_string()),
        );
        key_manager.import_private_key(imported_key).unwrap();

        // Generate a stealth address
        let recipient_public_key = CompressedPublicKey::from_private_key(&test_key);
        let stealth_address = StealthAddressManager::generate_stealth_address(
            &PrivateKey::random(),
            &recipient_public_key,
        ).unwrap();

        // Create a test transaction output encrypted with the stealth key
        let stealth_private_key = StealthAddressManager::recover_stealth_private_key(
            &test_key,
            &stealth_address.ephemeral_public_key,
        ).unwrap();

        let encrypted_data = EncryptedData::encrypt_data(
            &stealth_private_key,
            &CompressedCommitment::new([1u8; 33]),
            MicroMinotari::new(1000),
            &PrivateKey::random(),
            PaymentId::Empty,
        ).unwrap();

        let transaction_output = LightweightTransactionOutput::new(
            1,
            crate::data_structures::wallet_output::LightweightOutputFeatures::default(),
            CompressedCommitment::new([1u8; 33]),
            Some(crate::data_structures::wallet_output::LightweightRangeProof::default()),
            crate::data_structures::wallet_output::LightweightScript::default(),
            CompressedPublicKey::new([2u8; 32]),
            crate::data_structures::wallet_output::LightweightSignature::default(),
            crate::data_structures::wallet_output::LightweightCovenant::default(),
            encrypted_data,
            MicroMinotari::new(1000),
        );

        // Create recovery manager and recover the stealth key
        let recovery_manager = StealthKeyRecoveryManager::new(key_manager);
        let result = recovery_manager.recover_and_decrypt(
            &transaction_output,
            &stealth_address.ephemeral_public_key,
        ).unwrap();

        // Verify the result
        assert!(result.success);
        assert_eq!(result.recovery_key_id, "test_key");
    }

    #[test]
    fn test_stealth_key_recovery_no_key() {
        // Create a key manager without any keys
        let key_manager = ConcreteKeyManager::new([1u8; 32]);

        // Generate a stealth address
        let test_key = PrivateKey::random();
        let recipient_public_key = CompressedPublicKey::from_private_key(&test_key);
        let stealth_address = StealthAddressManager::generate_stealth_address(
            &PrivateKey::random(),
            &recipient_public_key,
        ).unwrap();

        // Create recovery manager with options that disable key derivation
        let mut options = StealthKeyRecoveryOptions::default();
        options.try_all_keys = false;
        let recovery_manager = StealthKeyRecoveryManager::with_options(key_manager, options);
        let result = recovery_manager.recover_stealth_private_key(
            &stealth_address.ephemeral_public_key,
        );

        // Debug: Print the actual result
        println!("DEBUG: Actual result: {:?}", result);

        // Should fail with no suitable key
        assert!(matches!(
            result,
            Err(StealthKeyRecoveryError::NoSuitableKey)
        ));
    }
}
*/
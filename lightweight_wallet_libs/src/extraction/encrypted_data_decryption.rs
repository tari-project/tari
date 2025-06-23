// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Encrypted data decryption using provided keys
//!
//! This module provides functionality to decrypt encrypted data from transaction outputs
//! using various types of keys (derived keys, imported keys, etc.).

use crate::{
    data_structures::{
        encrypted_data::EncryptedData,
        payment_id::PaymentId,
        types::{CompressedCommitment, MicroMinotari, PrivateKey},
        transaction_output::LightweightTransactionOutput,
    },
    errors::{LightweightWalletError, EncryptionError, KeyManagementError},
    key_management::{KeyManager, KeyStore, ImportedPrivateKey},
};

/// Options for encrypted data decryption
#[derive(Debug, Clone)]
pub struct DecryptionOptions {
    /// Whether to try all available keys if the first one fails
    pub try_all_keys: bool,
    /// Whether to validate the decrypted data
    pub validate_decrypted_data: bool,
    /// Maximum number of keys to try (0 = unlimited)
    pub max_keys_to_try: usize,
    /// Whether to return partial results on failure
    pub return_partial_results: bool,
}

impl Default for DecryptionOptions {
    fn default() -> Self {
        Self {
            try_all_keys: true,
            validate_decrypted_data: true,
            max_keys_to_try: 0, // Unlimited
            return_partial_results: false,
        }
    }
}

/// Result of encrypted data decryption
#[derive(Debug, Clone)]
pub struct DecryptionResult {
    /// Whether the decryption was successful
    pub success: bool,
    /// The decrypted value (if successful)
    pub value: Option<MicroMinotari>,
    /// The decrypted mask (if successful)
    pub mask: Option<PrivateKey>,
    /// The extracted payment ID (if successful)
    pub payment_id: Option<PaymentId>,
    /// The key that was used for decryption (if successful)
    pub used_key: Option<PrivateKey>,
    /// Error message if decryption failed
    pub error: Option<String>,
    /// Number of keys tried
    pub keys_tried: usize,
}

impl DecryptionResult {
    /// Create a successful decryption result
    pub fn success(
        value: MicroMinotari,
        mask: PrivateKey,
        payment_id: PaymentId,
        used_key: PrivateKey,
        keys_tried: usize,
    ) -> Self {
        Self {
            success: true,
            value: Some(value),
            mask: Some(mask),
            payment_id: Some(payment_id),
            used_key: Some(used_key),
            error: None,
            keys_tried,
        }
    }

    /// Create a failed decryption result
    pub fn failure(error: String, keys_tried: usize) -> Self {
        Self {
            success: false,
            value: None,
            mask: None,
            payment_id: None,
            used_key: None,
            error: Some(error),
            keys_tried,
        }
    }

    /// Check if the decryption was successful
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Get the error message if decryption failed
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Encrypted data decryptor for lightweight wallets
#[derive(Debug, Clone)]
pub struct EncryptedDataDecryptor {
    /// Key store containing available keys
    key_store: KeyStore,
    /// Default decryption options
    default_options: DecryptionOptions,
}

impl EncryptedDataDecryptor {
    /// Create a new encrypted data decryptor
    pub fn new(key_store: KeyStore) -> Self {
        Self {
            key_store,
            default_options: DecryptionOptions::default(),
        }
    }

    /// Create a new encrypted data decryptor with custom options
    pub fn with_options(key_store: KeyStore, options: DecryptionOptions) -> Self {
        Self {
            key_store,
            default_options: options,
        }
    }

    /// Get the key store
    pub fn key_store(&self) -> &KeyStore {
        &self.key_store
    }

    /// Get a mutable reference to the key store
    pub fn key_store_mut(&mut self) -> &mut KeyStore {
        &mut self.key_store
    }

    /// Set the default decryption options
    pub fn set_default_options(&mut self, options: DecryptionOptions) {
        self.default_options = options;
    }

    /// Get the default decryption options
    pub fn default_options(&self) -> &DecryptionOptions {
        &self.default_options
    }

    /// Decrypt encrypted data using a specific key
    ///
    /// # Arguments
    /// * `encrypted_data` - The encrypted data to decrypt
    /// * `commitment` - The commitment associated with the encrypted data
    /// * `key` - The private key to use for decryption
    /// * `options` - Decryption options
    ///
    /// # Returns
    /// * `Ok(DecryptionResult)` with the decryption result
    /// * `Err(LightweightWalletError)` if an error occurred
    pub fn decrypt_with_key(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        key: &PrivateKey,
        options: &DecryptionOptions,
    ) -> Result<DecryptionResult, LightweightWalletError> {
        match EncryptedData::decrypt_data(key, commitment, encrypted_data) {
            Ok((value, mask, payment_id)) => {
                // Validate decrypted data if requested
                if options.validate_decrypted_data {
                    self.validate_decrypted_data(&value, &mask, &payment_id)?;
                }

                Ok(DecryptionResult::success(
                    value,
                    mask,
                    payment_id,
                    key.clone(),
                    1,
                ))
            }
            Err(e) => {
                let error_msg = match e {
                    LightweightWalletError::EncryptionError(EncryptionError::DecryptionFailed(msg)) => {
                        format!("Decryption failed: {}", msg)
                    }
                    _ => format!("Decryption error: {}", e),
                };
                Ok(DecryptionResult::failure(error_msg, 1))
            }
        }
    }

    /// Decrypt encrypted data using all available keys
    ///
    /// # Arguments
    /// * `encrypted_data` - The encrypted data to decrypt
    /// * `commitment` - The commitment associated with the encrypted data
    /// * `options` - Decryption options (if None, uses default options)
    ///
    /// # Returns
    /// * `Ok(DecryptionResult)` with the decryption result
    /// * `Err(LightweightWalletError)` if an error occurred
    pub fn decrypt_with_all_keys(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        options: Option<&DecryptionOptions>,
    ) -> Result<DecryptionResult, LightweightWalletError> {
        let options = options.unwrap_or(&self.default_options);
        let mut keys_tried = 0;
        let max_keys = if options.max_keys_to_try == 0 {
            usize::MAX
        } else {
            options.max_keys_to_try
        };

        // Try imported keys first
        for imported_key in self.key_store.get_imported_keys() {
            if keys_tried >= max_keys {
                break;
            }

            let result = self.decrypt_with_key(
                encrypted_data,
                commitment,
                &imported_key.private_key,
                options,
            )?;

            keys_tried += 1;

            if result.success {
                return Ok(result);
            }
        }

        // Try derived keys if we haven't reached the limit
        if keys_tried < max_keys {
            // For now, we'll try a reasonable range of derived keys
            // In a full implementation, this would be more sophisticated
            let start_index = self.key_store.current_key_index().saturating_sub(10);
            let end_index = self.key_store.current_key_index() + 10;

            for key_index in start_index..=end_index {
                if keys_tried >= max_keys {
                    break;
                }

                // Try to derive a key at this index
                // Note: This is a simplified approach - in practice, you'd need
                // the actual key derivation logic from the key manager
                if let Ok(derived_key) = self.try_derive_key_at_index(key_index) {
                    let result = self.decrypt_with_key(
                        encrypted_data,
                        commitment,
                        &derived_key,
                        options,
                    )?;

                    keys_tried += 1;

                    if result.success {
                        return Ok(result);
                    }
                }
            }
        }

        // If we get here, no key worked
        Ok(DecryptionResult::failure(
            "No valid key found for decryption".to_string(),
            keys_tried,
        ))
    }

    /// Decrypt encrypted data from a transaction output
    ///
    /// # Arguments
    /// * `transaction_output` - The transaction output containing encrypted data
    /// * `options` - Decryption options (if None, uses default options)
    ///
    /// # Returns
    /// * `Ok(DecryptionResult)` with the decryption result
    /// * `Err(LightweightWalletError)` if an error occurred
    pub fn decrypt_transaction_output(
        &self,
        transaction_output: &LightweightTransactionOutput,
        options: Option<&DecryptionOptions>,
    ) -> Result<DecryptionResult, LightweightWalletError> {
        let encrypted_data = transaction_output.encrypted_data();
        let commitment = transaction_output.commitment();

        self.decrypt_with_all_keys(encrypted_data, commitment, options)
    }

    /// Try to decrypt with a specific key index (for derived keys)
    ///
    /// # Arguments
    /// * `key_index` - The key index to try
    ///
    /// # Returns
    /// * `Ok(PrivateKey)` if the key was successfully derived
    /// * `Err(LightweightWalletError)` if key derivation failed
    fn try_derive_key_at_index(&self, key_index: u64) -> Result<PrivateKey, LightweightWalletError> {
        // This is a simplified implementation
        // In practice, you'd need the actual key derivation logic from the key manager
        // For now, we'll return an error to indicate that this needs to be implemented
        Err(KeyManagementError::KeyDerivationFailed(
            "Key derivation not yet implemented".to_string()
        ).into())
    }

    /// Validate decrypted data
    ///
    /// # Arguments
    /// * `value` - The decrypted value
    /// * `mask` - The decrypted mask
    /// * `payment_id` - The extracted payment ID
    ///
    /// # Returns
    /// * `Ok(())` if the data is valid
    /// * `Err(LightweightWalletError)` if the data is invalid
    fn validate_decrypted_data(
        &self,
        value: &MicroMinotari,
        mask: &PrivateKey,
        payment_id: &PaymentId,
    ) -> Result<(), LightweightWalletError> {
        // Validate value is reasonable (not zero unless it's a special case)
        if value.as_u64() == 0 {
            // Zero values might be valid in some cases (e.g., burn outputs)
            // but we should log a warning
            // In a full implementation, you might want to check the output features
        }

        // Validate mask is not all zeros
        if mask.as_bytes().iter().all(|&b| b == 0) {
            return Err(EncryptionError::decryption_failed(
                "Decrypted mask is all zeros"
            ).into());
        }

        // Validate payment ID structure
        match payment_id {
            PaymentId::Empty => {
                // Empty payment ID is always valid
            }
            PaymentId::U256 { .. } => {
                // U256 payment ID is always valid
            }
            PaymentId::Open { .. } => {
                // Open payment ID is always valid
            }
            PaymentId::AddressAndData { address, data } => {
                // Validate address is not all zeros
                if address.iter().all(|&b| b == 0) {
                    return Err(EncryptionError::decryption_failed(
                        "Payment ID address is all zeros"
                    ).into());
                }
                // Validate data is not empty
                if data.is_empty() {
                    return Err(EncryptionError::decryption_failed(
                        "Payment ID data is empty"
                    ).into());
                }
            }
            PaymentId::TransactionInfo { .. } => {
                // Transaction info payment ID is always valid
            }
            PaymentId::Raw { data } => {
                // Validate raw data is not empty
                if data.is_empty() {
                    return Err(EncryptionError::decryption_failed(
                        "Raw payment ID data is empty"
                    ).into());
                }
            }
        }

        Ok(())
    }

    /// Add an imported key to the key store
    ///
    /// # Arguments
    /// * `imported_key` - The imported key to add
    ///
    /// # Returns
    /// * `Ok(())` if the key was added successfully
    /// * `Err(LightweightWalletError)` if adding the key failed
    pub fn add_imported_key(&mut self, imported_key: ImportedPrivateKey) -> Result<(), LightweightWalletError> {
        self.key_store.add_imported_key(imported_key)
            .map_err(|e| e.into())
    }

    /// Import a private key from hex string
    ///
    /// # Arguments
    /// * `hex` - The hex string containing the private key
    /// * `label` - Optional label for the imported key
    ///
    /// # Returns
    /// * `Ok(())` if the key was imported successfully
    /// * `Err(LightweightWalletError)` if importing the key failed
    pub fn import_private_key_from_hex(&mut self, hex: &str, label: Option<String>) -> Result<(), LightweightWalletError> {
        self.key_store.import_private_key_from_hex(hex, label)
            .map_err(|e| e.into())
    }

    /// Import a private key from bytes
    ///
    /// # Arguments
    /// * `bytes` - The bytes containing the private key
    /// * `label` - Optional label for the imported key
    ///
    /// # Returns
    /// * `Ok(())` if the key was imported successfully
    /// * `Err(LightweightWalletError)` if importing the key failed
    pub fn import_private_key_from_bytes(&mut self, bytes: [u8; 32], label: Option<String>) -> Result<(), LightweightWalletError> {
        self.key_store.import_private_key_from_bytes(bytes, label)
            .map_err(|e| e.into())
    }

    /// Get the number of imported keys
    pub fn imported_key_count(&self) -> usize {
        self.key_store.imported_key_count()
    }

    /// Get the number of derived keys
    pub fn derived_key_count(&self) -> usize {
        self.key_store.derived_key_count()
    }

    /// Get the total number of keys
    pub fn total_key_count(&self) -> usize {
        self.key_store.total_key_count()
    }
}

impl Default for EncryptedDataDecryptor {
    fn default() -> Self {
        Self::new(KeyStore::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::{
        payment_id::PaymentId,
        types::{CompressedCommitment, PrivateKey},
    };

    fn create_test_encrypted_data() -> (EncryptedData, CompressedCommitment, PrivateKey) {
        let encryption_key = PrivateKey::random();
        let commitment = CompressedCommitment::new([0x08; 33]);
        let value = MicroMinotari::new(1000);
        let mask = PrivateKey::random();
        let payment_id = PaymentId::Empty;

        let encrypted_data = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id,
        ).unwrap();

        (encrypted_data, commitment, encryption_key)
    }

    #[test]
    fn test_decryptor_creation() {
        let key_store = KeyStore::default();
        let decryptor = EncryptedDataDecryptor::new(key_store);
        
        assert_eq!(decryptor.imported_key_count(), 0);
        assert_eq!(decryptor.derived_key_count(), 0);
        assert_eq!(decryptor.total_key_count(), 0);
    }

    #[test]
    fn test_decrypt_with_correct_key() {
        let (encrypted_data, commitment, key) = create_test_encrypted_data();
        let key_store = KeyStore::default();
        let decryptor = EncryptedDataDecryptor::new(key_store);
        let options = DecryptionOptions::default();

        let result = decryptor.decrypt_with_key(&encrypted_data, &commitment, &key, &options).unwrap();

        assert!(result.is_success());
        assert_eq!(result.value.unwrap(), MicroMinotari::new(1000));
        assert_eq!(result.keys_tried, 1);
    }

    #[test]
    fn test_decrypt_with_wrong_key() {
        let (encrypted_data, commitment, _) = create_test_encrypted_data();
        let wrong_key = PrivateKey::random();
        let key_store = KeyStore::default();
        let decryptor = EncryptedDataDecryptor::new(key_store);
        let options = DecryptionOptions::default();

        let result = decryptor.decrypt_with_key(&encrypted_data, &commitment, &wrong_key, &options).unwrap();

        assert!(!result.is_success());
        assert!(result.error_message().is_some());
        assert_eq!(result.keys_tried, 1);
    }

    #[test]
    fn test_decrypt_with_imported_key() {
        let (encrypted_data, commitment, key) = create_test_encrypted_data();
        let mut key_store = KeyStore::default();
        let imported_key = ImportedPrivateKey::new(key.clone(), Some("test_key".to_string()));
        key_store.add_imported_key(imported_key).unwrap();
        
        let decryptor = EncryptedDataDecryptor::new(key_store);
        let options = DecryptionOptions::default();

        let result = decryptor.decrypt_with_all_keys(&encrypted_data, &commitment, Some(&options)).unwrap();

        assert!(result.is_success());
        assert_eq!(result.value.unwrap(), MicroMinotari::new(1000));
        assert_eq!(result.used_key.as_ref().unwrap(), &key);
    }

    #[test]
    fn test_import_private_key_from_hex() {
        let mut decryptor = EncryptedDataDecryptor::default();
        let key = PrivateKey::random();
        let hex = key.to_hex();

        let result = decryptor.import_private_key_from_hex(&hex, Some("test".to_string()));
        assert!(result.is_ok());
        assert_eq!(decryptor.imported_key_count(), 1);
    }

    #[test]
    fn test_import_private_key_from_bytes() {
        let mut decryptor = EncryptedDataDecryptor::default();
        let key_bytes = [1u8; 32];

        let result = decryptor.import_private_key_from_bytes(key_bytes, Some("test".to_string()));
        assert!(result.is_ok());
        assert_eq!(decryptor.imported_key_count(), 1);
    }

    #[test]
    fn test_decryption_options() {
        let options = DecryptionOptions {
            try_all_keys: false,
            validate_decrypted_data: true,
            max_keys_to_try: 5,
            return_partial_results: true,
        };

        assert!(!options.try_all_keys);
        assert!(options.validate_decrypted_data);
        assert_eq!(options.max_keys_to_try, 5);
        assert!(options.return_partial_results);
    }

    #[test]
    fn test_decryption_result_success() {
        let value = MicroMinotari::new(1000);
        let mask = PrivateKey::random();
        let payment_id = PaymentId::Empty;
        let used_key = PrivateKey::random();

        let result = DecryptionResult::success(value, mask.clone(), payment_id.clone(), used_key.clone(), 1);

        assert!(result.is_success());
        assert_eq!(result.value.unwrap(), value);
        assert_eq!(result.mask.as_ref().unwrap(), &mask);
        assert_eq!(result.payment_id.as_ref().unwrap(), &payment_id);
        assert_eq!(result.used_key.as_ref().unwrap(), &used_key);
        assert_eq!(result.keys_tried, 1);
        assert!(result.error_message().is_none());
    }

    #[test]
    fn test_decryption_result_failure() {
        let error = "Test error".to_string();
        let result = DecryptionResult::failure(error.clone(), 5);

        assert!(!result.is_success());
        assert!(result.value.is_none());
        assert!(result.mask.is_none());
        assert!(result.payment_id.is_none());
        assert!(result.used_key.is_none());
        assert_eq!(result.keys_tried, 5);
        assert_eq!(result.error_message().unwrap(), &error);
    }
} 
// Copyright 2022 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

//! Lightweight key management and derivation for Tari wallet
//! 
//! This module provides simplified key management functionality for lightweight wallets,
//! including deterministic key derivation from seed phrases and imported private keys.

pub mod seed_phrase;
pub mod key_derivation;
pub mod stealth_address;

use crate::data_structures::types::PrivateKey;
use crate::errors::KeyManagementError;
use zeroize::Zeroize;

/// Key derivation path for deterministic key generation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyDerivationPath {
    /// Branch Seed
    pub branch_seed: String,
    
    /// Key Index
    pub key_index: u64,
}

impl KeyDerivationPath {
    /// Create a new key derivation path
    pub fn new(branch_seed: String, key_index: u64) -> Self {
        Self {
            branch_seed,
            key_index,
        }
    }

    /// Create a standard Tari key derivation path
    pub fn tari_standard(account: u32, change: u32, address_index: u32) -> Self {
        Self::new("".to_string(), 0)
    }

    /// Convert path to string representation (e.g., "m/44'/123456'/0'/0/0")
    pub fn to_string(&self) -> String {
        format!(
            "m/{}'/{:06}'",
            self.branch_seed, self.key_index
        )
    }

    /// Parse path from string representation
    pub fn from_string(path: &str) -> Result<Self, KeyManagementError> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 2 || parts[0] != "m" {
            return Err(KeyManagementError::InvalidKeyDerivationPath(
                "Invalid path format".to_string()
            ));
        }

        let key_index = parts[1].trim_end_matches('\'').parse::<u64>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid key index".to_string()))?;

        Ok(Self {
            branch_seed: "".to_string(),
            key_index,
        })
    }
}

impl Default for KeyDerivationPath {
    fn default() -> Self {
        Self::new("".to_string(), 0)
    }
}

/// Derived key pair with index information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedKeyPair {
    /// The derived private key
    pub private_key: PrivateKey,
    /// The derived public key
    pub public_key: crate::data_structures::types::CompressedPublicKey,
    /// The key index
    pub key_index: u64,
    /// The derivation path used
    pub derivation_path: KeyDerivationPath,
}

impl DerivedKeyPair {
    /// Create a new derived key pair
    pub fn new(
        private_key: PrivateKey,
        public_key: crate::data_structures::types::CompressedPublicKey,
        key_index: u64,
        derivation_path: KeyDerivationPath,
    ) -> Self {
        Self {
            private_key,
            public_key,
            key_index,
            derivation_path,
        }
    }
}

/// Key manager for deterministic key derivation
pub trait KeyManager {
    /// Derive a key pair from the given path
    fn derive_key_pair(&self, path: &KeyDerivationPath) -> Result<DerivedKeyPair, KeyManagementError>;
    
    /// Derive a private key from the given path
    fn derive_private_key(&self, path: &KeyDerivationPath) -> Result<PrivateKey, KeyManagementError>;
    
    /// Derive a public key from the given path
    fn derive_public_key(&self, path: &KeyDerivationPath) -> Result<crate::data_structures::types::CompressedPublicKey, KeyManagementError>;
    
    /// Get the next key pair in sequence
    fn next_key_pair(&mut self) -> Result<DerivedKeyPair, KeyManagementError>;
    
    /// Get the current key index
    fn current_key_index(&self) -> u64;
    
    /// Update the current key index
    fn update_key_index(&mut self, new_index: u64);
}

/// Imported private key with metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPrivateKey {
    /// The imported private key
    pub private_key: PrivateKey,
    /// Optional label for the imported key
    pub label: Option<String>,
    /// Whether the key is from a seed phrase
    pub from_seed_phrase: bool,
    /// Optional derivation path if from seed phrase
    pub derivation_path: Option<KeyDerivationPath>,
}

impl ImportedPrivateKey {
    /// Create a new imported private key
    pub fn new(private_key: PrivateKey, label: Option<String>) -> Self {
        Self {
            private_key,
            label,
            from_seed_phrase: false,
            derivation_path: None,
        }
    }

    /// Create a new imported private key from seed phrase
    pub fn from_seed_phrase(
        private_key: PrivateKey,
        derivation_path: KeyDerivationPath,
        label: Option<String>,
    ) -> Self {
        Self {
            private_key,
            label,
            from_seed_phrase: true,
            derivation_path: Some(derivation_path),
        }
    }
}

impl Zeroize for ImportedPrivateKey {
    fn zeroize(&mut self) {
        self.private_key.zeroize();
        // Clear label and derivation path for additional security
        self.label = None;
        self.derivation_path = None;
    }
}

impl Drop for ImportedPrivateKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

pub use seed_phrase::{mnemonic_to_master_key, generate_seed_phrase, validate_seed_phrase, bytes_to_mnemonic, mnemonic_to_bytes, CipherSeed};
pub use stealth_address::{StealthAddress, StealthAddressManager};
pub use key_derivation::{derive_view_and_spend_keys_from_entropy, derive_private_key_from_entropy};

/// Key store for managing both derived and imported keys
#[derive(Debug)]
pub struct KeyStore {
    /// Derived keys from seed phrase
    derived_keys: Vec<DerivedKeyPair>,
    /// Imported private keys
    imported_keys: Vec<ImportedPrivateKey>,
    /// Current key index for derived keys
    current_key_index: u64,
}

impl KeyStore {
    /// Create a new empty key store
    pub fn new() -> Self {
        Self {
            derived_keys: Vec::new(),
            imported_keys: Vec::new(),
            current_key_index: 0,
        }
    }

    /// Add an imported private key to the store
    pub fn add_imported_key(&mut self, imported_key: ImportedPrivateKey) -> Result<(), KeyManagementError> {
        // Check for duplicates (by comparing private key bytes)
        for existing_key in &self.imported_keys {
            if existing_key.private_key.as_bytes() == imported_key.private_key.as_bytes() {
                return Err(KeyManagementError::KeyImportFailed(
                    "Private key already exists in store".to_string()
                ));
            }
        }
        
        self.imported_keys.push(imported_key);
        Ok(())
    }

    /// Import a private key from hex string
    pub fn import_private_key_from_hex(&mut self, hex: &str, label: Option<String>) -> Result<(), KeyManagementError> {
        let private_key = PrivateKey::from_hex(hex)
            .map_err(|e| KeyManagementError::InvalidPrivateKey(e.to_string()))?;
        
        let imported_key = ImportedPrivateKey::new(private_key, label);
        self.add_imported_key(imported_key)
    }

    /// Import a private key from bytes
    pub fn import_private_key_from_bytes(&mut self, bytes: [u8; 32], label: Option<String>) -> Result<(), KeyManagementError> {
        let private_key = PrivateKey::new(bytes);
        let imported_key = ImportedPrivateKey::new(private_key, label);
        self.add_imported_key(imported_key)
    }

    /// Get all imported keys
    pub fn get_imported_keys(&self) -> &[ImportedPrivateKey] {
        &self.imported_keys
    }

    /// Get imported key by index
    pub fn get_imported_key(&self, index: usize) -> Result<&ImportedPrivateKey, KeyManagementError> {
        self.imported_keys.get(index)
            .ok_or_else(|| KeyManagementError::KeyNotFound(format!("Imported key at index {}", index)))
    }

    /// Get imported key by label
    pub fn get_imported_key_by_label(&self, label: &str) -> Result<&ImportedPrivateKey, KeyManagementError> {
        self.imported_keys.iter()
            .find(|key| key.label.as_ref().map_or(false, |l| l == label))
            .ok_or_else(|| KeyManagementError::KeyNotFound(format!("Imported key with label '{}'", label)))
    }

    /// Remove imported key by index
    pub fn remove_imported_key(&mut self, index: usize) -> Result<ImportedPrivateKey, KeyManagementError> {
        if index >= self.imported_keys.len() {
            return Err(KeyManagementError::KeyNotFound(format!("Imported key at index {}", index)));
        }
        Ok(self.imported_keys.remove(index))
    }

    /// Get the number of imported keys
    pub fn imported_key_count(&self) -> usize {
        self.imported_keys.len()
    }

    /// Get the number of derived keys
    pub fn derived_key_count(&self) -> usize {
        self.derived_keys.len()
    }

    /// Get total number of keys (derived + imported)
    pub fn total_key_count(&self) -> usize {
        self.derived_keys.len() + self.imported_keys.len()
    }

    /// Get current key index for derived keys
    pub fn current_key_index(&self) -> u64 {
        self.current_key_index
    }

    /// Update current key index for derived keys
    pub fn update_key_index(&mut self, new_index: u64) {
        self.current_key_index = new_index;
    }

    /// Clear all keys from the store (secure)
    pub fn clear(&mut self) {
        // Zeroize all private keys before clearing
        for key_pair in &mut self.derived_keys {
            key_pair.private_key.zeroize();
        }
        for imported_key in &mut self.imported_keys {
            imported_key.zeroize();
        }
        
        self.derived_keys.clear();
        self.imported_keys.clear();
        self.current_key_index = 0;
    }

    /// Get a copy of the key store with zeroized sensitive data
    pub fn clone_public_only(&self) -> Self {
        Self {
            derived_keys: Vec::new(), // Don't clone private keys
            imported_keys: Vec::new(), // Don't clone private keys
            current_key_index: self.current_key_index,
        }
    }
}

impl Zeroize for KeyStore {
    fn zeroize(&mut self) {
        self.clear();
    }
}

impl Drop for KeyStore {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl Clone for KeyStore {
    fn clone(&self) -> Self {
        // Only clone public data, not private keys
        self.clone_public_only()
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Zeroize for DerivedKeyPair {
    fn zeroize(&mut self) {
        self.private_key.zeroize();
    }
}

impl Drop for DerivedKeyPair {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Secure key buffer for temporary key operations
pub struct SecureKeyBuffer {
    /// The key data
    data: Vec<u8>,
}

impl SecureKeyBuffer {
    /// Create a new secure key buffer
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create a new secure key buffer with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Get the key data as a slice
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Get the key data as a mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Get the length of the key data
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_len: usize, value: u8) {
        self.data.resize(new_len, value);
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Extend the buffer with data
    pub fn extend_from_slice(&mut self, other: &[u8]) {
        self.data.extend_from_slice(other);
    }
}

impl Zeroize for SecureKeyBuffer {
    fn zeroize(&mut self) {
        self.data.zeroize();
    }
}

impl Drop for SecureKeyBuffer {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl From<Vec<u8>> for SecureKeyBuffer {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl From<&[u8]> for SecureKeyBuffer {
    fn from(data: &[u8]) -> Self {
        Self::new(data.to_vec())
    }
}

/// Secure key operations trait
pub trait SecureKeyOps {
    /// Securely copy key data
    fn secure_copy(&self) -> Self;
    
    /// Securely compare with another key
    fn secure_compare(&self, other: &Self) -> bool;
    
    /// Securely clear the key data
    fn secure_clear(&mut self);
}

impl SecureKeyOps for PrivateKey {
    fn secure_copy(&self) -> Self {
        Self::new(self.as_bytes())
    }
    
    fn secure_compare(&self, other: &Self) -> bool {
        // Use constant-time comparison to prevent timing attacks
        let self_bytes = self.as_bytes();
        let other_bytes = other.as_bytes();
        
        if self_bytes.len() != other_bytes.len() {
            return false;
        }
        
        let mut result = 0u8;
        for (a, b) in self_bytes.iter().zip(other_bytes.iter()) {
            result |= a ^ b;
        }
        
        result == 0
    }
    
    fn secure_clear(&mut self) {
        self.zeroize();
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::types::CompressedPublicKey;


    #[test]
    fn test_imported_private_key() {
        let private_key = PrivateKey::new([1u8; 32]);
        let imported = ImportedPrivateKey::new(private_key.clone(), Some("test".to_string()));
        assert_eq!(imported.private_key, private_key);
        assert_eq!(imported.label, Some("test".to_string()));
        assert!(!imported.from_seed_phrase);
        assert!(imported.derivation_path.is_none());
    }

    #[test]
    fn test_key_store_creation() {
        let store = KeyStore::new();
        assert_eq!(store.imported_key_count(), 0);
        assert_eq!(store.derived_key_count(), 0);
        assert_eq!(store.total_key_count(), 0);
        assert_eq!(store.current_key_index(), 0);
    }

    #[test]
    fn test_import_private_key_from_bytes() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        
        store.import_private_key_from_bytes(key_bytes, Some("test_key".to_string())).unwrap();
        
        assert_eq!(store.imported_key_count(), 1);
        assert_eq!(store.total_key_count(), 1);
        
        let imported_key = store.get_imported_key(0).unwrap();
        assert_eq!(imported_key.private_key.as_bytes(), key_bytes);
        assert_eq!(imported_key.label, Some("test_key".to_string()));
    }

    #[test]
    fn test_import_private_key_from_hex() {
        let mut store = KeyStore::new();
        let hex_key = "0101010101010101010101010101010101010101010101010101010101010101";
        
        store.import_private_key_from_hex(hex_key, Some("hex_key".to_string())).unwrap();
        
        assert_eq!(store.imported_key_count(), 1);
        
        let imported_key = store.get_imported_key(0).unwrap();
        assert_eq!(imported_key.private_key.to_hex(), hex_key);
        assert_eq!(imported_key.label, Some("hex_key".to_string()));
    }

    #[test]
    fn test_import_duplicate_key() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        
        store.import_private_key_from_bytes(key_bytes, Some("key1".to_string())).unwrap();
        
        // Try to import the same key again
        let result = store.import_private_key_from_bytes(key_bytes, Some("key2".to_string()));
        assert!(result.is_err());
        assert_eq!(store.imported_key_count(), 1);
    }

    #[test]
    fn test_get_imported_key_by_label() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        
        store.import_private_key_from_bytes(key_bytes, Some("test_label".to_string())).unwrap();
        
        let imported_key = store.get_imported_key_by_label("test_label").unwrap();
        assert_eq!(imported_key.private_key.as_bytes(), key_bytes);
        assert_eq!(imported_key.label, Some("test_label".to_string()));
    }

    #[test]
    fn test_get_imported_key_by_nonexistent_label() {
        let store = KeyStore::new();
        let result = store.get_imported_key_by_label("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_imported_key() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        
        store.import_private_key_from_bytes(key_bytes, Some("test_key".to_string())).unwrap();
        assert_eq!(store.imported_key_count(), 1);
        
        let removed_key = store.remove_imported_key(0).unwrap();
        assert_eq!(removed_key.private_key.as_bytes(), key_bytes);
        assert_eq!(store.imported_key_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let mut store = KeyStore::new();
        let result = store.remove_imported_key(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_store_update_index() {
        let mut store = KeyStore::new();
        store.update_key_index(42);
        assert_eq!(store.current_key_index(), 42);
    }

    #[test]
    fn test_secure_key_buffer() {
        let data = vec![1, 2, 3, 4, 5];
        let mut buffer = SecureKeyBuffer::new(data.clone());
        
        assert_eq!(buffer.as_slice(), &data);
        assert_eq!(buffer.len(), 5);
        assert!(!buffer.is_empty());
        
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_secure_key_buffer_with_capacity() {
        let buffer = SecureKeyBuffer::with_capacity(32);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_secure_key_buffer_resize() {
        let mut buffer = SecureKeyBuffer::new(vec![1, 2, 3]);
        buffer.resize(5, 0);
        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.as_slice(), &[1, 2, 3, 0, 0]);
    }

    #[test]
    fn test_secure_key_buffer_extend() {
        let mut buffer = SecureKeyBuffer::new(vec![1, 2, 3]);
        buffer.extend_from_slice(&[4, 5, 6]);
        assert_eq!(buffer.as_slice(), &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_secure_key_ops() {
        let key1 = PrivateKey::new([1u8; 32]);
        let key2 = PrivateKey::new([1u8; 32]);
        let key3 = PrivateKey::new([2u8; 32]);
        
        // Test secure copy
        let key1_copy = key1.secure_copy();
        assert!(key1.secure_compare(&key1_copy));
        
        // Test secure compare
        assert!(key1.secure_compare(&key2));
        assert!(!key1.secure_compare(&key3));
        
        // Test secure clear
        let mut key_to_clear = key1.secure_copy();
        key_to_clear.secure_clear();
        // After clearing, the key should be zeroized
        assert_eq!(key_to_clear.as_bytes(), [0u8; 32]);
    }

    #[test]
    fn test_imported_private_key_zeroization() {
        let private_key = PrivateKey::new([1u8; 32]);
        let mut imported = ImportedPrivateKey::new(private_key, Some("test".to_string()));
        
        // Verify the key exists
        assert_eq!(imported.private_key.as_bytes(), [1u8; 32]);
        assert_eq!(imported.label, Some("test".to_string()));
        
        // Zeroize the key
        imported.zeroize();
        
        // Verify the key is zeroized and metadata is cleared
        assert_eq!(imported.private_key.as_bytes(), [0u8; 32]);
        assert_eq!(imported.label, None);
        assert_eq!(imported.derivation_path, None);
    }

    #[test]
    fn test_derived_key_pair_zeroization() {
        let private_key = PrivateKey::new([1u8; 32]);
        let public_key = CompressedPublicKey::from_private_key(&private_key);
        let path = KeyDerivationPath::tari_standard(0, 0, 0);
        let mut key_pair = DerivedKeyPair::new(private_key, public_key, 0, path);
        
        // Verify the key exists
        assert_eq!(key_pair.private_key.as_bytes(), [1u8; 32]);
        
        // Zeroize the key
        key_pair.zeroize();
        
        // Verify the key is zeroized
        assert_eq!(key_pair.private_key.as_bytes(), [0u8; 32]);
    }

    #[test]
    fn test_key_store_secure_clone() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        store.import_private_key_from_bytes(key_bytes, Some("test_key".to_string())).unwrap();
        
        // Clone the store
        let cloned_store = store.clone();
        
        // Verify that the cloned store doesn't contain private keys
        assert_eq!(cloned_store.imported_key_count(), 0);
        assert_eq!(cloned_store.derived_key_count(), 0);
        assert_eq!(cloned_store.total_key_count(), 0);
        
        // But the original store still has the key
        assert_eq!(store.imported_key_count(), 1);
        assert_eq!(store.total_key_count(), 1);
    }

    #[test]
    fn test_key_store_clear() {
        let mut store = KeyStore::new();
        let key_bytes = [1u8; 32];
        store.import_private_key_from_bytes(key_bytes, Some("test_key".to_string())).unwrap();
        
        // Verify the key exists
        assert_eq!(store.imported_key_count(), 1);
        
        // Clear the store
        store.clear();
        
        // Verify the store is empty
        assert_eq!(store.imported_key_count(), 0);
        assert_eq!(store.derived_key_count(), 0);
        assert_eq!(store.total_key_count(), 0);
        assert_eq!(store.current_key_index(), 0);
    }

    #[test]
    fn test_secure_key_buffer_zeroization() {
        let data = vec![1, 2, 3, 4, 5];
        let mut buffer = SecureKeyBuffer::new(data);
        
        // Verify the data exists
        assert_eq!(buffer.as_slice(), &[1, 2, 3, 4, 5]);
        
        // Zeroize the buffer
        buffer.zeroize();
        
        // Verify the buffer is zeroized
        assert_eq!(buffer.as_slice(), &[] as &[u8]);
        
        // Now clear the buffer
        buffer.clear();
        assert_eq!(buffer.as_slice(), &[] as &[u8]);
    }

   
}


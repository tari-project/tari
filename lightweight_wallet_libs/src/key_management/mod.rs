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

/// Key derivation path for deterministic key generation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyDerivationPath {
    /// Purpose (e.g., 44 for BIP-44)
    pub purpose: u32,
    /// Coin type (e.g., 123456 for Tari)
    pub coin_type: u32,
    /// Account index
    pub account: u32,
    /// Change (0 for external, 1 for internal)
    pub change: u32,
    /// Address index
    pub address_index: u32,
}

impl KeyDerivationPath {
    /// Create a new key derivation path
    pub fn new(purpose: u32, coin_type: u32, account: u32, change: u32, address_index: u32) -> Self {
        Self {
            purpose,
            coin_type,
            account,
            change,
            address_index,
        }
    }

    /// Create a standard Tari key derivation path
    pub fn tari_standard(account: u32, change: u32, address_index: u32) -> Self {
        Self::new(44, 123456, account, change, address_index)
    }

    /// Convert path to string representation (e.g., "m/44'/123456'/0'/0/0")
    pub fn to_string(&self) -> String {
        format!(
            "m/{}'/{:06}'/{}'/{}/{}",
            self.purpose, self.coin_type, self.account, self.change, self.address_index
        )
    }

    /// Parse path from string representation
    pub fn from_string(path: &str) -> Result<Self, KeyManagementError> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 6 || parts[0] != "m" {
            return Err(KeyManagementError::InvalidKeyDerivationPath(
                "Invalid path format".to_string()
            ));
        }

        let purpose = parts[1].trim_end_matches('\'').parse::<u32>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid purpose".to_string()))?;
        let coin_type = parts[2].trim_end_matches('\'').parse::<u32>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid coin type".to_string()))?;
        let account = parts[3].trim_end_matches('\'').parse::<u32>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid account".to_string()))?;
        let change = parts[4].parse::<u32>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid change".to_string()))?;
        let address_index = parts[5].parse::<u32>()
            .map_err(|_| KeyManagementError::InvalidKeyDerivationPath("Invalid address index".to_string()))?;

        Ok(Self {
            purpose,
            coin_type,
            account,
            change,
            address_index,
        })
    }
}

impl Default for KeyDerivationPath {
    fn default() -> Self {
        Self::tari_standard(0, 0, 0)
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

pub use seed_phrase::mnemonic_to_master_key;
pub use key_derivation::LightweightKeyManager;
pub use stealth_address::{StealthAddress, StealthAddressManager};

/// Key store for managing both derived and imported keys
#[derive(Debug, Clone)]
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
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_path_creation() {
        let path = KeyDerivationPath::new(44, 123456, 0, 0, 0);
        assert_eq!(path.purpose, 44);
        assert_eq!(path.coin_type, 123456);
        assert_eq!(path.account, 0);
        assert_eq!(path.change, 0);
        assert_eq!(path.address_index, 0);
    }

    #[test]
    fn test_tari_standard_path() {
        let path = KeyDerivationPath::tari_standard(1, 0, 5);
        assert_eq!(path.purpose, 44);
        assert_eq!(path.coin_type, 123456);
        assert_eq!(path.account, 1);
        assert_eq!(path.change, 0);
        assert_eq!(path.address_index, 5);
    }

    #[test]
    fn test_path_to_string() {
        let path = KeyDerivationPath::tari_standard(0, 0, 0);
        assert_eq!(path.to_string(), "m/44'/123456'/0'/0/0");
    }

    #[test]
    fn test_path_from_string() {
        let path_str = "m/44'/123456'/0'/0/0";
        let path = KeyDerivationPath::from_string(path_str).unwrap();
        assert_eq!(path.purpose, 44);
        assert_eq!(path.coin_type, 123456);
        assert_eq!(path.account, 0);
        assert_eq!(path.change, 0);
        assert_eq!(path.address_index, 0);
    }

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
        assert_eq!(store.current_key_index(), 0);
        
        store.update_key_index(5);
        assert_eq!(store.current_key_index(), 5);
    }
} 
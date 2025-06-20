// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Deterministic key derivation for lightweight wallets

use blake2::{Blake2b, Digest};
use digest::consts::U64;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use zeroize::Zeroize;

use crate::{
    data_structures::types::{CompressedPublicKey, PrivateKey},
    errors::KeyManagementError,
};
use crate::key_management::{KeyManager, KeyDerivationPath, DerivedKeyPair};

/// Domain separator for key derivation (matching Tari key manager)
const HASHER_LABEL_DERIVE_KEY: &[u8] = b"tari_key_manager_derive_key";

/// Lightweight key manager for deterministic key derivation
#[derive(Debug, Clone)]
pub struct LightweightKeyManager {
    /// Master key (32 bytes) - equivalent to CipherSeed entropy
    master_key: [u8; 32],
    /// Branch seed for key derivation
    branch_seed: String,
    /// Current key index
    current_key_index: u64,
}

impl LightweightKeyManager {
    /// Create a new key manager from a master key
    pub fn new(master_key: [u8; 32]) -> Self {
        Self {
            master_key,
            branch_seed: "".to_string(),
            current_key_index: 0,
        }
    }

    /// Create a key manager from a master key with branch seed
    pub fn with_branch_seed(master_key: [u8; 32], branch_seed: String) -> Self {
        Self {
            master_key,
            branch_seed,
            current_key_index: 0,
        }
    }

    /// Create a key manager from a mnemonic phrase
    pub fn from_mnemonic(mnemonic: &str, passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        use crate::key_management::mnemonic_to_master_key;
        let master_key = mnemonic_to_master_key(mnemonic, passphrase)?;
        Ok(Self::new(master_key))
    }

    /// Create a key manager from a mnemonic phrase with branch seed
    pub fn from_mnemonic_with_branch_seed(
        mnemonic: &str, 
        passphrase: Option<&str>, 
        branch_seed: String
    ) -> Result<Self, KeyManagementError> {
        use crate::key_management::mnemonic_to_master_key;
        let master_key = mnemonic_to_master_key(mnemonic, passphrase)?;
        Ok(Self::with_branch_seed(master_key, branch_seed))
    }

    /// Derive a private key using the Tari key manager pattern
    /// derived_key = H(master_key || branch_seed || key_index)
    fn derive_private_key_internal(&self, key_index: u64) -> Result<PrivateKey, KeyManagementError> {
        let mut hasher = Blake2b::<U64>::new();
        hasher.update(HASHER_LABEL_DERIVE_KEY);
        hasher.update(&self.master_key);
        hasher.update(self.branch_seed.as_bytes());
        hasher.update(key_index.to_le_bytes());
        
        let result = hasher.finalize();
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&result[..32]);
        
        Ok(PrivateKey::new(key_bytes))
    }

    /// Derive a public key from a private key using Ristretto
    fn derive_public_key_from_private(&self, private_key: &PrivateKey) -> Result<CompressedPublicKey, KeyManagementError> {
        // Use proper Ristretto point multiplication
        let point = private_key.0 * RISTRETTO_BASEPOINT_POINT;
        Ok(CompressedPublicKey::from_point(&point))
    }

    /// Get the current branch seed
    pub fn branch_seed(&self) -> &str {
        &self.branch_seed
    }

    /// Set the branch seed
    pub fn set_branch_seed(&mut self, branch_seed: String) {
        self.branch_seed = branch_seed;
    }
}

impl KeyManager for LightweightKeyManager {
    fn derive_key_pair(&self, path: &KeyDerivationPath) -> Result<DerivedKeyPair, KeyManagementError> {
        // For compatibility, use the address_index as the key_index
        let key_index = path.key_index;
        let private_key = self.derive_private_key_internal(key_index)?;
        let public_key = self.derive_public_key_from_private(&private_key)?;
        
        Ok(DerivedKeyPair::new(
            private_key,
            public_key,
            key_index,
            path.clone(),
        ))
    }

    fn derive_private_key(&self, path: &KeyDerivationPath) -> Result<PrivateKey, KeyManagementError> {
        // For compatibility, use the address_index as the key_index
        let key_index = path.key_index;
        self.derive_private_key_internal(key_index)
    }

    fn derive_public_key(&self, path: &KeyDerivationPath) -> Result<CompressedPublicKey, KeyManagementError> {
        let private_key = self.derive_private_key(path)?;
        self.derive_public_key_from_private(&private_key)
    }

    fn next_key_pair(&mut self) -> Result<DerivedKeyPair, KeyManagementError> {
        self.current_key_index += 1;
        let path = KeyDerivationPath::new("".to_string(), self.current_key_index);
        self.derive_key_pair(&path)
    }

    fn current_key_index(&self) -> u64 {
        self.current_key_index
    }

    fn update_key_index(&mut self, new_index: u64) {
        self.current_key_index = new_index;
    }
}

impl Zeroize for LightweightKeyManager {
    fn zeroize(&mut self) {
        self.master_key.zeroize();
    }
}

impl Drop for LightweightKeyManager {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_manager_creation() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        assert_eq!(km.current_key_index(), 0);
        assert_eq!(km.branch_seed(), "");
    }

    #[test]
    fn test_key_manager_with_branch_seed() {
        let master_key = [1u8; 32];
        let branch_seed = "test_branch".to_string();
        let km = LightweightKeyManager::with_branch_seed(master_key, branch_seed.clone());
        assert_eq!(km.current_key_index(), 0);
        assert_eq!(km.branch_seed(), "test_branch");
    }

    #[test]
    fn test_key_derivation() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path = KeyDerivationPath::new("".to_string(), 0);
        
        let key_pair = km.derive_key_pair(&path).unwrap();
        assert_eq!(key_pair.key_index, 0);
        assert_eq!(key_pair.derivation_path, path);
    }

    #[test]
    fn test_deterministic_derivation() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path = KeyDerivationPath::new("".to_string(), 0);
        
        let key_pair1 = km.derive_key_pair(&path).unwrap();
        let key_pair2 = km.derive_key_pair(&path).unwrap();
        
        assert_eq!(key_pair1.private_key, key_pair2.private_key);
        assert_eq!(key_pair1.public_key, key_pair2.public_key);
    }

    #[test]
    fn test_different_paths_different_keys() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path1 = KeyDerivationPath::new("".to_string(), 0);
        let path2 = KeyDerivationPath::new("".to_string(), 1);
        
        let key_pair1 = km.derive_key_pair(&path1).unwrap();
        let key_pair2 = km.derive_key_pair(&path2).unwrap();
        
        assert_ne!(key_pair1.private_key, key_pair2.private_key);
        assert_ne!(key_pair1.public_key, key_pair2.public_key);
    }

    #[test]
    fn test_branch_seed_affects_derivation() {
        let master_key = [1u8; 32];
        let km1 = LightweightKeyManager::with_branch_seed(master_key, "branch1".to_string());
        let km2 = LightweightKeyManager::with_branch_seed(master_key, "branch2".to_string());
        
        let path = KeyDerivationPath::new("".to_string(), 0);
        let key_pair1 = km1.derive_key_pair(&path).unwrap();
        let key_pair2 = km2.derive_key_pair(&path).unwrap();
        
        // Different branch seeds should produce different keys
        assert_ne!(key_pair1.private_key, key_pair2.private_key);
        assert_ne!(key_pair1.public_key, key_pair2.public_key);
    }

    #[test]
    fn test_next_key_pair() {
        let master_key = [1u8; 32];
        let mut km = LightweightKeyManager::new(master_key);
        
        let key_pair1 = km.next_key_pair().unwrap();
        let key_pair2 = km.next_key_pair().unwrap();
        
        assert_eq!(key_pair1.key_index, 1);
        assert_eq!(key_pair2.key_index, 2);
        assert_ne!(key_pair1.private_key, key_pair2.private_key);
    }

    #[test]
    fn test_from_mnemonic() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let km = LightweightKeyManager::from_mnemonic(mnemonic, None).unwrap();
        assert_eq!(km.current_key_index(), 0);
    }

    #[test]
    fn test_from_mnemonic_with_branch_seed() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let branch_seed = "test_branch".to_string();
        let km = LightweightKeyManager::from_mnemonic_with_branch_seed(mnemonic, None, branch_seed.clone()).unwrap();
        assert_eq!(km.current_key_index(), 0);
        assert_eq!(km.branch_seed(), "test_branch");
    }

    #[test]
    fn test_set_branch_seed() {
        let master_key = [1u8; 32];
        let mut km = LightweightKeyManager::new(master_key);
        assert_eq!(km.branch_seed(), "");
        
        km.set_branch_seed("new_branch".to_string());
        assert_eq!(km.branch_seed(), "new_branch");
    }
} 
// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Deterministic key derivation for lightweight wallets

use blake2::{Blake2b, Digest};
use digest::consts::U64;
use zeroize::Zeroize;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::scalar::Scalar;

use crate::data_structures::types::{PrivateKey, CompressedPublicKey};
use crate::errors::KeyManagementError;
use crate::key_management::{KeyManager, KeyDerivationPath, DerivedKeyPair};

/// Domain separator for key derivation
const KEY_DERIVATION_DOMAIN: &[u8] = b"TARI_KEY_DERIVATION";

/// Lightweight key manager for deterministic key derivation
#[derive(Debug, Clone)]
pub struct LightweightKeyManager {
    /// Master key (32 bytes)
    master_key: [u8; 32],
    /// Current key index
    current_key_index: u64,
}

impl LightweightKeyManager {
    /// Create a new key manager from a master key
    pub fn new(master_key: [u8; 32]) -> Self {
        Self {
            master_key,
            current_key_index: 0,
        }
    }

    /// Create a key manager from a mnemonic phrase
    pub fn from_mnemonic(mnemonic: &str, passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        use crate::key_management::mnemonic_to_master_key;
        let master_key = mnemonic_to_master_key(mnemonic, passphrase)?;
        Ok(Self::new(master_key))
    }

    /// Derive a private key from the master key using a derivation path
    fn derive_private_key_internal(&self, path: &KeyDerivationPath) -> Result<PrivateKey, KeyManagementError> {
        let mut hasher = Blake2b::<U64>::new();
        hasher.update(KEY_DERIVATION_DOMAIN);
        hasher.update(&self.master_key);
        hasher.update(path.purpose.to_le_bytes());
        hasher.update(path.coin_type.to_le_bytes());
        hasher.update(path.account.to_le_bytes());
        hasher.update(path.change.to_le_bytes());
        hasher.update(path.address_index.to_le_bytes());
        
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
}

impl KeyManager for LightweightKeyManager {
    fn derive_key_pair(&self, path: &KeyDerivationPath) -> Result<DerivedKeyPair, KeyManagementError> {
        let private_key = self.derive_private_key_internal(path)?;
        let public_key = self.derive_public_key_from_private(&private_key)?;
        
        Ok(DerivedKeyPair::new(
            private_key,
            public_key,
            path.address_index as u64,
            path.clone(),
        ))
    }

    fn derive_private_key(&self, path: &KeyDerivationPath) -> Result<PrivateKey, KeyManagementError> {
        self.derive_private_key_internal(path)
    }

    fn derive_public_key(&self, path: &KeyDerivationPath) -> Result<CompressedPublicKey, KeyManagementError> {
        let private_key = self.derive_private_key_internal(path)?;
        self.derive_public_key_from_private(&private_key)
    }

    fn next_key_pair(&mut self) -> Result<DerivedKeyPair, KeyManagementError> {
        self.current_key_index += 1;
        let path = KeyDerivationPath::tari_standard(0, 0, self.current_key_index as u32);
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
    }

    #[test]
    fn test_key_derivation() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path = KeyDerivationPath::tari_standard(0, 0, 0);
        
        let key_pair = km.derive_key_pair(&path).unwrap();
        assert_eq!(key_pair.key_index, 0);
        assert_eq!(key_pair.derivation_path, path);
    }

    #[test]
    fn test_deterministic_derivation() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path = KeyDerivationPath::tari_standard(0, 0, 0);
        
        let key_pair1 = km.derive_key_pair(&path).unwrap();
        let key_pair2 = km.derive_key_pair(&path).unwrap();
        
        assert_eq!(key_pair1.private_key, key_pair2.private_key);
        assert_eq!(key_pair1.public_key, key_pair2.public_key);
    }

    #[test]
    fn test_different_paths_different_keys() {
        let master_key = [1u8; 32];
        let km = LightweightKeyManager::new(master_key);
        let path1 = KeyDerivationPath::tari_standard(0, 0, 0);
        let path2 = KeyDerivationPath::tari_standard(0, 0, 1);
        
        let key_pair1 = km.derive_key_pair(&path1).unwrap();
        let key_pair2 = km.derive_key_pair(&path2).unwrap();
        
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
} 
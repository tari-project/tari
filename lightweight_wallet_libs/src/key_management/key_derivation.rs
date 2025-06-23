// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Key derivation functions for lightweight wallets
//! 
//! This implementation follows the Tari key derivation specification for compatibility
//! with the main Tari wallet implementation.

use crate::errors::KeyManagementError;
use crate::crypto::{DomainSeparatedHasher, KeyManagerDomain, RistrettoSecretKey, RistrettoPublicKey};
use crate::crypto::keys::ByteArray;
use crate::data_structures::types::{PrivateKey, CompressedPublicKey};
use crate::key_management::{KeyDerivationPath, DerivedKeyPair};
use blake2::Blake2b;
use digest::{Digest, consts::U64};
use zeroize::Zeroize;

const HASHER_LABEL_DERIVE_KEY: &str = "derive_key";

/// Lightweight key manager for deterministic key derivation
#[derive(Debug, Clone)]
pub struct LightweightKeyManager {
    master_key: [u8; 32],
    branch_seed: String,
    current_index: u64,
}

impl Zeroize for LightweightKeyManager {
    fn zeroize(&mut self) {
        self.master_key.zeroize();
        self.branch_seed.clear();
        self.current_index = 0;
    }
}

impl LightweightKeyManager {
    /// Create a new lightweight key manager
    pub fn new(master_key: [u8; 32]) -> Self {
        Self {
            master_key,
            branch_seed: String::new(),
            current_index: 0,
        }
    }

    /// Create a new lightweight key manager with a specific branch seed
    pub fn with_branch_seed(master_key: [u8; 32], branch_seed: String) -> Self {
        Self {
            master_key,
            branch_seed,
            current_index: 0,
        }
    }

    /// Get the current key index
    pub fn current_index(&self) -> u64 {
        self.current_index
    }

    /// Update the current key index
    pub fn update_index(&mut self, new_index: u64) {
        self.current_index = new_index;
    }

    /// Get the branch seed
    pub fn branch_seed(&self) -> &str {
        &self.branch_seed
    }

    /// Set the branch seed
    pub fn set_branch_seed(&mut self, branch_seed: String) {
        self.branch_seed = branch_seed;
    }

    /// Derive a key pair at the current index
    pub fn derive_key_pair(&self) -> Result<DerivedKeyPair, KeyManagementError> {
        self.derive_key_pair_at_index(self.current_index)
    }

    /// Derive a key pair at a specific index
    pub fn derive_key_pair_at_index(&self, index: u64) -> Result<DerivedKeyPair, KeyManagementError> {
        let secret_key = derive_private_key(&self.master_key, &self.branch_seed, index)?;
        let public_key = RistrettoPublicKey::from_secret_key(&secret_key);
        
        let private_key = PrivateKey::from_canonical_bytes(secret_key.as_bytes())
            .map_err(|e| KeyManagementError::key_derivation_failed(&format!("Failed to create private key: {}", e)))?;
        
        let compressed_public_key = CompressedPublicKey::from_point(&public_key.point);
        
        let derivation_path = KeyDerivationPath::new(self.branch_seed.clone(), index);
        
        Ok(DerivedKeyPair::new(
            private_key,
            compressed_public_key,
            index,
            derivation_path,
        ))
    }

    /// Get the next key pair
    pub fn next_key_pair(&mut self) -> Result<DerivedKeyPair, KeyManagementError> {
        let key_pair = self.derive_key_pair()?;
        self.current_index += 1;
        Ok(key_pair)
    }

    /// Derive a private key from a derivation path
    pub fn derive_private_key(&self, path: &KeyDerivationPath) -> Result<PrivateKey, KeyManagementError> {
        let secret_key = derive_private_key(&self.master_key, &path.branch_seed, path.key_index)?;
        PrivateKey::from_canonical_bytes(secret_key.as_bytes())
            .map_err(|e| KeyManagementError::key_derivation_failed(&format!("Failed to create private key: {}", e)))
    }

    /// Create a key manager from a mnemonic phrase
    pub fn from_mnemonic(mnemonic: &str, passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        let master_key = crate::key_management::seed_phrase::mnemonic_to_master_key(mnemonic, passphrase)?;
        Ok(Self::new(master_key))
    }

    /// Create a key manager from a mnemonic phrase with a specific branch seed
    pub fn from_mnemonic_with_branch_seed(
        mnemonic: &str, 
        passphrase: Option<&str>, 
        branch_seed: String
    ) -> Result<Self, KeyManagementError> {
        let master_key = crate::key_management::seed_phrase::mnemonic_to_master_key(mnemonic, passphrase)?;
        Ok(Self::with_branch_seed(master_key, branch_seed))
    }
}



/// Derives a public key from a private key
pub fn derive_public_key_from_private(
    private_key: &RistrettoSecretKey,
) -> Result<RistrettoPublicKey, KeyManagementError> {
    Ok(private_key.public_key())
}

/// Derives view and spend keys from CipherSeed entropy using Tari's exact key derivation pattern
/// This matches the main Tari KeyManager implementation which uses entropy directly
pub fn derive_view_and_spend_keys_from_entropy(entropy: &[u8; 16]) -> Result<(RistrettoSecretKey, RistrettoSecretKey), KeyManagementError> {
    // Tari uses specific branch seeds for view and spend keys
    // These constants match the main Tari wallet implementation
    const VIEW_KEY_BRANCH: &str = "data encryption";  // For encrypted data decryption (view key)
    const SPEND_KEY_BRANCH: &str = "comms"; // For wallet communications and spending
    
    let view_key = derive_private_key_from_entropy(entropy, VIEW_KEY_BRANCH, 0)
        .map_err(|e| KeyManagementError::view_key_derivation_failed(
            &format!("Failed to derive view key: {}", e)
        ))?;
        
    let spend_key = derive_private_key_from_entropy(entropy, SPEND_KEY_BRANCH, 0)
        .map_err(|e| KeyManagementError::spend_key_derivation_failed(
            &format!("Failed to derive spend key: {}", e)
        ))?;
    
    Ok((view_key, spend_key))
}

/// Derives a private key directly from CipherSeed entropy using Tari's key derivation specification
/// This matches the main Tari KeyManager.derive_private_key implementation exactly
pub fn derive_private_key_from_entropy(
    entropy: &[u8; 16],
    branch_seed: &str,
    key_index: u64,
) -> Result<RistrettoSecretKey, KeyManagementError> {
    if branch_seed.is_empty() {
        return Err(KeyManagementError::invalid_derivation_index(
            "empty",
            key_index
        ));
    }
    
    // This matches the main Tari KeyManager implementation exactly:
    // DomainSeparatedHasher::new_with_label(HASHER_LABEL_DERIVE_KEY)
    //   .chain(self.seed.entropy())  // CipherSeed entropy directly (16 bytes)
    //   .chain(self.branch_seed.as_bytes())
    //   .chain(key_index.to_le_bytes())
    let derive_key = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerDomain>::new_with_label("derive_key")
        .chain(entropy)  // Use the 16-byte CipherSeed entropy directly
        .chain(branch_seed.as_bytes())
        .chain(key_index.to_le_bytes())
        .finalize();
    
    let derive_key = derive_key.as_ref();
    RistrettoSecretKey::from_uniform_bytes(derive_key)
        .map_err(|e| KeyManagementError::branch_key_derivation_failed(
            branch_seed,
            key_index,
            &format!("Failed to create private key: {}", e)
        ))
}

/// Derives a stealth address from view and spend public keys
pub fn derive_stealth_address(
    view_public_key: &RistrettoPublicKey,
    spend_public_key: &RistrettoPublicKey,
) -> Result<[u8; 32], KeyManagementError> {
    // This is a simplified implementation - in practice, Tari stealth addresses
    // use a more complex derivation involving the view and spend keys
    let mut hasher = Blake2b::<U64>::new();
    hasher.update(view_public_key.as_bytes());
    hasher.update(spend_public_key.as_bytes());
    let result = hasher.finalize();
    
    let mut stealth_address = [0u8; 32];
    stealth_address.copy_from_slice(&result[..32]);
    Ok(stealth_address)
}

/// Creates a lightweight key manager from a mnemonic phrase using enhanced error handling
pub fn create_key_manager_from_mnemonic(
    mnemonic: &str,
    passphrase: Option<&str>,
) -> Result<[u8; 32], KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    crate::key_management::seed_phrase::mnemonic_to_master_key(mnemonic, passphrase)
        .map_err(|e| {
            match e.category() {
                "seed_phrase" => e,
                "cipher_seed" => e,
                "passphrase" => e,
                _ => KeyManagementError::master_key_derivation_failed(
                    &format!("Failed to create key manager from mnemonic: {}", e)
                ),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::ByteArray;


    #[test]
    fn test_mnemonic_to_master_key() {
        let seed_phrase = "theory train hurt word now piece large material message toddler abuse prize roast maximum bronze drink legal city liberty glimpse present found slush silent";

        // Convert seed phrase to CipherSeed and use entropy directly
        let encrypted_bytes = crate::key_management::seed_phrase::mnemonic_to_bytes(seed_phrase).unwrap();
        let cipher_seed = crate::key_management::seed_phrase::CipherSeed::from_enciphered_bytes(&encrypted_bytes, None).unwrap();
        let entropy: [u8; 16] = cipher_seed.entropy().try_into().unwrap();
        
        let (view_private_key, _spend_private_key) = derive_view_and_spend_keys_from_entropy(&entropy)
            .expect("Failed to derive view and spend keys");
        let expected_private_key = "5a7b695be289e8967257d076a74a38d43b5f0c63bc981d9c962fc0c8dec7d001";
        let actual_private_key = hex::encode(view_private_key.as_bytes());
        println!("Actual Private Key: {}", actual_private_key);
        assert_eq!(expected_private_key, actual_private_key);
    }

    #[test]
    fn test_key_derivation_from_seed_phrase() {
        // Test seed phrase from the user's request
        let seed_phrase = "theory train hurt word now piece large material message toddler abuse prize roast maximum bronze drink legal city liberty glimpse present found slush silent";
        
        // Expected values from the user's request
        let expected_view_private_key = "5a7b695be289e8967257d076a74a38d43b5f0c63bc981d9c962fc0c8dec7d001";
        let expected_spend_private_key = "eb87dffa93010735e67333e931b067678f7c08bf644ad31fa80f2dd2a3b7df08";
        let expected_view_public_key = "36a875d408ba2190c7bb0427432512696b5eef6d0c320f2bff0b1af78deeb055";
        let expected_spend_public_key = "d4f2960842db68fd1dd19bd53fbbfb0fd2aa9c6a62c008bac996b79665573173";
        
        // Convert seed phrase to encrypted bytes
        let encrypted_bytes = crate::key_management::seed_phrase::mnemonic_to_bytes(seed_phrase)
            .expect("Failed to convert mnemonic to bytes");
        
        // Decrypt the CipherSeed to get the entropy
        let cipher_seed = crate::key_management::seed_phrase::CipherSeed::from_enciphered_bytes(&encrypted_bytes, None)
            .expect("Failed to decrypt CipherSeed");
        
        // Use the entropy directly for key derivation (matching main Tari implementation)
        let entropy: [u8; 16] = cipher_seed.entropy().try_into()
            .expect("Failed to convert entropy to 16-byte array");
        
        // Derive view and spend keys using entropy directly
        let (view_private_key, spend_private_key) = derive_view_and_spend_keys_from_entropy(&entropy)
            .expect("Failed to derive view and spend keys");
        
        // Convert to public keys
        let view_public_key = RistrettoPublicKey::from_secret_key(&view_private_key);
        let spend_public_key = RistrettoPublicKey::from_secret_key(&spend_private_key);
        
        // Convert to hex strings for comparison
        let actual_view_private_key = hex::encode(view_private_key.as_bytes());
        let actual_spend_private_key = hex::encode(spend_private_key.as_bytes());
        let actual_view_public_key = hex::encode(view_public_key.as_bytes());
        let actual_spend_public_key = hex::encode(spend_public_key.as_bytes());
        
        // Verify that we can derive keys successfully and they're different
        assert_ne!(view_private_key, spend_private_key, "View and spend private keys should be different");
        assert_ne!(view_public_key, spend_public_key, "View and spend public keys should be different");
        
        // Verify that the public keys correspond to the private keys
        assert_eq!(view_public_key, RistrettoPublicKey::from_secret_key(&view_private_key));
        assert_eq!(spend_public_key, RistrettoPublicKey::from_secret_key(&spend_private_key));

        // Verify that our keys match the expected Tari values exactly
        assert_eq!(actual_view_private_key, expected_view_private_key);
        assert_eq!(actual_spend_private_key, expected_spend_private_key);
        assert_eq!(actual_view_public_key, expected_view_public_key);
        assert_eq!(actual_spend_public_key, expected_spend_public_key);
    }

    #[test]
    fn test_key_derivation_consistency() {
        let master_key = [1u8; 32];
        let branch_seed = "test_branch";
        
        // Derive the same key multiple times
        let key1 = derive_private_key(&master_key, branch_seed, 0).unwrap();
        let key2 = derive_private_key(&master_key, branch_seed, 0).unwrap();
        let key3 = derive_private_key(&master_key, branch_seed, 1).unwrap();
        
        // Same parameters should produce same key
        assert_eq!(key1, key2);
        
        // Different index should produce different key
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_different_branch_seeds() {
        let master_key = [1u8; 32];
        
        let key1 = derive_private_key(&master_key, "branch1", 0).unwrap();
        let key2 = derive_private_key(&master_key, "branch2", 0).unwrap();
        
        // Different branch seeds should produce different keys
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_lightweight_key_manager() {
        let master_key = [1u8; 32];
        let mut key_manager = LightweightKeyManager::new(master_key);
        
        // Test initial state
        assert_eq!(key_manager.current_index(), 0);
        assert_eq!(key_manager.branch_seed(), "");
        
        // Test setting branch seed
        key_manager.set_branch_seed("test_branch".to_string());
        assert_eq!(key_manager.branch_seed(), "test_branch");
        
        // Test deriving key pair
        let key_pair = key_manager.derive_key_pair().unwrap();
        assert_eq!(key_pair.key_index, 0);
        assert_eq!(key_pair.derivation_path.branch_seed, "test_branch");
        
        // Test next key pair
        let next_key_pair = key_manager.next_key_pair().unwrap();
        assert_eq!(next_key_pair.key_index, 0); // Should be 0 since we just derived it
        assert_eq!(key_manager.current_index(), 1); // But index should be incremented
        
        // Test deriving at specific index
        let key_pair_at_5 = key_manager.derive_key_pair_at_index(5).unwrap();
        assert_eq!(key_pair_at_5.key_index, 5);
    }

    #[test]
    fn test_tari_test_vector_validation() {
        // Official Tari test vector data for validation
        let seed_phrase = "scare harsh invite normal satisfy subject similar excite dragon gap fence machine monster flavor spoon tape rice require risk sting health nurse orange stick";
        
        // Expected keys from the test vector
        let expected_view_private_key = "7755e59ca4a10d19d14f56a014826d005d029ff9a5053c850d63f9322005080a";
        let expected_spend_private_key = "ef5d6881f2b1ff65dd6d62a77f73be2179cad40c6d587d5ff9f4ed49b5378b05";
        let expected_view_public_key = "c64341cddadc29e1e31ce1f568d3bbd0262ef2f9bfdbf2405d85735d45f1bb02";
        let expected_spend_public_key = "5285073b72f698132432e1be6b76e170d437e4ba11bfaf5f7539d5c998523226";
        
        // Expected addresses (for future validation once address generation is implemented)
        let expected_base58_address = "12JVm6ARPDg2GvBEpaKxADBW4SkacGRWZYhowEzoUvHrz9kFWCVv4QSYUE6JWiLFYcjEeZv43YJw8W7E8ynrMUWsDm5";
        let expected_emoji_address = "🐢📟📈🎉🤖⏰🔪🔬🍟😂😈🍋😂🚜🏦🔑💦🔋🍗🍪🚓🚨💯🔫🚓🎃🎼🐯🐔🎼🎓🚒💦🌈🎮🐯🤔🍺🐑🚢💅🍀🍔🍯😂➕🐀🐘😂🦁🔔🍶🤑💤🌻💯💊🎾🐗🍸🔥📎💅🎮🍯🍗💄";
        
        println!("=== Testing Tari Test Vector ===");
        println!("Seed phrase: {}", seed_phrase);
        
        // Convert seed phrase to encrypted bytes (correct approach)
        let encrypted_bytes = crate::key_management::seed_phrase::mnemonic_to_bytes(seed_phrase)
            .expect("Failed to convert mnemonic to bytes");
        
        // Decrypt the CipherSeed to get the entropy
        let cipher_seed = crate::key_management::seed_phrase::CipherSeed::from_enciphered_bytes(&encrypted_bytes, None)
            .expect("Failed to decrypt CipherSeed");
        
        // Use the entropy directly for key derivation (matching main Tari implementation)
        let entropy: [u8; 16] = cipher_seed.entropy().try_into()
            .expect("Failed to convert entropy to 16-byte array");
        
        println!("CipherSeed entropy: {}", hex::encode(entropy));
        
        // Derive view and spend keys using entropy directly
        let (view_private_key, spend_private_key) = derive_view_and_spend_keys_from_entropy(&entropy)
            .expect("Failed to derive view and spend keys");
        
        // Convert to public keys
        let view_public_key = RistrettoPublicKey::from_secret_key(&view_private_key);
        let spend_public_key = RistrettoPublicKey::from_secret_key(&spend_private_key);
        
        // Convert to hex strings for comparison
        let actual_view_private_key = hex::encode(view_private_key.as_bytes());
        let actual_spend_private_key = hex::encode(spend_private_key.as_bytes());
        let actual_view_public_key = hex::encode(view_public_key.as_bytes());
        let actual_spend_public_key = hex::encode(spend_public_key.as_bytes());
        
        println!("Expected View Private Key:  {}", expected_view_private_key);
        println!("Actual View Private Key:    {}", actual_view_private_key);
        println!("Expected Spend Private Key: {}", expected_spend_private_key);
        println!("Actual Spend Private Key:   {}", actual_spend_private_key);
        println!("Expected View Public Key:   {}", expected_view_public_key);
        println!("Actual View Public Key:     {}", actual_view_public_key);
        println!("Expected Spend Public Key:  {}", expected_spend_public_key);
        println!("Actual Spend Public Key:    {}", actual_spend_public_key);
        
        // Validate that we can derive keys successfully and they're different
        assert_ne!(view_private_key, spend_private_key, "View and spend private keys should be different");
        assert_ne!(view_public_key, spend_public_key, "View and spend public keys should be different");
        
        // Validate that public keys correspond to private keys
        assert_eq!(view_public_key, RistrettoPublicKey::from_secret_key(&view_private_key));
        assert_eq!(spend_public_key, RistrettoPublicKey::from_secret_key(&spend_private_key));
        
        // Now test the exact value validation - this is the real test of correctness
        assert_eq!(actual_view_private_key, expected_view_private_key, "View private key mismatch");
        assert_eq!(actual_spend_private_key, expected_spend_private_key, "Spend private key mismatch");
        assert_eq!(actual_view_public_key, expected_view_public_key, "View public key mismatch");
        assert_eq!(actual_spend_public_key, expected_spend_public_key, "Spend public key mismatch");
        
        // Store expected addresses for future validation
        let _ = expected_base58_address;
        let _ = expected_emoji_address;
        
        println!("✅ Exact Tari test vector validation passed!");
    }
} 
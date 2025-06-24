// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Key derivation functions for lightweight wallets
//! 
//! This implementation follows the Tari key derivation specification for compatibility
//! with the main Tari wallet implementation.

use crate::errors::KeyManagementError;
use crate::crypto::{DomainSeparatedHasher, KeyManagerDomain, RistrettoSecretKey, RistrettoPublicKey};
use crate::crypto::keys::ByteArray;
use blake2::Blake2b;
use digest::{Digest, consts::U64};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::ByteArray;

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

    #[test]
    fn test_entropy_based_key_derivation_consistency() {
        let entropy = [1u8; 16];
        let branch_seed = "test_branch";
        
        // Derive the same key multiple times
        let key1 = derive_private_key_from_entropy(&entropy, branch_seed, 0).unwrap();
        let key2 = derive_private_key_from_entropy(&entropy, branch_seed, 0).unwrap();
        let key3 = derive_private_key_from_entropy(&entropy, branch_seed, 1).unwrap();
        
        // Same parameters should produce same key
        assert_eq!(key1, key2);
        
        // Different index should produce different key
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_entropy_based_different_branch_seeds() {
        let entropy = [1u8; 16];
        
        let key1 = derive_private_key_from_entropy(&entropy, "branch1", 0).unwrap();
        let key2 = derive_private_key_from_entropy(&entropy, "branch2", 0).unwrap();
        
        // Different branch seeds should produce different keys
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_view_and_spend_keys_are_different() {
        let entropy = [1u8; 16];
        
        let (view_key, spend_key) = derive_view_and_spend_keys_from_entropy(&entropy).unwrap();
        
        // View and spend keys should be different
        assert_ne!(view_key, spend_key);
        
        // Verify they can be converted to public keys
        let view_public = RistrettoPublicKey::from_secret_key(&view_key);
        let spend_public = RistrettoPublicKey::from_secret_key(&spend_key);
        assert_ne!(view_public, spend_public);
    }
} 
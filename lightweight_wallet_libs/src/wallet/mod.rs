// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Wallet functionality for lightweight Tari wallets
//!
//! This module provides the core wallet struct and operations for managing
//! master keys, seed phrases, and wallet metadata.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;
use rand_core::{OsRng, RngCore};
use crate::data_structures::SafeArray;
use crate::data_structures::address::{TariAddress, DualAddress, SingleAddress, TariAddressFeatures, Network};
use crate::data_structures::types::{CompressedPublicKey, PrivateKey};
use crate::errors::KeyManagementError;
use crate::key_management::{mnemonic_to_master_key, validate_seed_phrase, bytes_to_mnemonic, CipherSeed};

// Constants from Tari CipherSeed specification for birthday calculation
const BIRTHDAY_GENESIS_FROM_UNIX_EPOCH: u64 = 1640995200; // seconds to 2022-01-01 00:00:00 UTC
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// Core wallet struct containing master key, birthday, and metadata
#[derive(Debug, Clone)]
pub struct Wallet {
    /// Master key derived from seed phrase (32 bytes, securely stored)
    master_key: SafeArray<32>,
    /// Wallet creation timestamp for scanning optimization (Unix timestamp)
    birthday: u64,
    /// Wallet metadata for additional configuration and state
    metadata: WalletMetadata,
    /// Original seed phrase (stored only if wallet was created from a seed phrase)
    original_seed_phrase: Option<String>,
}

/// Wallet metadata containing additional configuration and state information
#[derive(Debug, Clone, Default)]
pub struct WalletMetadata {
    /// Optional wallet label/name
    pub label: Option<String>,
    /// Network the wallet is configured for (mainnet, stagenet, etc.)
    pub network: String,
    /// Current key index for deterministic key derivation
    pub current_key_index: u64,
    /// Additional custom properties
    pub properties: HashMap<String, String>,
}

impl Wallet {
    /// Create a new wallet with the given master key and birthday
    pub fn new(master_key: [u8; 32], birthday: u64) -> Self {
        Self {
            master_key: SafeArray::new(master_key),
            birthday,
            metadata: WalletMetadata::default(),
            original_seed_phrase: None,
        }
    }

    /// Create a new wallet from a seed phrase and optional passphrase
    pub fn new_from_seed_phrase(phrase: &str, passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        // Convert seed phrase to master key
        let master_key = mnemonic_to_master_key(phrase, passphrase)?;
        
        // Calculate current birthday as days since genesis
        let birthday = Self::calculate_current_birthday();
        
        Ok(Self {
            master_key: SafeArray::new(master_key),
            birthday,
            metadata: WalletMetadata::default(),
            original_seed_phrase: Some(phrase.to_string()),
        })
    }

    /// Generate a new wallet with random entropy
    /// 
    /// Creates a wallet with completely random 32-byte master key entropy.
    /// Note: The passphrase parameter is included for API consistency but is not
    /// currently used since we generate random entropy directly rather than
    /// deriving from a mnemonic phrase.
    pub fn generate_new(_passphrase: Option<&str>) -> Self {
        // Generate 32 bytes of cryptographically secure random entropy
        let mut master_key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut master_key_bytes);
        
        // Calculate current birthday
        let birthday = Self::calculate_current_birthday();
        
        Self {
            master_key: SafeArray::new(master_key_bytes),
            birthday,
            metadata: WalletMetadata::default(),
            original_seed_phrase: None,
        }
    }

    /// Generate a new wallet with a fresh seed phrase
    /// 
    /// Creates a wallet using a randomly generated 24-word BIP39 seed phrase.
    /// The original seed phrase is stored and can be exported using `export_seed_phrase()`.
    pub fn generate_new_with_seed_phrase(passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        // Create a new CipherSeed with random entropy
        let cipher_seed = CipherSeed::new();
        
        // Encrypt the CipherSeed with the provided passphrase (or default if None)
        let encrypted_bytes = cipher_seed.encipher(passphrase)
            .map_err(|e| KeyManagementError::cipher_seed_encryption_failed(&e.to_string()))?;
        
        // Convert encrypted bytes to mnemonic words
        let seed_phrase = bytes_to_mnemonic(&encrypted_bytes)?;
        
        // Create wallet from the generated seed phrase with the same passphrase
        Self::new_from_seed_phrase(&seed_phrase, passphrase)
    }

    /// Calculate the current birthday (days since Tari genesis date)
    fn calculate_current_birthday() -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default() // default to epoch on error
            .as_secs();
        
        if now < BIRTHDAY_GENESIS_FROM_UNIX_EPOCH {
            return 0; // Before genesis date
        }
        
        let seconds_since_genesis = now - BIRTHDAY_GENESIS_FROM_UNIX_EPOCH;
        seconds_since_genesis / SECONDS_PER_DAY
    }

    /// Get the wallet birthday (creation timestamp)
    pub fn birthday(&self) -> u64 {
        self.birthday
    }

    /// Set the wallet birthday
    pub fn set_birthday(&mut self, birthday: u64) {
        self.birthday = birthday;
    }

    /// Get a reference to the wallet metadata
    pub fn metadata(&self) -> &WalletMetadata {
        &self.metadata
    }

    /// Get a mutable reference to the wallet metadata
    pub fn metadata_mut(&mut self) -> &mut WalletMetadata {
        &mut self.metadata
    }

    /// Set the wallet label
    pub fn set_label(&mut self, label: Option<String>) {
        self.metadata.label = label;
    }

    /// Get the wallet label
    pub fn label(&self) -> Option<&String> {
        self.metadata.label.as_ref()
    }

    /// Set the network
    pub fn set_network(&mut self, network: String) {
        self.metadata.network = network;
    }

    /// Get the network
    pub fn network(&self) -> &str {
        &self.metadata.network
    }

    /// Get the current key index
    pub fn current_key_index(&self) -> u64 {
        self.metadata.current_key_index
    }

    /// Set the current key index
    pub fn set_current_key_index(&mut self, index: u64) {
        self.metadata.current_key_index = index;
    }

    /// Add a custom property to the wallet metadata
    pub fn set_property(&mut self, key: String, value: String) {
        self.metadata.properties.insert(key, value);
    }

    /// Get a custom property from the wallet metadata
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.metadata.properties.get(key)
    }

    /// Remove a custom property from the wallet metadata
    pub fn remove_property(&mut self, key: &str) -> Option<String> {
        self.metadata.properties.remove(key)
    }

    /// Export the original seed phrase if available
    /// 
    /// Returns the original seed phrase that was used to create this wallet.
    /// Returns an error if the wallet was created using `generate_new()` or other
    /// methods that don't use a seed phrase.
    pub fn export_seed_phrase(&self) -> Result<String, KeyManagementError> {
        self.original_seed_phrase
            .clone()
            .ok_or_else(|| KeyManagementError::SeedPhraseError(
                "Wallet was not created from a seed phrase".to_string()
            ))
    }

    /// Generate a dual address with view and spend keys
    /// 
    /// Creates a dual Tari address using derived view and spend keys from the master key.
    /// This allows for stealth payments and other advanced functionality.
    pub fn get_dual_address(&self, features: TariAddressFeatures, payment_id: Option<Vec<u8>>) -> Result<TariAddress, KeyManagementError> {
        // Derive view and spend keys from master key
        let (view_key, spend_key) = self.derive_key_pair()?;
        
        // Convert to CompressedPublicKey
        let view_public_key = CompressedPublicKey::from_private_key(&view_key);
        let spend_public_key = CompressedPublicKey::from_private_key(&spend_key);
        
        // Use the network from wallet metadata or default to Esmeralda
        let network = match self.metadata.network.as_str() {
            "mainnet" => Network::MainNet,
            "stagenet" => Network::StageNet,
            "localnet" => Network::LocalNet,
            _ => Network::Esmeralda, // default
        };
        
        // Create dual address
        let dual_address = DualAddress::new(
            view_public_key,
            spend_public_key,
            network,
            features,
            payment_id,
        ).map_err(|e| KeyManagementError::SeedPhraseError(format!("Failed to create dual address: {}", e)))?;
        
        Ok(TariAddress::Dual(dual_address))
    }

    /// Generate a single address with spend key only
    /// 
    /// Creates a single Tari address using only a spend key derived from the master key.
    /// This is simpler than dual addresses but has fewer features.
    pub fn get_single_address(&self, features: TariAddressFeatures) -> Result<TariAddress, KeyManagementError> {
        // Derive spend key from master key
        let (_, spend_key) = self.derive_key_pair()?;
        
        // Convert to CompressedPublicKey
        let spend_public_key = CompressedPublicKey::from_private_key(&spend_key);
        
        // Use the network from wallet metadata or default to Esmeralda
        let network = match self.metadata.network.as_str() {
            "mainnet" => Network::MainNet,
            "stagenet" => Network::StageNet,
            "localnet" => Network::LocalNet,
            _ => Network::Esmeralda, // default
        };
        
        // Create single address
        let single_address = SingleAddress::new(
            spend_public_key,
            network,
            features,
        ).map_err(|e| KeyManagementError::SeedPhraseError(format!("Failed to create single address: {}", e)))?;
        
        Ok(TariAddress::Single(single_address))
    }

    /// Derive view and spend key pair from master key
    /// 
    /// Uses the master key to derive a view key and spend key following Tari's
    /// key derivation specification.
    fn derive_key_pair(&self) -> Result<(PrivateKey, PrivateKey), KeyManagementError> {
        // Get master key bytes
        let master_key_bytes = self.master_key.as_bytes();
        
        // For now, we'll use a simple approach where:
        // - view_key = master_key
        // - spend_key = hash(master_key || "spend")
        // 
        // TODO: In the future, this should use proper hierarchical key derivation
        // with branch seeds as specified in the Tari key management documentation
        
        let view_key = PrivateKey::from_canonical_bytes(master_key_bytes)
            .map_err(|e| KeyManagementError::SeedPhraseError(format!("Failed to create view key: {}", e)))?;
        
        // Create spend key by hashing master_key + "spend" string
        use blake2b_simd::blake2b;
        let mut hasher_input = Vec::new();
        hasher_input.extend_from_slice(master_key_bytes);
        hasher_input.extend_from_slice(b"spend");
        
        let spend_key_hash = blake2b(&hasher_input);
        let spend_key_bytes: [u8; 32] = spend_key_hash.as_bytes()[0..32].try_into()
            .map_err(|_| KeyManagementError::SeedPhraseError("Failed to create spend key bytes".to_string()))?;
        
        let spend_key = PrivateKey::from_canonical_bytes(&spend_key_bytes)
            .map_err(|e| KeyManagementError::SeedPhraseError(format!("Failed to create spend key: {}", e)))?;
        
        Ok((view_key, spend_key))
    }

    /// Get a copy of the master key bytes (for demonstration purposes)
    /// WARNING: This exposes sensitive cryptographic material. Use with caution.
    pub fn master_key_bytes(&self) -> [u8; 32] {
        *self.master_key.as_bytes()
    }

    fn string_to_network(&self, network_str: &str) -> Network {
        match network_str.to_lowercase().as_str() {
            "mainnet" => Network::MainNet,
            "stagenet" => Network::StageNet,
            "localnet" => Network::LocalNet,
            "esmeralda" | "esme" => Network::Esmeralda,
            _ => Network::Esmeralda, // Default to Esmeralda for unknown networks
        }
    }

    /// Convert network to string representation
    fn network_to_string(&self, network: Network) -> String {
        match network {
            Network::MainNet => "mainnet".to_string(),
            Network::StageNet => "stagenet".to_string(),
            Network::LocalNet => "localnet".to_string(),
            Network::Esmeralda => "esmeralda".to_string(),
            Network::NextNet => "nextnet".to_string(),
            Network::Igor => "igor".to_string(),
        }
    }
}

impl Zeroize for Wallet {
    fn zeroize(&mut self) {
        self.master_key.zeroize();
        self.birthday = 0;
        self.metadata.zeroize();
        if let Some(ref mut seed_phrase) = self.original_seed_phrase {
            seed_phrase.zeroize();
        }
        self.original_seed_phrase = None;
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl Zeroize for WalletMetadata {
    fn zeroize(&mut self) {
        if let Some(ref mut label) = self.label {
            label.zeroize();
        }
        self.network.zeroize();
        self.current_key_index = 0;
        for (_key, _value) in self.properties.iter_mut() {
            // Note: We can't zeroize String keys/values directly in HashMap iteration
            // This is a limitation, but the metadata is not as sensitive as the master key
        }
        self.properties.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_creation() {
        let master_key = [1u8; 32];
        let birthday = 1640995200; // Jan 1, 2022
        let wallet = Wallet::new(master_key, birthday);

        assert_eq!(wallet.birthday(), birthday);
        assert_eq!(wallet.master_key_bytes(), master_key);
        assert_eq!(wallet.current_key_index(), 0);
        assert_eq!(wallet.network(), "");
        assert!(wallet.label().is_none());
    }

    #[test]
    fn test_wallet_metadata() {
        let mut wallet = Wallet::new([0u8; 32], 0);

        // Test label
        wallet.set_label(Some("Test Wallet".to_string()));
        assert_eq!(wallet.label(), Some(&"Test Wallet".to_string()));

        // Test network
        wallet.set_network("mainnet".to_string());
        assert_eq!(wallet.network(), "mainnet");

        // Test key index
        wallet.set_current_key_index(42);
        assert_eq!(wallet.current_key_index(), 42);

        // Test custom properties
        wallet.set_property("version".to_string(), "1.0".to_string());
        assert_eq!(wallet.get_property("version"), Some(&"1.0".to_string()));
        
        let removed = wallet.remove_property("version");
        assert_eq!(removed, Some("1.0".to_string()));
        assert_eq!(wallet.get_property("version"), None);
    }

    #[test]
    fn test_wallet_zeroization() {
        let master_key = [42u8; 32];
        let mut wallet = Wallet::new(master_key, 1234567890);
        wallet.set_label(Some("Secret Wallet".to_string()));
        wallet.set_network("testnet".to_string());

        // Verify initial state
        assert_eq!(wallet.master_key_bytes(), master_key);
        assert_eq!(wallet.birthday(), 1234567890);
        assert_eq!(wallet.label(), Some(&"Secret Wallet".to_string()));

        // Zeroize
        wallet.zeroize();

        // Verify zeroization
        assert_eq!(wallet.master_key_bytes(), [0u8; 32]);
        assert_eq!(wallet.birthday(), 0);
        assert_eq!(wallet.current_key_index(), 0);
    }

    #[test]
    fn test_wallet_metadata_default() {
        let metadata = WalletMetadata::default();
        assert!(metadata.label.is_none());
        assert_eq!(metadata.network, "");
        assert_eq!(metadata.current_key_index, 0);
        assert!(metadata.properties.is_empty());
    }

    #[test]
    fn test_wallet_new_from_seed_phrase() {
        // Test with no passphrase
        let seed_phrase = crate::key_management::generate_seed_phrase().unwrap();
        
        let wallet = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        
        // Verify the wallet was created successfully
        assert!(wallet.birthday() > 0); // Should have a valid birthday
        assert_eq!(wallet.current_key_index(), 0);
        assert_eq!(wallet.network(), "");
        assert!(wallet.label().is_none());
        
        // Verify that the same seed phrase produces the same master key
        let wallet2 = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        assert_eq!(wallet.master_key_bytes(), wallet2.master_key_bytes());
        
        // Test with passphrase - need to generate seed phrase with the same passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("test")).unwrap();
        let seed_phrase_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        
        let wallet_with_pass = Wallet::new_from_seed_phrase(&seed_phrase_with_pass, Some("test")).unwrap();
        let wallet_with_pass2 = Wallet::new_from_seed_phrase(&seed_phrase_with_pass, Some("test")).unwrap();
        assert_eq!(wallet_with_pass.master_key_bytes(), wallet_with_pass2.master_key_bytes());
    }

    #[test]
    fn test_wallet_new_from_seed_phrase_without_passphrase() {
        let seed_phrase = crate::key_management::generate_seed_phrase().unwrap();
        
        let wallet = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        
        // Should create a valid wallet
        assert!(wallet.birthday() > 0);
        assert_eq!(wallet.current_key_index(), 0);
    }

    #[test]
    fn test_wallet_new_from_seed_phrase_different_passphrases() {
        // Create different CipherSeeds with different passphrases
        let cipher_seed1 = CipherSeed::new();
        let cipher_seed2 = CipherSeed::new();
        let cipher_seed3 = CipherSeed::new();
        
        let encrypted1 = cipher_seed1.encipher(Some("passphrase1")).unwrap();
        let seed_phrase1 = bytes_to_mnemonic(&encrypted1).unwrap();
        
        let encrypted2 = cipher_seed2.encipher(Some("passphrase2")).unwrap();
        let seed_phrase2 = bytes_to_mnemonic(&encrypted2).unwrap();
        
        let encrypted3 = cipher_seed3.encipher(None).unwrap();
        let seed_phrase3 = bytes_to_mnemonic(&encrypted3).unwrap();
        
        let wallet1 = Wallet::new_from_seed_phrase(&seed_phrase1, Some("passphrase1")).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(&seed_phrase2, Some("passphrase2")).unwrap();
        let wallet3 = Wallet::new_from_seed_phrase(&seed_phrase3, None).unwrap();
        
        // Verify all wallets are created successfully
        assert!(wallet1.birthday() > 0);
        assert!(wallet2.birthday() > 0);
        assert!(wallet3.birthday() > 0);
        
        // Different seed phrases should produce different master keys
        assert_ne!(wallet1.master_key_bytes(), wallet2.master_key_bytes());
        assert_ne!(wallet1.master_key_bytes(), wallet3.master_key_bytes());
        assert_ne!(wallet2.master_key_bytes(), wallet3.master_key_bytes());
        
        // Same seed phrase and passphrase should produce the same master key
        let wallet1_duplicate = Wallet::new_from_seed_phrase(&seed_phrase1, Some("passphrase1")).unwrap();
        assert_eq!(wallet1.master_key_bytes(), wallet1_duplicate.master_key_bytes());
    }

    #[test]
    fn test_wallet_new_from_invalid_seed_phrase() {
        let invalid_phrase = "invalid seed phrase";
        
        let result = Wallet::new_from_seed_phrase(invalid_phrase, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_current_birthday() {
        let birthday = Wallet::calculate_current_birthday();
        
        // Birthday should be a reasonable number (days since 2022-01-01)
        // As of 2024, this should be at least 365 days but less than 10000 days
        assert!(birthday >= 365);
        assert!(birthday < 10000);
    }

    #[test]
    fn test_wallet_generate_new() {
        // Generate a new wallet without passphrase
        let wallet1 = Wallet::generate_new(None);
        
        // Verify basic properties
        assert!(wallet1.birthday() > 0); // Should have a valid birthday
        assert_eq!(wallet1.current_key_index(), 0);
        assert_eq!(wallet1.network(), "");
        assert!(wallet1.label().is_none());
        
        // Generate another wallet with passphrase (should still work)
        let wallet2 = Wallet::generate_new(Some("test_passphrase"));
        
        // Both wallets should have valid birthdays (around the same time)
        assert!(wallet2.birthday() > 0);
        let birthday_diff = if wallet1.birthday() > wallet2.birthday() {
            wallet1.birthday() - wallet2.birthday()
        } else {
            wallet2.birthday() - wallet1.birthday()
        };
        assert!(birthday_diff <= 1); // Should be created within the same day
        
        // Each wallet should have different random master keys
        assert_ne!(wallet1.master_key_bytes(), wallet2.master_key_bytes());
    }

    #[test]
    fn test_wallet_generate_new_randomness() {
        // Generate multiple wallets to verify randomness
        let wallet1 = Wallet::generate_new(None);
        let wallet2 = Wallet::generate_new(None);
        let wallet3 = Wallet::generate_new(Some("passphrase"));
        let wallet4 = Wallet::generate_new(Some("different_passphrase"));
        
        // All should have different master keys (highly unlikely to be the same with proper randomness)
        let keys = [
            wallet1.master_key_bytes(),
            wallet2.master_key_bytes(),
            wallet3.master_key_bytes(),
            wallet4.master_key_bytes(),
        ];
        
        // Verify no two keys are the same
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                assert_ne!(keys[i], keys[j], "Wallets {} and {} have the same master key", i, j);
            }
        }
        
        // All should have the same birthday (created within a short time span)
        let birthdays = [wallet1.birthday(), wallet2.birthday(), wallet3.birthday(), wallet4.birthday()];
        let min_birthday = *birthdays.iter().min().unwrap();
        let max_birthday = *birthdays.iter().max().unwrap();
        assert!(max_birthday - min_birthday <= 1); // All created within the same day
    }

    #[test]
    fn test_wallet_generate_new_vs_manual_creation() {
        let generated_wallet = Wallet::generate_new(None);
        
        // Create a manual wallet with the same birthday for comparison
        let manual_wallet = Wallet::new([42u8; 32], generated_wallet.birthday());
        
        // Should have the same birthday but different master keys
        assert_eq!(generated_wallet.birthday(), manual_wallet.birthday());
        assert_ne!(generated_wallet.master_key_bytes(), manual_wallet.master_key_bytes());
        
        // Generated wallet should have non-zero entropy (extremely unlikely to be all zeros)
        assert_ne!(generated_wallet.master_key_bytes(), [0u8; 32]);
    }

    #[test]
    fn test_wallet_export_seed_phrase_from_phrase() {
        // Generate a seed phrase with no passphrase (default from generate_seed_phrase)
        let seed_phrase = crate::key_management::generate_seed_phrase().unwrap();
        
        let wallet = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        
        // Should be able to export the original seed phrase
        let exported_phrase = wallet.export_seed_phrase().unwrap();
        assert_eq!(exported_phrase, seed_phrase);
        
        // Test with a passphrase - need to create seed phrase with the same passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("test")).unwrap();
        let seed_phrase_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        
        let wallet_with_pass = Wallet::new_from_seed_phrase(&seed_phrase_with_pass, Some("test")).unwrap();
        let exported_phrase_with_pass = wallet_with_pass.export_seed_phrase().unwrap();
        assert_eq!(exported_phrase_with_pass, seed_phrase_with_pass);
    }

    #[test]
    fn test_wallet_export_seed_phrase_from_generated() {
        let wallet = Wallet::generate_new(None);
        
        // Should fail to export seed phrase since wallet was generated randomly
        let result = wallet.export_seed_phrase();
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(e.to_string().contains("Wallet was not created from a seed phrase"));
        }
    }

    #[test]
    fn test_wallet_export_seed_phrase_from_manual() {
        let wallet = Wallet::new([42u8; 32], 1234567890);
        
        // Should fail to export seed phrase since wallet was created manually
        let result = wallet.export_seed_phrase();
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(e.to_string().contains("Wallet was not created from a seed phrase"));
        }
    }

    #[test]
    fn test_wallet_export_seed_phrase_different_phrases() {
        let phrase1 = crate::key_management::generate_seed_phrase().unwrap();
        let phrase2 = crate::key_management::generate_seed_phrase().unwrap();
        
        let wallet1 = Wallet::new_from_seed_phrase(&phrase1, None).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(&phrase2, None).unwrap();
        
        let exported1 = wallet1.export_seed_phrase().unwrap();
        let exported2 = wallet2.export_seed_phrase().unwrap();
        
        assert_eq!(exported1, phrase1);
        assert_eq!(exported2, phrase2);
        assert_ne!(exported1, exported2);
    }

    #[test]
    fn test_wallet_zeroization_with_seed_phrase() {
        let seed_phrase = crate::key_management::generate_seed_phrase().unwrap();
        let mut wallet = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        
        // Verify seed phrase is stored
        assert_eq!(wallet.export_seed_phrase().unwrap(), seed_phrase);
        
        // Zeroize the wallet
        wallet.zeroize();
        
        // Verify seed phrase is no longer available
        let result = wallet.export_seed_phrase();
        assert!(result.is_err());
    }

    #[test]
    fn test_wallet_seed_phrase_consistency() {
        // Create seed phrases with specific passphrases
        let cipher_seed1 = CipherSeed::new();
        let cipher_seed2 = CipherSeed::new();
        
        let encrypted1 = cipher_seed1.encipher(Some("test1")).unwrap();
        let seed_phrase1 = bytes_to_mnemonic(&encrypted1).unwrap();
        
        let encrypted2 = cipher_seed2.encipher(Some("test2")).unwrap();
        let seed_phrase2 = bytes_to_mnemonic(&encrypted2).unwrap();
        
        let wallet1 = Wallet::new_from_seed_phrase(&seed_phrase1, Some("test1")).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(&seed_phrase2, Some("test2")).unwrap();
        
        // Each should export their respective seed phrase
        let exported1 = wallet1.export_seed_phrase().unwrap();
        let exported2 = wallet2.export_seed_phrase().unwrap();
        assert_eq!(exported1, seed_phrase1);
        assert_eq!(exported2, seed_phrase2);
        
        // Different seed phrases should be different
        assert_ne!(exported1, exported2);
        
        // They should have different master keys due to different underlying CipherSeeds
        assert_ne!(wallet1.master_key_bytes(), wallet2.master_key_bytes());
        
        // Test that same seed phrase with same passphrase produces consistent results
        let wallet1_duplicate = Wallet::new_from_seed_phrase(&seed_phrase1, Some("test1")).unwrap();
        assert_eq!(wallet1.master_key_bytes(), wallet1_duplicate.master_key_bytes());
        assert_eq!(wallet1.export_seed_phrase().unwrap(), wallet1_duplicate.export_seed_phrase().unwrap());
    }

    #[test]
    fn test_wallet_generate_new_with_seed_phrase() {
        // Generate wallets with seed phrases
        let wallet1 = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let wallet2 = Wallet::generate_new_with_seed_phrase(None).unwrap();
        
        // Should be able to export seed phrases
        let phrase1 = wallet1.export_seed_phrase().unwrap();
        let phrase2 = wallet2.export_seed_phrase().unwrap();
        
        // Phrases should be different (different random CipherSeeds)
        assert_ne!(phrase1, phrase2);
        
        // Phrases should be valid 24-word mnemonics
        assert_eq!(phrase1.split_whitespace().count(), 24);
        assert_eq!(phrase2.split_whitespace().count(), 24);
        
        // Should be able to validate the phrases
        assert!(validate_seed_phrase(&phrase1).is_ok());
        assert!(validate_seed_phrase(&phrase2).is_ok());
        
        // Should be able to recreate the wallets from the exported phrases
        let recreated1 = Wallet::new_from_seed_phrase(&phrase1, None).unwrap();
        let recreated2 = Wallet::new_from_seed_phrase(&phrase2, None).unwrap();
        
        // Recreated wallets should have the same master keys
        assert_eq!(wallet1.master_key_bytes(), recreated1.master_key_bytes());
        assert_eq!(wallet2.master_key_bytes(), recreated2.master_key_bytes());
        
        // Test with passphrase separately
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("test")).unwrap();
        let phrase_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        
        let wallet_with_pass = Wallet::new_from_seed_phrase(&phrase_with_pass, Some("test")).unwrap();
        let recreated_with_pass = Wallet::new_from_seed_phrase(&phrase_with_pass, Some("test")).unwrap();
        assert_eq!(wallet_with_pass.master_key_bytes(), recreated_with_pass.master_key_bytes());
    }

    #[test]
    fn test_wallet_generate_new_with_seed_phrase_vs_generate_new() {
        let wallet_with_phrase = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let wallet_random = Wallet::generate_new(None);
        
        // Wallet with seed phrase should allow export
        assert!(wallet_with_phrase.export_seed_phrase().is_ok());
        
        // Randomly generated wallet should not allow export
        assert!(wallet_random.export_seed_phrase().is_err());
        
        // Both should have valid birthdays
        assert!(wallet_with_phrase.birthday() > 0);
        assert!(wallet_random.birthday() > 0);
        
        // Should have different master keys
        assert_ne!(wallet_with_phrase.master_key_bytes(), wallet_random.master_key_bytes());
    }

    #[test]
    fn test_wallet_generate_new_with_seed_phrase_deterministic() {
        // Generate a wallet with seed phrase (no passphrase)
        let wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let exported_phrase = wallet.export_seed_phrase().unwrap();
        
        // Create another wallet from the same phrase
        let wallet2 = Wallet::new_from_seed_phrase(&exported_phrase, None).unwrap();
        
        // Should have the same master key
        assert_eq!(wallet.master_key_bytes(), wallet2.master_key_bytes());
        
        // Should export the same phrase
        assert_eq!(wallet.export_seed_phrase().unwrap(), wallet2.export_seed_phrase().unwrap());
        
        // Test with passphrase - need to create the CipherSeed properly
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("passphrase")).unwrap();
        let seed_phrase_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        
        let wallet_with_pass = Wallet::new_from_seed_phrase(&seed_phrase_with_pass, Some("passphrase")).unwrap();
        let wallet_with_pass2 = Wallet::new_from_seed_phrase(&seed_phrase_with_pass, Some("passphrase")).unwrap();
        
        // Should have the same master key
        assert_eq!(wallet_with_pass.master_key_bytes(), wallet_with_pass2.master_key_bytes());
        
        // Should export the same phrase
        assert_eq!(wallet_with_pass.export_seed_phrase().unwrap(), wallet_with_pass2.export_seed_phrase().unwrap());
    }

    #[test]
    fn test_wallet_generate_new_with_seed_phrase_randomness() {
        // Generate multiple wallets to verify randomness
        let wallet1 = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let wallet2 = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let wallet3 = Wallet::generate_new_with_seed_phrase(Some("passphrase1")).unwrap();
        let wallet4 = Wallet::generate_new_with_seed_phrase(Some("passphrase2")).unwrap();
        
        // All should have different master keys
        let keys = [
            wallet1.master_key_bytes(),
            wallet2.master_key_bytes(),
            wallet3.master_key_bytes(),
            wallet4.master_key_bytes(),
        ];
        
        // Verify no two keys are the same
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                assert_ne!(keys[i], keys[j], "Wallets {} and {} have the same master key", i, j);
            }
        }
        
        // All should be able to export their seed phrases
        assert!(wallet1.export_seed_phrase().is_ok());
        assert!(wallet2.export_seed_phrase().is_ok());
        assert!(wallet3.export_seed_phrase().is_ok());
        assert!(wallet4.export_seed_phrase().is_ok());
    }

    #[test]
    fn test_wallet_get_dual_address() {
        let wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        
        // Test basic dual address generation
        let address = wallet.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        
        // Verify it's a dual address
        assert!(matches!(address, TariAddress::Dual(_)));
        assert!(address.public_view_key().is_some());
        assert_eq!(address.network(), Network::Esmeralda); // Default network
        assert_eq!(address.features(), TariAddressFeatures::create_interactive_and_one_sided());
        
        // Test dual address with payment ID
        let payment_id = vec![1u8, 2, 3, 4, 5];
        let address_with_payment = wallet.get_dual_address(
            TariAddressFeatures::create_interactive_only(),
            Some(payment_id.clone())
        ).unwrap();
        
        assert!(matches!(address_with_payment, TariAddress::Dual(_)));
        assert!(address_with_payment.features().contains(TariAddressFeatures::PAYMENT_ID));
        
        // Test that the same wallet produces the same address
        let address2 = wallet.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        assert_eq!(address.to_hex(), address2.to_hex());
    }

    #[test]
    fn test_wallet_get_single_address() {
        let wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        
        // Test basic single address generation
        let address = wallet.get_single_address(TariAddressFeatures::create_interactive_only()).unwrap();
        
        // Verify it's a single address
        assert!(matches!(address, TariAddress::Single(_)));
        assert!(address.public_view_key().is_none());
        assert_eq!(address.network(), Network::Esmeralda); // Default network
        assert_eq!(address.features(), TariAddressFeatures::create_interactive_only());
        
        // Test that the same wallet produces the same address
        let address2 = wallet.get_single_address(TariAddressFeatures::create_interactive_only()).unwrap();
        assert_eq!(address.to_hex(), address2.to_hex());
    }

    #[test]
    fn test_wallet_address_generation_with_different_networks() {
        let mut wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        
        // Test default network (Esmeralda)
        let address_default = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
        assert_eq!(address_default.network(), Network::Esmeralda);
        
        // Test mainnet
        wallet.set_network("mainnet".to_string());
        let address_mainnet = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
        assert_eq!(address_mainnet.network(), Network::MainNet);
        
        // Test stagenet
        wallet.set_network("stagenet".to_string());
        let address_stagenet = wallet.get_single_address(TariAddressFeatures::create_one_sided_only()).unwrap();
        assert_eq!(address_stagenet.network(), Network::StageNet);
        
        // Test localnet
        wallet.set_network("localnet".to_string());
        let address_localnet = wallet.get_single_address(TariAddressFeatures::create_one_sided_only()).unwrap();
        assert_eq!(address_localnet.network(), Network::LocalNet);
        
        // Test unknown network defaults to Esmeralda
        wallet.set_network("unknown".to_string());
        let address_unknown = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
        assert_eq!(address_unknown.network(), Network::Esmeralda);
    }

    #[test]
    fn test_wallet_address_generation_deterministic() {
        // Create two wallets from the same seed phrase
        let seed_phrase = crate::key_management::generate_seed_phrase().unwrap();
        let wallet1 = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(&seed_phrase, None).unwrap();
        
        // They should generate the same addresses
        let dual_addr1 = wallet1.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        let dual_addr2 = wallet2.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        assert_eq!(dual_addr1.to_hex(), dual_addr2.to_hex());
        
        let single_addr1 = wallet1.get_single_address(TariAddressFeatures::create_interactive_only()).unwrap();
        let single_addr2 = wallet2.get_single_address(TariAddressFeatures::create_interactive_only()).unwrap();
        assert_eq!(single_addr1.to_hex(), single_addr2.to_hex());
    }

    #[test]
    fn test_wallet_address_generation_different_features() {
        let wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        
        // Generate addresses with different features
        let interactive_only = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
        let one_sided_only = wallet.get_dual_address(TariAddressFeatures::create_one_sided_only(), None).unwrap();
        let both_features = wallet.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        
        // Verify features are correctly set
        assert_eq!(interactive_only.features(), TariAddressFeatures::create_interactive_only());
        assert_eq!(one_sided_only.features(), TariAddressFeatures::create_one_sided_only());
        assert_eq!(both_features.features(), TariAddressFeatures::create_interactive_and_one_sided());
        
        // Different features should produce different addresses (due to feature byte in address)
        assert_ne!(interactive_only.to_hex(), one_sided_only.to_hex());
        assert_ne!(interactive_only.to_hex(), both_features.to_hex());
        assert_ne!(one_sided_only.to_hex(), both_features.to_hex());
    }

    #[test]
    fn test_wallet_address_formats() {
        let wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
        let address = wallet.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None).unwrap();
        
        // Test all address formats
        let emoji = address.to_emoji_string();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        
        // All should be valid and non-empty
        assert!(!emoji.is_empty());
        assert!(!base58.is_empty());
        assert!(!hex.is_empty());
        
        // Should be able to parse them back
        assert!(TariAddress::from_emoji_string(&emoji).is_ok());
        assert!(TariAddress::from_base58(&base58).is_ok());
        assert!(TariAddress::from_hex(&hex).is_ok());
    }
} 
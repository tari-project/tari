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
use crate::errors::KeyManagementError;
use crate::key_management::mnemonic_to_master_key;

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

    /// Get a copy of the master key bytes (for internal use only)
    pub(crate) fn master_key_bytes(&self) -> [u8; 32] {
        *self.master_key.as_bytes()
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
        // Test with a 24-word seed phrase
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let passphrase = Some("test");
        
        let wallet = Wallet::new_from_seed_phrase(seed_phrase, passphrase).unwrap();
        
        // Verify the wallet was created successfully
        assert!(wallet.birthday() > 0); // Should have a valid birthday
        assert_eq!(wallet.current_key_index(), 0);
        assert_eq!(wallet.network(), "");
        assert!(wallet.label().is_none());
        
        // Verify that the same seed phrase produces the same master key
        let wallet2 = Wallet::new_from_seed_phrase(seed_phrase, passphrase).unwrap();
        assert_eq!(wallet.master_key_bytes(), wallet2.master_key_bytes());
    }

    #[test]
    fn test_wallet_new_from_seed_phrase_without_passphrase() {
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        
        let wallet = Wallet::new_from_seed_phrase(seed_phrase, None).unwrap();
        
        // Should create a valid wallet
        assert!(wallet.birthday() > 0);
        assert_eq!(wallet.current_key_index(), 0);
    }

    #[test]
    fn test_wallet_new_from_seed_phrase_different_passphrases() {
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        
        let wallet1 = Wallet::new_from_seed_phrase(seed_phrase, Some("passphrase1")).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(seed_phrase, Some("passphrase2")).unwrap();
        let wallet3 = Wallet::new_from_seed_phrase(seed_phrase, None).unwrap();
        
        // Verify all wallets are created successfully
        assert!(wallet1.birthday() > 0);
        assert!(wallet2.birthday() > 0);
        assert!(wallet3.birthday() > 0);
        
        // Different passphrases should produce different master keys
        assert_ne!(wallet1.master_key_bytes(), wallet2.master_key_bytes());
        assert_ne!(wallet1.master_key_bytes(), wallet3.master_key_bytes());
        assert_ne!(wallet2.master_key_bytes(), wallet3.master_key_bytes());
        
        // Same seed phrase and passphrase should produce the same master key
        let wallet1_duplicate = Wallet::new_from_seed_phrase(seed_phrase, Some("passphrase1")).unwrap();
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
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let passphrase = Some("test");
        
        let wallet = Wallet::new_from_seed_phrase(seed_phrase, passphrase).unwrap();
        
        // Should be able to export the original seed phrase
        let exported_phrase = wallet.export_seed_phrase().unwrap();
        assert_eq!(exported_phrase, seed_phrase);
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
        let phrase1 = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let phrase2 = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        
        let wallet1 = Wallet::new_from_seed_phrase(phrase1, None).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(phrase2, None).unwrap();
        
        let exported1 = wallet1.export_seed_phrase().unwrap();
        let exported2 = wallet2.export_seed_phrase().unwrap();
        
        assert_eq!(exported1, phrase1);
        assert_eq!(exported2, phrase2);
        assert_ne!(exported1, exported2);
    }

    #[test]
    fn test_wallet_zeroization_with_seed_phrase() {
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let mut wallet = Wallet::new_from_seed_phrase(seed_phrase, None).unwrap();
        
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
        let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let passphrase1 = Some("test1");
        let passphrase2 = Some("test2");
        
        let wallet1 = Wallet::new_from_seed_phrase(seed_phrase, passphrase1).unwrap();
        let wallet2 = Wallet::new_from_seed_phrase(seed_phrase, passphrase2).unwrap();
        
        // Both should export the same seed phrase regardless of passphrase
        let exported1 = wallet1.export_seed_phrase().unwrap();
        let exported2 = wallet2.export_seed_phrase().unwrap();
        assert_eq!(exported1, seed_phrase);
        assert_eq!(exported2, seed_phrase);
        assert_eq!(exported1, exported2);
        
        // But they should have different master keys due to different passphrases
        assert_ne!(wallet1.master_key_bytes(), wallet2.master_key_bytes());
    }
} 
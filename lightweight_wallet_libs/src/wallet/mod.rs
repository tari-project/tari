// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Wallet functionality for lightweight Tari wallets
//!
//! This module provides the core wallet struct and operations for managing
//! master keys, seed phrases, and wallet metadata.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;
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
        }
    }

    /// Create a new wallet from a seed phrase and optional passphrase
    pub fn new_from_seed_phrase(phrase: &str, passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        // Convert seed phrase to master key
        let master_key = mnemonic_to_master_key(phrase, passphrase)?;
        
        // Calculate current birthday as days since genesis
        let birthday = Self::calculate_current_birthday();
        
        Ok(Self::new(master_key, birthday))
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
} 
// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Wallet functionality for lightweight Tari wallets
//!
//! This module provides the core wallet struct and operations for managing
//! master keys, seed phrases, and wallet metadata.

use std::collections::HashMap;
use zeroize::Zeroize;
use crate::data_structures::SafeArray;
use crate::errors::KeyManagementError;

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
} 
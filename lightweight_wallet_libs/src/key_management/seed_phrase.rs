// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Minimal BIP39-like mnemonic-to-seed logic for lightweight wallets (English only)

use crate::errors::KeyManagementError;
use blake2::{Blake2b, Digest};
use digest::consts::U64;

/// Converts a mnemonic phrase and optional passphrase to a 32-byte master key using Blake2b
pub fn mnemonic_to_master_key(mnemonic: &str, passphrase: Option<&str>) -> Result<[u8; 32], KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::MnemonicError("Mnemonic phrase is empty".to_string()));
    }
    // For simplicity, just concatenate mnemonic and passphrase and hash with Blake2b
    let salt = passphrase.unwrap_or("");
    let input = format!("{}{}", mnemonic.trim(), salt);
    let mut hasher = Blake2b::<U64>::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result[..32]);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mnemonic_to_master_key() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let passphrase = Some("test");
        let key = mnemonic_to_master_key(mnemonic, passphrase).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_empty_mnemonic() {
        let result = mnemonic_to_master_key("", None);
        assert!(result.is_err());
    }
} 
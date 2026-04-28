// Copyright 2025. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Secure keystore implementation using the OS keyring with passphrase-based encryption.
//!
//! Keys are encrypted with ChaCha20-Poly1305 using a key derived from the passphrase via Argon2id,
//! then stored in the OS keyring (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux).

use argon2::{Argon2, password_hash, password_hash::PasswordHasher};
use chacha20poly1305::{
    ChaCha20Poly1305,
    Nonce,
    aead::{Aead, KeyInit},
};
use keyring::Entry;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::PrivateKey;
use tari_utilities::{ByteArray, hex::Hex};
use zeroize::Zeroizing;

use crate::error::OfflineSignerError;

const SERVICE_NAME: &str = "minotari_offline_signer";
const SPEND_KEY_ENTRY: &str = "spend_key";
const VIEW_KEY_ENTRY: &str = "view_key";

/// Encrypted key data stored in the keyring
#[derive(Serialize, Deserialize)]
struct EncryptedKeyData {
    /// Salt used for key derivation (base64 encoded)
    salt: String,
    /// Nonce used for encryption (hex encoded)
    nonce: String,
    /// Encrypted key data (hex encoded)
    ciphertext: String,
}

/// Derives an encryption key from a passphrase and salt using Argon2id
fn derive_encryption_key(passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, OfflineSignerError> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password_with_salt(passphrase.as_bytes(), salt)
        .map_err(|e| OfflineSignerError::EncryptionError(format!("Failed to derive key: {}", e)))?;

    let hash_bytes = hash
        .hash
        .ok_or_else(|| OfflineSignerError::EncryptionError("Failed to get hash output".to_string()))?;

    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(
        hash_bytes
            .as_bytes()
            .get(..32)
            .ok_or_else(|| OfflineSignerError::EncryptionError("Derived key is shorter than expected".to_string()))?,
    );
    Ok(key)
}

/// Encrypts a private key using ChaCha20-Poly1305
fn encrypt_key(key: &PrivateKey, passphrase: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
    // Generate random salt and nonce
    let salt = password_hash::generate_salt();
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Derive encryption key from passphrase
    let encryption_key = derive_encryption_key(passphrase, &salt)?;

    // Encrypt the private key
    let cipher = ChaCha20Poly1305::new_from_slice(&*encryption_key)
        .map_err(|e| OfflineSignerError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

    let plaintext = key.to_vec();
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| OfflineSignerError::EncryptionError(format!("Failed to encrypt: {}", e)))?;

    Ok(EncryptedKeyData {
        salt: salt.to_hex(),
        nonce: nonce_bytes.to_hex(),
        ciphertext: ciphertext.to_hex(),
    })
}

/// Decrypts a private key using ChaCha20-Poly1305
fn decrypt_key(data: &EncryptedKeyData, passphrase: &str) -> Result<PrivateKey, OfflineSignerError> {
    // Parse salt and nonce
    let salt =
        Vec::from_hex(&data.salt).map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid salt: {}", e)))?;

    let nonce_bytes =
        Vec::from_hex(&data.nonce).map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid nonce: {}", e)))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = Vec::from_hex(&data.ciphertext)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid ciphertext: {}", e)))?;

    // Derive encryption key from passphrase
    let encryption_key = derive_encryption_key(passphrase, &salt)?;

    // Decrypt the private key
    let cipher = ChaCha20Poly1305::new_from_slice(&*encryption_key)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Failed to create cipher: {}", e)))?;

    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|_| {
        OfflineSignerError::DecryptionError("Failed to decrypt: invalid passphrase or corrupted data".to_string())
    })?;

    PrivateKey::from_vec(&plaintext)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid key data: {}", e)))
}

/// Stores a key in the OS keyring
fn store_in_keyring(entry_name: &str, data: &EncryptedKeyData) -> Result<(), OfflineSignerError> {
    let entry = Entry::new(SERVICE_NAME, entry_name)
        .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

    let json = serde_json::to_string(data)
        .map_err(|e| OfflineSignerError::SerializationError(format!("Failed to serialize key data: {}", e)))?;

    entry
        .set_password(&json)
        .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to store in keyring: {}", e)))?;

    Ok(())
}

/// Retrieves a key from the OS keyring
fn retrieve_from_keyring(entry_name: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
    let entry = Entry::new(SERVICE_NAME, entry_name)
        .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

    let json = entry
        .get_password()
        .map_err(|e| OfflineSignerError::NotInitialized(format!("Key not found in keyring: {}", e)))?;

    serde_json::from_str(&json).map_err(|e| OfflineSignerError::ParseError(format!("Failed to parse key data: {}", e)))
}

/// Deletes a key from the OS keyring
fn delete_from_keyring(entry_name: &str) -> Result<(), OfflineSignerError> {
    let entry = Entry::new(SERVICE_NAME, entry_name)
        .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

    entry
        .delete_credential()
        .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to delete from keyring: {}", e)))?;

    Ok(())
}

/// Initializes the keystore with spend and view keys
pub fn init_keystore(
    spend_key: &PrivateKey,
    view_key: &PrivateKey,
    passphrase: &str,
) -> Result<(), OfflineSignerError> {
    // Encrypt both keys
    let encrypted_spend = encrypt_key(spend_key, passphrase)?;
    let encrypted_view = encrypt_key(view_key, passphrase)?;

    // Store in keyring
    store_in_keyring(SPEND_KEY_ENTRY, &encrypted_spend)?;
    store_in_keyring(VIEW_KEY_ENTRY, &encrypted_view)?;

    Ok(())
}

/// Retrieves the spend and view keys from the keystore
pub fn get_keys(passphrase: &str) -> Result<(PrivateKey, PrivateKey), OfflineSignerError> {
    let encrypted_spend = retrieve_from_keyring(SPEND_KEY_ENTRY)?;
    let encrypted_view = retrieve_from_keyring(VIEW_KEY_ENTRY)?;

    let spend_key = decrypt_key(&encrypted_spend, passphrase)?;
    let view_key = decrypt_key(&encrypted_view, passphrase)?;

    Ok((spend_key, view_key))
}

/// Checks if the keystore has been initialized
pub fn is_initialized() -> bool {
    Entry::new(SERVICE_NAME, SPEND_KEY_ENTRY)
        .and_then(|e| e.get_password())
        .is_ok()
}

/// Clears all keys from the keystore
pub fn clear_keystore() -> Result<(), OfflineSignerError> {
    // Try to delete both keys, ignoring errors if they don't exist
    let _unused = delete_from_keyring(SPEND_KEY_ENTRY);
    let _unused = delete_from_keyring(VIEW_KEY_ENTRY);
    Ok(())
}

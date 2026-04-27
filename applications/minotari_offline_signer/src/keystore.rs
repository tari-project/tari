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

use argon2::{Argon2, password_hash::PasswordHasher};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use keyring::Entry;
use phc::Salt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::PrivateKey;
use tari_utilities::{ByteArray, hex::Hex};
use zeroize::Zeroizing;

use crate::error::OfflineSignerError;

const SERVICE_NAME: &str = "minotari_offline_signer";
const SPEND_KEY_ENTRY: &str = "spend_key";
const VIEW_KEY_ENTRY: &str = "view_key";

#[cfg(feature = "test-keystore")]
pub(crate) use test_keystore::set_test_keystore_file;

/// Encrypted key data stored by a keystore backend.
#[derive(Clone, Serialize, Deserialize)]
struct EncryptedKeyData {
    /// Salt used for key derivation (base64 encoded)
    salt: String,
    /// Nonce used for encryption (hex encoded)
    nonce: String,
    /// Encrypted key data (hex encoded)
    ciphertext: String,
}

trait KeystoreBackend {
    fn store(&self, entry_name: &str, data: &EncryptedKeyData) -> Result<(), OfflineSignerError>;
    fn retrieve(&self, entry_name: &str) -> Result<EncryptedKeyData, OfflineSignerError>;
    fn delete(&self, entry_name: &str) -> Result<(), OfflineSignerError>;
    fn description(&self) -> &'static str;
}

enum KeystoreBackendKind {
    OsKeyring(OsKeyringBackend),
    #[cfg(feature = "test-keystore")]
    TestFile(test_keystore::TestFileBackend),
}

impl KeystoreBackend for KeystoreBackendKind {
    fn store(&self, entry_name: &str, data: &EncryptedKeyData) -> Result<(), OfflineSignerError> {
        match self {
            Self::OsKeyring(backend) => backend.store(entry_name, data),
            #[cfg(feature = "test-keystore")]
            Self::TestFile(backend) => backend.store(entry_name, data),
        }
    }

    fn retrieve(&self, entry_name: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
        match self {
            Self::OsKeyring(backend) => backend.retrieve(entry_name),
            #[cfg(feature = "test-keystore")]
            Self::TestFile(backend) => backend.retrieve(entry_name),
        }
    }

    fn delete(&self, entry_name: &str) -> Result<(), OfflineSignerError> {
        match self {
            Self::OsKeyring(backend) => backend.delete(entry_name),
            #[cfg(feature = "test-keystore")]
            Self::TestFile(backend) => backend.delete(entry_name),
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::OsKeyring(backend) => backend.description(),
            #[cfg(feature = "test-keystore")]
            Self::TestFile(backend) => backend.description(),
        }
    }
}

pub struct Keystore {
    backend: KeystoreBackendKind,
}

impl Keystore {
    pub fn new() -> Self {
        #[cfg(feature = "test-keystore")]
        if let Some(path) = test_keystore::current_file() {
            return Self {
                backend: KeystoreBackendKind::TestFile(test_keystore::TestFileBackend::new(path)),
            };
        }

        Self {
            backend: KeystoreBackendKind::OsKeyring(OsKeyringBackend),
        }
    }

    pub fn init(
        &self,
        spend_key: &PrivateKey,
        view_key: &PrivateKey,
        passphrase: &str,
    ) -> Result<(), OfflineSignerError> {
        let encrypted_spend = encrypt_key(spend_key, passphrase)?;
        let encrypted_view = encrypt_key(view_key, passphrase)?;

        self.backend.store(SPEND_KEY_ENTRY, &encrypted_spend)?;
        self.backend.store(VIEW_KEY_ENTRY, &encrypted_view)?;

        Ok(())
    }

    pub fn get_keys(&self, passphrase: &str) -> Result<(PrivateKey, PrivateKey), OfflineSignerError> {
        let encrypted_spend = self.backend.retrieve(SPEND_KEY_ENTRY)?;
        let encrypted_view = self.backend.retrieve(VIEW_KEY_ENTRY)?;

        let spend_key = decrypt_key(&encrypted_spend, passphrase)?;
        let view_key = decrypt_key(&encrypted_view, passphrase)?;

        Ok((spend_key, view_key))
    }

    pub fn is_initialized(&self) -> bool {
        self.backend.retrieve(SPEND_KEY_ENTRY).is_ok()
    }

    pub fn clear(&self) -> Result<(), OfflineSignerError> {
        // Try to delete both keys, ignoring errors if they do not exist.
        let _unused = self.backend.delete(SPEND_KEY_ENTRY);
        let _unused = self.backend.delete(VIEW_KEY_ENTRY);
        Ok(())
    }

    pub fn description(&self) -> &'static str {
        self.backend.description()
    }
}

impl Default for Keystore {
    fn default() -> Self {
        Self::new()
    }
}

struct OsKeyringBackend;

impl KeystoreBackend for OsKeyringBackend {
    fn store(&self, entry_name: &str, data: &EncryptedKeyData) -> Result<(), OfflineSignerError> {
        let entry = Entry::new(SERVICE_NAME, entry_name)
            .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

        let json = serde_json::to_string(data)
            .map_err(|e| OfflineSignerError::SerializationError(format!("Failed to serialize key data: {}", e)))?;

        entry
            .set_password(&json)
            .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to store in keyring: {}", e)))?;

        Ok(())
    }

    fn retrieve(&self, entry_name: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
        let entry = Entry::new(SERVICE_NAME, entry_name)
            .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

        let json = entry
            .get_password()
            .map_err(|e| OfflineSignerError::NotInitialized(format!("Key not found in keyring: {}", e)))?;

        serde_json::from_str(&json)
            .map_err(|e| OfflineSignerError::ParseError(format!("Failed to parse key data: {}", e)))
    }

    fn delete(&self, entry_name: &str) -> Result<(), OfflineSignerError> {
        let entry = Entry::new(SERVICE_NAME, entry_name)
            .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to create keyring entry: {}", e)))?;

        entry
            .delete_credential()
            .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to delete from keyring: {}", e)))?;

        Ok(())
    }

    fn description(&self) -> &'static str {
        "the OS keystore"
    }
}

#[cfg(feature = "test-keystore")]
mod test_keystore {
    use std::{cell::RefCell, collections::BTreeMap, path::PathBuf};

    use super::{EncryptedKeyData, KeystoreBackend, OfflineSignerError};

    thread_local! {
        static TEST_KEYSTORE_FILE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    pub(crate) struct TestKeystoreFileGuard {
        previous_path: Option<PathBuf>,
    }

    pub fn set_test_keystore_file(path: Option<PathBuf>) -> TestKeystoreFileGuard {
        let previous_path = TEST_KEYSTORE_FILE.replace(path);
        TestKeystoreFileGuard { previous_path }
    }

    pub(super) fn current_file() -> Option<PathBuf> {
        TEST_KEYSTORE_FILE.with(|path| path.borrow().clone())
    }

    impl Drop for TestKeystoreFileGuard {
        fn drop(&mut self) {
            TEST_KEYSTORE_FILE.replace(self.previous_path.take());
        }
    }

    pub(super) struct TestFileBackend {
        path: PathBuf,
    }

    impl TestFileBackend {
        pub(super) fn new(path: PathBuf) -> Self {
            Self { path }
        }

        fn read(&self) -> Result<BTreeMap<String, EncryptedKeyData>, OfflineSignerError> {
            match std::fs::read_to_string(&self.path) {
                Ok(json) => serde_json::from_str(&json)
                    .map_err(|e| OfflineSignerError::ParseError(format!("Failed to parse test keystore: {}", e))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
                Err(e) => Err(OfflineSignerError::KeystoreError(format!(
                    "Failed to read test keystore: {}",
                    e
                ))),
            }
        }

        fn write(&self, entries: &BTreeMap<String, EncryptedKeyData>) -> Result<(), OfflineSignerError> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    OfflineSignerError::KeystoreError(format!("Failed to create test keystore directory: {}", e))
                })?;
            }
            let json = serde_json::to_string(entries).map_err(|e| {
                OfflineSignerError::SerializationError(format!("Failed to serialize test keystore: {}", e))
            })?;
            std::fs::write(&self.path, json)
                .map_err(|e| OfflineSignerError::KeystoreError(format!("Failed to write test keystore: {}", e)))?;
            Ok(())
        }
    }

    impl KeystoreBackend for TestFileBackend {
        fn store(&self, entry_name: &str, data: &EncryptedKeyData) -> Result<(), OfflineSignerError> {
            let mut entries = self.read()?;
            entries.insert(entry_name.to_string(), data.clone());
            self.write(&entries)
        }

        fn retrieve(&self, entry_name: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
            let entries = self.read()?;
            entries
                .get(entry_name)
                .cloned()
                .ok_or_else(|| OfflineSignerError::NotInitialized("Key not found in test keystore".to_string()))
        }

        fn delete(&self, entry_name: &str) -> Result<(), OfflineSignerError> {
            let mut entries = self.read()?;
            entries.remove(entry_name);
            self.write(&entries)
        }

        fn description(&self) -> &'static str {
            "the test keystore"
        }
    }
}

/// Derives an encryption key from a passphrase and salt using Argon2id.
fn derive_encryption_key(passphrase: &str, salt: &Salt) -> Result<Zeroizing<[u8; 32]>, OfflineSignerError> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password_with_salt(passphrase.as_bytes(), salt.as_ref())
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

/// Encrypts a private key using ChaCha20-Poly1305.
fn encrypt_key(key: &PrivateKey, passphrase: &str) -> Result<EncryptedKeyData, OfflineSignerError> {
    let salt = Salt::generate();
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encryption_key = derive_encryption_key(passphrase, &salt)?;

    let cipher = ChaCha20Poly1305::new_from_slice(&*encryption_key)
        .map_err(|e| OfflineSignerError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

    let plaintext = key.to_vec();
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| OfflineSignerError::EncryptionError(format!("Failed to encrypt: {}", e)))?;

    Ok(EncryptedKeyData {
        salt: salt.to_string(),
        nonce: nonce_bytes.to_hex(),
        ciphertext: ciphertext.to_hex(),
    })
}

/// Decrypts a private key using ChaCha20-Poly1305.
fn decrypt_key(data: &EncryptedKeyData, passphrase: &str) -> Result<PrivateKey, OfflineSignerError> {
    let salt =
        Salt::from_b64(&data.salt).map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid salt: {}", e)))?;

    let nonce_bytes =
        Vec::from_hex(&data.nonce).map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid nonce: {}", e)))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = Vec::from_hex(&data.ciphertext)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid ciphertext: {}", e)))?;

    let encryption_key = derive_encryption_key(passphrase, &salt)?;

    let cipher = ChaCha20Poly1305::new_from_slice(&*encryption_key)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Failed to create cipher: {}", e)))?;

    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|_| {
        OfflineSignerError::DecryptionError("Failed to decrypt: invalid passphrase or corrupted data".to_string())
    })?;

    PrivateKey::from_vec(&plaintext)
        .map_err(|e| OfflineSignerError::DecryptionError(format!("Invalid key data: {}", e)))
}

/// Initializes the keystore with spend and view keys.
pub fn init_keystore(
    spend_key: &PrivateKey,
    view_key: &PrivateKey,
    passphrase: &str,
) -> Result<(), OfflineSignerError> {
    Keystore::new().init(spend_key, view_key, passphrase)
}

/// Retrieves the spend and view keys from the keystore.
pub fn get_keys(passphrase: &str) -> Result<(PrivateKey, PrivateKey), OfflineSignerError> {
    Keystore::new().get_keys(passphrase)
}

/// Checks if the keystore has been initialized.
pub fn is_initialized() -> bool {
    Keystore::new().is_initialized()
}

/// Clears all keys from the keystore.
pub fn clear_keystore() -> Result<(), OfflineSignerError> {
    Keystore::new().clear()
}

/// Returns a human-readable description of the active keystore backend.
pub fn storage_description() -> &'static str {
    Keystore::new().description()
}

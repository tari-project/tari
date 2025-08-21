// Copyright 2023 The Tari Project
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

use std::{
    collections::HashMap,
    mem::size_of,
    sync::{Arc, RwLock},
};

use chacha20poly1305::Key;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    seeds::cipher_seed::CipherSeed,
    types::{CompressedPublicKey, PrivateKey},
    wallet_types::WalletType,
};
use zeroize::Zeroizing;

use crate::{
    crypto_factories::CryptoFactories,
    key_manager::{
        error::{KeyManagerServiceError, KeyManagerStorageError},
        KeyManagerState,
        TransactionKeyManagerBackend,
        TransactionKeyManagerWrapper,
    },
};
pub type MemoryKeyManager = TransactionKeyManagerWrapper<MemoryKeyManagerBackend>;

pub fn create_memory_key_manager_with_range_proof_size(
    size: usize,
) -> Result<MemoryKeyManager, KeyManagerServiceError> {
    let cipher = CipherSeed::new();

    create_memory_key_manager_from_seed(cipher, size)
}

pub fn create_memory_key_manager_from_seed(
    seed: CipherSeed,
    rangeproof_size: usize,
) -> Result<MemoryKeyManager, KeyManagerServiceError> {
    let cipher = seed;

    let mut key = Zeroizing::new([0u8; size_of::<Key>()]);
    OsRng.fill_bytes(key.as_mut());
    let factory = CryptoFactories::new(rangeproof_size);

    let backend = MemoryKeyManagerBackend::new();
    TransactionKeyManagerWrapper::new(cipher, backend, factory, Arc::new(WalletType::default()))
}

pub fn create_memory_key_manager() -> Result<MemoryKeyManager, KeyManagerServiceError> {
    create_memory_key_manager_with_range_proof_size(64)
}

#[derive(Clone, Debug, Default)]
pub struct MemoryKeyManagerBackend {
    key_manger_states: Arc<RwLock<HashMap<String, KeyManagerState>>>,
    private_keys: Arc<RwLock<HashMap<CompressedPublicKey, PrivateKey>>>,
}

impl MemoryKeyManagerBackend {
    pub fn new() -> Self {
        Self {
            key_manger_states: Arc::new(RwLock::new(HashMap::new())),
            private_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl TransactionKeyManagerBackend for MemoryKeyManagerBackend {
    fn get_key_manager(&self, branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        let key_manager_states = self
            .key_manger_states
            .read()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        Ok(key_manager_states.get(branch).cloned())
    }

    fn add_key_manager(&self, key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        let mut key_manager_states = self
            .key_manger_states
            .write()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        key_manager_states.insert(key_manager.branch_seed.clone(), key_manager);
        Ok(())
    }

    fn increment_key_index(&self, branch: &str) -> Result<(), KeyManagerStorageError> {
        let mut key_manager_states = self
            .key_manger_states
            .write()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        if let Some(key_manager) = key_manager_states.get_mut(branch) {
            key_manager.primary_key_index += 1;
            Ok(())
        } else {
            Err(KeyManagerStorageError::ValueNotFound)
        }
    }

    fn set_key_index(&self, branch: &str, index: u64) -> Result<(), KeyManagerStorageError> {
        let mut key_manager_states = self
            .key_manger_states
            .write()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        if let Some(key_manager) = key_manager_states.get_mut(branch) {
            key_manager.primary_key_index = index;
            Ok(())
        } else {
            Err(KeyManagerStorageError::ValueNotFound)
        }
    }

    fn insert_imported_key(
        &self,
        public_key: CompressedPublicKey,
        private_key: PrivateKey,
    ) -> Result<(), KeyManagerStorageError> {
        let mut private_keys = self
            .private_keys
            .write()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        if private_keys.contains_key(&public_key) {
            return Err(KeyManagerStorageError::StorageError("Already exists".to_string()));
        }
        private_keys.insert(public_key, private_key);
        Ok(())
    }

    fn get_imported_key(&self, public_key: &CompressedPublicKey) -> Result<PrivateKey, KeyManagerStorageError> {
        let private_keys = self
            .private_keys
            .read()
            .map_err(|_| KeyManagerStorageError::StorageError("Poisoned lock".to_string()))?;
        if let Some(private_key) = private_keys.get(public_key) {
            Ok(private_key.clone())
        } else {
            Err(KeyManagerStorageError::ValueNotFound)
        }
    }
}

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

use std::{mem::size_of, sync::Arc};

use chacha20poly1305::Key;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    seeds::cipher_seed::CipherSeed,
    types::{CompressedPublicKey, PrivateKey},
};
use zeroize::Zeroizing;

use crate::{
    crypto_factories::CryptoFactories,
    legacy_key_manager::{
        error::{KeyManagerServiceError, KeyManagerStorageError},
        wallet_types::WalletType,
        KeyManagerState,
        TransactionKeyManagerBackend,
        TransactionKeyManagerWrapper,
    },
};
pub type MemoryKeyManager = TransactionKeyManagerWrapper<MemoryKeyManagerBackend>;

pub async fn create_new_random_key_manager_with_range_proof_size(
    size: usize,
) -> Result<MemoryKeyManager, KeyManagerServiceError> {
    let cipher = CipherSeed::random();

    create_new_random_key_manager_from_seed(cipher, size).await
}

pub async fn create_new_random_key_manager_from_seed(
    seed: CipherSeed,
    rangeproof_size: usize,
) -> Result<MemoryKeyManager, KeyManagerServiceError> {
    let cipher = seed;

    let mut key = Zeroizing::new([0u8; size_of::<Key>()]);
    OsRng.fill_bytes(key.as_mut());
    let factory = CryptoFactories::new(rangeproof_size);

    TransactionKeyManagerWrapper::new(Some(cipher), factory, Arc::new(WalletType::default())).await
}

pub async fn create_new_random_key_manager() -> Result<MemoryKeyManager, KeyManagerServiceError> {
    create_new_random_key_manager_with_range_proof_size(64).await
}

#[derive(Clone, Debug, Default)]
pub struct MemoryKeyManagerBackend {}

impl MemoryKeyManagerBackend {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl TransactionKeyManagerBackend for MemoryKeyManagerBackend {
    async fn get_key_manager(&self, _branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }

    async fn add_key_manager(&self, _key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }

    async fn increment_key_index(&self, _branch: &str) -> Result<(), KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }

    async fn set_key_index(&self, _branch: &str, _index: u64) -> Result<(), KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }

    async fn insert_imported_key(
        &self,
        _public_key: CompressedPublicKey,
        _private_key: PrivateKey,
    ) -> Result<(), KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }

    async fn get_imported_key(&self, _public_key: &CompressedPublicKey) -> Result<PrivateKey, KeyManagerStorageError> {
        Err(KeyManagerStorageError::UnexpectedResult("Not implemented".to_string()))
    }
}

//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use tari_crypto::keys::{PublicKey, SecretKey};
use tokio::sync::RwLock;

use crate::{
    cipher_seed::CipherSeed,
    key_manager_service::{
        error::KeyManagerServiceError,
        interface::NextKeyResult,
        storage::database::{KeyManagerBackend, KeyManagerDatabase},
        AddResult,
        KeyId,
        KeyManagerInner,
        KeyManagerInterface,
    },
};

/// The key manager provides a hierarchical key derivation function (KDF) that derives uniformly random secret keys from
/// a single seed key for arbitrary branches, using an implementation of `KeyManagerBackend` to store the current index
/// for each branch.
///
/// This handle can be cloned cheaply and safely shared across multiple threads.
#[derive(Clone)]
pub struct KeyManagerHandle<TBackend, PK: PublicKey> {
    key_manager_inner: Arc<RwLock<KeyManagerInner<TBackend, PK>>>,
}

impl<TBackend, PK> KeyManagerHandle<TBackend, PK>
where
    TBackend: KeyManagerBackend<PK> + 'static,
    PK: PublicKey,
{
    /// Creates a new key manager.
    /// * `master_seed` is the primary seed that will be used to derive all unique branch keys with their indexes
    /// * `db` implements `KeyManagerBackend` and is used for persistent storage of branches and indices.
    pub fn new(master_seed: CipherSeed, db: KeyManagerDatabase<TBackend, PK>) -> Self {
        KeyManagerHandle {
            key_manager_inner: Arc::new(RwLock::new(KeyManagerInner::new(master_seed, db))),
        }
    }
}

#[async_trait::async_trait]
impl<TBackend, PK> KeyManagerInterface<PK> for KeyManagerHandle<TBackend, PK>
where
    TBackend: KeyManagerBackend<PK> + 'static,
    PK: PublicKey + Send + Sync + 'static,
    PK::K: SecretKey + Send + Sync + 'static,
{
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .write()
            .await
            .add_key_manager_branch(&branch.into())
    }

    async fn get_next_key<T: Into<String> + Send>(
        &self,
        branch: T,
    ) -> Result<NextKeyResult<PK>, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .get_next_key(&branch.into())
            .await
    }

    async fn get_next_key_id<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .get_next_key_id(&branch.into())
            .await
    }

    async fn get_static_key_id<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .get_static_key_id(&branch.into())
            .await
    }

    async fn get_key_at_index<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<PK::K, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .get_key_at_index(&branch.into(), index)
            .await
    }

    async fn get_public_key_at_key_id(&self, key_id: &KeyId) -> Result<PK, KeyManagerServiceError> {
        unimplemented!("KeyManagerHandle::get_public_key_at_key_id({})", key_id)
    }

    async fn find_key_index<T: Into<String> + Send>(&self, branch: T, key: &PK) -> Result<u64, KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .find_key_index(&branch.into(), key)
            .await
    }

    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        (*self.key_manager_inner)
            .read()
            .await
            .update_current_key_index_if_higher(&branch.into(), index)
            .await
    }

    async fn import_key(&self, private_key: PK::K) -> Result<(), KeyManagerServiceError> {
        (*self.key_manager_inner).read().await.import_key(private_key).await
    }
}

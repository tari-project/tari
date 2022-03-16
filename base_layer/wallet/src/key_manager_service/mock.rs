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

use aes_gcm::Aes256Gcm;
use log::*;
use tari_common_types::types::PrivateKey;
use tari_key_manager::{cipher_seed::CipherSeed, key_manager::KeyManager};
use tokio::sync::RwLock;

use crate::{
    key_manager_service::{interface::NextKeyResult, AddResult, KeyManagerInterface},
    types::KeyDigest,
};

const LOG_TARGET: &str = "wallet::Key_manager_mock";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;
use std::{collections::HashMap, sync::Arc};

use crate::key_manager_service::{error::KeyManagerError, storage::database::KeyManagerState};

#[derive(Clone)]
pub struct KeyManagerMock {
    key_managers: Arc<RwLock<HashMap<String, KeyManager<PrivateKey, KeyDigest>>>>,
    master_seed: CipherSeed,
}

impl KeyManagerMock {
    pub fn new(master_seed: CipherSeed) -> Self {
        KeyManagerMock {
            key_managers: Arc::new(RwLock::new(HashMap::new())),
            master_seed,
        }
    }
}

impl KeyManagerMock {
    pub async fn add_key_manager_mock(&self, branch: String) -> Result<AddResult, KeyManagerError> {
        let result = if self.key_managers.read().await.contains_key(&branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = KeyManagerState {
            branch_seed: branch.to_string(),
            primary_key_index: 0,
        };

        self.key_managers.write().await.insert(
            branch,
            KeyManager::<PrivateKey, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            ),
        );
        Ok(result)
    }

    pub async fn get_next_key_mock(&self, branch: String) -> Result<NextKeyResult, KeyManagerError> {
        let mut lock = self.key_managers.write().await;
        let km = lock.get_mut(&branch).ok_or(KeyManagerError::UnknownKeyBranch)?;
        let key = km.next_key()?;
        Ok(NextKeyResult {
            key: key.k,
            index: km.key_index(),
        })
    }

    pub async fn get_key_at_index_mock(&self, branch: String, index: u64) -> Result<PrivateKey, KeyManagerError> {
        let lock = self.key_managers.read().await;
        let km = lock.get(&branch).ok_or(KeyManagerError::UnknownKeyBranch)?;
        let key = km.derive_key(index)?;
        Ok(key.k)
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index_mock(&self, branch: String, key: &PrivateKey) -> Result<u64, KeyManagerError> {
        let lock = self.key_managers.read().await;
        let km = lock.get(&branch).ok_or(KeyManagerError::UnknownKeyBranch)?;

        let current_index = km.key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            if km.derive_key(i)?.k == *key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(i);
            }
        }

        Err(KeyManagerError::KeyNotFoundInKeyChain)
    }

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    pub async fn update_current_key_index_if_higher_mock(
        &self,
        branch: String,
        index: u64,
    ) -> Result<(), KeyManagerError> {
        let lock = self.key_managers.write().await;
        let km = lock.get(&branch).ok_or(KeyManagerError::UnknownKeyBranch)?;
        let current_index = km.key_index();
        if index > current_index {
            // km.update_key_index(index);
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl KeyManagerInterface for KeyManagerMock {
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerError> {
        self.add_key_manager_mock(branch.into()).await
    }

    async fn get_next_key<T: Into<String> + Send>(&self, branch: T) -> Result<NextKeyResult, KeyManagerError> {
        self.get_next_key_mock(branch.into()).await
    }

    async fn get_key_at_index<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<PrivateKey, KeyManagerError> {
        self.get_key_at_index_mock(branch.into(), index).await
    }

    async fn apply_encryption(&self, _cipher: Aes256Gcm) -> Result<(), KeyManagerError> {
        unimplemented!("Not supported");
    }

    async fn remove_encryption(&self) -> Result<(), KeyManagerError> {
        unimplemented!("Not supported");
    }

    async fn find_key_index<T: Into<String> + Send>(
        &self,
        branch: T,
        key: &PrivateKey,
    ) -> Result<u64, KeyManagerError> {
        self.find_key_index_mock(branch.into(), key).await
    }

    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerError> {
        self.update_current_key_index_if_higher_mock(branch.into(), index).await
    }
}

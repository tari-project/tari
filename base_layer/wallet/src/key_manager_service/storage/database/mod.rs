// Copyright 2022. The Tari Project
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

mod backend;
use std::sync::Arc;

use aes_gcm::Aes256Gcm;
pub use backend::KeyManagerBackend;

use crate::key_manager_service::error::KeyManagerStorageError;

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct KeyManagerState {
    pub branch_seed: String,
    pub primary_key_index: u64,
}

/// This structure holds an inner type that implements the `KeyManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct KeyManagerDatabase<T> {
    db: Arc<T>,
}

impl<T> KeyManagerDatabase<T>
where T: KeyManagerBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub async fn get_key_manager_state(
        &self,
        branch: String,
    ) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.get_key_manager(branch))
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn set_key_manager_state(&self, state: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.add_key_manager(state))
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn increment_key_index(&self, branch: String) -> Result<(), KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.increment_key_index(branch))
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn set_key_index(&self, branch: String, index: u64) -> Result<(), KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.set_key_index(branch, index))
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(cipher))
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn remove_encryption(&self) -> Result<(), KeyManagerStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.remove_encryption())
            .await
            .map_err(|err| KeyManagerStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }
}

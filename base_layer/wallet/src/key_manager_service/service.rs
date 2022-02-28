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
use futures::lock::Mutex;
use log::*;
use tari_common_types::types::PrivateKey;
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    mnemonic::{Mnemonic, MnemonicLanguage},
};

use crate::types::KeyDigest;

const LOG_TARGET: &str = "wallet::Key_manager";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

use std::collections::HashMap;

use crate::key_manager_service::{
    error::KeyManagerError,
    storage::database::{KeyManagerBackend, KeyManagerDatabase, KeyManagerState},
};

pub struct KeyManagerInner<TBackend> {
    key_managers: HashMap<String, Mutex<KeyManager<PrivateKey, KeyDigest>>>,
    db: KeyManagerDatabase<TBackend>,
    master_seed: CipherSeed,
}

impl<TBackend> KeyManagerInner<TBackend>
where TBackend: KeyManagerBackend + 'static
{
    pub fn new(master_seed: CipherSeed, db: KeyManagerDatabase<TBackend>) -> Self {
        KeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
        }
    }

    pub async fn add_key_manager(&mut self, branch: String) -> Result<(), KeyManagerError> {
        dbg!("pleh");
        if self.key_managers.contains_key(&branch) {
            return Err(KeyManagerError::BranchAllreadyExists);
        }
        dbg!("pleh1");
        let state = match self.db.get_key_manager_state(branch.clone()).await? {
            None => {
                let starting_state = KeyManagerState {
                    branch_seed: branch.to_string(),
                    primary_key_index: 0,
                };
                dbg!("pleh2");
                self.db.set_key_manager_state(starting_state.clone()).await?;
                starting_state
            },
            Some(km) => km,
        };
        dbg!("pleh3");
        self.key_managers.insert(
            branch,
            Mutex::new(KeyManager::<PrivateKey, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(())
    }

    pub async fn get_next_key(&self, branch: String) -> Result<PrivateKey, KeyManagerError> {
        let mut km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerError::UnknownKeyBranch)?
            .lock()
            .await;
        let key = km.next_key()?;
        self.db.increment_key_index(branch).await?;
        Ok(key.k)
    }

    pub async fn get_key_at_index(&self, branch: String, index: u64) -> Result<PrivateKey, KeyManagerError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerError::UnknownKeyBranch)?
            .lock()
            .await;
        let key = km.derive_key(index)?;
        self.db.set_key_index(branch, index).await?;
        Ok(key.k)
    }

    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), KeyManagerError> {
        self.db.apply_encryption(cipher).await?;
        Ok(())
    }

    pub async fn remove_encryption(&self) -> Result<(), KeyManagerError> {
        self.db.remove_encryption().await?;
        Ok(())
    }

    /// Return the Seed words for the current Master Key set in the Key Manager
    pub async fn get_seed_words(
        &self,
        branch: String,
        language: &MnemonicLanguage,
    ) -> Result<Vec<String>, KeyManagerError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerError::UnknownKeyBranch)?
            .lock()
            .await;
        let seed_words = (*km).cipher_seed().to_mnemonic(language, None)?;
        Ok(seed_words)
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, branch: String, key: PrivateKey) -> Result<u64, KeyManagerError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerError::UnknownKeyBranch)?
            .lock()
            .await;

        let current_index = km.key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            if km.derive_key(i)?.k == key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(i);
            }
        }

        Err(KeyManagerError::KeyNotFoundInKeyChain)
    }

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    pub async fn update_current_key_index_if_higher(&self, branch: String, index: u64) -> Result<(), KeyManagerError> {
        let mut km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerError::UnknownKeyBranch)?
            .lock()
            .await;
        let current_index = km.key_index();
        if index > current_index {
            km.update_key_index(index);
            self.db.set_key_index(branch, index).await?;
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }
}

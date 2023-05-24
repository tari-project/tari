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
use std::collections::HashMap;

use futures::lock::Mutex;
use log::*;
use tari_crypto::keys::PublicKey;
use tari_utilities::hex::Hex;

use crate::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    key_manager_service::{
        error::KeyManagerServiceError,
        interface::NextKeyResult,
        storage::database::{KeyManagerBackend, KeyManagerDatabase, KeyManagerState},
        AddResult,
        KeyDigest,
    },
};

const LOG_TARGET: &str = "key_manager::key_manager_service";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

pub struct KeyManagerInner<TBackend, PK: PublicKey> {
    key_managers: HashMap<String, Mutex<KeyManager<PK, KeyDigest>>>,
    db: KeyManagerDatabase<TBackend, PK>,
    master_seed: CipherSeed,
}

impl<TBackend, PK> KeyManagerInner<TBackend, PK>
where
    TBackend: KeyManagerBackend<PK> + 'static,
    PK: PublicKey,
{
    pub fn new(master_seed: CipherSeed, db: KeyManagerDatabase<TBackend, PK>) -> Self {
        KeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
        }
    }

    pub fn add_key_manager_branch(&mut self, branch: String) -> Result<AddResult, KeyManagerServiceError> {
        let result = if self.key_managers.contains_key(&branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = match self.db.get_key_manager_state(branch.clone())? {
            None => {
                let starting_state = KeyManagerState {
                    branch_seed: branch.to_string(),
                    primary_key_index: 0,
                };
                self.db.set_key_manager_state(starting_state.clone())?;
                starting_state
            },
            Some(km) => km,
        };
        self.key_managers.insert(
            branch,
            Mutex::new(KeyManager::<PK, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: String) -> Result<NextKeyResult<PK>, KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let derived_key = km.next_key()?;
        self.db.increment_key_index(branch)?;
        Ok(NextKeyResult {
            key: derived_key.key,
            index: km.key_index(),
        })
    }

    pub async fn get_key_at_index(&self, branch: String, index: u64) -> Result<PK::K, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let derived_key = km.derive_key(index)?;
        Ok(derived_key.key)
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, branch: String, key: &PK) -> Result<u64, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;

        let current_index = km.key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            let public_key = PK::from_secret_key(&km.derive_key(i)?.key);
            if public_key == *key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(i);
            }
        }

        Err(KeyManagerServiceError::KeyNotFoundInKeyChain)
    }

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    pub async fn update_current_key_index_if_higher(
        &self,
        branch: String,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let current_index = km.key_index();
        if index > current_index {
            km.update_key_index(index);
            self.db.set_key_index(branch, index)?;
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }

    pub async fn import_key(&self, private_key: PK::K) -> Result<(), KeyManagerServiceError> {
        let public_key = PK::from_secret_key(&private_key);
        let hex_key = public_key.to_hex();
        self.db.insert_imported_key(public_key, private_key)?;
        trace!(target: LOG_TARGET, "Imported key {}", hex_key);
        Ok(())
    }

    // async fn get_private_key(&self, key_id: &KeyId<PK>) -> Result<PK::K, KeyManagerServiceError> {
    //     match key_id {
    //         KeyId::Default { branch, index } => {
    //             let km = self
    //                 .key_managers
    //                 .get(branch)
    //                 .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
    //                 .lock()
    //                 .await;
    //             let key = km.get_private_key(*index)?;
    //             Ok(key)
    //         },
    //         KeyId::Imported { key } => {
    //             let pvt_key = self.db.get_imported_key(key)?;
    //             Ok(pvt_key)
    //         },
    //     }
    // }
}

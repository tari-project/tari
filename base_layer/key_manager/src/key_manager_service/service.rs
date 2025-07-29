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
use std::{collections::HashMap, str::FromStr};

use argon2::password_hash::rand_core::OsRng;
use blake2::Blake2b;
use digest::consts::U64;
use futures::lock::Mutex;
use tari_crypto::{
    hash_domain,
    hashing::DomainSeparatedHasher,
    keys::{PublicKey, SecretKey},
};
use tari_utilities::ByteArray;

use crate::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    key_manager_service::{
        error::KeyManagerServiceError,
        interface::KeyAndId,
        storage::database::{KeyManagerBackend, KeyManagerDatabase, KeyManagerState},
        AddResult,
        KeyDigest,
        KeyId,
    },
};

hash_domain!(KeyManagerHashingDomain, "com.tari.base_layer.key_manager", 1);

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

    pub fn add_key_manager_branch(&mut self, branch: &str) -> Result<AddResult, KeyManagerServiceError> {
        let result = if self.key_managers.contains_key(branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = match self.db.get_key_manager_state(branch)? {
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
            branch.to_string(),
            Mutex::new(KeyManager::<PK, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: &str) -> Result<KeyAndId<PK>, KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch(branch.to_string()))?
            .lock()
            .await;
        self.db.increment_key_index(branch)?;
        let index = km.increment_key_index(1);
        let key = km.derive_public_key(index)?.key;

        Ok(KeyAndId {
            key_id: KeyId::Managed {
                branch: branch.to_string(),
                index,
            },
            pub_key: key,
        })
    }

    pub async fn get_random_key(&self) -> Result<KeyAndId<PK>, KeyManagerServiceError> {
        let random_private_key = PK::K::random(&mut OsRng);
        let key_id = self.import_key(random_private_key).await?;
        let public_key = self.get_public_key_at_key_id(&key_id).await?;
        Ok(KeyAndId {
            key_id,
            pub_key: public_key,
        })
    }

    pub async fn get_static_key(&self, branch: &str) -> Result<KeyId<PK>, KeyManagerServiceError> {
        match self.key_managers.get(branch) {
            None => Err(KeyManagerServiceError::UnknownKeyBranch(branch.to_string())),
            Some(_) => Ok(KeyId::Managed {
                branch: branch.to_string(),
                index: 0,
            }),
        }
    }

    pub async fn get_public_key_at_key_id(&self, key_id: &KeyId<PK>) -> Result<PK, KeyManagerServiceError> {
        match key_id {
            KeyId::Managed { branch, index } => {
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch(branch.to_string()))?
                    .lock()
                    .await;
                Ok(km.derive_public_key(*index)?.key)
            },
            KeyId::Derived { key } => {
                let key = KeyId::<PK>::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let branch = key
                    .managed_branch()
                    .ok_or_else(|| KeyManagerServiceError::KeyIdWithoutBranch)?;
                let index = key.managed_index().ok_or(KeyManagerServiceError::KeyIdWithoutIndex)?;
                let km = self
                    .key_managers
                    .get(&branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch(branch.to_string()))?
                    .lock()
                    .await;
                let branch_key = km.get_private_key(index)?;

                let public_key = {
                    let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerHashingDomain>::new_with_label(
                        "Key manager derived key",
                    );
                    let hasher = hasher.chain(branch_key.as_bytes()).finalize();
                    let private_key = PK::K::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                        KeyManagerServiceError::UnknownError(
                            "Invalid private key for Key manager derived key".to_string(),
                        )
                    })?;
                    PK::from_secret_key(&private_key)
                };
                Ok(public_key)
            },
            KeyId::Imported { key } => Ok(key.clone()),
            KeyId::Zero => Ok(PK::default()),
        }
    }

    pub async fn import_key(&self, private_key: PK::K) -> Result<KeyId<PK>, KeyManagerServiceError> {
        let public_key = PK::from_secret_key(&private_key);
        self.db.insert_imported_key(public_key.clone(), private_key)?;
        let key_id = KeyId::Imported { key: public_key };
        Ok(key_id)
    }
}

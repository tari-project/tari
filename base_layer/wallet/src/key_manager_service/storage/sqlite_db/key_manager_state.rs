//  Copyright 2022. The Tari Project
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

use std::convert::TryFrom;

use aes_gcm::Aes256Gcm;
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, SqliteConnection};

use crate::{
    key_manager_service::{error::KeyManagerStorageError, storage::database::KeyManagerState},
    schema::key_manager_states,
    util::{
        diesel_ext::ExpectedRowsExtension,
        encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    },
};

#[derive(Clone, Debug, Queryable, Identifiable)]
#[table_name = "key_manager_states"]
#[primary_key(id)]
pub struct KeyManagerStateSql {
    pub id: i32,
    pub branch_seed: String,
    pub primary_key_index: Vec<u8>,
    pub timestamp: NaiveDateTime,
}

#[derive(Clone, Debug, Insertable)]
#[table_name = "key_manager_states"]
pub struct NewKeyManagerStateSql {
    branch_seed: String,
    primary_key_index: Vec<u8>,
    timestamp: NaiveDateTime,
}

impl From<KeyManagerState> for NewKeyManagerStateSql {
    fn from(km: KeyManagerState) -> Self {
        Self {
            branch_seed: km.branch_seed,
            primary_key_index: km.primary_key_index.to_le_bytes().to_vec(),
            timestamp: Utc::now().naive_utc(),
        }
    }
}
impl TryFrom<KeyManagerStateSql> for KeyManagerState {
    type Error = KeyManagerStorageError;

    fn try_from(km: KeyManagerStateSql) -> Result<Self, Self::Error> {
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&km.primary_key_index[..8]);
        Ok(Self {
            branch_seed: km.branch_seed,
            primary_key_index: u64::from_le_bytes(bytes),
        })
    }
}

impl NewKeyManagerStateSql {
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), KeyManagerStorageError> {
        diesel::insert_into(key_manager_states::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }
}

impl KeyManagerStateSql {
    pub fn index(conn: &SqliteConnection) -> Result<Vec<KeyManagerStateSql>, KeyManagerStorageError> {
        Ok(key_manager_states::table.load::<KeyManagerStateSql>(conn)?)
    }

    pub fn get_state(branch: &str, conn: &SqliteConnection) -> Result<KeyManagerStateSql, KeyManagerStorageError> {
        key_manager_states::table
            .filter(key_manager_states::branch_seed.eq(branch.to_string()))
            .first::<KeyManagerStateSql>(conn)
            .map_err(|_| KeyManagerStorageError::KeyManagerNotInitialized)
    }

    pub fn set_state(&self, conn: &SqliteConnection) -> Result<(), KeyManagerStorageError> {
        match KeyManagerStateSql::get_state(&self.branch_seed, conn) {
            Ok(km) => {
                let update = KeyManagerStateUpdateSql {
                    branch_seed: Some(self.branch_seed.clone()),
                    primary_key_index: Some(self.primary_key_index.clone()),
                };

                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
            },
            Err(_) => {
                let inserter = NewKeyManagerStateSql {
                    branch_seed: self.branch_seed.clone(),
                    primary_key_index: self.primary_key_index.clone(),
                    timestamp: self.timestamp,
                };
                inserter.commit(conn)?;
            },
        }
        Ok(())
    }

    pub fn increment_index(
        branch: String,
        cipher: Option<&Aes256Gcm>,
        conn: &SqliteConnection,
    ) -> Result<u64, KeyManagerStorageError> {
        Ok(match KeyManagerStateSql::get_state(&branch, conn) {
            Ok(mut km) => {
                if km.primary_key_index.len() > 8 {
                    match cipher {
                        Some(cip) => km.decrypt(cip).map_err(KeyManagerStorageError::AeadError)?,
                        _ => return Err(KeyManagerStorageError::ValueEncrypted),
                    }
                }
                let mut bytes: [u8; 8] = [0u8; 8];
                bytes.copy_from_slice(&km.primary_key_index[..8]);
                let new_index = u64::from_le_bytes(bytes) + 1;
                km.primary_key_index = new_index.to_le_bytes().to_vec();
                if let Some(cip) = cipher {
                    km.encrypt(cip).map_err(KeyManagerStorageError::AeadError)?;
                }
                let update = KeyManagerStateUpdateSql {
                    branch_seed: None,
                    primary_key_index: Some(km.primary_key_index),
                };
                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
                new_index
            },
            Err(_) => return Err(KeyManagerStorageError::KeyManagerNotInitialized),
        })
    }

    pub fn set_index(
        branch: String,
        index: u64,
        cipher: Option<&Aes256Gcm>,
        conn: &SqliteConnection,
    ) -> Result<(), KeyManagerStorageError> {
        match KeyManagerStateSql::get_state(&branch, conn) {
            Ok(mut km) => {
                km.primary_key_index = index.to_le_bytes().to_vec();
                if let Some(cip) = cipher {
                    km.encrypt(cip).map_err(KeyManagerStorageError::AeadError)?;
                }
                let update = KeyManagerStateUpdateSql {
                    branch_seed: None,
                    primary_key_index: Some(km.primary_key_index),
                };
                diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&km.id)))
                    .set(update)
                    .execute(conn)
                    .num_rows_affected_or_not_found(1)?;
                Ok(())
            },
            Err(_) => Err(KeyManagerStorageError::KeyManagerNotInitialized),
        }
    }
}

#[derive(AsChangeset)]
#[table_name = "key_manager_states"]
pub struct KeyManagerStateUpdateSql {
    branch_seed: Option<String>,
    primary_key_index: Option<Vec<u8>>,
}

impl Encryptable<Aes256Gcm> for KeyManagerStateSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let encrypted_index = encrypt_bytes_integral_nonce(cipher, self.primary_key_index.clone())?;
        self.primary_key_index = encrypted_index;
        Ok(())
    }

    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let decrypted_index = decrypt_bytes_integral_nonce(cipher, self.primary_key_index.clone())?;
        self.primary_key_index = decrypted_index;

        Ok(())
    }
}

impl Encryptable<Aes256Gcm> for NewKeyManagerStateSql {
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), String> {
        let encrypted_index = encrypt_bytes_integral_nonce(cipher, self.primary_key_index.clone())?;
        self.primary_key_index = encrypted_index;
        Ok(())
    }

    fn decrypt(&mut self, _cipher: &Aes256Gcm) -> Result<(), String> {
        unimplemented!("Not supported")
        // let decrypted_master_key = decrypt_bytes_integral_nonce(&cipher, self.master_key.clone())?;
        // let decrypted_branch_seed =
        //     decrypt_bytes_integral_nonce(&cipher, from_hex(self.branch_seed.as_str()).map_err(|_| Error)?)?;
        // self.master_key = decrypted_master_key;
        // self.branch_seed = from_utf8(decrypted_branch_seed.as_slice())
        //     .map_err(|_| Error)?
        //     .to_string();
        // Ok(())
    }
}

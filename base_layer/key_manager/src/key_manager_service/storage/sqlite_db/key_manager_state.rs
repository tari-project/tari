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

use chacha20poly1305::XChaCha20Poly1305;
use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, SqliteConnection};
use tari_common_sqlite::util::diesel_ext::ExpectedRowsExtension;
use tari_common_types::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce};
use tari_utilities::{ByteArray, Hidden};

use crate::{
    key_manager_service::{
        error::KeyManagerStorageError,
        storage::{database::KeyManagerState, sqlite_db::Encryptable},
    },
    schema::key_manager_states,
};

/// Represents a row in the key_manager_states table.
#[derive(Clone, Debug, Queryable, Identifiable)]
#[diesel(table_name = key_manager_states)]
#[diesel(primary_key(id))]
pub struct KeyManagerStateSql {
    pub id: i32,
    pub branch_seed: String,
    pub primary_key_index: Vec<u8>,
    pub timestamp: NaiveDateTime,
}

/// Struct used to create a new Key manager in the database
#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = key_manager_states)]
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
    /// Commits a new key manager into the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), KeyManagerStorageError> {
        diesel::insert_into(key_manager_states::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }
}

impl KeyManagerStateSql {
    /// Retrieve every key manager branch currently in the database.
    /// Returns a `Vec` of [KeyManagerStateSql], if none are found, it will return an empty `Vec`.
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<KeyManagerStateSql>, KeyManagerStorageError> {
        Ok(key_manager_states::table.load::<KeyManagerStateSql>(conn)?)
    }

    /// Retrieve the key manager for the provided branch
    /// Will return Err if the branch does not exist in the database
    pub fn get_state(branch: &str, conn: &mut SqliteConnection) -> Result<KeyManagerStateSql, KeyManagerStorageError> {
        key_manager_states::table
            .filter(key_manager_states::branch_seed.eq(branch.to_string()))
            .first::<KeyManagerStateSql>(conn)
            .map_err(|_| KeyManagerStorageError::KeyManagerNotInitialized)
    }

    /// Creates or updates the database with the key manager state in this instance.
    pub fn set_state(&self, conn: &mut SqliteConnection) -> Result<(), KeyManagerStorageError> {
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

    /// Updates the key index of the of the provided key manager indicated by the id.
    pub fn set_index(id: i32, index: Vec<u8>, conn: &mut SqliteConnection) -> Result<(), KeyManagerStorageError> {
        let update = KeyManagerStateUpdateSql {
            branch_seed: None,
            primary_key_index: Some(index),
        };
        diesel::update(key_manager_states::table.filter(key_manager_states::id.eq(&id)))
            .set(update)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(())
    }
}

impl Encryptable<XChaCha20Poly1305> for KeyManagerStateSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        // Because there are two variable-length inputs in the concatenation, we prepend the length of the first
        [
            Self::KEY_MANAGER,
            (self.branch_seed.len() as u64).to_le_bytes().as_bytes(),
            self.branch_seed.as_bytes(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.primary_key_index = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("primary_key_index"),
            Hidden::hide(self.primary_key_index.clone()),
        )?;

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.primary_key_index =
            decrypt_bytes_integral_nonce(cipher, self.domain("primary_key_index"), &self.primary_key_index)?;

        Ok(self)
    }
}

impl Encryptable<XChaCha20Poly1305> for NewKeyManagerStateSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        // Because there are two variable-length inputs in the concatenation, we prepend the length of the first
        [
            Self::KEY_MANAGER,
            (self.branch_seed.len() as u64).to_le_bytes().as_bytes(),
            self.branch_seed.as_bytes(),
            field_name.as_bytes(),
        ]
        .concat()
        .to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.primary_key_index = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("primary_key_index"),
            Hidden::hide(self.primary_key_index.clone()),
        )?;

        Ok(self)
    }

    fn decrypt(self, _cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        unimplemented!("Not supported")
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = key_manager_states)]
pub struct KeyManagerStateUpdateSql {
    branch_seed: Option<String>,
    primary_key_index: Option<Vec<u8>>,
}

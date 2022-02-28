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

use std::{
    convert::TryFrom,
    sync::{Arc, RwLock},
};

use aes_gcm::Aes256Gcm;
use diesel::SqliteConnection;
pub use key_manager_state::{KeyManagerStateSql, NewKeyManagerStateSql};
use log::*;
use tokio::time::Instant;

use crate::{
    key_manager_service::{
        error::KeyManagerStorageError,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, KeyManagerBackend, KeyManagerState, WriteOperation},
            sqlite_db::key_manager_state_old::KeyManagerStateSqlOld,
        },
    },
    output_manager_service::resources::KeyManagerOmsBranch,
    storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
    util::encryption::Encryptable,
};

mod key_manager_state;
mod key_manager_state_old;

const LOG_TARGET: &str = "wallet::key_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct KeyManagerSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<Option<Aes256Gcm>>>,
}

impl KeyManagerSqliteDatabase {
    pub fn new(
        database_connection: WalletDbConnection,
        cipher: Option<Aes256Gcm>,
    ) -> Result<Self, KeyManagerStorageError> {
        let db = Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        };
        db.do_migration()?;
        Ok(db)
    }

    fn decrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), KeyManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|_| KeyManagerStorageError::AeadError("Decryption Error".to_string()))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), KeyManagerStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.encrypt(cipher)
                .map_err(|_| KeyManagerStorageError::AeadError("Encryption Error".to_string()))?;
        }
        Ok(())
    }

    fn insert(&self, key_value_pair: DbKeyValuePair, conn: &SqliteConnection) -> Result<(), KeyManagerStorageError> {
        match key_value_pair {
            DbKeyValuePair::KeyManagerState(km) => {
                let mut km_sql = NewKeyManagerStateSql::from(km);
                self.encrypt_if_necessary(&mut km_sql)?;
                km_sql.commit(conn)?
            },
        }
        Ok(())
    }

    fn do_migration(&self) -> Result<(), KeyManagerStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        let old_state = KeyManagerStateSqlOld::index(&conn)?;
        if old_state.len() > 0 {
            // there should only be 1 if there is an old state.
            let spending_km = DbKeyValuePair::KeyManagerState(KeyManagerState {
                branch_seed: KeyManagerOmsBranch::Spend.to_string(),
                primary_key_index: old_state[0].primary_key_index as u64,
            });
            let spending_script_km = DbKeyValuePair::KeyManagerState(KeyManagerState {
                branch_seed: KeyManagerOmsBranch::SpendScript.to_string(),
                primary_key_index: old_state[0].primary_key_index as u64,
            });
            self.insert(spending_km, &conn)?;
            self.insert(spending_script_km, &conn)?;
            KeyManagerStateSqlOld::delete(&conn)?;
        }
        Ok(())
    }
}

impl KeyManagerBackend for KeyManagerSqliteDatabase {
    #[allow(clippy::cognitive_complexity)]
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::KeyManagerState(branch) => match KeyManagerStateSql::get_state(branch, &conn).ok() {
                None => None,
                Some(mut km) => {
                    self.decrypt_if_necessary(&mut km)?;
                    Some(DbValue::KeyManagerState(KeyManagerState::try_from(km)?))
                },
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch '{}': lock {} + db_op {} = {} ms",
                key,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        match op {
            WriteOperation::Insert(kvp) => self.insert(kvp, &conn)?,
            WriteOperation::Remove(k) => match k {
                DbKey::KeyManagerState(_) => return Err(KeyManagerStorageError::OperationNotSupported),
            },
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - write Insert: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(None)
    }

    fn increment_key_index(&self, branch: String) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        KeyManagerStateSql::increment_index(branch, cipher.as_ref(), &conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - increment_key_index: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn set_key_index(&self, branch: String, index: u64) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        KeyManagerStateSql::set_index(branch, index, cipher.as_ref(), &conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_key_index: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), KeyManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);

        if (*current_cipher).is_some() {
            return Err(KeyManagerStorageError::AlreadyEncrypted);
        }

        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let mut key_manager_states = KeyManagerStateSql::index(&conn)?;
        for key_manager_state in key_manager_states.iter_mut() {
            key_manager_state
                .encrypt(&cipher)
                .map_err(|_| KeyManagerStorageError::AeadError("Encryption Error".to_string()))?;
            key_manager_state.set_state(&conn)?;
        }

        (*current_cipher) = Some(cipher);
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - apply_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), KeyManagerStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut key_manager_states = KeyManagerStateSql::index(&conn)?;

        for key_manager_state in key_manager_states.iter_mut() {
            key_manager_state
                .decrypt(&cipher)
                .map_err(|_| KeyManagerStorageError::AeadError("Encryption Error".to_string()))?;
            key_manager_state.set_state(&conn)?;
        }
        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - remove_encryption: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use aes_gcm::{
        aead::{generic_array::GenericArray, NewAead},
        Aes256Gcm,
    };
    use diesel::{Connection, SqliteConnection};
    use tari_test_utils::random;
    use tempfile::tempdir;

    use crate::{
        key_manager_service::storage::sqlite_db::{KeyManagerState, KeyManagerStateSql, NewKeyManagerStateSql},
        util::encryption::Encryptable,
    };

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();
        let branch = random::string(8);
        assert!(KeyManagerStateSql::get_state(&branch, &conn).is_err());

        let state1 = KeyManagerState {
            branch_seed: branch.clone(),
            primary_key_index: 0,
        };

        NewKeyManagerStateSql::from(state1.clone()).commit(&conn).unwrap();
        let state1_read = KeyManagerStateSql::get_state(&branch, &conn).unwrap();

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        KeyManagerStateSql::increment_index(branch.clone(), None, &conn).unwrap();
        KeyManagerStateSql::increment_index(branch.clone(), None, &conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&branch, &conn).unwrap();
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&state3_read.primary_key_index[..8]);
        assert_eq!(u64::from_le_bytes(bytes), 2);
    }

    #[test]
    fn test_key_manager_encryption() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);
        let branch = "boop boop".to_string();
        let starting_state = KeyManagerState {
            branch_seed: branch.clone(),
            primary_key_index: 1,
        };
        NewKeyManagerStateSql::from(starting_state.clone())
            .commit(&conn)
            .unwrap();

        let state_sql = KeyManagerStateSql::get_state(&branch, &conn).unwrap();

        let mut encrypted_state = state_sql;
        encrypted_state.encrypt(&cipher).unwrap();

        encrypted_state.set_state(&conn).unwrap();
        assert!(KeyManagerStateSql::increment_index(branch.clone(), None, &conn).is_err());
        KeyManagerStateSql::increment_index(branch.clone(), Some(&cipher), &conn).unwrap();

        let mut db_state = KeyManagerStateSql::get_state(&branch, &conn).unwrap();

        db_state.decrypt(&cipher).unwrap();

        let decrypted_data = KeyManagerState::try_from(db_state).unwrap();

        assert_eq!(decrypted_data.primary_key_index, 2);
    }
}

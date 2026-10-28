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
use chacha20poly1305::XChaCha20Poly1305;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use log::*;
use tari_common_types::{
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce},
    types::{CompressedPublicKey, PrivateKey},
};
use tari_utilities::{ByteArray, Hidden, hex::Hex};

use crate::{
    legacy_key_manager::error::KeyManagerStorageError,
    schema::imported_keys::{private_key, public_key, table, timestamp},
    storage::{
        database::ImportedKey,
        sqlite_db::{Encryptable, LOG_TARGET, imported_keys},
    },
};

/// Represents a row in the imported keys table.
#[derive(Clone, Debug, Queryable, Identifiable)]
#[diesel(table_name = imported_keys)]
#[diesel(primary_key(id))]
pub struct ImportedKeySql {
    pub id: i32,
    pub private_key: Vec<u8>,
    pub public_key: String,
    pub timestamp: NaiveDateTime,
}

/// Struct used to create a new Key manager in the database
#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = imported_keys)]
pub struct NewImportedKeySql {
    pub private_key: Vec<u8>,
    pub public_key: String,
    pub timestamp: NaiveDateTime,
}

impl NewImportedKeySql {
    // Creates a new ImportedKey with encrypted values
    pub fn new_from_imported_key(key: ImportedKey, cipher: &XChaCha20Poly1305) -> Result<Self, KeyManagerStorageError> {
        let imported_key_sql = NewImportedKeySql {
            private_key: key.private_key.to_vec(),
            public_key: key.public_key.to_hex(),
            timestamp: Utc::now().naive_utc(),
        };
        let key = imported_key_sql
            .encrypt(cipher)
            .map_err(|_| KeyManagerStorageError::AeadError("Encryption Error".to_string()))?;
        Ok(key)
    }

    /// Commits a new key manager into the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), KeyManagerStorageError> {
        diesel::insert_into(imported_keys::table)
            .values(self.clone())
            .execute(conn)
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        Ok(())
    }
}

impl ImportedKeySql {
    /// Retrieve every imported key currently in the database.
    /// Returns a `Vec` of [ImportedKeySql], if none are found, it will return an empty `Vec`.
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ImportedKeySql>, KeyManagerStorageError> {
        imported_keys::table
            .load::<ImportedKeySql>(conn)
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))
    }

    /// Decrypt the row and check the private key matches the stored public key. A row still encrypted under the
    /// legacy domain, which does not bind the public key, is re-encrypted under the current domain and written back.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_imported_key(
        self,
        cipher: &XChaCha20Poly1305,
        conn: &mut SqliteConnection,
    ) -> Result<ImportedKey, KeyManagerStorageError> {
        let stored_public_key = CompressedPublicKey::from_hex(&self.public_key)?;
        let (plaintext, is_legacy) =
            match decrypt_bytes_integral_nonce(cipher, private_key_domain(&self.public_key), &self.private_key) {
                Ok(plaintext) => (plaintext, false),
                Err(_) => {
                    let plaintext =
                        decrypt_bytes_integral_nonce(cipher, legacy_private_key_domain(), &self.private_key)
                            .map_err(|_| KeyManagerStorageError::AeadError("Decryption Error".to_string()))?;
                    (plaintext, true)
                },
            };
        let decrypted_key = PrivateKey::from_vec(plaintext.reveal())?;
        if CompressedPublicKey::from_secret_key(&decrypted_key) != stored_public_key {
            return Err(KeyManagerStorageError::UnexpectedResult(
                "Imported private key does not match its public key".to_string(),
            ));
        }

        // A failed rewrite does not stop the key from being used; the next load tries again
        if is_legacy {
            let rewrite = encrypt_bytes_integral_nonce(
                cipher,
                private_key_domain(&self.public_key),
                Hidden::hide(decrypted_key.to_vec()),
            )
            .map_err(|e| e.to_string())
            .and_then(|ciphertext| {
                diesel::update(imported_keys::table.find(self.id))
                    .set(imported_keys::private_key.eq(ciphertext))
                    .execute(conn)
                    .map_err(|e| e.to_string())
            });
            if let Err(e) = rewrite {
                warn!(
                    target: LOG_TARGET,
                    "Could not re-encrypt imported key {} under the current domain: {e}", self.public_key
                );
            }
        }

        Ok(ImportedKey {
            private_key: decrypted_key,
            public_key: stored_public_key,
        })
    }

    /// Retrieve the key manager for the provided branch
    /// Will return Err if the branch does not exist in the database
    pub fn get_key(
        key: &CompressedPublicKey,
        conn: &mut SqliteConnection,
    ) -> Result<ImportedKeySql, KeyManagerStorageError> {
        imported_keys::table
            .filter(imported_keys::public_key.eq(key.to_hex()))
            .first::<ImportedKeySql>(conn)
            .map_err(|_| KeyManagerStorageError::KeyManagerNotInitialized)
    }

    pub fn key_exists(key: &CompressedPublicKey, conn: &mut SqliteConnection) -> Result<bool, KeyManagerStorageError> {
        let count: i64 = imported_keys::table
            .filter(imported_keys::public_key.eq(key.to_hex()))
            .count()
            .get_result(conn)
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        Ok(count > 0)
    }
}

/// The encryption domain of an imported private key, bound to the key's public key so that rows cannot be swapped
fn private_key_domain(public_key_hex: &str) -> Vec<u8> {
    [
        <NewImportedKeySql as Encryptable<XChaCha20Poly1305>>::KEY_MANAGER,
        public_key_hex.as_bytes(),
        b"private_key",
    ]
    .concat()
}

/// The encryption domain imported private keys were written under before it was bound to the public key
fn legacy_private_key_domain() -> Vec<u8> {
    [
        <NewImportedKeySql as Encryptable<XChaCha20Poly1305>>::KEY_MANAGER,
        b"private_key",
    ]
    .concat()
}

impl Encryptable<XChaCha20Poly1305> for NewImportedKeySql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [Self::KEY_MANAGER, self.public_key.as_bytes(), field_name.as_bytes()].concat()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key = encrypt_bytes_integral_nonce(
            cipher,
            private_key_domain(&self.public_key),
            Hidden::hide(self.private_key.clone()),
        )?;

        Ok(self)
    }

    fn decrypt(self, _cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        unimplemented!("Not supported")
    }
}

#[cfg(test)]
mod test {
    use chacha20poly1305::{Key, KeyInit};
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::MigrationHarness;
    use tari_crypto::keys::SecretKey as _;
    use tempfile::tempdir;

    use super::*;
    use crate::storage::sqlite_db::MIGRATIONS;

    fn new_key() -> ImportedKey {
        let secret = PrivateKey::random(&mut rand::rng());
        ImportedKey {
            public_key: CompressedPublicKey::from_secret_key(&secret),
            private_key: secret,
        }
    }

    /// Store `stored_private_key` in the row for `public_key`, encrypted under `domain`
    fn insert_raw_row(
        conn: &mut SqliteConnection,
        cipher: &XChaCha20Poly1305,
        row_public_key: &CompressedPublicKey,
        stored_private_key: &PrivateKey,
        domain: Vec<u8>,
    ) {
        let ciphertext =
            encrypt_bytes_integral_nonce(cipher, domain, Hidden::hide(stored_private_key.to_vec())).unwrap();
        NewImportedKeySql {
            private_key: ciphertext,
            public_key: row_public_key.to_hex(),
            timestamp: Utc::now().naive_utc(),
        }
        .commit(conn)
        .unwrap();
    }

    #[test]
    fn imported_keys_are_bound_to_their_public_key() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.sqlite3");
        let mut conn = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&[7u8; 32]));

        // A key written now reads back
        let key = new_key();
        NewImportedKeySql::new_from_imported_key(key.clone(), &cipher)
            .unwrap()
            .commit(&mut conn)
            .unwrap();
        let read = ImportedKeySql::get_key(&key.public_key, &mut conn)
            .unwrap()
            .to_imported_key(&cipher, &mut conn)
            .unwrap();
        assert_eq!(read.private_key, key.private_key);

        // A row written under the legacy domain reads back and is rewritten under the current domain
        let legacy = new_key();
        insert_raw_row(
            &mut conn,
            &cipher,
            &legacy.public_key,
            &legacy.private_key,
            legacy_private_key_domain(),
        );
        let read = ImportedKeySql::get_key(&legacy.public_key, &mut conn)
            .unwrap()
            .to_imported_key(&cipher, &mut conn)
            .unwrap();
        assert_eq!(read.private_key, legacy.private_key);
        let row = ImportedKeySql::get_key(&legacy.public_key, &mut conn).unwrap();
        assert!(decrypt_bytes_integral_nonce(&cipher, private_key_domain(&row.public_key), &row.private_key).is_ok());
        assert!(decrypt_bytes_integral_nonce(&cipher, legacy_private_key_domain(), &row.private_key).is_err());

        // A legacy row still reads when the rewrite fails
        let unrewritable = new_key();
        insert_raw_row(
            &mut conn,
            &cipher,
            &unrewritable.public_key,
            &unrewritable.private_key,
            legacy_private_key_domain(),
        );
        diesel::sql_query(
            "CREATE TRIGGER no_updates BEFORE UPDATE ON imported_keys BEGIN SELECT RAISE(ABORT, 'read only'); END;",
        )
        .execute(&mut conn)
        .unwrap();
        let read = ImportedKeySql::get_key(&unrewritable.public_key, &mut conn)
            .unwrap()
            .to_imported_key(&cipher, &mut conn)
            .unwrap();
        assert_eq!(read.private_key, unrewritable.private_key);
        let row = ImportedKeySql::get_key(&unrewritable.public_key, &mut conn).unwrap();
        assert!(decrypt_bytes_integral_nonce(&cipher, legacy_private_key_domain(), &row.private_key).is_ok());
        diesel::sql_query("DROP TRIGGER no_updates").execute(&mut conn).unwrap();

        // Another key's ciphertext in a row is refused, under either domain
        let victim = new_key();
        insert_raw_row(
            &mut conn,
            &cipher,
            &victim.public_key,
            &key.private_key,
            private_key_domain(&key.public_key.to_hex()),
        );
        assert!(matches!(
            ImportedKeySql::get_key(&victim.public_key, &mut conn)
                .unwrap()
                .to_imported_key(&cipher, &mut conn),
            Err(KeyManagerStorageError::AeadError(_))
        ));
        let victim = new_key();
        insert_raw_row(
            &mut conn,
            &cipher,
            &victim.public_key,
            &key.private_key,
            legacy_private_key_domain(),
        );
        assert!(matches!(
            ImportedKeySql::get_key(&victim.public_key, &mut conn)
                .unwrap()
                .to_imported_key(&cipher, &mut conn),
            Err(KeyManagerStorageError::UnexpectedResult(_))
        ));
    }
}

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
use tari_common_types::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce};
use tari_crypto::keys::PublicKey;
use tari_utilities::{hex::Hex, ByteArray, Hidden};
use zeroize::Zeroize;

use crate::{
    key_manager_service::{
        error::KeyManagerStorageError,
        storage::{
            database::ImportedKey,
            sqlite_db::{imported_keys, Encryptable},
        },
    },
    schema::imported_keys::{private_key, public_key, table, timestamp},
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
    pub fn new_from_imported_key<PK: PublicKey>(
        key: ImportedKey<PK>,
        cipher: &XChaCha20Poly1305,
    ) -> Result<Self, KeyManagerStorageError> {
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
            .execute(conn)?;
        Ok(())
    }
}

impl ImportedKeySql {
    /// Retrieve every imported key currently in the database.
    /// Returns a `Vec` of [ImportedKeySql], if none are found, it will return an empty `Vec`.
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ImportedKeySql>, KeyManagerStorageError> {
        Ok(imported_keys::table.load::<ImportedKeySql>(conn)?)
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn to_imported_key<PK: PublicKey>(
        self,
        cipher: &XChaCha20Poly1305,
    ) -> Result<ImportedKey<PK>, KeyManagerStorageError> {
        let mut decrypted = self
            .decrypt(cipher)
            .map_err(|_| KeyManagerStorageError::AeadError("Decryption Error".to_string()))?;

        let imported_key = ImportedKey {
            private_key: <PK::K>::from_vec(&decrypted.private_key)?,
            public_key: PK::from_hex(&decrypted.public_key)?,
        };
        decrypted.private_key.zeroize();
        Ok(imported_key)
    }

    /// Retrieve the key manager for the provided branch
    /// Will return Err if the branch does not exist in the database
    pub fn get_key<PK: PublicKey>(
        key: &PK,
        conn: &mut SqliteConnection,
    ) -> Result<ImportedKeySql, KeyManagerStorageError> {
        imported_keys::table
            .filter(imported_keys::public_key.eq(key.to_hex()))
            .first::<ImportedKeySql>(conn)
            .map_err(|_| KeyManagerStorageError::KeyManagerNotInitialized)
    }
}

impl Encryptable<XChaCha20Poly1305> for ImportedKeySql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [Self::KEY_MANAGER, field_name.as_bytes()].concat().to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("private_key"),
            Hidden::hide(self.private_key.clone()),
        )?;

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key = decrypt_bytes_integral_nonce(cipher, self.domain("private_key"), &self.private_key)?;

        Ok(self)
    }
}

impl Encryptable<XChaCha20Poly1305> for NewImportedKeySql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [Self::KEY_MANAGER, field_name.as_bytes()].concat().to_vec()
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.private_key = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("private_key"),
            Hidden::hide(self.private_key.clone()),
        )?;

        Ok(self)
    }

    fn decrypt(self, _cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        unimplemented!("Not supported")
    }
}

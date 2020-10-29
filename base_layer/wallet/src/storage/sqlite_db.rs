// Copyright 2019. The Tari Project
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

use crate::{
    error::WalletStorageError,
    schema::{client_key_values, wallet_settings},
    storage::database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable, AES_NONCE_BYTES},
};
use aes_gcm::{
    aead::{generic_array::GenericArray, Aead},
    Aes256Gcm,
    Error as AeadError,
};
use diesel::{prelude::*, SqliteConnection};
use log::*;
use std::{
    str::from_utf8,
    sync::{Arc, Mutex, RwLock},
};
use tari_comms::types::{CommsPublicKey, CommsSecretKey};
use tari_crypto::{
    keys::PublicKey,
    tari_utilities::{
        hex::{from_hex, Hex},
        ByteArray,
    },
};

const LOG_TARGET: &str = "wallet::storage::sqlite_db";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct WalletSqliteDatabase {
    database_connection: Arc<Mutex<SqliteConnection>>,
    cipher: Arc<RwLock<Option<Aes256Gcm>>>,
}
impl WalletSqliteDatabase {
    pub fn new(
        database_connection: Arc<Mutex<SqliteConnection>>,
        cipher: Option<Aes256Gcm>,
    ) -> Result<Self, WalletStorageError>
    {
        // Here we validate if the database is encrypted or not and if a cipher is provided that it is the correct one.
        // Unencrypted the database should contain a CommsPrivateKey and associated CommsPublicKey
        // Encrypted the data should contain a CommsPublicKey in the clear and an encrypted CommsPrivateKey
        // To confirm if the provided Cipher is correct we decrypt the CommsPrivateKey and see if it produces the same
        // CommsPublicKey that is stored in the db
        {
            let conn = acquire_lock!(database_connection);
            let secret_key = WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)?;
            let db_public_key = WalletSettingSql::get(format!("{}", DbKey::CommsPublicKey), &conn)?;

            if cipher.is_some() && secret_key.is_none() {
                error!(
                    target: LOG_TARGET,
                    "Cipher is provided but there is no Comms Secret Key in DB to decrypt"
                );
                return Err(WalletStorageError::InvalidEncryptionCipher);
            }

            if let Some(sk) = secret_key {
                let comms_secret_key = match CommsSecretKey::from_hex(sk.as_str()) {
                    Ok(sk) => {
                        // This means the key was unencrypted
                        if cipher.is_some() {
                            error!(
                                target: LOG_TARGET,
                                "Cipher is provided but Comms Secret Key is not encrypted"
                            );
                            return Err(WalletStorageError::InvalidEncryptionCipher);
                        }
                        sk
                    },
                    Err(_) => {
                        // This means the secret key was encrypted. Try decrypt
                        if let Some(cipher_inner) = cipher.clone() {
                            let mut sk_bytes: Vec<u8> = from_hex(sk.as_str())?;
                            if sk_bytes.len() < AES_NONCE_BYTES {
                                return Err(WalletStorageError::MissingNonce);
                            }
                            // This leaves the nonce in sk_bytes
                            let data = sk_bytes.split_off(AES_NONCE_BYTES);
                            let nonce = GenericArray::from_slice(sk_bytes.as_slice());

                            let decrypted_key = cipher_inner
                                .decrypt(nonce, data.as_ref())
                                .map_err(|_| WalletStorageError::AeadError("Decryption Error".to_string()))?;
                            CommsSecretKey::from_bytes(decrypted_key.as_slice()).map_err(|_| {
                                error!(
                                    target: LOG_TARGET,
                                    "Decrypted Comms Secret Key cannot be parsed into a RistrettoSecretKey"
                                );
                                WalletStorageError::InvalidEncryptionCipher
                            })?
                        } else {
                            error!(
                                target: LOG_TARGET,
                                "Cipher was not provided but Comms Private Key is encrypted"
                            );
                            return Err(WalletStorageError::InvalidEncryptionCipher);
                        }
                    },
                };

                if let Some(pk_hex) = db_public_key {
                    let db_comms_public_key = CommsPublicKey::from_hex(pk_hex.as_str())?;
                    let public_key = CommsPublicKey::from_secret_key(&comms_secret_key);
                    if public_key != db_comms_public_key {
                        if cipher.is_some() {
                            error!(
                                target: LOG_TARGET,
                                "Cipher is provided does not decrypt stored Comms Private Key that produces stored \
                                 Comms Public Key"
                            );
                            return Err(WalletStorageError::InvalidEncryptionCipher);
                        } else {
                            // If the db is not encypted then update the stored public key to keep it in sync.
                            WalletSettingSql::new(format!("{}", DbKey::CommsPublicKey), public_key.to_hex())
                                .set(&conn)?;
                        }
                    }
                } else {
                    if cipher.is_some() {
                        // This means the database was not in the correct state for a Cipher to be provided.
                        error!(
                            target: LOG_TARGET,
                            "Cipher is provided but Comms Public Key is not present in the database"
                        );
                        return Err(WalletStorageError::InvalidEncryptionCipher);
                    }
                    // Due to migration the associated public key is not stored and should be
                    let public_key_hex = CommsPublicKey::from_secret_key(&comms_secret_key).to_hex();
                    WalletSettingSql::new(format!("{}", DbKey::CommsPublicKey), public_key_hex).set(&conn)?;
                }
            }
        }

        Ok(Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        })
    }

    fn set_comms_private_key(
        &self,
        secret_key: &CommsSecretKey,
        conn: &SqliteConnection,
    ) -> Result<(), WalletStorageError>
    {
        let cipher = acquire_read_lock!(self.cipher);

        match cipher.as_ref() {
            None => {
                WalletSettingSql::new(DbKey::CommsSecretKey.to_string(), secret_key.to_hex()).set(&conn)?;
                let public_key = CommsPublicKey::from_secret_key(&secret_key);
                WalletSettingSql::new(DbKey::CommsPublicKey.to_string(), public_key.to_hex()).set(&conn)?;
            },
            Some(cipher) => {
                let public_key = CommsPublicKey::from_secret_key(&secret_key);
                WalletSettingSql::new(DbKey::CommsPublicKey.to_string(), public_key.to_hex()).set(&conn)?;
                let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, secret_key.to_vec())
                    .map_err(|_| WalletStorageError::AeadError("Encryption Error".to_string()))?;
                WalletSettingSql::new(DbKey::CommsSecretKey.to_string(), ciphertext_integral_nonce.to_hex())
                    .set(&conn)?;
            },
        }

        Ok(())
    }

    fn decrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|_| WalletStorageError::AeadError("Decryption Error".to_string()))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.encrypt(cipher)
                .map_err(|_| WalletStorageError::AeadError("Encryption Error".to_string()))?;
        }
        Ok(())
    }
}

impl WalletBackend for WalletSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = acquire_lock!(self.database_connection);
        let cipher = acquire_read_lock!(self.cipher);

        let result = match key {
            DbKey::CommsSecretKey => {
                if let Some(key_str) = WalletSettingSql::get(format!("{}", key), &conn)? {
                    let secret_key = match cipher.as_ref() {
                        None => CommsSecretKey::from_hex(key_str.as_str())?,
                        Some(cipher) => {
                            let decrypted_key_bytes =
                                decrypt_bytes_integral_nonce(&cipher, from_hex(key_str.as_str())?)
                                    .map_err(|_| WalletStorageError::AeadError("Decryption Error".to_string()))?;
                            CommsSecretKey::from_bytes(decrypted_key_bytes.as_slice())?
                        },
                    };

                    Some(DbValue::CommsSecretKey(secret_key))
                } else {
                    None
                }
            },
            DbKey::CommsPublicKey => {
                if let Some(key_str) = WalletSettingSql::get(format!("{}", key), &conn)? {
                    Some(DbValue::CommsPublicKey(CommsPublicKey::from_hex(key_str.as_str())?))
                } else {
                    None
                }
            },
            DbKey::ClientKey(k) => match ClientKeyValueSql::get(k, &conn)? {
                None => None,
                Some(mut v) => {
                    self.decrypt_if_necessary(&mut v)?;
                    Some(DbValue::ClientValue(v.value))
                },
            },
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = acquire_lock!(self.database_connection);
        match op {
            WriteOperation::Insert(kvp) => match kvp {
                DbKeyValuePair::CommsSecretKey(sk) => {
                    self.set_comms_private_key(&sk, &(*conn))?;
                },
                DbKeyValuePair::ClientKeyValue(k, v) => {
                    // First see if we will overwrite a value so we can return the old value
                    let value_to_return = if let Some(mut found_value) = ClientKeyValueSql::get(&k, &conn)? {
                        self.decrypt_if_necessary(&mut found_value)?;
                        Some(found_value)
                    } else {
                        None
                    };

                    let mut client_key_value = ClientKeyValueSql::new(k, v);
                    self.encrypt_if_necessary(&mut client_key_value)?;

                    client_key_value.set(&conn)?;

                    return Ok(value_to_return.map(|v| DbValue::ClientValue(v.value)));
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::CommsSecretKey => {
                    let _ = WalletSettingSql::clear(format!("{}", DbKey::CommsSecretKey), &conn)?;
                },
                DbKey::CommsPublicKey => return Err(WalletStorageError::OperationNotSupported),
                DbKey::ClientKey(k) => {
                    if ClientKeyValueSql::clear(&k, &conn)? {
                        return Ok(Some(DbValue::ValueCleared));
                    }
                },
            },
        }

        Ok(None)
    }

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), WalletStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        if current_cipher.is_some() {
            return Err(WalletStorageError::AlreadyEncrypted);
        }

        let conn = acquire_lock!(self.database_connection);
        let secret_key_str = match WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)? {
            None => return Err(WalletStorageError::ValueNotFound(DbKey::CommsSecretKey)),
            Some(sk) => sk,
        };
        // If this fails then the database is already encrypted.
        let secret_key = CommsSecretKey::from_hex(&secret_key_str).map_err(|_| WalletStorageError::AlreadyEncrypted)?;
        let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, secret_key.to_vec())
            .map_err(|_| WalletStorageError::AeadError("Encryption Error".to_string()))?;
        WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), ciphertext_integral_nonce.to_hex()).set(&conn)?;

        // Encrypt all the client values
        let mut client_key_values = ClientKeyValueSql::index(&conn)?;
        for ckv in client_key_values.iter_mut() {
            ckv.encrypt(&cipher)
                .map_err(|_| WalletStorageError::AeadError("Encryption Error".to_string()))?;
            ckv.set(&conn)?;
        }

        (*current_cipher) = Some(cipher);

        Ok(())
    }

    fn remove_encryption(&self) -> Result<(), WalletStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        let cipher = if let Some(cipher) = (*current_cipher).clone().take() {
            cipher
        } else {
            return Ok(());
        };
        let conn = acquire_lock!(self.database_connection);
        let secret_key_str = match WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)? {
            None => return Err(WalletStorageError::ValueNotFound(DbKey::CommsSecretKey)),
            Some(sk) => sk,
        };

        let secret_key_bytes = decrypt_bytes_integral_nonce(&cipher, from_hex(secret_key_str.as_str())?)
            .map_err(|_| WalletStorageError::AeadError("Decryption Error".to_string()))?;
        let decrypted_key = CommsSecretKey::from_bytes(secret_key_bytes.as_slice())?;
        WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), decrypted_key.to_hex()).set(&conn)?;

        // Decrypt all the client values
        let mut client_key_values = ClientKeyValueSql::index(&conn)?;
        for ckv in client_key_values.iter_mut() {
            ckv.decrypt(&cipher)
                .map_err(|_| WalletStorageError::AeadError("Decryption Error".to_string()))?;
            ckv.set(&conn)?;
        }

        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();

        Ok(())
    }
}

/// A Sql version of the wallet setting key-value table
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "wallet_settings"]
struct WalletSettingSql {
    key: String,
    value: String,
}

impl WalletSettingSql {
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }

    pub fn set(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::replace_into(wallet_settings::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }

    pub fn get(key: String, conn: &SqliteConnection) -> Result<Option<String>, WalletStorageError> {
        wallet_settings::table
            .filter(wallet_settings::key.eq(key))
            .first::<WalletSettingSql>(conn)
            .map(|v: WalletSettingSql| Some(v.value))
            .or_else(|err| match err {
                diesel::result::Error::NotFound => Ok(None),
                err => Err(err.into()),
            })
    }

    pub fn clear(key: String, conn: &SqliteConnection) -> Result<bool, WalletStorageError> {
        let num_deleted = diesel::delete(wallet_settings::table.filter(wallet_settings::key.eq(key))).execute(conn)?;

        Ok(num_deleted > 0)
    }
}

/// A Sql version of the wallet setting key-value table
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "client_key_values"]
struct ClientKeyValueSql {
    key: String,
    value: String,
}

impl ClientKeyValueSql {
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }

    pub fn index(conn: &SqliteConnection) -> Result<Vec<Self>, WalletStorageError> {
        Ok(client_key_values::table.load::<ClientKeyValueSql>(conn)?)
    }

    pub fn set(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::replace_into(client_key_values::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }

    pub fn get(key: &str, conn: &SqliteConnection) -> Result<Option<Self>, WalletStorageError> {
        client_key_values::table
            .filter(client_key_values::key.eq(key))
            .first::<ClientKeyValueSql>(conn)
            .map(Some)
            .or_else(|err| match err {
                diesel::result::Error::NotFound => Ok(None),
                err => Err(err.into()),
            })
    }

    pub fn clear(key: &str, conn: &SqliteConnection) -> Result<bool, WalletStorageError> {
        let num_deleted =
            diesel::delete(client_key_values::table.filter(client_key_values::key.eq(key))).execute(conn)?;

        Ok(num_deleted > 0)
    }
}

impl Encryptable<Aes256Gcm> for ClientKeyValueSql {
    #[allow(unused_assignments)]
    fn encrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let encrypted_value = encrypt_bytes_integral_nonce(&cipher, self.clone().value.as_bytes().to_vec())?;
        self.value = encrypted_value.to_hex();
        Ok(())
    }

    #[allow(unused_assignments)]
    fn decrypt(&mut self, cipher: &Aes256Gcm) -> Result<(), AeadError> {
        let decrypted_value =
            decrypt_bytes_integral_nonce(&cipher, from_hex(self.value.as_str()).map_err(|_| aes_gcm::Error)?)?;
        self.value = from_utf8(decrypted_value.as_slice())
            .map_err(|_| AeadError)?
            .to_string();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::storage::{
        database::{DbKey, DbValue, WalletBackend},
        sqlite_db::{ClientKeyValueSql, WalletSettingSql, WalletSqliteDatabase},
        sqlite_utilities::run_migration_and_create_sqlite_connection,
    };
    use aes_gcm::{
        aead::{generic_array::GenericArray, Aead, NewAead},
        Aes256Gcm,
    };
    use rand::{rngs::OsRng, RngCore};
    use tari_comms::types::{CommsPublicKey, CommsSecretKey};
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        tari_utilities::{hex::Hex, ByteArray},
    };
    use tari_test_utils::random::string;
    use tempfile::tempdir;

    #[test]
    fn test_unencrypted_secret_public_key_setting() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let tempdir = tempdir().unwrap();
        let db_folder = tempdir.path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();
        let secret_key1 = CommsSecretKey::random(&mut OsRng);
        let public_key1 = CommsPublicKey::from_secret_key(&secret_key1);
        {
            let conn = acquire_lock!(connection);
            WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), secret_key1.to_hex())
                .set(&conn)
                .unwrap();
        }

        let db = WalletSqliteDatabase::new(connection.clone(), None).unwrap();

        if let DbValue::CommsSecretKey(sk) = db.fetch(&DbKey::CommsSecretKey).unwrap().unwrap() {
            assert_eq!(sk, secret_key1);
        } else {
            assert!(false, "Should be a Comms Secret Key");
        };
        if let DbValue::CommsPublicKey(pk) = db.fetch(&DbKey::CommsPublicKey).unwrap().unwrap() {
            assert_eq!(pk, public_key1);
        } else {
            assert!(false, "Should be a Comms Public Key");
        };

        let secret_key2 = CommsSecretKey::random(&mut OsRng);
        let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);
        {
            let conn = acquire_lock!(connection);
            db.set_comms_private_key(&secret_key2, &conn).unwrap();
        }
        if let DbValue::CommsPublicKey(pk) = db.fetch(&DbKey::CommsPublicKey).unwrap().unwrap() {
            assert_eq!(pk, public_key2);
        } else {
            assert!(false, "Should be a Comms Public Key");
        };
    }

    #[test]
    pub fn test_encrypted_secret_public_key_validation_during_startup() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = tempdir().unwrap().path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        let secret_key = CommsSecretKey::random(&mut OsRng);
        {
            let conn = acquire_lock!(connection);
            WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), secret_key.to_hex())
                .set(&conn)
                .unwrap();
        }
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        // encrypt the private key
        let secret_key_bytes = secret_key.to_vec();
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let nonce_ga = GenericArray::from_slice(&nonce);
        let mut ciphertext = cipher
            .encrypt(nonce_ga, secret_key_bytes.as_ref())
            .expect("encryption failure!");

        let mut ciphertext_integral_nonce = nonce.to_vec();

        ciphertext_integral_nonce.append(&mut ciphertext);

        {
            let conn = acquire_lock!(connection);
            WalletSettingSql::new(format!("{}", DbKey::CommsSecretKey), ciphertext_integral_nonce.to_hex())
                .set(&conn)
                .unwrap();
        }

        assert!(WalletSqliteDatabase::new(connection.clone(), None).is_err());
        // No public key to compare against
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        // insert incorrect public key
        let incorrect_public_key = CommsPublicKey::from_secret_key(&CommsSecretKey::random(&mut OsRng));
        {
            let conn = acquire_lock!(connection);
            WalletSettingSql::new(format!("{}", DbKey::CommsPublicKey), incorrect_public_key.to_hex())
                .set(&conn)
                .unwrap();
        }
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        // insert correct public key
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        {
            let conn = acquire_lock!(connection);
            WalletSettingSql::new(format!("{}", DbKey::CommsPublicKey), public_key.to_hex())
                .set(&conn)
                .unwrap();
        }
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_ok());
    }

    #[test]
    fn test_apply_and_remove_encryption() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}/{}", db_folder, db_name);
        let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

        let secret_key = CommsSecretKey::random(&mut OsRng);
        let mut key_values = vec![];
        key_values.push(ClientKeyValueSql::new("key1".to_string(), "value1".to_string()));
        key_values.push(ClientKeyValueSql::new("key2".to_string(), "value2".to_string()));
        key_values.push(ClientKeyValueSql::new("key3".to_string(), "value3".to_string()));

        let db = WalletSqliteDatabase::new(connection.clone(), None).unwrap();
        {
            let conn = acquire_lock!(connection);
            db.set_comms_private_key(&secret_key, &conn).unwrap();
            for kv in key_values.iter() {
                kv.set(&conn).unwrap();
            }
        }

        let read_secret_key1 = match db.fetch(&DbKey::CommsSecretKey).unwrap().unwrap() {
            DbValue::CommsSecretKey(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(secret_key, read_secret_key1);

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);
        db.apply_encryption(cipher).unwrap();
        let read_secret_key2 = match db.fetch(&DbKey::CommsSecretKey).unwrap().unwrap() {
            DbValue::CommsSecretKey(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };

        for kv in key_values.iter() {
            match db.fetch(&DbKey::ClientKey(kv.key.clone())).unwrap().unwrap() {
                DbValue::ClientValue(v) => {
                    assert_eq!(kv.value, v);
                },
                _ => {
                    panic!("Should be able to read Key/Value");
                },
            }
        }

        assert_eq!(secret_key, read_secret_key2);
        {
            let conn = acquire_lock!(connection);
            let secret_key_str = WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)
                .unwrap()
                .unwrap();
            assert!(secret_key_str.len() > 64);
            db.set_comms_private_key(&secret_key, &conn).unwrap();
            let secret_key_str = WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)
                .unwrap()
                .unwrap();
            assert!(secret_key_str.len() > 64);
        }

        db.remove_encryption().unwrap();
        let read_secret_key3 = match db.fetch(&DbKey::CommsSecretKey).unwrap().unwrap() {
            DbValue::CommsSecretKey(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(secret_key, read_secret_key3);

        {
            let conn = acquire_lock!(connection);
            let secret_key_str = WalletSettingSql::get(format!("{}", DbKey::CommsSecretKey), &conn)
                .unwrap()
                .unwrap();
            assert_eq!(secret_key_str.len(), 64);
        }

        for kv in key_values.iter() {
            match db.fetch(&DbKey::ClientKey(kv.key.clone())).unwrap().unwrap() {
                DbValue::ClientValue(v) => {
                    assert_eq!(kv.value, v);
                },
                _ => {
                    panic!("Should be able to read Key/Value2");
                },
            }
        }
    }

    #[test]
    fn test_client_key_value_store() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = tempdir().unwrap().path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();
        let conn = acquire_lock!(connection);

        let key1 = "key1".to_string();
        let value1 = "value1".to_string();
        let key2 = "key2".to_string();
        let value2 = "value2".to_string();

        ClientKeyValueSql::new(key1.clone(), value1.clone()).set(&conn).unwrap();
        assert!(ClientKeyValueSql::get(&key2, &conn).unwrap().is_none());
        if let Some(ckv) = ClientKeyValueSql::get(&key1, &conn).unwrap() {
            assert_eq!(ckv.value, value1);
        } else {
            assert!(false, "Should find value");
        }
        assert!(!ClientKeyValueSql::clear(&key2, &conn).unwrap());

        ClientKeyValueSql::new(key2.clone(), value2.clone()).set(&conn).unwrap();

        let values = ClientKeyValueSql::index(&conn).unwrap();
        assert_eq!(values.len(), 2);

        assert!(values[0].value == value1);
        assert!(values[1].value == value2);

        assert!(ClientKeyValueSql::clear(&key1, &conn).unwrap());
        assert!(ClientKeyValueSql::get(&key1, &conn).unwrap().is_none());

        if let Some(ckv) = ClientKeyValueSql::get(&key2, &conn).unwrap() {
            assert_eq!(ckv.value, value2);
        } else {
            assert!(false, "Should find value2");
        }
    }
}

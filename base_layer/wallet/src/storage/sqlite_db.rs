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
    storage::{
        database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
        sqlite_utilities::WalletDbConnection,
    },
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
    str::{from_utf8, FromStr},
    sync::{Arc, RwLock},
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::PeerFeatures,
    tor::TorIdentity,
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::{
    keys::PublicKey,
    tari_utilities::{
        hex::{from_hex, Hex},
        message_format::MessageFormat,
        ByteArray,
    },
};

const LOG_TARGET: &str = "wallet::storage::sqlite_db";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct WalletSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<Option<Aes256Gcm>>>,
}
impl WalletSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, cipher: Option<Aes256Gcm>) -> Result<Self, WalletStorageError> {
        check_db_encryption_status(&database_connection, cipher.clone())?;

        Ok(Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        })
    }

    fn set_master_secret_key(
        &self,
        secret_key: &CommsSecretKey,
        conn: &SqliteConnection,
    ) -> Result<(), WalletStorageError>
    {
        let cipher = acquire_read_lock!(self.cipher);

        match cipher.as_ref() {
            None => {
                WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), secret_key.to_hex()).set(&conn)?;
                let public_key = CommsPublicKey::from_secret_key(&secret_key);
                WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), public_key.to_hex()).set(&conn)?;
            },
            Some(cipher) => {
                let public_key = CommsPublicKey::from_secret_key(&secret_key);
                WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), public_key.to_hex()).set(&conn)?;
                let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, secret_key.to_vec())
                    .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
                WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), ciphertext_integral_nonce.to_hex())
                    .set(&conn)?;
            },
        }

        Ok(())
    }

    fn get_master_secret_key(&self, conn: &SqliteConnection) -> Result<Option<CommsSecretKey>, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(key_str) = WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)? {
            let secret_key = match cipher.as_ref() {
                None => CommsSecretKey::from_hex(key_str.as_str())?,
                Some(cipher) => {
                    let decrypted_key_bytes = decrypt_bytes_integral_nonce(&cipher, from_hex(key_str.as_str())?)
                        .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
                    CommsSecretKey::from_bytes(decrypted_key_bytes.as_slice())?
                },
            };

            Ok(Some(secret_key))
        } else {
            Ok(None)
        }
    }

    fn decrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.decrypt(cipher)
                .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
        }
        Ok(())
    }

    fn encrypt_if_necessary<T: Encryptable<Aes256Gcm>>(&self, o: &mut T) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(cipher) = cipher.as_ref() {
            o.encrypt(cipher)
                .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
        }
        Ok(())
    }

    fn get_comms_address(&self, conn: &SqliteConnection) -> Result<Option<Multiaddr>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(DbKey::CommsAddress.to_string(), &conn)? {
            Ok(Some(
                Multiaddr::from_str(key_str.as_str())
                    .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }

    fn get_comms_features(&self, conn: &SqliteConnection) -> Result<Option<PeerFeatures>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(DbKey::CommsFeatures.to_string(), &conn)? {
            let features = u64::from_str(&key_str).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            let peer_features = PeerFeatures::from_bits(features);
            Ok(peer_features)
        } else {
            Ok(None)
        }
    }

    fn set_tor_id(&self, tor: TorIdentity, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        let tor_string = tor
            .to_json()
            .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
        match cipher.as_ref() {
            None => {
                WalletSettingSql::new(DbKey::TorId.to_string(), tor_string).set(&conn)?;
            },
            Some(cipher) => {
                let bytes = bincode::serialize(&tor).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
                let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, bytes)
                    .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
                WalletSettingSql::new(DbKey::TorId.to_string(), ciphertext_integral_nonce.to_hex()).set(&conn)?;
            },
        }

        Ok(())
    }

    fn get_tor_id(&self, conn: &SqliteConnection) -> Result<Option<DbValue>, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(key_str) = WalletSettingSql::get(DbKey::TorId.to_string(), &conn)? {
            let id = match cipher.as_ref() {
                None => {
                    TorIdentity::from_json(&key_str).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?
                },
                Some(cipher) => {
                    let decrypted_key_bytes = decrypt_bytes_integral_nonce(&cipher, from_hex(&key_str)?)
                        .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
                    bincode::deserialize(&decrypted_key_bytes)
                        .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?
                },
            };
            Ok(Some(DbValue::TorId(id)))
        } else {
            Ok(None)
        }
    }

    fn set_chain_metadata(&self, chain: ChainMetadata, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        let bytes = bincode::serialize(&chain).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
        WalletSettingSql::new(DbKey::BaseNodeChainMetadata.to_string(), bytes.to_hex()).set(&conn)?;
        Ok(())
    }

    fn get_chain_metadata(&self, conn: &SqliteConnection) -> Result<Option<ChainMetadata>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(DbKey::BaseNodeChainMetadata.to_string(), &conn)? {
            let chain_metadata = bincode::deserialize(&from_hex(&key_str)?)
                .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            Ok(Some(chain_metadata))
        } else {
            Ok(None)
        }
    }

    fn insert_key_value_pair(&self, kvp: DbKeyValuePair) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = self.database_connection.acquire_lock();
        match kvp {
            DbKeyValuePair::MasterSecretKey(sk) => {
                self.set_master_secret_key(&sk, &(*conn))?;
            },
            DbKeyValuePair::TorId(node_id) => {
                self.set_tor_id(node_id, &(*conn))?;
            },
            DbKeyValuePair::BaseNodeChainMetadata(metadata) => {
                self.set_chain_metadata(metadata, &(*conn))?;
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
            DbKeyValuePair::CommsAddress(ca) => {
                WalletSettingSql::new(DbKey::CommsAddress.to_string(), ca.to_string()).set(&conn)?;
            },
            DbKeyValuePair::CommsFeatures(cf) => {
                WalletSettingSql::new(DbKey::CommsFeatures.to_string(), cf.bits().to_string()).set(&conn)?;
            },
        }
        Ok(None)
    }

    fn remove_key(&self, k: DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = self.database_connection.acquire_lock();
        match k {
            DbKey::MasterSecretKey => {
                let _ = WalletSettingSql::clear(DbKey::MasterSecretKey.to_string(), &conn)?;
            },
            DbKey::MasterPublicKey => return Err(WalletStorageError::OperationNotSupported),
            DbKey::ClientKey(k) => {
                if ClientKeyValueSql::clear(&k, &conn)? {
                    return Ok(Some(DbValue::ValueCleared));
                }
            },
            DbKey::CommsFeatures => {
                return Err(WalletStorageError::OperationNotSupported);
            },
            DbKey::CommsAddress => {
                return Err(WalletStorageError::OperationNotSupported);
            },
            DbKey::BaseNodeChainMetadata => {
                return Err(WalletStorageError::OperationNotSupported);
            },
            DbKey::TorId => {
                let _ = WalletSettingSql::clear(DbKey::TorId.to_string(), &conn)?;
            },
        };
        Ok(None)
    }
}

impl WalletBackend for WalletSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let conn = self.database_connection.acquire_lock();

        let result = match key {
            DbKey::MasterSecretKey => self.get_master_secret_key(&conn)?.map(DbValue::MasterSecretKey),
            DbKey::MasterPublicKey => {
                if let Some(key_str) = WalletSettingSql::get(key.to_string(), &conn)? {
                    Some(DbValue::MasterPublicKey(CommsPublicKey::from_hex(key_str.as_str())?))
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
            DbKey::CommsAddress => self.get_comms_address(&conn)?.map(DbValue::CommsAddress),
            DbKey::TorId => self.get_tor_id(&conn)?,
            DbKey::CommsFeatures => self.get_comms_features(&conn)?.map(DbValue::CommsFeatures),
            DbKey::BaseNodeChainMetadata => self.get_chain_metadata(&conn)?.map(DbValue::BaseNodeChainMetadata),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        match op {
            WriteOperation::Insert(kvp) => self.insert_key_value_pair(kvp),
            WriteOperation::Remove(k) => self.remove_key(k),
        }
    }

    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), WalletStorageError> {
        let mut current_cipher = acquire_write_lock!(self.cipher);
        if current_cipher.is_some() {
            return Err(WalletStorageError::AlreadyEncrypted);
        }

        let conn = self.database_connection.acquire_lock();
        let secret_key_str = match WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)? {
            None => return Err(WalletStorageError::ValueNotFound(DbKey::MasterSecretKey)),
            Some(sk) => sk,
        };
        // If this fails then the database is already encrypted.
        let secret_key = CommsSecretKey::from_hex(&secret_key_str).map_err(|_| WalletStorageError::AlreadyEncrypted)?;
        let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, secret_key.to_vec())
            .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
        WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), ciphertext_integral_nonce.to_hex()).set(&conn)?;

        // Encrypt all the client values
        let mut client_key_values = ClientKeyValueSql::index(&conn)?;
        for ckv in client_key_values.iter_mut() {
            ckv.encrypt(&cipher)
                .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
            ckv.set(&conn)?;
        }

        // Encrypt tor_id if present
        let tor_id = WalletSettingSql::get(DbKey::TorId.to_string(), &conn)?;
        if let Some(v) = tor_id {
            let tor = TorIdentity::from_json(&v).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            let bytes = bincode::serialize(&tor).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, bytes)
                .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e.to_string())))?;
            WalletSettingSql::new(DbKey::TorId.to_string(), ciphertext_integral_nonce.to_hex()).set(&conn)?;
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
        let conn = self.database_connection.acquire_lock();
        let secret_key_str = match WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)? {
            None => return Err(WalletStorageError::ValueNotFound(DbKey::MasterSecretKey)),
            Some(sk) => sk,
        };

        let secret_key_bytes = decrypt_bytes_integral_nonce(&cipher, from_hex(secret_key_str.as_str())?)
            .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
        let decrypted_key = CommsSecretKey::from_bytes(secret_key_bytes.as_slice())?;
        WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), decrypted_key.to_hex()).set(&conn)?;

        // Decrypt all the client values
        let mut client_key_values = ClientKeyValueSql::index(&conn)?;
        for ckv in client_key_values.iter_mut() {
            ckv.decrypt(&cipher)
                .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
            ckv.set(&conn)?;
        }

        // remove tor id encryption if present
        let key_str = WalletSettingSql::get(DbKey::TorId.to_string(), &conn)?;
        if let Some(v) = key_str {
            let decrypted_key_bytes = decrypt_bytes_integral_nonce(&cipher, from_hex(v.as_str())?)
                .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e.to_string())))?;
            let tor_id: TorIdentity = bincode::deserialize(&decrypted_key_bytes)
                .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            let tor_string = tor_id
                .to_json()
                .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            WalletSettingSql::new(DbKey::TorId.to_string(), tor_string).set(&conn)?;
        }

        // Now that all the decryption has been completed we can safely remove the cipher fully
        let _ = (*current_cipher).take();

        Ok(())
    }
}

/// Confirm if database is encrypted or not and if a cipher is provided confirm the cipher is correct.
/// Unencrypted the database should contain a MasterSecretKey and associated MasterPublicKey
/// Encrypted the data should contain a Master Public Key in the clear and an encrypted MasterSecretKey
/// To confirm if the provided Cipher is correct we decrypt the Master PrivateSecretKey and see if it produces the same
/// Master Public Key that is stored in the db
fn check_db_encryption_status(
    database_connection: &WalletDbConnection,
    cipher: Option<Aes256Gcm>,
) -> Result<(), WalletStorageError>
{
    let conn = database_connection.acquire_lock();
    let secret_key = WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)?;
    let db_public_key = WalletSettingSql::get(DbKey::MasterPublicKey.to_string(), &conn)?;

    if cipher.is_some() && secret_key.is_none() {
        error!(
            target: LOG_TARGET,
            "Cipher is provided but there is no Master Secret Key in DB to decrypt"
        );
        return Err(WalletStorageError::InvalidEncryptionCipher);
    }

    if let Some(sk) = secret_key {
        let master_secret_key = match CommsSecretKey::from_hex(sk.as_str()) {
            Ok(sk) => {
                // This means the key was unencrypted
                if cipher.is_some() {
                    error!(
                        target: LOG_TARGET,
                        "Cipher is provided but Master Secret Key is not encrypted"
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

                    let decrypted_key = cipher_inner.decrypt(nonce, data.as_ref()).map_err(|e| {
                        error!(target: LOG_TARGET, "Incorrect password ({})", e);
                        WalletStorageError::IncorrectPassword
                    })?;
                    CommsSecretKey::from_bytes(decrypted_key.as_slice()).map_err(|_| {
                        error!(
                            target: LOG_TARGET,
                            "Decrypted Master Secret Key cannot be parsed into a RistrettoSecretKey"
                        );
                        WalletStorageError::InvalidEncryptionCipher
                    })?
                } else {
                    error!(
                        target: LOG_TARGET,
                        "Cipher was not provided but Master Secret Key is encrypted"
                    );
                    return Err(WalletStorageError::NoPasswordError);
                }
            },
        };

        if let Some(pk_hex) = db_public_key {
            let db_master_public_key = CommsPublicKey::from_hex(pk_hex.as_str())?;
            let public_key = CommsPublicKey::from_secret_key(&master_secret_key);
            if public_key != db_master_public_key {
                if cipher.is_some() {
                    error!(
                        target: LOG_TARGET,
                        "Cipher is provided but does not decrypt the stored Master Secret Key that produces the \
                         stored Comms Public Key."
                    );
                    return Err(WalletStorageError::InvalidEncryptionCipher);
                } else {
                    // If the db is not encrypted then update the stored public key to keep it in sync.
                    WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), public_key.to_hex()).set(&conn)?;
                }
            }
        } else {
            if cipher.is_some() {
                // This means the database was not in the correct state for a Cipher to be provided.
                error!(
                    target: LOG_TARGET,
                    "Cipher is provided but Master Public Key is not present in the database"
                );
                return Err(WalletStorageError::InvalidEncryptionCipher);
            }
            // Due to migration the associated public key is not stored and should be
            let public_key_hex = CommsPublicKey::from_secret_key(&master_secret_key).to_hex();
            WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), public_key_hex).set(&conn)?;
        }
    }

    Ok(())
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
            let conn = connection.acquire_lock();
            WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), secret_key1.to_hex())
                .set(&conn)
                .unwrap();
        }

        let db = WalletSqliteDatabase::new(connection.clone(), None).unwrap();

        if let DbValue::MasterSecretKey(sk) = db.fetch(&DbKey::MasterSecretKey).unwrap().unwrap() {
            assert_eq!(sk, secret_key1);
        } else {
            panic!("Should be a Master Secret Key");
        };
        if let DbValue::MasterPublicKey(pk) = db.fetch(&DbKey::MasterPublicKey).unwrap().unwrap() {
            assert_eq!(pk, public_key1);
        } else {
            panic!("Should be a Master Public Key");
        };

        let secret_key2 = CommsSecretKey::random(&mut OsRng);
        let public_key2 = CommsPublicKey::from_secret_key(&secret_key2);
        {
            let conn = connection.acquire_lock();
            db.set_master_secret_key(&secret_key2, &conn).unwrap();
        }
        if let DbValue::MasterPublicKey(pk) = db.fetch(&DbKey::MasterPublicKey).unwrap().unwrap() {
            assert_eq!(pk, public_key2);
        } else {
            panic!("Should be a Master Public Key");
        };
    }

    #[test]
    pub fn test_encrypted_secret_public_key_validation_during_startup() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);

        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        let secret_key = CommsSecretKey::random(&mut OsRng);
        {
            let conn = connection.acquire_lock();
            WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), secret_key.to_hex())
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
            let conn = connection.acquire_lock();
            WalletSettingSql::new(DbKey::MasterSecretKey.to_string(), ciphertext_integral_nonce.to_hex())
                .set(&conn)
                .unwrap();
        }

        assert!(WalletSqliteDatabase::new(connection.clone(), None).is_err());
        // No public key to compare against
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        // insert incorrect public key
        let incorrect_public_key = CommsPublicKey::from_secret_key(&CommsSecretKey::random(&mut OsRng));
        {
            let conn = connection.acquire_lock();
            WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), incorrect_public_key.to_hex())
                .set(&conn)
                .unwrap();
        }
        assert!(WalletSqliteDatabase::new(connection.clone(), Some(cipher.clone())).is_err());

        // insert correct public key
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        {
            let conn = connection.acquire_lock();
            WalletSettingSql::new(DbKey::MasterPublicKey.to_string(), public_key.to_hex())
                .set(&conn)
                .unwrap();
        }
        assert!(WalletSqliteDatabase::new(connection, Some(cipher)).is_ok());
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
            let conn = connection.acquire_lock();
            db.set_master_secret_key(&secret_key, &conn).unwrap();
            for kv in key_values.iter() {
                kv.set(&conn).unwrap();
            }
        }

        let read_secret_key1 = match db.fetch(&DbKey::MasterSecretKey).unwrap().unwrap() {
            DbValue::MasterSecretKey(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(secret_key, read_secret_key1);

        let key = GenericArray::from_slice(b"an example very very secret key.");
        let cipher = Aes256Gcm::new(key);
        db.apply_encryption(cipher).unwrap();
        let read_secret_key2 = match db.fetch(&DbKey::MasterSecretKey).unwrap().unwrap() {
            DbValue::MasterSecretKey(sk) => sk,
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
            let conn = connection.acquire_lock();
            let secret_key_str = WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)
                .unwrap()
                .unwrap();
            assert!(secret_key_str.len() > 64);
            db.set_master_secret_key(&secret_key, &conn).unwrap();
            let secret_key_str = WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)
                .unwrap()
                .unwrap();
            assert!(secret_key_str.len() > 64);
        }

        db.remove_encryption().unwrap();
        let read_secret_key3 = match db.fetch(&DbKey::MasterSecretKey).unwrap().unwrap() {
            DbValue::MasterSecretKey(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(secret_key, read_secret_key3);

        {
            let conn = connection.acquire_lock();
            let secret_key_str = WalletSettingSql::get(DbKey::MasterSecretKey.to_string(), &conn)
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
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();
        let conn = connection.acquire_lock();

        let key1 = "key1".to_string();
        let value1 = "value1".to_string();
        let key2 = "key2".to_string();
        let value2 = "value2".to_string();

        ClientKeyValueSql::new(key1.clone(), value1.clone()).set(&conn).unwrap();
        assert!(ClientKeyValueSql::get(&key2, &conn).unwrap().is_none());
        if let Some(ckv) = ClientKeyValueSql::get(&key1, &conn).unwrap() {
            assert_eq!(ckv.value, value1);
        } else {
            panic!("Should find value");
        }
        assert!(!ClientKeyValueSql::clear(&key2, &conn).unwrap());

        ClientKeyValueSql::new(key2.clone(), value2.clone()).set(&conn).unwrap();

        let values = ClientKeyValueSql::index(&conn).unwrap();
        assert_eq!(values.len(), 2);

        assert_eq!(values[0].value, value1);
        assert_eq!(values[1].value, value2);

        assert!(ClientKeyValueSql::clear(&key1, &conn).unwrap());
        assert!(ClientKeyValueSql::get(&key1, &conn).unwrap().is_none());

        if let Some(ckv) = ClientKeyValueSql::get(&key2, &conn).unwrap() {
            assert_eq!(ckv.value, value2);
        } else {
            panic!("Should find value2");
        }
    }
}

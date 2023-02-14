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

use std::{
    convert::TryFrom,
    mem::size_of,
    str::{from_utf8, FromStr},
    sync::{Arc, RwLock},
};

use argon2::password_hash::{
    rand_core::{OsRng, RngCore},
    SaltString,
};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use diesel::{prelude::*, result::Error, SqliteConnection};
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, PeerFeatures},
    tor::TorIdentity,
};
use tari_key_manager::cipher_seed::CipherSeed;
use tari_utilities::{
    hex::{from_hex, Hex},
    hidden_type,
    safe_array::SafeArray,
    Hidden,
    SafePassword,
};
use tokio::time::Instant;
use zeroize::Zeroize;

use crate::{
    error::WalletStorageError,
    schema::{client_key_values, wallet_settings},
    storage::{
        database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
        sqlite_db::scanned_blocks::ScannedBlockSql,
        sqlite_utilities::wallet_db_connection::WalletDbConnection,
    },
    util::encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    utxo_scanner_service::service::ScannedBlock,
};

const LOG_TARGET: &str = "wallet::storage::wallet";

// The main `XChaCha20-Poly1305` key used for database encryption
// This isn't a `SafeArray` because of how we populate it from an authenticated decryption
// However, it is `Hidden` and therefore should be safe to use
hidden_type!(WalletMainEncryptionKey, Vec<u8>);

// The secondary `XChaCha20-Poly1305` key used to encrypt the main key
hidden_type!(WalletSecondaryEncryptionKey, SafeArray<u8, { size_of::<Key>() }>);

// Authenticated data prefix for main key encryption; append the encryption version later
const MAIN_KEY_AAD_PREFIX: &str = "wallet_main_key_encryption_v";

/// A structure to hold `Argon2` parameter versions, which may change over time and must be supported
#[derive(Clone)]
pub struct Argon2Parameters {
    id: u8,                       // version identifier
    algorithm: argon2::Algorithm, // algorithm variant
    version: argon2::Version,     // algorithm version
    params: argon2::Params,       // memory, iteration count, parallelism, output length
}
impl Argon2Parameters {
    /// Construct and return `Argon2` parameters by version identifier
    /// If you pass in `None`, you'll get the most recent
    pub fn from_version(id: Option<u8>) -> Result<Self, WalletStorageError> {
        // Each subsequent version identifier _must_ increase!
        // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
        match id {
            // Be sure to update the `None` behavior when updating this!
            None | Some(1) => Ok(Argon2Parameters {
                id: 1,
                algorithm: argon2::Algorithm::Argon2id,
                version: argon2::Version::V0x13,
                params: argon2::Params::new(46 * 1024, 1, 1, Some(size_of::<Key>()))
                    .map_err(|e| WalletStorageError::AeadError(e.to_string()))?,
            }),
            Some(id) => Err(WalletStorageError::BadEncryptionVersion(id.to_string())),
        }
    }
}

/// A structure to hold encryption-related database field data, to make atomic operations cleaner
pub struct DatabaseEncryptionFields {
    secondary_key_version: u8,   // the encryption parameter version
    secondary_key_salt: String,  // the high-entropy salt used to derive the secondary key
    encrypted_main_key: Vec<u8>, // the main key, encrypted with the secondary key
}
impl DatabaseEncryptionFields {
    /// Read and parse field data from the database atomically
    pub fn read(connection: &SqliteConnection) -> Result<Option<Self>, WalletStorageError> {
        let mut secondary_key_version: Option<String> = None;
        let mut secondary_key_salt: Option<String> = None;
        let mut encrypted_main_key: Option<String> = None;

        // Read all fields atomically
        connection
            .transaction::<_, Error, _>(|| {
                secondary_key_version = WalletSettingSql::get(&DbKey::SecondaryKeyVersion, connection)
                    .map_err(|_| Error::RollbackTransaction)?;
                secondary_key_salt = WalletSettingSql::get(&DbKey::SecondaryKeySalt, connection)
                    .map_err(|_| Error::RollbackTransaction)?;
                encrypted_main_key = WalletSettingSql::get(&DbKey::EncryptedMainKey, connection)
                    .map_err(|_| Error::RollbackTransaction)?;

                Ok(())
            })
            .map_err(|_| WalletStorageError::UnexpectedResult("Unable to read key fields from database".into()))?;

        // Parse the fields
        match (secondary_key_version, secondary_key_salt, encrypted_main_key) {
            // It's fine if none of the fields are set
            (None, None, None) => Ok(None),

            // If all of the fields are set, they must be parsed as valid
            (Some(secondary_key_version), Some(secondary_key_salt), Some(encrypted_main_key)) => {
                let secondary_key_version = u8::from_str(&secondary_key_version)
                    .map_err(|e| WalletStorageError::BadEncryptionVersion(e.to_string()))?;
                let encrypted_main_key =
                    from_hex(&encrypted_main_key).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;

                Ok(Some(DatabaseEncryptionFields {
                    secondary_key_version,
                    secondary_key_salt,
                    encrypted_main_key,
                }))
            },

            // If only some fields are present, there is an invalid state
            _ => Err(WalletStorageError::UnexpectedResult(
                "Not all key data is present in the database".into(),
            )),
        }
    }

    /// Encode and write field data to the database atomically
    pub fn write(&self, connection: &SqliteConnection) -> Result<(), WalletStorageError> {
        // Because the encoding can't fail, just do it inside the write transaction
        connection
            .transaction::<_, Error, _>(|| {
                WalletSettingSql::new(DbKey::SecondaryKeyVersion, self.secondary_key_version.to_string())
                    .set(connection)
                    .map_err(|_| Error::RollbackTransaction)?;
                WalletSettingSql::new(DbKey::SecondaryKeySalt, self.secondary_key_salt.to_string())
                    .set(connection)
                    .map_err(|_| Error::RollbackTransaction)?;
                WalletSettingSql::new(DbKey::EncryptedMainKey, self.encrypted_main_key.to_hex())
                    .set(connection)
                    .map_err(|_| Error::RollbackTransaction)?;

                Ok(())
            })
            .map_err(|_| WalletStorageError::UnexpectedResult("Unable to write key fields into database".into()))?;

        Ok(())
    }
}

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct WalletSqliteDatabase {
    database_connection: WalletDbConnection,
    cipher: Arc<RwLock<XChaCha20Poly1305>>,
}
impl WalletSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection, passphrase: SafePassword) -> Result<Self, WalletStorageError> {
        let cipher = get_db_cipher(&database_connection, &passphrase)?;

        Ok(Self {
            database_connection,
            cipher: Arc::new(RwLock::new(cipher)),
        })
    }

    fn set_master_seed(&self, seed: &CipherSeed, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if WalletSettingSql::get(&DbKey::WalletBirthday, conn)?.is_none() {
            let birthday = seed.birthday();
            WalletSettingSql::new(DbKey::WalletBirthday, birthday.to_string()).set(conn)?;
        }

        let seed_bytes = Hidden::hide(seed.encipher(None)?);
        let ciphertext_integral_nonce =
            encrypt_bytes_integral_nonce(&cipher, b"wallet_setting_master_seed".to_vec(), seed_bytes)
                .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e)))?;
        WalletSettingSql::new(DbKey::MasterSeed, ciphertext_integral_nonce.to_hex()).set(conn)?;

        Ok(())
    }

    fn get_master_seed(&self, conn: &SqliteConnection) -> Result<Option<CipherSeed>, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(seed_str) = WalletSettingSql::get(&DbKey::MasterSeed, conn)? {
            let seed = {
                // Decrypted_key_bytes contains sensitive data regarding decrypted
                // seed words. For this reason, we should zeroize the underlying data buffer
                let decrypted_key_bytes = Hidden::hide(
                    decrypt_bytes_integral_nonce(
                        &cipher,
                        b"wallet_setting_master_seed".to_vec(),
                        &from_hex(seed_str.as_str())?,
                    )
                    .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e)))?,
                );
                CipherSeed::from_enciphered_bytes(decrypted_key_bytes.reveal(), None)?
            };

            Ok(Some(seed))
        } else {
            Ok(None)
        }
    }

    fn decrypt_value<T: Encryptable<XChaCha20Poly1305>>(&self, o: T) -> Result<T, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        let o = o
            .decrypt(&cipher)
            .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e)))?;
        Ok(o)
    }

    #[allow(dead_code)]
    fn encrypt_value<T: Encryptable<XChaCha20Poly1305>>(&self, o: T) -> Result<T, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        o.encrypt(&cipher)
            .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e)))
    }

    fn get_comms_address(&self, conn: &SqliteConnection) -> Result<Option<Multiaddr>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(&DbKey::CommsAddress, conn)? {
            Ok(Some(
                Multiaddr::from_str(key_str.as_str())
                    .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }

    fn get_comms_features(&self, conn: &SqliteConnection) -> Result<Option<PeerFeatures>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(&DbKey::CommsFeatures, conn)? {
            let features = u64::from_str(&key_str).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            let peer_features = PeerFeatures::from_bits(features);
            Ok(peer_features)
        } else {
            Ok(None)
        }
    }

    fn set_tor_id(&self, tor: TorIdentity, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);

        let bytes =
            Hidden::hide(bincode::serialize(&tor).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?);
        let ciphertext_integral_nonce = encrypt_bytes_integral_nonce(&cipher, b"wallet_setting_tor_id".to_vec(), bytes)
            .map_err(|e| WalletStorageError::AeadError(format!("Encryption Error:{}", e)))?;

        WalletSettingSql::new(DbKey::TorId, ciphertext_integral_nonce.to_hex()).set(conn)?;

        Ok(())
    }

    fn get_tor_id(&self, conn: &SqliteConnection) -> Result<Option<DbValue>, WalletStorageError> {
        let cipher = acquire_read_lock!(self.cipher);
        if let Some(key_str) = WalletSettingSql::get(&DbKey::TorId, conn)? {
            let id = {
                // we must zeroize decrypted_key_bytes, as this contains sensitive data,
                // including private key informations
                let decrypted_key_bytes = Hidden::hide(
                    decrypt_bytes_integral_nonce(&cipher, b"wallet_setting_tor_id".to_vec(), &from_hex(&key_str)?)
                        .map_err(|e| WalletStorageError::AeadError(format!("Decryption Error:{}", e)))?,
                );

                bincode::deserialize(decrypted_key_bytes.reveal())
                    .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?
            };
            Ok(Some(DbValue::TorId(id)))
        } else {
            Ok(None)
        }
    }

    fn set_chain_metadata(&self, chain: ChainMetadata, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        let bytes = bincode::serialize(&chain).map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
        WalletSettingSql::new(DbKey::BaseNodeChainMetadata, bytes.to_hex()).set(conn)?;
        Ok(())
    }

    fn get_chain_metadata(&self, conn: &SqliteConnection) -> Result<Option<ChainMetadata>, WalletStorageError> {
        if let Some(key_str) = WalletSettingSql::get(&DbKey::BaseNodeChainMetadata, conn)? {
            let chain_metadata = bincode::deserialize(&from_hex(&key_str)?)
                .map_err(|e| WalletStorageError::ConversionError(e.to_string()))?;
            Ok(Some(chain_metadata))
        } else {
            Ok(None)
        }
    }

    fn insert_key_value_pair(&self, kvp: DbKeyValuePair) -> Result<Option<DbValue>, WalletStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let kvp_text;
        match kvp {
            DbKeyValuePair::MasterSeed(seed) => {
                kvp_text = "MasterSeed";
                self.set_master_seed(&seed, &conn)?;
            },
            DbKeyValuePair::TorId(node_id) => {
                kvp_text = "TorId";
                self.set_tor_id(node_id, &conn)?;
            },
            DbKeyValuePair::BaseNodeChainMetadata(metadata) => {
                kvp_text = "BaseNodeChainMetadata";
                self.set_chain_metadata(metadata, &conn)?;
            },
            DbKeyValuePair::ClientKeyValue(k, v) => {
                // First see if we will overwrite a value so we can return the old value
                let value_to_return = if let Some(found_value) = ClientKeyValueSql::get(&k, &conn)? {
                    let found_value = self.decrypt_value(found_value)?;
                    Some(found_value)
                } else {
                    None
                };

                let client_key_value = ClientKeyValueSql::new(k, v, &cipher)?;

                client_key_value.set(&conn)?;
                if start.elapsed().as_millis() > 0 {
                    trace!(
                        target: LOG_TARGET,
                        "sqlite profile - insert_key_value_pair 'ClientKeyValue': lock {} + db_op {} = {} ms",
                        acquire_lock.as_millis(),
                        (start.elapsed() - acquire_lock).as_millis(),
                        start.elapsed().as_millis()
                    );
                }

                return Ok(value_to_return.map(|v| DbValue::ClientValue(v.value)));
            },
            DbKeyValuePair::CommsAddress(ca) => {
                kvp_text = "CommsAddress";
                WalletSettingSql::new(DbKey::CommsAddress, ca.to_string()).set(&conn)?;
            },
            DbKeyValuePair::CommsFeatures(cf) => {
                kvp_text = "CommsFeatures";
                WalletSettingSql::new(DbKey::CommsFeatures, cf.bits().to_string()).set(&conn)?;
            },
            DbKeyValuePair::CommsIdentitySignature(identity_sig) => {
                kvp_text = "CommsIdentitySignature";
                WalletSettingSql::new(DbKey::CommsIdentitySignature, identity_sig.to_bytes().to_hex()).set(&conn)?;
            },
        }
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - insert_key_value_pair '{}': lock {} + db_op {} = {} ms",
                kvp_text,
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(None)
    }

    fn remove_key(&self, k: DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        match k {
            DbKey::MasterSeed => {
                let _ = WalletSettingSql::clear(&DbKey::MasterSeed, &conn)?;
            },
            DbKey::ClientKey(ref k) => {
                if ClientKeyValueSql::clear(k, &conn)? {
                    return Ok(Some(DbValue::ValueCleared));
                }
            },
            DbKey::TorId => {
                let _ = WalletSettingSql::clear(&DbKey::TorId, &conn)?;
            },
            DbKey::CommsFeatures |
            DbKey::CommsAddress |
            DbKey::BaseNodeChainMetadata |
            DbKey::EncryptedMainKey |
            DbKey::SecondaryKeySalt |
            DbKey::SecondaryKeyVersion |
            DbKey::WalletBirthday |
            DbKey::CommsIdentitySignature => {
                return Err(WalletStorageError::OperationNotSupported);
            },
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - remove_key '{}': lock {} + db_op {} = {} &ms",
                k.to_key_string(),
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }
        Ok(None)
    }

    pub fn cipher(&self) -> XChaCha20Poly1305 {
        let cipher = acquire_read_lock!(self.cipher);
        (*cipher).clone()
    }
}

impl WalletBackend for WalletSqliteDatabase {
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match key {
            DbKey::MasterSeed => self.get_master_seed(&conn)?.map(DbValue::MasterSeed),
            DbKey::ClientKey(k) => match ClientKeyValueSql::get(k, &conn)? {
                None => None,
                Some(v) => {
                    let v = self.decrypt_value(v)?;
                    Some(DbValue::ClientValue(v.value))
                },
            },
            DbKey::CommsAddress => self.get_comms_address(&conn)?.map(DbValue::CommsAddress),
            DbKey::TorId => self.get_tor_id(&conn)?,
            DbKey::CommsFeatures => self.get_comms_features(&conn)?.map(DbValue::CommsFeatures),
            DbKey::BaseNodeChainMetadata => self.get_chain_metadata(&conn)?.map(DbValue::BaseNodeChainMetadata),
            DbKey::EncryptedMainKey => WalletSettingSql::get(key, &conn)?.map(DbValue::EncryptedMainKey),
            DbKey::SecondaryKeySalt => WalletSettingSql::get(key, &conn)?.map(DbValue::SecondaryKeySalt),
            DbKey::SecondaryKeyVersion => WalletSettingSql::get(key, &conn)?.map(DbValue::SecondaryKeyVersion),
            DbKey::WalletBirthday => WalletSettingSql::get(key, &conn)?.map(DbValue::WalletBirthday),
            DbKey::CommsIdentitySignature => WalletSettingSql::get(key, &conn)?
                .and_then(|s| from_hex(&s).ok())
                .and_then(|bytes| IdentitySignature::from_bytes(&bytes).ok())
                .map(Box::new)
                .map(DbValue::CommsIdentitySignature),
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch '{}': lock {} + db_op {} = {} ms",
                key.to_key_string(),
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError> {
        match op {
            WriteOperation::Insert(kvp) => self.insert_key_value_pair(kvp),
            WriteOperation::Remove(k) => self.remove_key(k),
        }
    }

    fn get_scanned_blocks(&self) -> Result<Vec<ScannedBlock>, WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        let sql_blocks = ScannedBlockSql::index(&conn)?;
        sql_blocks
            .into_iter()
            .map(ScannedBlock::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(WalletStorageError::ConversionError)
    }

    fn save_scanned_block(&self, scanned_block: ScannedBlock) -> Result<(), WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        ScannedBlockSql::from(scanned_block).commit(&conn)
    }

    fn clear_scanned_blocks(&self) -> Result<(), WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        ScannedBlockSql::clear_all(&conn)
    }

    fn clear_scanned_blocks_from_and_higher(&self, height: u64) -> Result<(), WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        ScannedBlockSql::clear_from_and_higher(height, &conn)
    }

    fn clear_scanned_blocks_before_height(
        &self,
        height: u64,
        exclude_recovered: bool,
    ) -> Result<(), WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;
        ScannedBlockSql::clear_before_height(height, exclude_recovered, &conn)
    }

    fn change_passphrase(&self, existing: &SafePassword, new: &SafePassword) -> Result<(), WalletStorageError> {
        let conn = self.database_connection.get_pooled_connection()?;

        // Get the existing key-related data so we can decrypt the main key
        match DatabaseEncryptionFields::read(&conn) {
            // Key-related data was present and valid
            Ok(Some(data)) => {
                // Use the given version if it is valid
                let argon2_params = Argon2Parameters::from_version(Some(data.secondary_key_version))?;

                // Derive a secondary key from the existing passphrase and salt
                let secondary_key = derive_secondary_key(existing, argon2_params.clone(), &data.secondary_key_salt)?;

                // Attempt to decrypt the encrypted main key
                let main_key = decrypt_main_key(&secondary_key, &data.encrypted_main_key, argon2_params.id)?;

                // Now use the most recent version
                let new_argon2_params = Argon2Parameters::from_version(None)?;

                // Derive a new secondary key from the new passphrase and a fresh salt
                let new_secondary_key_salt = SaltString::generate(&mut OsRng).to_string();
                let new_secondary_key = derive_secondary_key(new, new_argon2_params.clone(), &new_secondary_key_salt)?;

                // Encrypt the main key with the new secondary key
                let new_encrypted_main_key = encrypt_main_key(&new_secondary_key, &main_key, new_argon2_params.id)?;

                // Store the new key-related fields
                DatabaseEncryptionFields {
                    secondary_key_version: new_argon2_params.id,
                    secondary_key_salt: new_secondary_key_salt,
                    encrypted_main_key: new_encrypted_main_key,
                }
                .write(&conn)?;
            },

            // If any key-related is not present, this is an invalid state
            _ => {
                return Err(WalletStorageError::UnexpectedResult(
                    "Unable to get valid key-related data from database".into(),
                ))
            },
        };

        Ok(())
    }
}

/// Derive a secondary database key
fn derive_secondary_key(
    passphrase: &SafePassword,
    params: Argon2Parameters,
    salt: &String,
) -> Result<WalletSecondaryEncryptionKey, WalletStorageError> {
    // Derive a secondary key from the existing passphrase and salt
    let mut secondary_key = WalletSecondaryEncryptionKey::from(SafeArray::default());
    argon2::Argon2::new(params.algorithm, params.version, params.params)
        .hash_password_into(passphrase.reveal(), salt.as_bytes(), secondary_key.reveal_mut())
        .map_err(|e| WalletStorageError::AeadError(e.to_string()))?;

    Ok(secondary_key)
}

/// Encrypt the main database key using the secondary key
fn encrypt_main_key(
    secondary_key: &WalletSecondaryEncryptionKey,
    main_key: &WalletMainEncryptionKey,
    version: u8,
) -> Result<Vec<u8>, WalletStorageError> {
    // Set up the authenticated data
    let mut aad = MAIN_KEY_AAD_PREFIX.as_bytes().to_owned();
    aad.push(version);

    // Encrypt the main key
    let cipher = XChaCha20Poly1305::new(Key::from_slice(secondary_key.reveal()));
    let encrypted_main_key = encrypt_bytes_integral_nonce(&cipher, aad, Hidden::hide(main_key.reveal().clone()))
        .map_err(WalletStorageError::AeadError)?;

    Ok(encrypted_main_key)
}

/// Decrypt the main database key using the secondary key
fn decrypt_main_key(
    secondary_key: &WalletSecondaryEncryptionKey,
    encrypted_main_key: &[u8],
    version: u8,
) -> Result<WalletMainEncryptionKey, WalletStorageError> {
    // Set up the authenticated data
    let mut aad = MAIN_KEY_AAD_PREFIX.as_bytes().to_owned();
    aad.push(version);

    // Authenticate and decrypt the main key
    let cipher = XChaCha20Poly1305::new(Key::from_slice(secondary_key.reveal()));

    Ok(WalletMainEncryptionKey::from(
        decrypt_bytes_integral_nonce(&cipher, aad, encrypted_main_key)
            .map_err(|_| WalletStorageError::InvalidPassphrase)?,
    ))
}

/// Prepare the database encryption cipher
fn get_db_cipher(
    database_connection: &WalletDbConnection,
    passphrase: &SafePassword,
) -> Result<XChaCha20Poly1305, WalletStorageError> {
    let conn = database_connection.get_pooled_connection()?;

    // Either set up a new main key, or decrypt it using existing data
    let main_key = match DatabaseEncryptionFields::read(&conn) {
        // Encryption is not set up yet
        Ok(None) => {
            // Generate a high-entropy main key
            let mut main_key = WalletMainEncryptionKey::from(vec![0u8; size_of::<Key>()]);
            let mut rng = OsRng;
            rng.fill_bytes(main_key.reveal_mut());

            // Use the most recent `Argon2` parameters
            let argon2_params = Argon2Parameters::from_version(None)?;

            // Derive the secondary key from the user's passphrase and a high-entropy salt
            let secondary_key_salt = SaltString::generate(&mut rng).to_string();
            let secondary_key = derive_secondary_key(passphrase, argon2_params.clone(), &secondary_key_salt)?;

            // Use the secondary key to encrypt the main key
            let encrypted_main_key = encrypt_main_key(&secondary_key, &main_key, argon2_params.id)?;

            // Store the key-related fields
            DatabaseEncryptionFields {
                secondary_key_version: argon2_params.id,
                secondary_key_salt,
                encrypted_main_key,
            }
            .write(&conn)?;

            // Return the unencrypted main key
            main_key
        },

        // Encryption has already been set up
        Ok(Some(data)) => {
            // Use the given version if it is valid
            let argon2_params = Argon2Parameters::from_version(Some(data.secondary_key_version))?;

            // Derive the secondary key from the user's passphrase and salt
            let secondary_key = derive_secondary_key(passphrase, argon2_params, &data.secondary_key_salt)?;

            // Attempt to decrypt and return the encrypted main key
            decrypt_main_key(&secondary_key, &data.encrypted_main_key, data.secondary_key_version)?
        },

        // We couldn't get valid key-related data
        Err(_) => {
            return Err(WalletStorageError::UnexpectedResult(
                "Unable to parse key fields from database".into(),
            ));
        },
    };

    Ok(XChaCha20Poly1305::new(Key::from_slice(main_key.reveal())))
}

/// A Sql version of the wallet setting key-value table
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "wallet_settings"]
pub(crate) struct WalletSettingSql {
    key: String,
    value: String,
}

impl WalletSettingSql {
    pub fn new(key: DbKey, value: String) -> Self {
        Self {
            key: key.to_key_string(),
            value,
        }
    }

    pub fn set(&self, conn: &SqliteConnection) -> Result<(), WalletStorageError> {
        diesel::replace_into(wallet_settings::table)
            .values(self)
            .execute(conn)?;

        Ok(())
    }

    pub fn get(key: &DbKey, conn: &SqliteConnection) -> Result<Option<String>, WalletStorageError> {
        wallet_settings::table
            .filter(wallet_settings::key.eq(key.to_key_string()))
            .first::<WalletSettingSql>(conn)
            .map(|v: WalletSettingSql| Some(v.value))
            .or_else(|err| match err {
                diesel::result::Error::NotFound => Ok(None),
                err => Err(err.into()),
            })
    }

    pub fn clear(key: &DbKey, conn: &SqliteConnection) -> Result<bool, WalletStorageError> {
        let num_deleted = diesel::delete(wallet_settings::table.filter(wallet_settings::key.eq(key.to_key_string())))
            .execute(conn)?;
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
    pub fn new(key: String, value: String, cipher: &XChaCha20Poly1305) -> Result<Self, WalletStorageError> {
        let client_kv = Self { key, value };
        client_kv.encrypt(cipher).map_err(WalletStorageError::AeadError)
    }

    #[allow(dead_code)]
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

impl Encryptable<XChaCha20Poly1305> for ClientKeyValueSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        [Self::CLIENT_KEY_VALUE, self.key.as_bytes(), field_name.as_bytes()]
            .concat()
            .to_vec()
    }

    #[allow(unused_assignments)]
    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.value = encrypt_bytes_integral_nonce(
            cipher,
            self.domain("value"),
            Hidden::hide(self.value.as_bytes().to_vec()),
        )?
        .to_hex();

        Ok(self)
    }

    #[allow(unused_assignments)]
    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        let mut decrypted_value = decrypt_bytes_integral_nonce(
            cipher,
            self.domain("value"),
            &from_hex(self.value.as_str()).map_err(|e| e.to_string())?,
        )?;

        self.value = from_utf8(decrypted_value.as_slice())
            .map_err(|e| e.to_string())?
            .to_string();

        // we zeroize the decrypted value
        decrypted_value.zeroize();

        Ok(self)
    }
}

#[cfg(test)]
mod test {
    use tari_key_manager::cipher_seed::CipherSeed;
    use tari_test_utils::random::string;
    use tari_utilities::{hex::from_hex, ByteArray, SafePassword};
    use tempfile::tempdir;

    use crate::{
        storage::{
            database::{DbKey, DbValue, WalletBackend},
            sqlite_db::wallet::{ClientKeyValueSql, WalletSettingSql, WalletSqliteDatabase},
            sqlite_utilities::run_migration_and_create_sqlite_connection,
        },
        util::encryption::{decrypt_bytes_integral_nonce, Encryptable},
    };

    #[test]
    fn test_passphrase() {
        // Set up a database
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}/{}", db_folder, db_name);
        let connection = run_migration_and_create_sqlite_connection(db_path, 16).unwrap();

        // Encrypt with a passphrase
        let db = WalletSqliteDatabase::new(connection.clone(), "passphrase".to_string().into()).unwrap();

        // Load again with the correct passphrase
        assert!(WalletSqliteDatabase::new(connection.clone(), "passphrase".to_string().into()).is_ok());

        // Try to load with the wrong passphrase
        assert!(WalletSqliteDatabase::new(connection.clone(), "evil passphrase".to_string().into()).is_err());

        // Try to change the passphrase, but fail
        assert!(db
            .change_passphrase(
                &"evil passphrase".to_string().into(),
                &"new passphrase".to_string().into()
            )
            .is_err());

        // The existing passphrase still works
        assert!(WalletSqliteDatabase::new(connection.clone(), "passphrase".to_string().into()).is_ok());

        // The new passphrase doesn't
        assert!(WalletSqliteDatabase::new(connection.clone(), "new passphrase".to_string().into()).is_err());

        // Successfully change the passphrase
        assert!(db
            .change_passphrase(&"passphrase".to_string().into(), &"new passphrase".to_string().into())
            .is_ok());

        // The existing passphrase no longer works
        assert!(WalletSqliteDatabase::new(connection.clone(), "passphrase".to_string().into()).is_err());

        // The new passphrase does
        assert!(WalletSqliteDatabase::new(connection, "new passphrase".to_string().into()).is_ok());
    }

    #[test]
    fn test_encryption_is_forced() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let db_path = format!("{}/{}", db_folder, db_name);
        let connection = run_migration_and_create_sqlite_connection(db_path, 16).unwrap();

        let seed = CipherSeed::new();
        let passphrase = "a very very secret key example.".to_string().into();
        let db = WalletSqliteDatabase::new(connection.clone(), passphrase).unwrap();
        let cipher = db.cipher();

        let mut key_values = vec![
            ClientKeyValueSql::new("key1".to_string(), "value1".to_string(), &cipher).unwrap(),
            ClientKeyValueSql::new("key2".to_string(), "value2".to_string(), &cipher).unwrap(),
            ClientKeyValueSql::new("key3".to_string(), "value3".to_string(), &cipher).unwrap(),
        ];
        {
            let conn = connection.get_pooled_connection().unwrap();
            db.set_master_seed(&seed, &conn).unwrap();
            for kv in &mut key_values {
                kv.set(&conn).unwrap();
            }
        }

        let read_seed1 = match db.fetch(&DbKey::MasterSeed).unwrap().unwrap() {
            DbValue::MasterSeed(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(seed, read_seed1);

        let read_seed2 = match db.fetch(&DbKey::MasterSeed).unwrap().unwrap() {
            DbValue::MasterSeed(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };

        for kv in &mut key_values {
            *kv = kv.clone().decrypt(&db.cipher()).unwrap();
            match db.fetch(&DbKey::ClientKey(kv.key.clone())).unwrap().unwrap() {
                DbValue::ClientValue(v) => {
                    assert_eq!(kv.value, v);
                },
                _ => {
                    panic!("Should be able to read Key/Value");
                },
            }
        }

        assert_eq!(seed, read_seed2);
        {
            let conn = connection.get_pooled_connection().unwrap();
            let secret_key_str = WalletSettingSql::get(&DbKey::MasterSeed, &conn).unwrap().unwrap();
            assert!(secret_key_str.len() > 64);
            db.set_master_seed(&seed, &conn).unwrap();
            let secret_key_str = WalletSettingSql::get(&DbKey::MasterSeed, &conn).unwrap().unwrap();
            assert!(secret_key_str.len() > 64);
        }

        let read_seed3 = match db.fetch(&DbKey::MasterSeed).unwrap().unwrap() {
            DbValue::MasterSeed(sk) => sk,
            _ => {
                panic!("Should be able to read Key");
            },
        };
        assert_eq!(seed, read_seed3);

        for kv in &key_values {
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
        let connection = run_migration_and_create_sqlite_connection(format!("{}{}", db_folder, db_name), 16).unwrap();
        let conn = connection.get_pooled_connection().unwrap();

        let key1 = "key1".to_string();
        let value1 = "value1".to_string();
        let key2 = "key2".to_string();
        let value2 = "value2".to_string();

        let passphrase = "a very very secret key example.".to_string().into();
        let db = WalletSqliteDatabase::new(connection, passphrase).unwrap();
        let cipher = db.cipher();

        ClientKeyValueSql::new(key1.clone(), value1.clone(), &cipher)
            .unwrap()
            .set(&conn)
            .unwrap();
        assert!(ClientKeyValueSql::get(&key2, &conn).unwrap().is_none());
        if let Some(ckv) = ClientKeyValueSql::get(&key1, &conn).unwrap() {
            let ckv = ckv.decrypt(&cipher).unwrap();
            assert_eq!(ckv.value, value1);
        } else {
            panic!("Should find value");
        }
        assert!(!ClientKeyValueSql::clear(&key2, &conn).unwrap());

        ClientKeyValueSql::new(key2.clone(), value2.clone(), &cipher)
            .unwrap()
            .set(&conn)
            .unwrap();

        let values = ClientKeyValueSql::index(&conn).unwrap();
        assert_eq!(values.len(), 2);

        assert_eq!(values[0].clone().decrypt(&cipher).unwrap().value, value1);
        assert_eq!(values[1].clone().decrypt(&cipher).unwrap().value, value2);

        assert!(ClientKeyValueSql::clear(&key1, &conn).unwrap());
        assert!(ClientKeyValueSql::get(&key1, &conn).unwrap().is_none());

        if let Some(ckv) = ClientKeyValueSql::get(&key2, &conn).unwrap() {
            let ckv = ckv.decrypt(&cipher).unwrap();
            assert_eq!(ckv.value, value2);
        } else {
            panic!("Should find value2");
        }
    }

    #[test]
    fn test_set_master_seed() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_tempdir = tempdir().unwrap();
        let db_folder = db_tempdir.path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(format!("{}{}", db_folder, db_name), 16).unwrap();

        let passphrase = SafePassword::from("an example very very secret key.".to_string());

        let wallet = WalletSqliteDatabase::new(connection.clone(), passphrase).unwrap();

        let seed = CipherSeed::new();

        let conn = connection.get_pooled_connection().unwrap();
        wallet.set_master_seed(&seed, &conn).unwrap();

        let seed_bytes = seed.encipher(None).unwrap();

        let db_seed = WalletSettingSql::get(&DbKey::MasterSeed, &conn).unwrap().unwrap();
        assert_eq!(db_seed.len(), 146);

        let decrypted_db_seed = decrypt_bytes_integral_nonce(
            &wallet.cipher(),
            b"wallet_setting_master_seed".to_vec(),
            &from_hex(db_seed.as_str()).unwrap(),
        )
        .unwrap();

        assert_eq!(decrypted_db_seed, seed_bytes);
    }
}

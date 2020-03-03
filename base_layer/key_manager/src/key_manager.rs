// Copyright 2019 The Tari Project
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

use crate::mnemonic;
use derive_error::Error;
use digest::Digest;
use rand::{CryptoRng, Rng};
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};
use std::marker::PhantomData;
use tari_crypto::{
    keys::SecretKey,
    tari_utilities::{byte_array::ByteArrayError, hex::Hex},
};

#[derive(Debug, Error, PartialEq)]
pub enum KeyManagerError {
    // Could not convert into byte array
    ByteArrayError(ByteArrayError),
    // Could not convert provided Mnemonic into master key
    MnemonicError(mnemonic::MnemonicError),
}

#[derive(Clone, Debug)]
pub struct DerivedKey<K>
where K: SecretKey
{
    pub k: K,
    pub key_index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyManager<K: SecretKey, D: Digest> {
    pub master_key: K,
    pub branch_seed: String,
    pub primary_key_index: usize,
    digest_type: PhantomData<D>,
}

impl<K, D> KeyManager<K, D>
where
    K: SecretKey + serde::Serialize + DeserializeOwned + mnemonic::Mnemonic<K>,
    D: Digest,
{
    /// Creates a new KeyManager with a new randomly selected master_key
    pub fn new<R: CryptoRng + Rng>(rng: &mut R) -> KeyManager<K, D> {
        KeyManager {
            master_key: SecretKey::random(rng),
            branch_seed: "".to_string(),
            primary_key_index: 0,
            digest_type: PhantomData,
        }
    }

    /// Constructs a KeyManager from known parts
    pub fn from(master_key: K, branch_seed: String, primary_key_index: usize) -> KeyManager<K, D> {
        KeyManager {
            master_key,
            branch_seed,
            primary_key_index,
            digest_type: PhantomData,
        }
    }

    /// Constructs a KeyManager by generating a master_key=SHA256(seed_phrase) using a non-mnemonic seed phrase
    pub fn from_seed_phrase(
        seed_phrase: String,
        branch_seed: String,
        primary_key_index: usize,
    ) -> Result<KeyManager<K, D>, KeyManagerError>
    {
        match K::from_bytes(D::digest(&seed_phrase.into_bytes()).as_slice()) {
            Ok(master_key) => Ok(KeyManager {
                master_key,
                branch_seed,
                primary_key_index,
                digest_type: PhantomData,
            }),
            Err(e) => Err(KeyManagerError::from(e)),
        }
    }

    /// Creates a KeyManager from the provided sequence of mnemonic words, the language of the mnemonic sequence will be
    /// auto detected
    pub fn from_mnemonic(
        mnemonic_seq: &[String],
        branch_seed: String,
        primary_key_index: usize,
    ) -> Result<KeyManager<K, D>, KeyManagerError>
    {
        match K::from_mnemonic(mnemonic_seq) {
            Ok(master_key) => Ok(KeyManager {
                master_key,
                branch_seed,
                primary_key_index,
                digest_type: PhantomData,
            }),
            Err(e) => Err(KeyManagerError::from(e)),
        }
    }

    /// Derive a new private key from master key: derived_key=SHA256(master_key||branch_seed||index)
    pub fn derive_key(&self, key_index: usize) -> Result<DerivedKey<K>, ByteArrayError> {
        let concatenated = format!("{}{}", self.master_key.to_hex(), key_index.to_string());
        match K::from_bytes(D::digest(&concatenated.into_bytes()).as_slice()) {
            Ok(k) => Ok(DerivedKey { k, key_index }),
            Err(e) => Err(e),
        }
    }

    /// Generate next deterministic private key derived from master key
    pub fn next_key(&mut self) -> Result<DerivedKey<K>, ByteArrayError> {
        self.primary_key_index += 1;
        self.derive_key(self.primary_key_index)
    }
}

#[cfg(test)]
mod test {
    use crate::{file_backup::*, key_manager::*};
    use rand::rngs::OsRng;
    use sha2::Sha256;
    use std::fs::remove_file;
    use tari_crypto::ristretto::RistrettoSecretKey;

    #[test]
    fn test_new_keymanager() {
        let km1 = KeyManager::<RistrettoSecretKey, Sha256>::new(&mut OsRng);
        let km2 = KeyManager::<RistrettoSecretKey, Sha256>::new(&mut OsRng);
        assert_ne!(km1.master_key, km2.master_key);
    }

    #[test]
    fn test_from_seed_phrase() {
        let seed_phrase1 = "random seed phrase".to_string();
        let seed_phrase2 = "additional random Seed phrase".to_string();
        let branch_seed = "".to_string();
        let km1 = KeyManager::<RistrettoSecretKey, Sha256>::from_seed_phrase(seed_phrase1, branch_seed.clone(), 0);
        let km2 = KeyManager::<RistrettoSecretKey, Sha256>::from_seed_phrase(seed_phrase2, branch_seed, 0);
        if km1.is_ok() && km2.is_ok() {
            assert_ne!(km1.unwrap().master_key, km2.unwrap().master_key);
        } else {
            assert!(false)
        }
    }

    #[test]
    fn test_from_mnemonic() {
        let mnemonic_seq1 = vec![
            "clever", "jaguar", "bus", "engage", "oil", "august", "media", "high", "trick", "remove", "tiny", "join",
            "item", "tobacco", "orange", "pony", "tomorrow", "also", "dignity", "giraffe", "little", "board", "army",
            "scale",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();
        let mnemonic_seq2 = vec![
            "spatial", "travel", "remove", "few", "cinnamon", "three", "drift", "grit", "amazing", "isolate", "merge",
            "tonight", "apple", "garden", "damage", "job", "equal", "ahead", "wolf", "initial", "woman", "regret",
            "neither", "divorce",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();
        let branch_seed = "".to_string();
        let km1 = KeyManager::<RistrettoSecretKey, Sha256>::from_mnemonic(&mnemonic_seq1, branch_seed.clone(), 0);
        let km2 = KeyManager::<RistrettoSecretKey, Sha256>::from_mnemonic(&mnemonic_seq2, branch_seed, 0);

        if km1.is_ok() && km2.is_ok() {
            assert_ne!(km1.unwrap().master_key, km2.unwrap().master_key);
        } else {
            assert!(false)
        }
    }

    #[test]
    fn test_derive_and_next_key() {
        let mut km = KeyManager::<RistrettoSecretKey, Sha256>::new(&mut OsRng);
        let next_key1_result = km.next_key();
        let next_key2_result = km.next_key();
        let desired_key_index1 = 1;
        let desired_key_index2 = 2;
        let derived_key1_result = km.derive_key(desired_key_index1);
        let derived_key2_result = km.derive_key(desired_key_index2);
        if next_key1_result.is_ok() &&
            next_key2_result.is_ok() &&
            derived_key1_result.is_ok() &&
            derived_key2_result.is_ok()
        {
            let next_key1 = next_key1_result.unwrap();
            let next_key2 = next_key2_result.unwrap();
            let derived_key1 = derived_key1_result.unwrap();
            let derived_key2 = derived_key2_result.unwrap();
            assert_ne!(next_key1.k, next_key2.k);
            assert_eq!(next_key1.k, derived_key1.k);
            assert_eq!(next_key2.k, derived_key2.k);
            assert_eq!(next_key1.key_index, desired_key_index1);
            assert_eq!(next_key2.key_index, desired_key_index2);
        }
    }

    #[test]
    fn test_to_file_and_from_file() {
        let desired_km = KeyManager::<RistrettoSecretKey, Sha256>::new(&mut OsRng);
        let backup_filename = "test_km_backup.json".to_string();
        // Backup KeyManager to file
        match desired_km.to_file(&backup_filename) {
            Ok(_v) => {
                // Restore KeyManager from file
                let backup_km_result: Result<KeyManager<RistrettoSecretKey, Sha256>, FileError> =
                    KeyManager::from_file(&backup_filename);
                match backup_km_result {
                    Ok(backup_km) => {
                        // Remove temp key_manager backup file
                        remove_file(backup_filename).unwrap();

                        assert_eq!(desired_km.branch_seed, backup_km.branch_seed);
                        assert_eq!(desired_km.master_key, backup_km.master_key);
                        assert_eq!(desired_km.primary_key_index, backup_km.primary_key_index);
                    },
                    Err(_e) => assert!(false),
                };
            },
            Err(_e) => assert!(false),
        };
    }
}

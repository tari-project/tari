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

use std::marker::PhantomData;

use derivative::Derivative;
use digest::Digest;
use serde::{Deserialize, Serialize};
use tari_crypto::{
    hashing::{DomainSeparatedHasher, GenericHashDomain, LengthExtensionAttackResistant},
    keys::SecretKey,
    tari_utilities::{byte_array::ByteArrayError, hex::Hex},
};

use crate::cipher_seed::CipherSeed;

const DOMAIN_SEPARATION_LABEL: &str = "com.tari.key_manager.key_manager";

#[derive(Clone, Derivative, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct DerivedKey<K>
where K: SecretKey
{
    #[derivative(Debug = "ignore")]
    #[serde(skip_serializing)]
    pub k: K,
    pub key_index: u64,
}

#[derive(Clone, Derivative, PartialEq, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct KeyManager<K: SecretKey, D: Digest + LengthExtensionAttackResistant> {
    #[derivative(Debug = "ignore")]
    seed: CipherSeed,
    #[derivative(Debug = "ignore")]
    pub branch_seed: String,
    primary_key_index: u64,
    digest_type: PhantomData<D>,
    key_type: PhantomData<K>,
}

impl<K, D> KeyManager<K, D>
where
    K: SecretKey,
    D: Digest + LengthExtensionAttackResistant,
{
    /// Creates a new KeyManager with a new randomly selected entropy
    pub fn new() -> KeyManager<K, D> {
        KeyManager {
            seed: CipherSeed::new(),
            branch_seed: "".to_string(),
            primary_key_index: 0,
            digest_type: PhantomData,
            key_type: PhantomData,
        }
    }

    /// Constructs a KeyManager from known parts
    pub fn from(seed: CipherSeed, branch_seed: String, primary_key_index: u64) -> KeyManager<K, D> {
        KeyManager {
            seed,
            branch_seed,
            primary_key_index,
            digest_type: PhantomData,
            key_type: PhantomData,
        }
    }

    /// Derive a new private key from master key: derived_key=H(master_key||branch_seed||index), for some
    /// hash function H which is Length attack resistant, such as Blake2b.
    pub fn derive_key(&self, key_index: u64) -> Result<DerivedKey<K>, ByteArrayError> {
        // apply domain separation to generate derive key. Under the hood, the hashing api prepends the length of each
        // piece of data for concatenation, reducing the risk of collisions due to redundance of variable length
        // input
        let derive_key = DomainSeparatedHasher::<D, GenericHashDomain>::new(DOMAIN_SEPARATION_LABEL)
            .chain(self.seed.entropy())
            .chain(self.branch_seed.as_str().as_bytes())
            .chain(key_index.to_le_bytes())
            .finalize()
            .into_vec();

        match K::from_bytes(derive_key.as_slice()) {
            Ok(k) => Ok(DerivedKey { k, key_index }),
            Err(e) => Err(e),
        }
    }

    /// Generate next deterministic private key derived from master key
    pub fn next_key(&mut self) -> Result<DerivedKey<K>, ByteArrayError> {
        self.primary_key_index += 1;
        self.derive_key(self.primary_key_index)
    }

    pub fn cipher_seed(&self) -> &CipherSeed {
        &self.seed
    }

    pub fn key_index(&self) -> u64 {
        self.primary_key_index
    }

    pub fn update_key_index(&mut self, new_index: u64) {
        self.primary_key_index = new_index;
    }
}

impl<K, D> Default for KeyManager<K, D>
where
    K: SecretKey,
    D: Digest + LengthExtensionAttackResistant,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::{hash::blake2::Blake256, ristretto::RistrettoSecretKey};

    use crate::key_manager::*;

    #[test]
    fn test_new_keymanager() {
        let km1 = KeyManager::<RistrettoSecretKey, Blake256>::new();
        let km2 = KeyManager::<RistrettoSecretKey, Blake256>::new();
        assert_ne!(km1.seed, km2.seed);
    }

    #[test]
    fn test_derive_and_next_key() {
        let mut km = KeyManager::<RistrettoSecretKey, Blake256>::new();
        let next_key1_result = km.next_key();
        let next_key2_result = km.next_key();
        let desired_key_index1 = 1;
        let desired_key_index2 = 2;
        let derived_key1_result = km.derive_key(desired_key_index1);
        let derived_key2_result = km.derive_key(desired_key_index2);
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

    #[test]
    fn test_derive_and_next_key_with_branch_seed() {
        let mut km = KeyManager::<RistrettoSecretKey, Blake256>::from(CipherSeed::new(), "Test".to_string(), 0);
        let next_key1_result = km.next_key();
        let next_key2_result = km.next_key();
        let desired_key_index1 = 1;
        let desired_key_index2 = 2;
        let derived_key1_result = km.derive_key(desired_key_index1);
        let derived_key2_result = km.derive_key(desired_key_index2);
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

    #[test]
    fn test_use_of_branch_seed() {
        let x = CipherSeed::new();
        let mut km1 = KeyManager::<RistrettoSecretKey, Blake256>::from(x.clone(), "some".to_string(), 0);
        let mut km2 = KeyManager::<RistrettoSecretKey, Blake256>::from(x, "other".to_string(), 0);
        let next_key1 = km1.next_key().unwrap();
        let next_key2 = km2.next_key().unwrap();
        assert_ne!(next_key1.k, next_key2.k);
    }
}

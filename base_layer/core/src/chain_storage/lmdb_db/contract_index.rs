//  Copyright 2022, The Tari Project
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

use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    hash::{BuildHasherDefault, Hash},
    ops::Deref,
};

use lmdb_zero::{traits::AsLmdbBytes, ConstTransaction, Database, WriteTransaction};
use log::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tari_common_types::types::{BlockHash, FixedHash};
use tari_utilities::{hex::to_hex, Hashable};

use crate::{
    chain_storage::{
        lmdb_db::{
            composite_key::CompositeKey,
            lmdb::{lmdb_delete, lmdb_exists, lmdb_fetch_matching_after, lmdb_get, lmdb_insert, lmdb_replace},
        },
        ChainStorageError,
    },
    transactions::transaction_components::{OutputType, TransactionInput, TransactionOutput},
};

const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb::contract_index";

/// The Contract Index.
///
/// The contract index keeps track all UTXOs related to a particular contract as given by
/// [OutputFeatures::contract_id](OutputFeatures::contract_id].
pub(super) struct ContractIndex<'a, T> {
    txn: &'a T,
    db: &'a Database<'a>,
}

/// A hash set using the DefaultHasher. Since output hashes are not user controlled and uniformly random there is no
/// need to use RandomState hasher.
type DefaultHashSet<T> = HashSet<T, BuildHasherDefault<DefaultHasher>>;
type FixedHashSet = DefaultHashSet<FixedHash>;
type ContractValueHashSet = DefaultHashSet<ContractIndexValue>;

impl<'a, T> ContractIndex<'a, T>
where T: Deref<Target = ConstTransaction<'a>>
{
    /// Create a new ContractIndex
    pub fn new(txn: &'a T, db: &'a Database<'a>) -> Self {
        Self { txn, db }
    }

    pub fn find_by_contract_id(
        &self,
        contract_id: FixedHash,
        output_type: OutputType,
    ) -> Result<Vec<FixedHash>, ChainStorageError> {
        let key = ContractIndexKey::new(contract_id, output_type);
        match output_type {
            OutputType::ContractAmendment |
            OutputType::ContractDefinition |
            OutputType::ContractCheckpoint |
            OutputType::ContractConstitution => Ok(self
                .get::<_, ContractIndexValue>(&key)?
                .into_iter()
                .map(|v| v.output_hash)
                .collect()),

            OutputType::ContractValidatorAcceptance |
            OutputType::ContractConstitutionProposal |
            OutputType::ContractConstitutionChangeAcceptance => Ok(self
                .get::<_, ContractValueHashSet>(&key)?
                .into_iter()
                .flatten()
                .map(|v| v.output_hash)
                .collect()),
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Cannot fetch output type {} from contract index",
                output_type
            ))),
        }
    }

    pub fn find_by_block(
        &self,
        block_hash: FixedHash,
        output_type: OutputType,
    ) -> Result<Vec<FixedHash>, ChainStorageError> {
        let key = BlockContractIndexKey::prefixed(block_hash, output_type);
        match output_type {
            OutputType::ContractDefinition |
            OutputType::ContractCheckpoint |
            OutputType::ContractConstitution |
            OutputType::ContractAmendment => self.get_all_matching::<_, FixedHash>(&key),

            OutputType::ContractValidatorAcceptance |
            OutputType::ContractConstitutionProposal |
            OutputType::ContractConstitutionChangeAcceptance => Ok(self
                .get_all_matching::<_, FixedHashSet>(&key)?
                .into_iter()
                .flatten()
                .collect()),
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Cannot fetch output type {} from contract index",
                output_type
            ))),
        }
    }

    fn get<K: AsLmdbBytes, V: DeserializeOwned>(&self, key: &K) -> Result<Option<V>, ChainStorageError> {
        lmdb_get(&**self.txn, self.db, key)
    }

    fn get_all_matching<K: AsLmdbBytes, V: DeserializeOwned>(&self, key: &K) -> Result<Vec<V>, ChainStorageError> {
        lmdb_fetch_matching_after(&**self.txn, self.db, key.as_lmdb_bytes())
    }

    fn exists<K: AsLmdbBytes>(&self, key: &K) -> Result<bool, ChainStorageError> {
        lmdb_exists(&**self.txn, self.db, key)
    }
}

impl<'a> ContractIndex<'a, WriteTransaction<'a>> {
    /// Called when a new output must be added to the index
    pub fn add_output(&self, block_hash: &BlockHash, output: &TransactionOutput) -> Result<(), ChainStorageError> {
        let block_hash = FixedHash::try_from(block_hash.as_slice())
            .map_err(|_| ChainStorageError::CriticalError("block_hash was not 32-bytes".to_string()))?;
        let output_hash = FixedHash::try_from(output.hash())
            .map_err(|_| ChainStorageError::CriticalError("output.hash() was not 32-bytes".to_string()))?;

        let contract_id = output.features.contract_id().ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "Attempt to add non-contract output with hash {} to contract index.",
                output_hash
            ))
        })?;
        self.add_to_index(block_hash, contract_id, output.features.output_type, output_hash)
    }

    /// Updates the index, removing references to the output that the given input spends.
    pub fn spend(&self, input: &TransactionInput) -> Result<(), ChainStorageError> {
        let output_hash = FixedHash::try_from(input.output_hash())
            .map_err(|_| ChainStorageError::CriticalError("input.output_hash() was not 32-bytes".to_string()))?;

        let features = input.features()?;
        let contract_id = features.contract_id().ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "Attempt to add non-contract output with hash {} to contract index.",
                output_hash
            ))
        })?;
        self.remove_from_index(contract_id, features.output_type, output_hash)
    }

    /// Updates the index, rewinding (undoing) the effect of the output on the index state.
    pub fn rewind_output(&self, output: &TransactionOutput) -> Result<(), ChainStorageError> {
        let output_hash = FixedHash::try_from(output.hash())
            .map_err(|_| ChainStorageError::CriticalError("output.hash() was not 32-bytes".to_string()))?;
        let features = &output.features;
        let contract_id = features.contract_id().ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "Attempt to add non-contract output with hash {} to contract index.",
                output_hash
            ))
        })?;
        self.remove_from_index(contract_id, features.output_type, output_hash)
    }

    /// Updates the index, rewinding (undoing) the effect of the input on the index state.
    pub fn rewind_input(&self, block_hash: &[u8], input: &TransactionInput) -> Result<(), ChainStorageError> {
        let block_hash = block_hash
            .try_into()
            .map_err(|_| ChainStorageError::CriticalError("block_hash was not 32-bytes".to_string()))?;

        let output_hash = input
            .output_hash()
            .try_into()
            .map_err(|_| ChainStorageError::CriticalError("input.output_hash() was not 32-bytes".to_string()))?;

        let features = input.features()?;
        let contract_id = features.contract_id().ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "Attempt to add non-contract input with hash {} to contract index.",
                output_hash
            ))
        })?;
        self.add_to_index(block_hash, contract_id, features.output_type, output_hash)
    }

    fn add_to_index(
        &self,
        block_hash: FixedHash,
        contract_id: FixedHash,
        output_type: OutputType,
        output_hash: FixedHash,
    ) -> Result<(), ChainStorageError> {
        let contract_key = ContractIndexKey::new(contract_id, output_type);
        let block_key = BlockContractIndexKey::new(block_hash, output_type, contract_id);
        match output_type {
            OutputType::ContractDefinition => {
                debug!(
                    target: LOG_TARGET,
                    "inserting index for new contract_id {} in output {}.", contract_id, output_hash
                );
                self.insert(&contract_key, &ContractIndexValue {
                    block_hash,
                    output_hash,
                })?;
                self.insert(&block_key, &output_hash)?;
                Ok(())
            },
            // Only one contract checkpoint and constitution can exist at a time and can be overwritten. Consensus rules
            // decide whether this is valid but we just assume this is valid here.
            OutputType::ContractAmendment | OutputType::ContractConstitution | OutputType::ContractCheckpoint => {
                self.assert_definition_exists(contract_id)?;
                self.set(&contract_key, &ContractIndexValue {
                    block_hash,
                    output_hash,
                })?;
                self.set(&block_key, &output_hash)?;
                Ok(())
            },
            // These are collections of output hashes
            OutputType::ContractValidatorAcceptance |
            OutputType::ContractConstitutionProposal |
            OutputType::ContractConstitutionChangeAcceptance => {
                self.assert_definition_exists(contract_id)?;
                self.add_to_set(&contract_key, ContractIndexValue {
                    block_hash,
                    output_hash,
                })?;
                self.add_to_set(&block_key, output_hash)?;
                Ok(())
            },
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Invalid output_type for contract UTXO: output_hash: {}, contract_id: {}, output_type: {}",
                output_hash, contract_id, output_type
            ))),
        }
    }

    fn remove_from_index(
        &self,
        contract_id: FixedHash,
        output_type: OutputType,
        output_hash: FixedHash,
    ) -> Result<(), ChainStorageError> {
        let contract_key = ContractIndexKey::new(contract_id, output_type);

        match output_type {
            OutputType::ContractDefinition => {
                if self.has_dependent_outputs(&contract_key)? {
                    return Err(ChainStorageError::UnspendableDueToDependentUtxos {
                        details: format!(
                            "Cannot deregister contract definition for contract {} because there are dependent outputs",
                            contract_id
                        ),
                    });
                }

                let contract = self.get_and_delete::<_, ContractIndexValue>(&contract_key)?;
                let block_key = BlockContractIndexKey::new(contract.block_hash, output_type, contract_id);
                self.delete(&block_key)?;
                Ok(())
            },
            OutputType::ContractAmendment | OutputType::ContractConstitution | OutputType::ContractCheckpoint => {
                let contract = self.get_and_delete::<_, ContractIndexValue>(&contract_key)?;
                let block_key = BlockContractIndexKey::new(contract.block_hash, output_type, contract_id);
                self.delete(&block_key)?;
                Ok(())
            },
            OutputType::ContractValidatorAcceptance | OutputType::ContractConstitutionProposal => {
                let contract = self.remove_from_contract_index(&contract_key, &output_hash)?;
                let block_key = BlockContractIndexKey::new(contract.block_hash, output_type, contract_id);
                self.remove_from_set(&block_key, &output_hash)?;
                Ok(())
            },
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Invalid output_type {} for contract {} for UTXO {}",
                output_type, contract_id, output_hash
            ))),
        }
    }

    fn add_to_set<K: AsLmdbBytes, V: Eq + Hash + Serialize + DeserializeOwned>(
        &self,
        key: &K,
        value: V,
    ) -> Result<(), ChainStorageError> {
        let mut hash_set = self.get::<_, DefaultHashSet<V>>(key)?.unwrap_or_default();
        if !hash_set.insert(value) {
            return Err(ChainStorageError::InvalidOperation(format!(
                "UTXO with has already been added to contract index at key {}",
                to_hex(key.as_lmdb_bytes())
            )));
        }

        self.set(key, &hash_set)?;
        Ok(())
    }

    fn remove_from_contract_index(
        &self,
        key: &ContractIndexKey,
        output_hash: &FixedHash,
    ) -> Result<ContractIndexValue, ChainStorageError> {
        let mut hash_set = self.get::<_, ContractValueHashSet>(key)?.unwrap_or_default();
        let value = hash_set
            .iter()
            .find(|v| v.output_hash == *output_hash)
            .cloned()
            .ok_or_else(|| {
                ChainStorageError::InvalidOperation(format!(
                    "Contract output was not found in UTXO set with key {}",
                    to_hex(key.as_lmdb_bytes())
                ))
            })?;
        hash_set.remove(&value);
        if hash_set.is_empty() {
            self.delete(key)?;
        } else {
            self.set(key, &hash_set)?;
        }
        Ok(value)
    }

    fn remove_from_set<K: AsLmdbBytes, V: Eq + Hash + Serialize + DeserializeOwned>(
        &self,
        key: &K,
        value: &V,
    ) -> Result<(), ChainStorageError> {
        let mut hash_set = self.get::<_, DefaultHashSet<V>>(key)?.unwrap_or_default();
        if !hash_set.remove(value) {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Contract output was not found in UTXO set with key {}",
                to_hex(key.as_lmdb_bytes())
            )));
        }
        if hash_set.is_empty() {
            self.delete(key)?;
        } else {
            self.set(key, &hash_set)?;
        }
        Ok(())
    }

    fn has_dependent_outputs(&self, key: &ContractIndexKey) -> Result<bool, ChainStorageError> {
        let constitution_key = key.to_key_with_output_type(OutputType::ContractConstitution);
        if self.exists(&constitution_key)? {
            return Ok(true);
        }
        let checkpoint_key = key.to_key_with_output_type(OutputType::ContractCheckpoint);
        if self.exists(&checkpoint_key)? {
            return Ok(true);
        }
        Ok(false)
    }

    fn assert_definition_exists(&self, contract_id: FixedHash) -> Result<(), ChainStorageError> {
        let key = ContractIndexKey::new(contract_id, OutputType::ContractDefinition);
        if self.exists(&key)? {
            Ok(())
        } else {
            Err(ChainStorageError::InvalidOperation(format!(
                "No contract definition for contract id {}",
                contract_id
            )))
        }
    }

    fn insert<K: AsLmdbBytes + Debug, V: Serialize + Debug>(
        &self,
        key: &K,
        value: &V,
    ) -> Result<(), ChainStorageError> {
        lmdb_insert(self.txn, self.db, key, value, "contract_index")
    }

    fn set<K: AsLmdbBytes, V: Serialize>(&self, key: &K, value: &V) -> Result<(), ChainStorageError> {
        lmdb_replace(self.txn, self.db, key, value)
    }

    fn delete<K: AsLmdbBytes>(&self, key: &K) -> Result<(), ChainStorageError> {
        lmdb_delete(self.txn, self.db, key, "contract_index")
    }

    fn get_and_delete<K: AsLmdbBytes, V: DeserializeOwned>(&self, key: &K) -> Result<V, ChainStorageError> {
        let value = self.get(key)?.ok_or_else(|| ChainStorageError::ValueNotFound {
            entity: "contract_index",
            field: "<unknown>",
            value: to_hex(key.as_lmdb_bytes()),
        })?;
        self.delete(key)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
struct ContractIndexValue {
    pub block_hash: FixedHash,
    pub output_hash: FixedHash,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum KeyType {
    PerContract = 0,
    PerBlock = 1,
}

/// An index key constisting of {block_hash, output_type, contract_id}.
#[derive(Debug, Clone, Copy)]
struct BlockContractIndexKey {
    key: CompositeKey<{ Self::KEY_LEN }>,
}

impl BlockContractIndexKey {
    const KEY_LEN: usize = 1 + 32 + 1 + 32;

    pub fn new(block_hash: FixedHash, output_type: OutputType, contract_id: FixedHash) -> Self {
        let mut key = Self::prefixed(block_hash, output_type);
        assert!(key.key.push(&contract_id));
        key
    }

    pub fn prefixed(block_hash: FixedHash, output_type: OutputType) -> Self {
        let mut key = CompositeKey::new();
        assert!(key.push(&[KeyType::PerBlock as u8]));
        assert!(key.push(&block_hash));
        assert!(key.push(&[output_type.as_byte()]));
        Self { key }
    }
}

impl Deref for BlockContractIndexKey {
    type Target = CompositeKey<{ Self::KEY_LEN }>;

    fn deref(&self) -> &Self::Target {
        &self.key
    }
}

impl AsLmdbBytes for BlockContractIndexKey {
    fn as_lmdb_bytes(&self) -> &[u8] {
        &self.key
    }
}

/// An index key constisting of {contract_id, output_type}.
#[derive(Debug, Clone, Copy)]
struct ContractIndexKey {
    key: CompositeKey<{ Self::KEY_LEN }>,
}

impl ContractIndexKey {
    const KEY_LEN: usize = 1 + 32 + 1;

    pub fn new(contract_id: FixedHash, output_type: OutputType) -> Self {
        let mut key = CompositeKey::new();
        assert!(key.push(&[KeyType::PerContract as u8]));
        assert!(key.push(&*contract_id));
        assert!(key.push(&[output_type.as_byte()]));
        Self { key }
    }

    pub fn to_key_with_output_type(self, output_type: OutputType) -> Self {
        let mut key = self;
        key.key[FixedHash::byte_size() + 1] = output_type.as_byte();
        key
    }
}

impl Deref for ContractIndexKey {
    type Target = CompositeKey<{ Self::KEY_LEN }>;

    fn deref(&self) -> &Self::Target {
        &self.key
    }
}

impl AsLmdbBytes for ContractIndexKey {
    fn as_lmdb_bytes(&self) -> &[u8] {
        &self.key
    }
}

#[cfg(test)]
mod tests {
    use digest::Digest;
    use tari_common_types::types::HashDigest;

    use super::*;
    mod contract_index_key {
        use super::*;

        #[test]
        fn it_represents_a_well_formed_contract_index_key() {
            let hash = HashDigest::new().chain(b"foobar").finalize().into();
            let key = ContractIndexKey::new(hash, OutputType::ContractCheckpoint);
            assert_eq!(key.as_lmdb_bytes()[0], KeyType::PerContract as u8);
            assert_eq!(key.as_lmdb_bytes()[1..33], *hash.as_slice());
            assert_eq!(
                OutputType::from_byte(key.as_lmdb_bytes()[33]).unwrap(),
                OutputType::ContractCheckpoint
            );
        }
    }
}

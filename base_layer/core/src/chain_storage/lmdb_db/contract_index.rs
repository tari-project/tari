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
    fmt::{Debug, Display, Formatter},
    hash::BuildHasherDefault,
    ops::Deref,
};

use lmdb_zero::{traits::AsLmdbBytes, ConstTransaction, Database, WriteTransaction};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use tari_common_types::types::FixedHash;
use tari_utilities::{hex::to_hex, Hashable};

use crate::{
    chain_storage::{
        lmdb_db::lmdb::{lmdb_delete, lmdb_exists, lmdb_get, lmdb_insert, lmdb_replace},
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

impl<'a, T> ContractIndex<'a, T>
where T: Deref<Target = ConstTransaction<'a>>
{
    /// Create a new ContractIndex
    pub fn new(txn: &'a T, db: &'a Database<'a>) -> Self {
        Self { txn, db }
    }

    pub fn fetch(&self, contract_id: FixedHash, output_type: OutputType) -> Result<Vec<FixedHash>, ChainStorageError> {
        let key = ContractIndexKey::new(contract_id, output_type);

        match output_type {
            OutputType::ContractDefinition | OutputType::ContractCheckpoint | OutputType::ContractConstitution => {
                Ok(self.find::<FixedHash>(&key)?.into_iter().collect())
            },
            OutputType::ContractValidatorAcceptance |
            OutputType::ContractConstitutionProposal |
            OutputType::ContractConstitutionChangeAcceptance => {
                Ok(self.find::<FixedHashSet>(&key)?.into_iter().flatten().collect())
            },
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Cannot fetch output type {} from contract index",
                output_type
            ))),
        }
    }

    fn find<V: DeserializeOwned>(&self, key: &ContractIndexKey) -> Result<Option<V>, ChainStorageError> {
        lmdb_get(&**self.txn, self.db, key)
    }

    fn exists(&self, key: &ContractIndexKey) -> Result<bool, ChainStorageError> {
        lmdb_exists(&**self.txn, self.db, key)
    }
}

impl<'a> ContractIndex<'a, WriteTransaction<'a>> {
    /// Called when a new output must be added to the index
    pub fn add_output(&self, output: &TransactionOutput) -> Result<(), ChainStorageError> {
        let output_hash = FixedHash::try_from(output.hash())
            .map_err(|_| ChainStorageError::CriticalError("output.hash() was not 32-bytes".to_string()))?;

        let contract_id = output.features.contract_id().ok_or_else(|| {
            ChainStorageError::InvalidOperation(format!(
                "Attempt to add non-contract output with hash {} to contract index.",
                output_hash
            ))
        })?;
        self.add_to_index(contract_id, output.features.output_type, output_hash)
    }

    /// Updates the index, removing references to the output that the given input spends.
    pub fn spend(&self, input: &TransactionInput) -> Result<(), ChainStorageError> {
        let output_hash = FixedHash::try_from(input.output_hash()).unwrap();
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
        let output_hash = FixedHash::try_from(output.hash()).unwrap();
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
    pub fn rewind_input(&self, input: &TransactionInput) -> Result<(), ChainStorageError> {
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
        self.add_to_index(contract_id, features.output_type, output_hash)
    }

    fn add_to_index(
        &self,
        contract_id: FixedHash,
        output_type: OutputType,
        output_hash: FixedHash,
    ) -> Result<(), ChainStorageError> {
        let key = ContractIndexKey::new(contract_id, output_type);
        match output_type {
            OutputType::ContractDefinition => {
                debug!(
                    target: LOG_TARGET,
                    "inserting index for new contract_id {} in output {}.", contract_id, output_hash
                );
                self.insert(&key, &output_hash)?;

                Ok(())
            },
            // Only one contract checkpoint and constitution can exist at a time and can be overwritten. Consensus rules
            // decide whether this is valid but we just assume this is valid here.
            OutputType::ContractConstitution | OutputType::ContractCheckpoint => {
                self.assert_definition_exists(contract_id)?;
                self.set(&key, &*output_hash)?;
                Ok(())
            },
            // These are collections of output hashes
            OutputType::ContractValidatorAcceptance |
            OutputType::ContractConstitutionProposal |
            OutputType::ContractAmendment => {
                self.assert_definition_exists(contract_id)?;
                let mut hashes = self.find::<FixedHashSet>(&key)?.unwrap_or_default();

                if !hashes.insert(output_hash) {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "{} UTXO for contract {} with hash {} has already been added to index",
                        output_type, contract_id, output_hash
                    )));
                }

                self.set(&key, &hashes)?;
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
        let key = ContractIndexKey::new(contract_id, output_type);

        match output_type {
            OutputType::ContractDefinition => {
                if self.has_dependent_outputs(&key)? {
                    return Err(ChainStorageError::UnspendableDueToDependentUtxos {
                        details: format!(
                            "Cannot deregister contract definition for contract {} because there are dependent outputs",
                            contract_id
                        ),
                    });
                }

                self.delete(&key)?;
                Ok(())
            },
            OutputType::ContractConstitution | OutputType::ContractCheckpoint => {
                self.delete(&key)?;
                Ok(())
            },
            OutputType::ContractValidatorAcceptance | OutputType::ContractConstitutionProposal => {
                let mut hash_set = self.find::<FixedHashSet>(&key)?.unwrap_or_default();
                if !hash_set.remove(&output_hash) {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "Output {} was not found in {} UTXO set for contract_id {}",
                        output_hash, output_type, contract_id
                    )));
                }
                if hash_set.is_empty() {
                    self.delete(&key)?;
                } else {
                    self.set(&key, &hash_set)?;
                }
                Ok(())
            },
            _ => Err(ChainStorageError::InvalidOperation(format!(
                "Invalid output_type {} for contract {} for UTXO {}",
                output_type, contract_id, output_hash
            ))),
        }
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

    fn insert<V: Serialize + Debug>(&self, key: &ContractIndexKey, value: &V) -> Result<(), ChainStorageError> {
        lmdb_insert(self.txn, self.db, key, value, "contract_index")
    }

    fn set<V: Serialize>(&self, key: &ContractIndexKey, value: &V) -> Result<(), ChainStorageError> {
        lmdb_replace(self.txn, self.db, key, value)
    }

    fn delete(&self, key: &ContractIndexKey) -> Result<(), ChainStorageError> {
        lmdb_delete(self.txn, self.db, key, "contract_index")
    }
}

/// A hash set using the DefaultHasher. Since output hashes are not user controlled and uniformly random there is no
/// need to use RandomState hasher.
type FixedHashSet = HashSet<FixedHash, BuildHasherDefault<DefaultHasher>>;

/// A 33-byte contract ID index key.
///
/// The first 32-bytes are the contract ID, the next byte is the `OutputType`.
#[derive(Debug, Clone, Copy)]
pub(self) struct ContractIndexKey {
    bytes: [u8; Self::FULL_KEY_LEN],
}

impl ContractIndexKey {
    const FULL_KEY_LEN: usize = FixedHash::byte_size() + 1;

    pub fn new(contract_id: FixedHash, output_type: OutputType) -> Self {
        Self {
            bytes: Self::bytes_from_parts(contract_id, output_type),
        }
    }

    pub fn to_key_with_output_type(self, output_type: OutputType) -> Self {
        let mut key = self;
        key.bytes[FixedHash::byte_size()] = output_type.as_byte();
        key
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..]
    }

    fn bytes_from_parts(contract_id: FixedHash, output_type: OutputType) -> [u8; Self::FULL_KEY_LEN] {
        let mut buf = Self::new_buf();
        buf[..FixedHash::byte_size()].copy_from_slice(&*contract_id);
        buf[FixedHash::byte_size()] = output_type.as_byte();
        buf
    }

    /// Returns a fixed 0-filled byte array.
    const fn new_buf() -> [u8; Self::FULL_KEY_LEN] {
        [0x0u8; Self::FULL_KEY_LEN]
    }
}

impl AsLmdbBytes for ContractIndexKey {
    fn as_lmdb_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Display for ContractIndexKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_hex(self.as_bytes()))
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
            assert_eq!(key.as_lmdb_bytes()[..32], *hash.as_slice());
            assert_eq!(
                OutputType::from_byte(key.as_lmdb_bytes()[32]).unwrap(),
                OutputType::ContractCheckpoint
            );
        }
    }
}

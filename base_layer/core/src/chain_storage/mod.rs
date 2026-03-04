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

//! This module is responsible for handling logic responsible for storing the blockchain state.
//!
//! It is structured in such a way that clients (e.g. base nodes) can configure the various components of the state
//! (kernels, utxos, etc) in whichever way they like. It's possible to have the UTXO set in memory, and the kernels
//! backed by LMDB, while the merkle trees are stored in flat files for example.

#[cfg(test)]
mod tests;

pub mod async_db;

mod block_add_result;

pub use block_add_result::BlockAddResult;
use primitive_types::U512;
use serde::{Deserialize, Serialize};

mod blockchain_database;
pub use blockchain_database::{
    calculate_mmr_roots,
    calculate_validator_node_mr,
    fetch_header,
    fetch_headers,
    fetch_target_difficulty_for_next_block,
    BlockchainDatabase,
    BlockchainDatabaseConfig,
    MmrRoots,
    Validators,
};
mod blockchain_backend;
pub use blockchain_backend::BlockchainBackend;
mod consts;
mod db_transaction;
pub use db_transaction::{DbKey, DbTransaction, DbValue, WriteOperation};
mod mmr_tree;
pub use mmr_tree::MmrTree;
mod error;
pub use error::{ChainStorageError, Optional, OrNotFound};
mod horizon_data;
pub use horizon_data::HorizonData;
mod reorg;
pub use reorg::Reorg;
mod lmdb_db;
pub use lmdb_db::{
    create_lmdb_database,
    create_lmdb_database_with_stats_channel,
    create_readonly_lmdb_environment,
    create_recovery_lmdb_database,
    get_all_database_names,
    lmdb_tree_reader::{LmdbTreeReader, OwnedLmdbTreeReader},
    AccumulatedDataRebuildStatus,
    BlockchainCheckRequest,
    BlockchainCheckStatus,
    CheckFailure,
    DatabaseStats,
    LMDBDatabase,
    PayrefRebuildStatus,
};
mod stats;
pub use stats::{DbBasicStats, DbSize, DbStat, DbTotalSizeStats};
mod target_difficulties;
mod utxo_mined_info;
pub use target_difficulties::TargetDifficulties;
pub use utxo_mined_info::*;
mod active_validator_node;
pub use active_validator_node::ValidatorNodeEntry;
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, HashOutput},
};
mod template_registation;
pub use template_registation::TemplateRegistrationEntry;
mod kernel_merkle_proof;
pub use kernel_merkle_proof::*;
mod smt_hasher;

pub use smt_hasher::SmtHasher;
use tari_transaction_components::{transaction_components::ValidatorNodeRegistration, MicroMinotari};

/// Tip data for an orphan chain, stored in LMDB.
///
/// # Serialization format
///
/// Fields are serialized as an ordered tuple using bincode. **The field order must not change** —
/// reordering is a breaking schema change that requires a migration.
///
/// | # | Field                         | Bincode bytes                              |
/// |---|-------------------------------|--------------------------------------------|
/// | 0 | `hash`                        | 32 bytes (FixedHash / `[u8; 32]`)          |
/// | 1 | `total_accumulated_difficulty`| 8-byte length prefix + 64 bytes (U512 LE) |
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChainTipData {
    pub hash: HashOutput,
    pub total_accumulated_difficulty: U512,
}

impl Serialize for ChainTipData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        // IMPORTANT: The serialization order of fields below is part of the LMDB schema.
        // DO NOT reorder — it is a breaking schema change.
        //
        // `U512` is from an external crate (`primitive_types`). We explicitly serialize it as 64
        // little-endian bytes to avoid depending on that crate's own serde implementation.
        let mut tup = s.serialize_tuple(2)?;
        tup.serialize_element(&self.hash)?;
        {
            let mut le_bytes = [0u8; 64];
            self.total_accumulated_difficulty.to_little_endian(&mut le_bytes);
            tup.serialize_element(le_bytes.as_slice())?;
        }
        tup.end()
    }
}

impl<'de> Deserialize<'de> for ChainTipData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, SeqAccess, Visitor};
        use std::fmt;

        struct ChainTipDataVisitor;

        impl<'de> Visitor<'de> for ChainTipDataVisitor {
            type Value = ChainTipData;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a tuple of 2 elements for ChainTipData")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"2 fields"))?;
                let le_bytes: Vec<u8> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"2 fields"))?;
                if le_bytes.len() != 64 {
                    return Err(de::Error::custom(format!(
                        "expected 64 bytes for U512, got {}",
                        le_bytes.len()
                    )));
                }
                let total_accumulated_difficulty = U512::from_little_endian(&le_bytes);
                Ok(ChainTipData {
                    hash,
                    total_accumulated_difficulty,
                })
            }
        }

        d.deserialize_tuple(2, ChainTipDataVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ValidatorNodeRegistrationInfo {
    pub public_key: CompressedPublicKey,
    pub sidechain_id: Option<CompressedPublicKey>,
    pub shard_key: [u8; 32],
    pub activation_epoch: VnEpoch,
    pub original_registration: ValidatorNodeRegistration,
    pub minimum_value_promise: MicroMinotari,
}

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

mod blockchain_database;
pub use blockchain_database::{
    calculate_mmr_roots,
    fetch_header,
    fetch_headers,
    fetch_tip_header,
    BlockAddResult,
    BlockchainBackend,
    BlockchainDatabase,
    BlockchainDatabaseConfig,
    MutableMmrState,
    Validators,
};

mod consts;

mod db_transaction;
pub use db_transaction::{
    DbKey,
    DbKeyValuePair,
    DbTransaction,
    DbValue,
    MetadataKey,
    MetadataValue,
    MmrTree,
    WriteOperation,
};

// mod entity;

mod error;
pub use error::ChainStorageError;

mod historical_block;
pub use historical_block::HistoricalBlock;

mod lmdb_db;
pub use lmdb_db::{
    create_lmdb_database,
    create_recovery_lmdb_database,
    remove_lmdb_database,
    LMDBDatabase,
    LMDB_DB_BLOCK_HASHES,
    LMDB_DB_HEADERS,
    LMDB_DB_KERNELS,
    LMDB_DB_METADATA,
    LMDB_DB_ORPHANS,
    LMDB_DB_UTXOS,
};

use croaring::Bitmap;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::fmt;
use tari_common_types::chain_metadata::ChainMetadata;
pub mod horizon_sync_state;
pub use horizon_sync_state::InProgressHorizonSyncState;
use tari_mmr::pruned_hashset::PrunedHashSet;

pub mod async_db;

#[derive(Debug)]
pub struct BlockAccumulatedData {
    kernels: PrunedHashSet,
    outputs: PrunedHashSet,
    // TODO: Remove this pub
    pub deleted: Bitmap,
    range_proofs: PrunedHashSet,
    total_kernel_sum: Commitment,
    total_utxo_sum: Commitment,
}

impl BlockAccumulatedData {
    pub fn new(
        kernels: PrunedHashSet,
        outputs: PrunedHashSet,
        range_proofs: PrunedHashSet,
        deleted: Bitmap,
        total_kernel_sum: Commitment,
        total_utxo_sum: Commitment,
    ) -> Self
    {
        Self {
            kernels,
            outputs,
            range_proofs,
            deleted,
            total_kernel_sum,
            total_utxo_sum,
        }
    }

    pub fn deleted(&self) -> &Bitmap {
        &self.deleted
    }
}

impl Serialize for BlockAccumulatedData {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where S: Serializer {
        let mut s = serializer.serialize_struct("MmrPeakData", 6)?;
        s.serialize_field("kernels", &self.kernels)?;
        s.serialize_field("outputs", &self.outputs)?;
        s.serialize_field("deleted", &self.deleted.serialize())?;
        s.serialize_field("range_proofs", &self.range_proofs)?;
        s.serialize_field("total_kernel_sum", &self.total_kernel_sum)?;
        s.serialize_field("total_utxo_sum", &self.total_utxo_sum)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for BlockAccumulatedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where D: Deserializer<'de> {
        const FIELDS: &[&str] = &[
            "kernels",
            "outputs",
            "deleted",
            "range_proofs",
            "total_kernel_sum",
            "total_utxo_sum",
        ];

        deserializer.deserialize_struct("MmrPeakData", FIELDS, BlockAccumulatedDataVisitor)
    }
}

struct BlockAccumulatedDataVisitor;

use crate::transactions::types::{BlindingFactor, Commitment, HashOutput};
use serde::de;

impl<'de> Visitor<'de> for BlockAccumulatedDataVisitor {
    type Value = BlockAccumulatedData;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("`kernels`, `outputs`, `deleted`,`range_proofs`,`total_kernel_sum` or `total_utxo_sum`")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
    where V: SeqAccess<'de> {
        let kernels = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let outputs = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let deleted: Vec<u8> = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
        let range_proofs = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(3, &self))?;
        let total_kernel_sum = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(4, &self))?;
        let total_utxo_sum = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(5, &self))?;
        Ok(BlockAccumulatedData {
            kernels,
            outputs,
            deleted: Bitmap::deserialize(&deleted),
            range_proofs,
            total_kernel_sum,
            total_utxo_sum,
        })
    }

    fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
    where V: MapAccess<'de> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Kernels,
            Outputs,
            Deleted,
            RangeProofs,
            TotalKernelSum,
            TotalUtxoSum,
        };
        let mut kernels = None;
        let mut outputs = None;
        let mut deleted = None;
        let mut range_proofs = None;
        let mut total_kernel_sum = None;
        let mut total_utxo_sum = None;
        while let Some(key) = map.next_key()? {
            match key {
                Field::Kernels => {
                    if kernels.is_some() {
                        return Err(de::Error::duplicate_field("kernels"));
                    }
                    kernels = Some(map.next_value()?);
                },
                Field::Outputs => {
                    if outputs.is_some() {
                        return Err(de::Error::duplicate_field("outputs"));
                    }
                    outputs = Some(map.next_value()?);
                },
                Field::Deleted => {
                    if deleted.is_some() {
                        return Err(de::Error::duplicate_field("deleted"));
                    }
                    deleted = Some(map.next_value()?);
                },
                Field::RangeProofs => {
                    if range_proofs.is_some() {
                        return Err(de::Error::duplicate_field("range_proofs"));
                    }
                    range_proofs = Some(map.next_value()?);
                },
                Field::TotalKernelSum => {
                    if total_kernel_sum.is_some() {
                        return Err(de::Error::duplicate_field("total_kernel_sum"));
                    }
                    total_kernel_sum = Some(map.next_value()?);
                },
                Field::TotalUtxoSum => {
                    if total_utxo_sum.is_some() {
                        return Err(de::Error::duplicate_field("total_utxo_sum"));
                    }
                    total_utxo_sum = Some(map.next_value()?);
                },
            }
        }
        let kernels = kernels.ok_or_else(|| de::Error::missing_field("kernels"))?;
        let outputs = outputs.ok_or_else(|| de::Error::missing_field("outputs"))?;
        let deleted: Vec<u8> = deleted.ok_or_else(|| de::Error::missing_field("deleted"))?;
        let range_proofs = range_proofs.ok_or_else(|| de::Error::missing_field("range_proofs"))?;
        let total_kernel_sum = total_kernel_sum.ok_or_else(|| de::Error::missing_field("total_kernel_sum"))?;
        let total_utxo_sum = total_utxo_sum.ok_or_else(|| de::Error::missing_field("total_utxo_sum"))?;

        Ok(BlockAccumulatedData {
            kernels,
            outputs,
            deleted: Bitmap::deserialize(&deleted),
            range_proofs,
            total_kernel_sum,
            total_utxo_sum,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlockHeaderAccumulatedData {
    pub hash: HashOutput,
    pub total_kernel_offset: BlindingFactor,
}

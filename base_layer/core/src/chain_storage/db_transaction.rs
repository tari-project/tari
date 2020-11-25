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
    blocks::{Block, BlockHeader},
    chain_storage::{
        error::ChainStorageError,
        BlockAccumulatedData,
        BlockHeaderAccumulatedData,
        InProgressHorizonSyncState,
    },
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use strum_macros::Display;
use tari_common_types::types::BlockHash;
use tari_crypto::tari_utilities::{
    hex::{to_hex, Hex},
    Hashable,
};

#[derive(Debug)]
pub struct DbTransaction {
    pub operations: Vec<WriteOperation>,
}

impl Display for DbTransaction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("Db transaction: \n")?;
        for write_op in &self.operations {
            fmt.write_str(&format!("{}\n", write_op))?;
        }
        Ok(())
    }
}

impl Default for DbTransaction {
    fn default() -> Self {
        DbTransaction {
            operations: Vec::with_capacity(128),
        }
    }
}

impl DbTransaction {
    /// Creates a new Database transaction. To commit the transactions call [BlockchainDatabase::execute] with the
    /// transaction as a parameter.
    pub fn new() -> Self {
        DbTransaction::default()
    }

    /// A general insert request. There are convenience functions for specific insert queries.
    pub fn insert(&mut self, insert: DbKeyValuePair) {
        self.operations.push(WriteOperation::Insert(insert));
    }

    /// Set a metadata entry
    pub fn set_metadata(&mut self, key: MetadataKey, value: MetadataValue) {
        self.insert(DbKeyValuePair::Metadata(key, value));
    }

    /// A general insert request. There are convenience functions for specific delete queries.
    pub fn delete(&mut self, delete: DbKey) {
        self.operations.push(WriteOperation::Delete(delete));
    }

    /// Delete a block
    pub fn delete_block(&mut self, block_hash: HashOutput) {
        self.operations.push(WriteOperation::DeleteBlock(block_hash));
    }

    /// Inserts a transaction kernel into the current transaction.
    pub fn insert_kernel(&mut self, kernel: TransactionKernel, header_hash: HashOutput, mmr_position: u32) {
        self.operations.push(WriteOperation::InsertKernel {
            header_hash,
            kernel: Box::new(kernel),
            mmr_position,
        });
    }

    /// Inserts a block header into the current transaction.
    pub fn insert_header(&mut self, header: BlockHeader) {
        let height = header.height;
        self.insert(DbKeyValuePair::BlockHeader(height, Box::new(header)));
    }

    /// Insert header accumulated data
    pub fn insert_header_accumulated_data(&mut self, accumulated_data: BlockHeaderAccumulatedData) {
        self.operations
            .push(WriteOperation::InsertHeaderAccumulatedData(Box::new(accumulated_data)))
    }

    /// Adds a UTXO into the current transaction and update the TXO MMR.
    pub fn insert_utxo(&mut self, utxo: TransactionOutput, header_hash: HashOutput, mmr_leaf_index: u32) {
        self.operations.push(WriteOperation::InsertOutput {
            header_hash,
            output: Box::new(utxo),
            mmr_position: mmr_leaf_index,
        });
    }

    pub fn insert_input(&mut self, input: TransactionInput, header_hash: HashOutput, mmr_leaf_index: u32) {
        self.operations.push(WriteOperation::InsertInput {
            header_hash,
            input: Box::new(input),
            mmr_position: mmr_leaf_index,
        });
    }

    /// Stores an orphan block. No checks are made as to whether this is actually an orphan. That responsibility lies
    /// with the calling function.
    pub fn insert_orphan(&mut self, orphan: Arc<Block>) {
        let hash = orphan.hash();
        self.insert(DbKeyValuePair::OrphanBlock(hash, orphan));
    }

    /// Remove an orphan from the orphan tip set
    pub fn remove_orphan_chain_tip(&mut self, hash: HashOutput) {
        self.operations.push(WriteOperation::DeleteOrphanChainTip(hash))
    }

    /// Add an orphan to the orphan tip set
    pub fn insert_orphan_chain_tip(&mut self, hash: HashOutput) {
        self.operations.push(WriteOperation::InsertOrphanChainTip(hash))
    }

    /// Set block accumulated data
    pub fn set_block_accumulated_data(&mut self, header_hash: HashOutput, data: BlockAccumulatedData) {
        self.operations
            .push(WriteOperation::UpdateBlockAccumulatedData(header_hash, data));
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WriteOperation {
    Insert(DbKeyValuePair),
    InsertInput {
        header_hash: HashOutput,
        input: Box<TransactionInput>,
        mmr_position: u32,
    },
    InsertKernel {
        header_hash: HashOutput,
        kernel: Box<TransactionKernel>,
        mmr_position: u32,
    },
    InsertOutput {
        header_hash: HashOutput,
        output: Box<TransactionOutput>,
        mmr_position: u32,
    },
    InsertHeaderAccumulatedData(Box<BlockHeaderAccumulatedData>),
    Delete(DbKey),
    DeleteBlock(HashOutput),
    UpdateBlockAccumulatedData(HashOutput, BlockAccumulatedData),
    DeleteOrphanChainTip(HashOutput),
    InsertOrphanChainTip(HashOutput),
}

impl fmt::Display for WriteOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use WriteOperation::*;
        match self {
            Insert(pair) => write!(f, "Insert({})", pair),
            InsertKernel {
                header_hash,
                kernel,
                mmr_position,
            } => write!(
                f,
                "Insert kernel {} in block:{} position: {}",
                kernel.hash().to_hex(),
                header_hash.to_hex(),
                mmr_position
            ),
            InsertOutput {
                header_hash,
                output,
                mmr_position,
            } => write!(
                f,
                "Insert output {} in block:{} position: {}",
                output.hash().to_hex(),
                header_hash.to_hex(),
                mmr_position
            ),
            InsertInput {
                header_hash,
                input,
                mmr_position,
            } => write!(
                f,
                "Insert input {} in block: {} position: {}",
                input.hash().to_hex(),
                header_hash.to_hex(),
                mmr_position
            ),
            Delete(key) => write!(f, "Delete({})", key),
            UpdateBlockAccumulatedData(header_hash, _) => {
                write!(f, "UpdateBlockAccumulatedData({})", header_hash.to_hex())
            },
            DeleteOrphanChainTip(hash) => write!(f, "DeleteOrphanChainTip({})", hash.to_hex()),
            InsertOrphanChainTip(hash) => write!(f, "InsertOrphanChainTip({})", hash.to_hex()),
            DeleteBlock(hash) => write!(f, "DeleteBlock({})", hash.to_hex()),
            InsertHeaderAccumulatedData(data) => write!(f, "InsertHeaderAccumulatedData({})", data.hash.to_hex()),
        }
    }
}

/// A list of key-value pairs that are required for each insert operation
#[derive(Debug, Display)]
pub enum DbKeyValuePair {
    Metadata(MetadataKey, MetadataValue),
    BlockHeader(u64, Box<BlockHeader>),
    OrphanBlock(HashOutput, Arc<Block>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Copy)]
pub enum MmrTree {
    Utxo,
    Kernel,
    RangeProof,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum MetadataKey {
    ChainHeight,
    BestBlock,
    AccumulatedWork,
    PruningHorizon,
    EffectivePrunedHeight,
    HorizonSyncState,
}

impl fmt::Display for MetadataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataKey::ChainHeight => f.write_str("Current chain height"),
            MetadataKey::AccumulatedWork => f.write_str("Total accumulated work"),
            MetadataKey::PruningHorizon => f.write_str("Pruning horizon"),
            MetadataKey::EffectivePrunedHeight => f.write_str("Effective pruned height"),
            MetadataKey::BestBlock => f.write_str("Chain tip block hash"),
            MetadataKey::HorizonSyncState => f.write_str("Database info"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum MetadataValue {
    ChainHeight(u64),
    BestBlock(BlockHash),
    AccumulatedWork(u128),
    PruningHorizon(u64),
    EffectivePrunedHeight(u64),
    HorizonSyncState(InProgressHorizonSyncState),
}

impl fmt::Display for MetadataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataValue::ChainHeight(h) => write!(f, "Chain height is {}", h),
            MetadataValue::AccumulatedWork(d) => write!(f, "Total accumulated work is {}", d),
            MetadataValue::PruningHorizon(h) => write!(f, "Pruning horizon is {}", h),
            MetadataValue::EffectivePrunedHeight(h) => write!(f, "Effective pruned height is {}", h),
            MetadataValue::BestBlock(hash) => write!(f, "Chain tip block hash is {}", hash.to_hex()),
            MetadataValue::HorizonSyncState(state) => write!(f, "Horizon state sync in progress: {}", state),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    Metadata(MetadataKey),
    BlockHeader(u64),
    BlockHash(BlockHash),
    TransactionKernel(HashOutput),
    OrphanBlock(HashOutput),
}

impl DbKey {
    pub fn to_value_not_found_error(&self) -> ChainStorageError {
        let (entity, field, value) = match self {
            DbKey::Metadata(key) => ("MetaData".to_string(), key.to_string(), "".to_string()),
            DbKey::BlockHeader(v) => ("BlockHeader".to_string(), "Height".to_string(), v.to_string()),
            DbKey::BlockHash(v) => ("Block".to_string(), "Hash".to_string(), v.to_hex()),
            DbKey::TransactionKernel(v) => ("Kernel".to_string(), "Hash".to_string(), v.to_hex()),
            DbKey::OrphanBlock(v) => ("Orphan".to_string(), "Hash".to_string(), v.to_hex()),
        };
        ChainStorageError::ValueNotFound { entity, field, value }
    }
}

#[derive(Debug)]
pub enum DbValue {
    Metadata(MetadataValue),
    BlockHeader(Box<BlockHeader>),
    BlockHash(Box<BlockHeader>),
    UnspentOutput(Box<TransactionOutput>),
    TransactionKernel(Box<TransactionKernel>),
    OrphanBlock(Box<Block>),
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Metadata(v) => v.fmt(f),
            DbValue::BlockHeader(_) => f.write_str("Block header"),
            DbValue::BlockHash(_) => f.write_str("Block hash"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent output"),
            DbValue::TransactionKernel(_) => f.write_str("Transaction kernel"),
            DbValue::OrphanBlock(_) => f.write_str("Orphan block"),
        }
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::Metadata(key) => key.fmt(f),
            DbKey::BlockHeader(v) => f.write_str(&format!("Block header (#{})", v)),
            DbKey::BlockHash(v) => f.write_str(&format!("Block hash (#{})", to_hex(v))),
            DbKey::TransactionKernel(v) => f.write_str(&format!("Transaction kernel ({})", to_hex(v))),
            DbKey::OrphanBlock(v) => f.write_str(&format!("Orphan block hash ({})", to_hex(v))),
        }
    }
}

impl Display for MmrTree {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            MmrTree::RangeProof => f.write_str("Range Proof"),
            MmrTree::Utxo => f.write_str("UTXO"),
            MmrTree::Kernel => f.write_str("Kernel"),
        }
    }
}

impl TryFrom<i32> for MmrTree {
    type Error = String;

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(MmrTree::Utxo),
            1 => Ok(MmrTree::Kernel),
            2 => Ok(MmrTree::RangeProof),
            _ => Err("Invalid MmrTree".into()),
        }
    }
}

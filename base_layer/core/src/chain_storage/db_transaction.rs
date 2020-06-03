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
    blocks::{blockheader::BlockHash, Block, BlockHeader},
    proof_of_work::Difficulty,
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
};
use strum_macros::Display;
use tari_crypto::tari_utilities::{hex::to_hex, Hashable};

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

    /// A general insert request. There are convenience functions for specific delete queries.
    pub fn delete(&mut self, delete: DbKey) {
        self.operations.push(WriteOperation::Delete(delete));
    }

    /// Inserts a transaction kernel into the current transaction.
    pub fn insert_kernel(&mut self, kernel: TransactionKernel, update_mmr: bool) {
        let hash = kernel.hash();
        self.insert(DbKeyValuePair::TransactionKernel(hash, Box::new(kernel), update_mmr));
    }

    /// Inserts a block header into the current transaction.
    pub fn insert_header(&mut self, header: BlockHeader) {
        let height = header.height;
        self.insert(DbKeyValuePair::BlockHeader(height, Box::new(header)));
    }

    /// Adds a UTXO into the current transaction and update the TXO MMR.
    pub fn insert_utxo(&mut self, utxo: TransactionOutput, update_mmr: bool) {
        let hash = utxo.hash();
        self.insert(DbKeyValuePair::UnspentOutput(hash, Box::new(utxo), update_mmr));
    }

    /// Adds a UTXO into the current transaction and update the TXO MMR. This is a test only function used to ensure we
    /// block duplicate entries. This function does not calculate the hash function but accepts one as a variable.
    pub fn insert_utxo_with_hash(&mut self, hash: Vec<u8>, utxo: TransactionOutput, update_mmr: bool) {
        self.insert(DbKeyValuePair::UnspentOutput(hash, Box::new(utxo), update_mmr));
    }

    /// Stores an orphan block. No checks are made as to whether this is actually an orphan. That responsibility lies
    /// with the calling function.
    pub fn insert_orphan(&mut self, orphan: Block) {
        let hash = orphan.hash();
        self.insert(DbKeyValuePair::OrphanBlock(hash, Box::new(orphan)));
    }

    /// Moves a UTXO to the STXO set and mark it as spent on the MRR. If the UTXO is not in the UTXO set, the
    /// transaction will fail with an `UnspendableOutput` error.
    pub fn spend_utxo(&mut self, utxo_hash: HashOutput) {
        self.operations
            .push(WriteOperation::Spend(DbKey::UnspentOutput(utxo_hash)));
    }

    /// Moves a STXO to the UTXO set.  If the STXO is not in the STXO set, the transaction will fail with an
    /// `UnspendError`.
    // TODO: unspend_utxo in memory_db doesn't unmark the node in the roaring bitmap.0
    pub fn unspend_stxo(&mut self, stxo_hash: HashOutput) {
        self.operations
            .push(WriteOperation::UnSpend(DbKey::SpentOutput(stxo_hash)));
    }

    /// Moves the given set of transaction inputs from the UTXO set to the STXO set. All the inputs *must* currently
    /// exist in the UTXO set, or the transaction will error with `ChainStorageError::UnspendableOutput`
    pub fn spend_inputs(&mut self, inputs: &[TransactionInput]) {
        for input in inputs {
            let input_hash = input.hash();
            self.spend_utxo(input_hash);
        }
    }

    /// Adds a marker operation that allows the database to perform any additional work after adding a new block to
    /// the database.
    pub fn commit_block(&mut self) {
        self.operations
            .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
        self.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
        self.operations
            .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    }

    /// Set the horizon beyond which we cannot be guaranteed provide detailed blockchain information anymore.
    /// A value of zero indicates that no pruning should be carried out at all. That is, this state should act as a
    /// archival node.
    ///
    /// This operation just sets the new horizon value. No pruning is done at this point.
    pub fn set_pruning_horizon(&mut self, new_pruning_horizon: u64) {
        self.operations.push(WriteOperation::Insert(DbKeyValuePair::Metadata(
            MetadataKey::PruningHorizon,
            MetadataValue::PruningHorizon(new_pruning_horizon),
        )));
    }

    /// Rewinds the Kernel MMR state by the given number of Checkpoints.
    pub fn rewind_kernel_mmr(&mut self, steps_back: usize) {
        self.operations
            .push(WriteOperation::RewindMmr(MmrTree::Kernel, steps_back));
    }

    /// Rewinds the UTXO MMR state by the given number of Checkpoints.
    pub fn rewind_utxo_mmr(&mut self, steps_back: usize) {
        self.operations
            .push(WriteOperation::RewindMmr(MmrTree::Utxo, steps_back));
    }

    /// Rewinds the RangeProof MMR state by the given number of Checkpoints.
    pub fn rewind_rp_mmr(&mut self, steps_back: usize) {
        self.operations
            .push(WriteOperation::RewindMmr(MmrTree::RangeProof, steps_back));
    }
}

#[derive(Debug, Display)]
pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Delete(DbKey),
    Spend(DbKey),
    UnSpend(DbKey),
    CreateMmrCheckpoint(MmrTree),
    RewindMmr(MmrTree, usize),
}

/// A list of key-value pairs that are required for each insert operation
#[derive(Debug)]
pub enum DbKeyValuePair {
    Metadata(MetadataKey, MetadataValue),
    BlockHeader(u64, Box<BlockHeader>),
    UnspentOutput(HashOutput, Box<TransactionOutput>, bool),
    TransactionKernel(HashOutput, Box<TransactionKernel>, bool),
    OrphanBlock(HashOutput, Box<Block>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MmrTree {
    Utxo,
    Kernel,
    RangeProof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataKey {
    ChainHeight,
    BestBlock,
    AccumulatedWork,
    PruningHorizon,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum MetadataValue {
    ChainHeight(Option<u64>),
    BestBlock(Option<BlockHash>),
    AccumulatedWork(Option<Difficulty>),
    PruningHorizon(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    Metadata(MetadataKey),
    BlockHeader(u64),
    BlockHash(BlockHash),
    UnspentOutput(HashOutput),
    SpentOutput(HashOutput),
    TransactionKernel(HashOutput),
    OrphanBlock(HashOutput),
}

#[derive(Debug)]
pub enum DbValue {
    Metadata(MetadataValue),
    BlockHeader(Box<BlockHeader>),
    BlockHash(Box<BlockHeader>),
    UnspentOutput(Box<TransactionOutput>),
    SpentOutput(Box<TransactionOutput>),
    TransactionKernel(Box<TransactionKernel>),
    OrphanBlock(Box<Block>),
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Metadata(MetadataValue::ChainHeight(_)) => f.write_str("Current chain height"),
            DbValue::Metadata(MetadataValue::AccumulatedWork(_)) => f.write_str("Total accumulated work"),
            DbValue::Metadata(MetadataValue::PruningHorizon(_)) => f.write_str("Pruning horizon"),
            DbValue::Metadata(MetadataValue::BestBlock(_)) => f.write_str("Chain tip block hash"),
            DbValue::BlockHeader(_) => f.write_str("Block header"),
            DbValue::BlockHash(_) => f.write_str("Block hash"),
            DbValue::UnspentOutput(_) => f.write_str("Unspent output"),
            DbValue::SpentOutput(_) => f.write_str("Spent output"),
            DbValue::TransactionKernel(_) => f.write_str("Transaction kernel"),
            DbValue::OrphanBlock(_) => f.write_str("Orphan block"),
        }
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::Metadata(MetadataKey::ChainHeight) => f.write_str("Current chain height"),
            DbKey::Metadata(MetadataKey::AccumulatedWork) => f.write_str("Total accumulated work"),
            DbKey::Metadata(MetadataKey::PruningHorizon) => f.write_str("Pruning horizon"),
            DbKey::Metadata(MetadataKey::BestBlock) => f.write_str("Chain tip block hash"),
            DbKey::BlockHeader(v) => f.write_str(&format!("Block header (#{})", v)),
            DbKey::BlockHash(v) => f.write_str(&format!("Block hash (#{})", to_hex(v))),
            DbKey::UnspentOutput(v) => f.write_str(&format!("Unspent output ({})", to_hex(v))),
            DbKey::SpentOutput(v) => f.write_str(&format!("Spent output ({})", to_hex(v))),
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

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
    fmt,
    fmt::{Display, Error, Formatter},
    sync::{Arc, RwLock},
};

use primitive_types::U256;
use tari_common_types::types::{BlockHash, Commitment, HashOutput};
use tari_utilities::hex::Hex;

use crate::{
    blocks::{Block, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader, UpdateBlockAccumulatedData},
    chain_storage::{error::ChainStorageError, HorizonData, Reorg},
    transactions::transaction_components::{OutputType, TransactionKernel, TransactionOutput},
    OutputSmt,
};

#[derive(Debug)]
pub struct DbTransaction {
    operations: Vec<WriteOperation>,
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

    /// deletes the orphan, and if it was a tip will delete the orphan tip and make its prev header the new tip. This
    /// will not fail if the orphan does not exist.
    pub fn delete_orphan(&mut self, hash: HashOutput) -> &mut Self {
        self.operations.push(WriteOperation::DeleteOrphan(hash));
        self
    }

    /// Delete a block header at the given height
    pub fn delete_header(&mut self, height: u64) -> &mut Self {
        self.operations.push(WriteOperation::DeleteHeader(height));
        self
    }

    /// Delete a block
    pub fn delete_tip_block(&mut self, block_hash: HashOutput, smt: Arc<RwLock<OutputSmt>>) -> &mut Self {
        self.operations.push(WriteOperation::DeleteTipBlock(block_hash, smt));
        self
    }

    /// Inserts a transaction kernel into the current transaction.
    pub fn insert_kernel(
        &mut self,
        kernel: TransactionKernel,
        header_hash: HashOutput,
        mmr_position: u64,
    ) -> &mut Self {
        self.operations.push(WriteOperation::InsertKernel {
            header_hash,
            kernel: Box::new(kernel),
            mmr_position,
        });
        self
    }

    /// Inserts a block header into the current transaction.
    pub fn insert_chain_header(&mut self, chain_header: ChainHeader) -> &mut Self {
        self.operations.push(WriteOperation::InsertChainHeader {
            header: Box::new(chain_header),
        });
        self
    }

    /// Adds a UTXO into the current transaction and update the TXO MMR.
    pub fn insert_utxo(
        &mut self,
        utxo: TransactionOutput,
        header_hash: HashOutput,
        header_height: u64,
        timestamp: u64,
    ) -> &mut Self {
        self.operations.push(WriteOperation::InsertOutput {
            header_hash,
            header_height,
            timestamp,
            output: Box::new(utxo),
        });
        self
    }

    pub fn prune_outputs_spent_at_hash(&mut self, block_hash: BlockHash) -> &mut Self {
        self.operations
            .push(WriteOperation::PruneOutputsSpentAtHash { block_hash });
        self
    }

    pub fn prune_output_from_all_dbs(
        &mut self,
        output_hash: HashOutput,
        commitment: Commitment,
        output_type: OutputType,
    ) -> &mut Self {
        self.operations.push(WriteOperation::PruneOutputFromAllDbs {
            output_hash,
            commitment,
            output_type,
        });
        self
    }

    pub fn delete_all_kernerls_in_block(&mut self, block_hash: BlockHash) -> &mut Self {
        self.operations
            .push(WriteOperation::DeleteAllKernelsInBlock { block_hash });
        self
    }

    pub fn delete_all_inputs_in_block(&mut self, block_hash: BlockHash) -> &mut Self {
        self.operations
            .push(WriteOperation::DeleteAllInputsInBlock { block_hash });
        self
    }

    pub fn update_block_accumulated_data(
        &mut self,
        header_hash: HashOutput,
        values: UpdateBlockAccumulatedData,
    ) -> &mut Self {
        self.operations
            .push(WriteOperation::UpdateBlockAccumulatedData { header_hash, values });
        self
    }

    /// Add the BlockHeader and contents of a `Block` (i.e. inputs, outputs and kernels) to the database.
    /// If the `BlockHeader` already exists, then just the contents are updated along with the relevant accumulated
    /// data.
    pub fn insert_tip_block_body(&mut self, block: Arc<ChainBlock>, smt: Arc<RwLock<OutputSmt>>) -> &mut Self {
        self.operations.push(WriteOperation::InsertTipBlockBody { block, smt });
        self
    }

    /// Inserts a block hash into the bad block list
    pub fn insert_bad_block(&mut self, block_hash: HashOutput, height: u64, reason: String) -> &mut Self {
        self.operations.push(WriteOperation::InsertBadBlock {
            hash: block_hash,
            height,
            reason,
        });
        self
    }

    /// Stores an orphan block. No checks are made as to whether this is actually an orphan. That responsibility lies
    /// with the calling function.
    /// The transaction will rollback and write will return an error if the orphan already exists.
    pub fn insert_orphan(&mut self, orphan: Arc<Block>) -> &mut Self {
        self.operations.push(WriteOperation::InsertOrphanBlock(orphan));
        self
    }

    /// Insert a "chained" orphan block.
    /// The transaction will rollback and write will return an error if the orphan already exists.
    pub fn insert_chained_orphan(&mut self, orphan: Arc<ChainBlock>) -> &mut Self {
        self.operations.push(WriteOperation::InsertChainOrphanBlock(orphan));
        self
    }

    /// Remove an orphan from the orphan tip set
    pub fn remove_orphan_chain_tip(&mut self, hash: HashOutput) -> &mut Self {
        self.operations.push(WriteOperation::DeleteOrphanChainTip(hash));
        self
    }

    /// Add an orphan to the orphan tip set
    pub fn insert_orphan_chain_tip(&mut self, hash: HashOutput, total_accumulated_difficulty: U256) -> &mut Self {
        self.operations
            .push(WriteOperation::InsertOrphanChainTip(hash, total_accumulated_difficulty));
        self
    }

    /// Sets accumulated data for the orphan block, "upgrading" the orphan block to a chained orphan.
    /// Any existing accumulated data is overwritten.
    /// The transaction will rollback and write will return an error if the orphan block does not exist.
    pub fn set_accumulated_data_for_orphan(&mut self, accumulated_data: BlockHeaderAccumulatedData) -> &mut Self {
        self.operations
            .push(WriteOperation::SetAccumulatedDataForOrphan(accumulated_data));
        self
    }

    pub fn set_best_block(
        &mut self,
        height: u64,
        hash: HashOutput,
        accumulated_difficulty: U256,
        expected_prev_best_block: HashOutput,
        timestamp: u64,
    ) -> &mut Self {
        self.operations.push(WriteOperation::SetBestBlock {
            height,
            hash,
            accumulated_difficulty,
            expected_prev_best_block,
            timestamp,
        });
        self
    }

    pub fn set_pruning_horizon(&mut self, pruning_horizon: u64) -> &mut Self {
        self.operations
            .push(WriteOperation::SetPruningHorizonConfig(pruning_horizon));
        self
    }

    pub fn set_pruned_height(&mut self, height: u64) -> &mut Self {
        self.operations.push(WriteOperation::SetPrunedHeight { height });
        self
    }

    pub fn set_horizon_data(&mut self, kernel_sum: Commitment, utxo_sum: Commitment) -> &mut Self {
        self.operations.push(WriteOperation::SetHorizonData {
            horizon_data: HorizonData::new(kernel_sum, utxo_sum),
        });
        self
    }

    pub(crate) fn operations(&self) -> &[WriteOperation] {
        &self.operations
    }

    /// This will store the seed key with the height. This is called when a block is accepted into the main chain.
    /// This will only update the hieght of the seed, if its lower then currently stored.
    pub fn insert_monero_seed_height(&mut self, monero_seed: Vec<u8>, height: u64) {
        self.operations
            .push(WriteOperation::InsertMoneroSeedHeight(monero_seed, height));
    }

    pub fn insert_reorg(&mut self, reorg: Reorg) -> &mut Self {
        self.operations.push(WriteOperation::InsertReorg { reorg });
        self
    }

    pub fn clear_all_reorgs(&mut self) -> &mut Self {
        self.operations.push(WriteOperation::ClearAllReorgs);
        self
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WriteOperation {
    InsertOrphanBlock(Arc<Block>),
    InsertChainOrphanBlock(Arc<ChainBlock>),
    InsertChainHeader {
        header: Box<ChainHeader>,
    },
    InsertTipBlockBody {
        block: Arc<ChainBlock>,
        smt: Arc<RwLock<OutputSmt>>,
    },
    InsertKernel {
        header_hash: HashOutput,
        kernel: Box<TransactionKernel>,
        mmr_position: u64,
    },
    InsertOutput {
        header_hash: HashOutput,
        header_height: u64,
        timestamp: u64,
        output: Box<TransactionOutput>,
    },
    InsertBadBlock {
        hash: HashOutput,
        height: u64,
        reason: String,
    },
    DeleteHeader(u64),
    DeleteOrphan(HashOutput),
    DeleteTipBlock(HashOutput, Arc<RwLock<OutputSmt>>),
    DeleteOrphanChainTip(HashOutput),
    InsertOrphanChainTip(HashOutput, U256),
    InsertMoneroSeedHeight(Vec<u8>, u64),
    UpdateBlockAccumulatedData {
        header_hash: HashOutput,
        values: UpdateBlockAccumulatedData,
    },
    PruneOutputsSpentAtHash {
        block_hash: BlockHash,
    },
    PruneOutputFromAllDbs {
        output_hash: HashOutput,
        commitment: Commitment,
        output_type: OutputType,
    },
    DeleteAllKernelsInBlock {
        block_hash: BlockHash,
    },
    DeleteAllInputsInBlock {
        block_hash: BlockHash,
    },
    SetAccumulatedDataForOrphan(BlockHeaderAccumulatedData),
    SetBestBlock {
        height: u64,
        hash: HashOutput,
        accumulated_difficulty: U256,
        expected_prev_best_block: HashOutput,
        timestamp: u64,
    },
    SetPruningHorizonConfig(u64),
    SetPrunedHeight {
        height: u64,
    },
    SetHorizonData {
        horizon_data: HorizonData,
    },
    InsertReorg {
        reorg: Reorg,
    },
    ClearAllReorgs,
}

impl fmt::Display for WriteOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use WriteOperation::*;
        match self {
            InsertOrphanBlock(block) => write!(
                f,
                "InsertOrphanBlock({}, {})",
                block.hash(),
                block.body.to_counts_string()
            ),
            InsertChainHeader { header } => {
                write!(f, "InsertChainHeader(#{} {})", header.height(), header.hash())
            },
            InsertTipBlockBody { block, smt: _ } => write!(
                f,
                "InsertTipBlockBody({}, {})",
                block.accumulated_data().hash,
                block.block().body.to_counts_string(),
            ),
            InsertKernel {
                header_hash,
                kernel,
                mmr_position,
            } => write!(
                f,
                "Insert kernel {} in block:{} position: {}",
                kernel.hash(),
                header_hash,
                mmr_position
            ),
            InsertOutput {
                header_hash,
                header_height,
                output,
                ..
            } => write!(
                f,
                "Insert output {} in block({}):{},",
                output.hash(),
                header_height,
                header_hash,
            ),
            DeleteOrphanChainTip(hash) => write!(f, "DeleteOrphanChainTip({})", hash),
            InsertOrphanChainTip(hash, total_accumulated_difficulty) => {
                write!(f, "InsertOrphanChainTip({}, {})", hash, total_accumulated_difficulty)
            },
            DeleteTipBlock(hash, _) => write!(f, "DeleteTipBlock({})", hash),
            InsertMoneroSeedHeight(data, height) => {
                write!(f, "Insert Monero seed string {} for height: {}", data.to_hex(), height)
            },
            InsertChainOrphanBlock(block) => write!(f, "InsertChainOrphanBlock({})", block.hash()),
            UpdateBlockAccumulatedData { header_hash, .. } => {
                write!(f, "Update Block data for block {}", header_hash)
            },
            PruneOutputsSpentAtHash { block_hash } => write!(f, "Prune output(s) at hash: {}", block_hash),
            PruneOutputFromAllDbs {
                output_hash,
                commitment,
                output_type,
            } => write!(
                f,
                "Prune output from all dbs, hash : {}, commitment: {},output_type: {}",
                output_hash,
                commitment.to_hex(),
                output_type,
            ),
            DeleteAllKernelsInBlock { block_hash } => write!(f, "Delete kernels in block {}", block_hash),
            DeleteAllInputsInBlock { block_hash } => write!(f, "Delete outputs in block {}", block_hash),
            SetAccumulatedDataForOrphan(accumulated_data) => {
                write!(f, "Set accumulated data for orphan {}", accumulated_data)
            },
            SetBestBlock {
                height,
                hash,
                accumulated_difficulty,
                expected_prev_best_block: _,
                timestamp,
            } => write!(
                f,
                "Update best block to height:{} ({}) with difficulty: {} and timestamp: {}",
                height, hash, accumulated_difficulty, timestamp
            ),
            SetPruningHorizonConfig(pruning_horizon) => write!(f, "Set config: pruning horizon to {}", pruning_horizon),
            SetPrunedHeight { height, .. } => write!(f, "Set pruned height to {}", height),
            DeleteHeader(height) => write!(f, "Delete header at height: {}", height),
            DeleteOrphan(hash) => write!(f, "Delete orphan with hash: {}", hash),
            InsertBadBlock { hash, height, reason } => {
                write!(f, "Insert bad block #{} {} for {}", height, hash, reason)
            },
            SetHorizonData { .. } => write!(f, "Set horizon data"),
            InsertReorg { .. } => write!(f, "Insert reorg"),
            ClearAllReorgs => write!(f, "Clear all reorgs"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbKey {
    HeaderHeight(u64),
    HeaderHash(BlockHash),
    OrphanBlock(HashOutput),
}

impl DbKey {
    pub fn to_value_not_found_error(&self) -> ChainStorageError {
        let (entity, field, value) = match self {
            DbKey::HeaderHeight(v) => ("BlockHeader", "Height", v.to_string()),
            DbKey::HeaderHash(v) => ("Header", "Hash", v.to_hex()),
            DbKey::OrphanBlock(v) => ("Orphan", "Hash", v.to_hex()),
        };
        ChainStorageError::ValueNotFound { entity, field, value }
    }
}

#[derive(Debug)]
pub enum DbValue {
    HeaderHeight(Box<BlockHeader>),
    HeaderHash(Box<BlockHeader>),
    OrphanBlock(Box<Block>),
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::HeaderHeight(_) => f.write_str("Header by height"),
            DbValue::HeaderHash(_) => f.write_str("Header by hash"),
            DbValue::OrphanBlock(_) => f.write_str("Orphan block"),
        }
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::HeaderHeight(v) => f.write_str(&format!("Header height (#{})", v)),
            DbKey::HeaderHash(v) => f.write_str(&format!("Header hash (#{})", v)),
            DbKey::OrphanBlock(v) => f.write_str(&format!("Orphan block hash ({})", v)),
        }
    }
}

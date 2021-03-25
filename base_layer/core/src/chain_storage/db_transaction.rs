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
    chain_storage::{error::ChainStorageError, BlockHeaderAccumulatedData, ChainBlock, ChainHeader, MmrTree},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{Commitment, HashOutput},
    },
};
use croaring::Bitmap;
use std::{
    fmt,
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_common_types::types::BlockHash;
use tari_crypto::tari_utilities::{
    hex::{to_hex, Hex},
    Hashable,
};
use tari_mmr::pruned_hashset::PrunedHashSet;

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

    /// A general insert request. There are convenience functions for specific delete queries.
    pub fn delete(&mut self, delete: DbKey) -> &mut Self {
        self.operations.push(WriteOperation::Delete(delete));
        self
    }

    pub fn delete_orphan(&mut self, hash: HashOutput) -> &mut Self {
        self.delete(DbKey::OrphanBlock(hash))
    }

    /// Delete a block header at the given height
    pub fn delete_header(&mut self, height: u64) -> &mut Self {
        self.operations.push(WriteOperation::Delete(DbKey::BlockHeader(height)));
        self
    }

    /// Delete a block
    pub fn delete_block(&mut self, block_hash: HashOutput) -> &mut Self {
        self.operations.push(WriteOperation::DeleteBlock(block_hash));
        self
    }

    /// Inserts a transaction kernel into the current transaction.
    pub fn insert_kernel(
        &mut self,
        kernel: TransactionKernel,
        header_hash: HashOutput,
        mmr_position: u32,
    ) -> &mut Self
    {
        self.operations.push(WriteOperation::InsertKernel {
            header_hash,
            kernel: Box::new(kernel),
            mmr_position,
        });
        self
    }

    /// Inserts a block header into the current transaction.
    pub fn insert_header(&mut self, header: BlockHeader, accum_data: BlockHeaderAccumulatedData) -> &mut Self {
        self.operations.push(WriteOperation::InsertHeader {
            header: Box::new(ChainHeader {
                header,
                accumulated_data: accum_data,
            }),
        });
        self
    }

    /// Adds a UTXO into the current transaction and update the TXO MMR.
    pub fn insert_utxo(&mut self, utxo: TransactionOutput, header_hash: HashOutput, header_height: u64, mmr_leaf_index: u32) -> &mut Self {
        self.operations.push(WriteOperation::InsertOutput {
            header_hash,
            header_height,
            output: Box::new(utxo),
            mmr_position: mmr_leaf_index,
        });
        self
    }

    pub fn insert_pruned_utxo(
        &mut self,
        output_hash: HashOutput,
        proof_hash: HashOutput,
        header_hash: HashOutput,
        header_height: u64,
        mmr_leaf_index: u32,
    ) -> &mut Self
    {
        self.operations.push(WriteOperation::InsertPrunedOutput {
            header_hash,
            header_height,
            output_hash,
            proof_hash,
            mmr_position: mmr_leaf_index,
        });
        self
    }

    pub fn insert_input(&mut self, input: TransactionInput, header_hash: HashOutput, mmr_leaf_index: u32) -> &mut Self {
        self.operations.push(WriteOperation::InsertInput {
            header_hash,
            input: Box::new(input),
            mmr_position: mmr_leaf_index,
        });
        self
    }

    pub fn update_pruned_hash_set(
        &mut self,
        mmr_tree: MmrTree,
        header_hash: HashOutput,
        pruned_hash_set: PrunedHashSet,
    ) -> &mut Self
    {
        self.operations.push(WriteOperation::UpdatePrunedHashSet {
            mmr_tree,
            header_hash,
            pruned_hash_set: Box::new(pruned_hash_set),
        });
        self
    }

    pub fn update_kernel_sum(&mut self, header_hash: HashOutput, kernel_sum: Commitment) -> &mut Self {
        self.operations.push(WriteOperation::UpdateKernelSum {
            header_hash,
            kernel_sum,
        });
        self
    }

    pub fn prune_outputs_and_update_horizon(&mut self, output_mmr_positions: Vec<u32>, horizon: u64) -> &mut Self {
        self.operations.push(WriteOperation::PruneOutputsAndUpdateHorizon {
            output_positions: output_mmr_positions,
            horizon,
        });
        self
    }

    pub fn update_deleted(&mut self, header_hash: HashOutput, deleted: Bitmap) -> &mut Self {
        self.operations
            .push(WriteOperation::UpdateDeletedBlockAccumulatedData { header_hash, deleted });
        self
    }

    /// Add the BlockHeader and contents of a `Block` (i.e. inputs, outputs and kernels) to the database.
    /// If the `BlockHeader` already exists, then just the contents are updated along with the relevant accumulated
    /// data.
    pub fn insert_block(&mut self, block: Arc<ChainBlock>) -> &mut Self {
        self.operations.push(WriteOperation::InsertBlock { block });
        self
    }

    /// Stores an orphan block. No checks are made as to whether this is actually an orphan. That responsibility lies
    /// with the calling function.
    pub fn insert_orphan(&mut self, orphan: Arc<Block>) -> &mut Self {
        self.operations.push(WriteOperation::InsertOrphanBlock(orphan));
        self
    }

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
    pub fn insert_orphan_chain_tip(&mut self, hash: HashOutput) -> &mut Self {
        self.operations.push(WriteOperation::InsertOrphanChainTip(hash));
        self
    }

    pub fn set_best_block(&mut self, height: u64, hash: HashOutput, accumulated_difficulty: u128) -> &mut Self {
        self.operations.push(WriteOperation::SetBestBlock {
            height,
            hash,
            accumulated_difficulty,
        });
        self
    }

    pub fn set_pruning_horizon(&mut self, pruning_horizon: u64) -> &mut Self {
        self.operations
            .push(WriteOperation::SetPruningHorizonConfig(pruning_horizon));
        self
    }

    pub fn set_pruned_height(&mut self, height: u64, kernel_sum: Commitment, utxo_sum: Commitment) -> &mut Self {
        self.operations.push(WriteOperation::SetPrunedHeight {
            height,
            kernel_sum,
            utxo_sum,
        });
        self
    }

    pub(crate) fn operations(&self) -> &[WriteOperation] {
        &self.operations
    }

    pub(crate) fn into_operations(self) -> Vec<WriteOperation> {
        self.operations
    }

    /// This will store the seed key with the height. This is called when a block is accepted into the main chain.
    /// This will only update the hieght of the seed, if its lower then currently stored.
    pub fn insert_monero_seed_height(&mut self, monero_seed: &str, height: u64) {
        let monero_seed_boxed = Box::new(monero_seed.to_string());
        self.operations
            .push(WriteOperation::InsertMoneroSeedHeight(monero_seed_boxed, height));
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WriteOperation {
    InsertOrphanBlock(Arc<Block>),
    InsertChainOrphanBlock(Arc<ChainBlock>),
    InsertHeader {
        header: Box<ChainHeader>,
    },
    InsertBlock {
        block: Arc<ChainBlock>,
    },
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
        header_height: u64,
        output: Box<TransactionOutput>,
        mmr_position: u32,
    },
    InsertPrunedOutput {
        header_hash: HashOutput,
        header_height: u64,
        output_hash: HashOutput,
        proof_hash: HashOutput,
        mmr_position: u32,
    },
    Delete(DbKey),
    DeleteBlock(HashOutput),
    DeleteOrphanChainTip(HashOutput),
    InsertOrphanChainTip(HashOutput),
    InsertMoneroSeedHeight(Box<String>, u64),
    UpdatePrunedHashSet {
        mmr_tree: MmrTree,
        header_hash: HashOutput,
        pruned_hash_set: Box<PrunedHashSet>,
    },
    UpdateDeletedBlockAccumulatedData {
        header_hash: HashOutput,
        deleted: Bitmap,
    },
    PruneOutputsAndUpdateHorizon {
        output_positions: Vec<u32>,
        horizon: u64,
    },
    UpdateKernelSum {
        header_hash: HashOutput,
        kernel_sum: Commitment,
    },
    SetBestBlock {
        height: u64,
        hash: HashOutput,
        accumulated_difficulty: u128,
    },
    SetPruningHorizonConfig(u64),
    SetPrunedHeight {
        height: u64,
        kernel_sum: Commitment,
        utxo_sum: Commitment,
    },
}

impl fmt::Display for WriteOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use WriteOperation::*;
        match self {
            InsertOrphanBlock(block) => write!(
                f,
                "InsertBlock({}, {})",
                block.hash().to_hex(),
                block.body.to_counts_string()
            ),
            InsertHeader { header } => write!(
                f,
                "InsertHeader(#{} {})",
                header.header.height,
                header.accumulated_data.hash.to_hex()
            ),
            InsertBlock { block } => write!(
                f,
                "InsertBlock({}, {})",
                block.accumulated_data.hash.to_hex(),
                block.block.body.to_counts_string(),
            ),
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
                header_height,
                output,
                mmr_position,
            } => write!(
                f,
                "Insert output {} in block:{},#{} position: {}",
                output.hash().to_hex(),
                header_hash.to_hex(),
                header_height,
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
            DeleteOrphanChainTip(hash) => write!(f, "DeleteOrphanChainTip({})", hash.to_hex()),
            InsertOrphanChainTip(hash) => write!(f, "InsertOrphanChainTip({})", hash.to_hex()),
            DeleteBlock(hash) => write!(f, "DeleteBlock({})", hash.to_hex()),
            InsertMoneroSeedHeight(data, height) => {
                write!(f, "Insert Monero seed string {} for height: {}", data, height)
            },
            InsertChainOrphanBlock(block) => {
                write!(f, "InsertChainOrphanBlock({})", block.accumulated_data.hash.to_hex())
            },
            UpdatePrunedHashSet {
                mmr_tree, header_hash, ..
            } => write!(
                f,
                "Update pruned hash set: {} header: {}",
                mmr_tree,
                header_hash.to_hex()
            ),
            InsertPrunedOutput {
                header_hash: _,
                header_height: _,
                output_hash: _,
                proof_hash: _,
                mmr_position: _,
            } => write!(f, "Insert pruned output"),
            UpdateDeletedBlockAccumulatedData {
                header_hash: _,
                deleted: _,
            } => write!(f, "Update deleted data for block"),
            PruneOutputsAndUpdateHorizon {
                output_positions,
                horizon,
            } => write!(
                f,
                "Prune {} outputs and set horizon to {}",
                output_positions.len(),
                horizon
            ),
            UpdateKernelSum { header_hash, .. } => write!(f, "Update kernel sum for block: {}", header_hash.to_hex()),
            SetBestBlock {
                height,
                hash,
                accumulated_difficulty,
            } => write!(
                f,
                "Update best block to height:{} ({}) with difficulty: {}",
                height,
                hash.to_hex(),
                accumulated_difficulty
            ),
            SetPruningHorizonConfig(pruning_horizon) => write!(f, "Set config: pruning horizon to {}", pruning_horizon),
            SetPrunedHeight { height, .. } => write!(f, "Set pruned height to {}", height),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    BlockHeader(u64),
    BlockHash(BlockHash),
    OrphanBlock(HashOutput),
}

impl DbKey {
    pub fn to_value_not_found_error(&self) -> ChainStorageError {
        let (entity, field, value) = match self {
            DbKey::BlockHeader(v) => ("BlockHeader".to_string(), "Height".to_string(), v.to_string()),
            DbKey::BlockHash(v) => ("Block".to_string(), "Hash".to_string(), v.to_hex()),
            DbKey::OrphanBlock(v) => ("Orphan".to_string(), "Hash".to_string(), v.to_hex()),
        };
        ChainStorageError::ValueNotFound { entity, field, value }
    }
}

#[derive(Debug)]
pub enum DbValue {
    BlockHeader(Box<BlockHeader>),
    BlockHash(Box<BlockHeader>),
    OrphanBlock(Box<Block>),
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::BlockHeader(_) => f.write_str("Block header"),
            DbValue::BlockHash(_) => f.write_str("Block hash"),
            DbValue::OrphanBlock(_) => f.write_str("Orphan block"),
        }
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::BlockHeader(v) => f.write_str(&format!("Block header (#{})", v)),
            DbKey::BlockHash(v) => f.write_str(&format!("Block hash (#{})", to_hex(v))),
            DbKey::OrphanBlock(v) => f.write_str(&format!("Orphan block hash ({})", to_hex(v))),
        }
    }
}

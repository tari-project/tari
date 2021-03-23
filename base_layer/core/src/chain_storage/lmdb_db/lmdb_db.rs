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
    blocks::{block_header::BlockHeader, Block},
    chain_storage::{
        accumulated_data::{BlockAccumulatedData, BlockHeaderAccumulatedData, DeletedBitmap},
        db_transaction::{DbKey, DbTransaction, DbValue, WriteOperation},
        error::{ChainStorageError, OrNotFound},
        lmdb_db::{
            lmdb::{
                lmdb_delete,
                lmdb_delete_key_value,
                lmdb_delete_keys_starting_with,
                lmdb_exists,
                lmdb_fetch_keys_starting_with,
                lmdb_filter_map_values,
                lmdb_first_after,
                lmdb_get,
                lmdb_get_multiple,
                lmdb_insert,
                lmdb_insert_dup,
                lmdb_last,
                lmdb_len,
                lmdb_replace,
            },
            TransactionInputRowData,
            TransactionKernelRowData,
            TransactionOutputRowData,
            LMDB_DB_BLOCK_ACCUMULATED_DATA,
            LMDB_DB_BLOCK_HASHES,
            LMDB_DB_HEADERS,
            LMDB_DB_HEADER_ACCUMULATED_DATA,
            LMDB_DB_INPUTS,
            LMDB_DB_KERNELS,
            LMDB_DB_KERNEL_EXCESS_INDEX,
            LMDB_DB_KERNEL_EXCESS_SIG_INDEX,
            LMDB_DB_KERNEL_MMR_SIZE_INDEX,
            LMDB_DB_METADATA,
            LMDB_DB_MONERO_SEED_HEIGHT,
            LMDB_DB_ORPHANS,
            LMDB_DB_ORPHAN_CHAIN_TIPS,
            LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA,
            LMDB_DB_ORPHAN_PARENT_MAP_INDEX,
            LMDB_DB_TXOS_HASH_TO_INDEX,
            LMDB_DB_UTXOS,
            LMDB_DB_UTXO_MMR_SIZE_INDEX,
        },
        BlockchainBackend,
        ChainHeader,
        HorizonData,
        MmrTree,
        PrunedOutput,
    },
    crypto::tari_utilities::hex::to_hex,
    transactions::{
        aggregated_body::AggregateBody,
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{Commitment, HashDigest, HashOutput, Signature},
    },
};
use croaring::Bitmap;
use fs2::FileExt;
use lmdb_zero::{ConstTransaction, Database, Environment, ReadTransaction, WriteTransaction};
use log::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{BlockHash, BLOCK_HASH_LENGTH},
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex, ByteArray};
use tari_mmr::{Hash, MerkleMountainRange, MutableMmr};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBConfig, LMDBStore};

type DatabaseRef = Arc<Database<'static>>;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db";

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase {
    env: Arc<Environment>,
    env_config: LMDBConfig,
    metadata_db: DatabaseRef,
    mem_metadata: Option<ChainMetadata>,
    headers_db: DatabaseRef,
    header_accumulated_data_db: DatabaseRef,
    block_accumulated_data_db: DatabaseRef,
    block_hashes_db: DatabaseRef,
    utxos_db: DatabaseRef,
    inputs_db: DatabaseRef,
    txos_hash_to_index_db: DatabaseRef,
    kernels_db: DatabaseRef,
    kernel_excess_index: DatabaseRef,
    kernel_excess_sig_index: DatabaseRef,
    kernel_mmr_size_index: DatabaseRef,
    output_mmr_size_index: DatabaseRef,
    orphans_db: DatabaseRef,
    monero_seed_height_db: DatabaseRef,
    orphan_header_accumulated_data_db: DatabaseRef,
    orphan_chain_tips_db: DatabaseRef,
    orphan_parent_map_index: DatabaseRef,
    is_mem_metadata_dirty: bool,
    _file_lock: Arc<File>,
}

impl LMDBDatabase {
    pub fn new(store: LMDBStore, file_lock: File) -> Result<Self, ChainStorageError> {
        let env = store.env();

        let mut res = Self {
            metadata_db: get_database(&store, LMDB_DB_METADATA)?,
            mem_metadata: None,
            headers_db: get_database(&store, LMDB_DB_HEADERS)?,
            header_accumulated_data_db: get_database(&store, LMDB_DB_HEADER_ACCUMULATED_DATA)?,
            block_accumulated_data_db: get_database(&store, LMDB_DB_BLOCK_ACCUMULATED_DATA)?,
            block_hashes_db: get_database(&store, LMDB_DB_BLOCK_HASHES)?,
            utxos_db: get_database(&store, LMDB_DB_UTXOS)?,
            inputs_db: get_database(&store, LMDB_DB_INPUTS)?,
            txos_hash_to_index_db: get_database(&store, LMDB_DB_TXOS_HASH_TO_INDEX)?,
            kernels_db: get_database(&store, LMDB_DB_KERNELS)?,
            kernel_excess_index: get_database(&store, LMDB_DB_KERNEL_EXCESS_INDEX)?,
            kernel_excess_sig_index: get_database(&store, LMDB_DB_KERNEL_EXCESS_SIG_INDEX)?,
            kernel_mmr_size_index: get_database(&store, LMDB_DB_KERNEL_MMR_SIZE_INDEX)?,
            output_mmr_size_index: get_database(&store, LMDB_DB_UTXO_MMR_SIZE_INDEX)?,
            orphans_db: get_database(&store, LMDB_DB_ORPHANS)?,
            orphan_header_accumulated_data_db: get_database(&store, LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA)?,
            monero_seed_height_db: get_database(&store, LMDB_DB_MONERO_SEED_HEIGHT)?,
            orphan_chain_tips_db: get_database(&store, LMDB_DB_ORPHAN_CHAIN_TIPS)?,
            orphan_parent_map_index: get_database(&store, LMDB_DB_ORPHAN_PARENT_MAP_INDEX)?,
            env,
            env_config: store.env_config(),
            is_mem_metadata_dirty: false,
            _file_lock: Arc::new(file_lock),
        };
        if !res.is_empty()? {
            res.refresh_chain_metadata()?;
        }
        Ok(res)
    }

    fn apply_db_transaction(&mut self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        use WriteOperation::*;
        let write_txn =
            WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for op in txn.into_operations() {
            trace!(target: LOG_TARGET, "[apply_db_transaction] WriteOperation: {}", op);
            match op {
                InsertOrphanBlock(block) => self.insert_orphan_block(&write_txn, &block)?,
                Delete(delete) => self.op_delete(&write_txn, delete)?,
                InsertHeader { header } => {
                    let height = header.header.height;
                    if !self.insert_header(&write_txn, &header.header, &header.accumulated_data)? {
                        return Err(ChainStorageError::InvalidOperation(format!(
                            "Duplicate `BlockHeader` key `{}`",
                            height
                        )));
                    }
                },
                InsertBlock { block } => {
                    // TODO: Sort out clones
                    self.insert_header(&write_txn, &block.block.header, &block.accumulated_data)?;
                    self.insert_block_body(&write_txn, &block.block.header, block.block.body.clone())?;
                },
                InsertKernel {
                    header_hash,
                    kernel,
                    mmr_position,
                } => {
                    trace!(
                        target: LOG_TARGET,
                        "Inserting kernel `{}`",
                        kernel.excess_sig.get_signature().to_hex()
                    );
                    self.insert_kernel(&write_txn, header_hash, *kernel, mmr_position)?;
                },
                InsertOutput {
                    header_hash,
                    output,
                    mmr_position,
                } => {
                    trace!(
                        target: LOG_TARGET,
                        "Inserting output `{}`",
                        to_hex(&output.commitment.as_bytes())
                    );
                    self.insert_output(&write_txn, header_hash, *output, mmr_position)?;
                },
                InsertPrunedOutput {
                    header_hash,
                    output_hash,
                    proof_hash,
                    mmr_position,
                } => {
                    self.insert_pruned_output(&write_txn, header_hash, output_hash, proof_hash, mmr_position)?;
                },
                InsertInput {
                    header_hash,
                    input,
                    mmr_position,
                } => {
                    trace!(
                        target: LOG_TARGET,
                        "Inserting input `{}`",
                        to_hex(&input.commitment.as_bytes())
                    );
                    self.insert_input(&write_txn, header_hash, *input, mmr_position)?;
                },
                DeleteOrphanChainTip(hash) => {
                    lmdb_delete(&write_txn, &self.orphan_chain_tips_db, &hash)?;
                },
                InsertOrphanChainTip(hash) => {
                    lmdb_replace(&write_txn, &self.orphan_chain_tips_db, &hash, &hash)?;
                },
                DeleteBlock(hash) => {
                    let hash_hex = hash.to_hex();
                    debug!(target: LOG_TARGET, "Deleting block `{}`", hash_hex);
                    debug!(target: LOG_TARGET, "Deleting UTXOs...");
                    if let Some(height) = self.fetch_height_from_hash(&write_txn, &hash)? {
                        lmdb_delete(&write_txn, &self.block_accumulated_data_db, &height)?;
                    }
                    let rows = lmdb_delete_keys_starting_with::<TransactionOutputRowData>(
                        &write_txn,
                        &self.utxos_db,
                        &hash_hex,
                    )?;

                    for utxo in rows {
                        trace!(target: LOG_TARGET, "Deleting UTXO `{}`", to_hex(&utxo.hash));
                        lmdb_delete(&write_txn, &self.txos_hash_to_index_db, utxo.hash.as_slice())?;
                    }
                    debug!(target: LOG_TARGET, "Deleting kernels...");
                    let kernels = lmdb_delete_keys_starting_with::<TransactionKernelRowData>(
                        &write_txn,
                        &self.kernels_db,
                        &hash_hex,
                    )?;
                    for kernel in kernels {
                        trace!(
                            target: LOG_TARGET,
                            "Deleting excess `{}`",
                            to_hex(kernel.kernel.excess.as_bytes())
                        );
                        lmdb_delete(&write_txn, &self.kernel_excess_index, kernel.kernel.excess.as_bytes())?;
                        let mut excess_sig_key = Vec::<u8>::new();
                        excess_sig_key.extend(kernel.kernel.excess_sig.get_public_nonce().as_bytes());
                        excess_sig_key.extend(kernel.kernel.excess_sig.get_signature().as_bytes());
                        trace!(
                            target: LOG_TARGET,
                            "Deleting excess signature `{}`",
                            to_hex(&excess_sig_key)
                        );
                        lmdb_delete(&write_txn, &self.kernel_excess_sig_index, excess_sig_key.as_slice())?;
                    }
                    debug!(target: LOG_TARGET, "Deleting Inputs...");
                    lmdb_delete_keys_starting_with::<TransactionInputRowData>(&write_txn, &self.inputs_db, &hash_hex)?;
                },
                WriteOperation::InsertMoneroSeedHeight(data, height) => {
                    let current_height =
                        lmdb_get(&write_txn, &self.monero_seed_height_db, &*data.as_str())?.unwrap_or(std::u64::MAX);
                    if height < current_height {
                        lmdb_replace(&write_txn, &self.monero_seed_height_db, &*data.as_str(), &height)?;
                    };
                },
                InsertChainOrphanBlock(chain_block) => {
                    self.insert_orphan_block(&write_txn, &chain_block.block)?;
                    lmdb_replace(
                        &write_txn,
                        &self.orphan_header_accumulated_data_db,
                        chain_block.accumulated_data.hash.as_slice(),
                        &chain_block.accumulated_data,
                    )?;
                },
                UpdatePrunedHashSet {
                    mmr_tree,
                    header_hash,
                    pruned_hash_set,
                } => {
                    let height = self.fetch_height_from_hash(&write_txn, &header_hash).or_not_found(
                        "BlockHash",
                        "hash",
                        header_hash.to_hex(),
                    )?;
                    let mut block_accum_data = self
                        .fetch_block_accumulated_data(&write_txn, height)?
                        .unwrap_or_else(BlockAccumulatedData::default);
                    match mmr_tree {
                        MmrTree::Kernel => block_accum_data.kernels = *pruned_hash_set,

                        MmrTree::Utxo => block_accum_data.outputs = *pruned_hash_set,
                        MmrTree::RangeProof => block_accum_data.range_proofs = *pruned_hash_set,
                    }

                    self.update_block_accumulated_data(&write_txn, height, &block_accum_data)?;
                },
                UpdateDeletedBlockAccumulatedDataWithDiff {
                    header_hash,
                    mut deleted,
                } => {
                    let height = self.fetch_height_from_hash(&write_txn, &header_hash).or_not_found(
                        "BlockHash",
                        "hash",
                        header_hash.to_hex(),
                    )?;
                    let prev_block_accum_data = if height > 0 {
                        self.fetch_block_accumulated_data(&write_txn, height - 1)?
                            .unwrap_or_else(BlockAccumulatedData::default)
                    } else {
                        return Err(ChainStorageError::InvalidOperation(
                            "Tried to update genesis block delete bitmap".to_string(),
                        ));
                    };
                    let mut block_accum_data = self
                        .fetch_block_accumulated_data(&write_txn, height)?
                        .unwrap_or_else(BlockAccumulatedData::default);

                    deleted.or_inplace(&prev_block_accum_data.deleted.deleted);
                    block_accum_data.deleted = DeletedBitmap { deleted };
                    self.update_block_accumulated_data(&write_txn, height, &block_accum_data)?;
                },
                PruneOutputsAndUpdateHorizon {
                    output_positions,
                    horizon,
                } => {
                    let horizon_data = self
                        .fetch_horizon_data()
                        .or_not_found("HorizonData", "", "".to_string())?;
                    let utxo_sum = horizon_data.utxo_sum().clone();
                    for pos in output_positions {
                        let (_height, hash) = lmdb_first_after::<_, (u64, Vec<u8>)>(
                            &write_txn,
                            &self.output_mmr_size_index,
                            &pos.to_be_bytes(),
                        )
                        .or_not_found("BlockHeader", "mmr_position", pos.to_string())?;
                        let key = format!("{}-{:010}", hash.to_hex(), pos,);
                        info!(target: LOG_TARGET, "Pruning output: {}", key);
                        self.prune_output(&write_txn, key.as_str())?;
                    }

                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PrunedHeight,
                        MetadataValue::PrunedHeight(horizon),
                    )?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::HorizonData,
                        MetadataValue::HorizonData(HorizonData::new(horizon_data.kernel_sum().clone(), utxo_sum)),
                    )?;
                },
                UpdateKernelSum {
                    header_hash,
                    kernel_sum,
                } => {
                    let height = self.fetch_height_from_hash(&write_txn, &header_hash).or_not_found(
                        "BlockHash",
                        "hash",
                        header_hash.to_hex(),
                    )?;
                    let mut block_accum_data = self
                        .fetch_block_accumulated_data(&write_txn, height)?
                        .unwrap_or_else(BlockAccumulatedData::default);

                    block_accum_data.kernel_sum = kernel_sum;
                    self.update_block_accumulated_data(&write_txn, height, &block_accum_data)?;
                },
                SetBestBlock {
                    height,
                    hash,
                    accumulated_difficulty,
                } => {
                    self.set_metadata(&write_txn, MetadataKey::ChainHeight, MetadataValue::ChainHeight(height))?;
                    self.set_metadata(&write_txn, MetadataKey::BestBlock, MetadataValue::BestBlock(hash))?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::AccumulatedWork,
                        MetadataValue::AccumulatedWork(accumulated_difficulty),
                    )?;
                },
                SetPruningHorizonConfig(pruning_horizon) => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PruningHorizon,
                        MetadataValue::PruningHorizon(pruning_horizon),
                    )?;
                },
                SetPrunedHeight {
                    height,
                    kernel_sum,
                    utxo_sum,
                } => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PrunedHeight,
                        MetadataValue::PrunedHeight(height),
                    )?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::HorizonData,
                        MetadataValue::HorizonData(HorizonData::new(kernel_sum, utxo_sum)),
                    )?;
                },
            }
        }
        write_txn
            .commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        if self.is_mem_metadata_dirty {
            self.refresh_chain_metadata()?;
        }
        Ok(())
    }

    fn refresh_chain_metadata(&mut self) -> Result<(), ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let metadata = fetch_metadata(&txn, &self.metadata_db)?;
        self.mem_metadata = Some(metadata);
        self.is_mem_metadata_dirty = false;
        Ok(())
    }

    fn prune_output(
        &mut self,
        txn: &WriteTransaction<'_>,
        key: &str,
    ) -> Result<Option<TransactionOutput>, ChainStorageError>
    {
        let mut output: TransactionOutputRowData =
            lmdb_get(txn, &self.utxos_db, key).or_not_found("TransactionOutput", "key", key.to_string())?;
        let result = output.output.clone();
        output.output = None;
        lmdb_replace(txn, &self.utxos_db, key, &output)?;
        Ok(result)
    }

    fn insert_output(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: HashOutput,
        output: TransactionOutput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError>
    {
        let output_hash = output.hash();
        let proof_hash = output.proof.hash();
        let key = format!("{}-{:010}", header_hash.to_hex(), mmr_position,);
        lmdb_insert(
            txn,
            &*self.txos_hash_to_index_db,
            output_hash.as_slice(),
            &(mmr_position, key.clone()),
            "txos_hash_to_index_db",
        )?;
        lmdb_insert(
            txn,
            &*self.utxos_db,
            key.as_str(),
            &TransactionOutputRowData {
                output: Some(output),
                header_hash,
                mmr_position,
                hash: output_hash,
                range_proof_hash: proof_hash,
            },
            "utxos_db",
        )
    }

    fn insert_pruned_output(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: HashOutput,
        output_hash: HashOutput,
        proof_hash: HashOutput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError>
    {
        let key = format!(
            "{}-{:010}-{}-{}",
            header_hash.to_hex(),
            mmr_position,
            output_hash.to_hex(),
            proof_hash.to_hex()
        );
        lmdb_insert(
            txn,
            &*self.txos_hash_to_index_db,
            output_hash.as_slice(),
            &(mmr_position, key.clone()),
            "txos_hash_to_index_db",
        )?;
        lmdb_insert(
            txn,
            &*self.utxos_db,
            key.as_str(),
            &TransactionOutputRowData {
                output: None,
                header_hash,
                mmr_position,
                hash: output_hash,
                range_proof_hash: proof_hash,
            },
            "utxos_db",
        )
    }

    fn insert_kernel(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: HashOutput,
        kernel: TransactionKernel,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError>
    {
        let hash = kernel.hash();
        let key = format!("{}-{:010}-{}", header_hash.to_hex(), mmr_position, hash.to_hex());

        lmdb_insert(
            txn,
            &*self.kernel_excess_index,
            kernel.excess.as_bytes(),
            &(header_hash.clone(), mmr_position, hash.clone()),
            "kernel_excess_index",
        )?;

        let mut excess_sig_key = Vec::<u8>::new();
        excess_sig_key.extend(kernel.excess_sig.get_public_nonce().as_bytes());
        excess_sig_key.extend(kernel.excess_sig.get_signature().as_bytes());
        lmdb_insert(
            txn,
            &*self.kernel_excess_sig_index,
            excess_sig_key.as_slice(),
            &(header_hash.clone(), mmr_position, hash.clone()),
            "kernel_excess_sig_index",
        )?;

        lmdb_insert(
            txn,
            &*self.kernels_db,
            key.as_str(),
            &TransactionKernelRowData {
                kernel,
                header_hash,
                mmr_position,
                hash,
            },
            "kernels_db",
        )
    }

    fn insert_input(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: HashOutput,
        input: TransactionInput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError>
    {
        let hash = input.hash();
        let key = format!("{}-{:010}-{}", header_hash.to_hex(), mmr_position, hash.to_hex());
        lmdb_insert(
            txn,
            &*self.inputs_db,
            key.as_str(),
            &TransactionInputRowData {
                input,
                header_hash,
                mmr_position,
                hash,
            },
            "inputs_db",
        )
    }

    fn set_metadata(
        &mut self,
        txn: &WriteTransaction<'_>,
        k: MetadataKey,
        v: MetadataValue,
    ) -> Result<(), ChainStorageError>
    {
        lmdb_replace(txn, &self.metadata_db, &(k as u32), &v)?;
        self.is_mem_metadata_dirty = true;
        Ok(())
    }

    fn insert_orphan_block(&mut self, txn: &WriteTransaction<'_>, block: &Block) -> Result<(), ChainStorageError> {
        let k = block.hash();
        if lmdb_exists(txn, &self.orphans_db, k.as_slice())? {
            return Ok(());
        }

        lmdb_insert_dup(txn, &self.orphan_parent_map_index, &block.header.prev_hash, &k)?;
        lmdb_replace(txn, &self.orphans_db, k.as_slice(), &block)?;

        Ok(())
    }

    /// Inserts the header and header accumulated data. True is returned if a new header is inserted, otherwise false if
    /// the header already exists
    fn insert_header(
        &mut self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        accum_data: &BlockHeaderAccumulatedData,
    ) -> Result<bool, ChainStorageError>
    {
        if let Some(current_header_at_height) = lmdb_get::<_, BlockHeader>(txn, &self.headers_db, &header.height)? {
            if current_header_at_height.hash() != accum_data.hash {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "There is a different header stored at height {} already. New header ({}), current header: ({})",
                    header.height,
                    accum_data.hash.to_hex(),
                    current_header_at_height.hash().to_hex(),
                )));
            }
            return Ok(false);
        }

        lmdb_replace(&txn, &self.header_accumulated_data_db, &header.height, &accum_data)?;
        lmdb_insert(
            txn,
            &self.block_hashes_db,
            header.hash().as_slice(),
            &header.height,
            "block_hashes_db",
        )?;
        lmdb_insert(txn, &self.headers_db, &header.height, header, "headers_db")?;
        lmdb_insert(
            txn,
            &self.kernel_mmr_size_index,
            &header.kernel_mmr_size.to_be_bytes(),
            &header.height,
            "kernel_mmr_size_index",
        )?;
        lmdb_insert(
            txn,
            &self.output_mmr_size_index,
            &header.output_mmr_size.to_be_bytes(),
            &(header.height, header.hash().as_slice()),
            "output_mmr_size_index",
        )?;
        Ok(true)
    }

    fn op_delete(&mut self, txn: &WriteTransaction<'_>, key: DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(txn, &self.headers_db, &k)?;
                if let Some(v) = val {
                    let hash = v.hash();
                    // Check that there are no utxos or kernels linked to this.

                    if !lmdb_fetch_keys_starting_with::<TransactionKernelRowData>(
                        hash.to_hex().as_str(),
                        &txn,
                        &self.kernels_db,
                    )?
                    .is_empty()
                    {
                        return Err(ChainStorageError::InvalidOperation(
                            "Cannot delete header because there are kernels linked to it".to_string(),
                        ));
                    }
                    if !lmdb_fetch_keys_starting_with::<TransactionOutputRowData>(
                        hash.to_hex().as_str(),
                        &txn,
                        &self.utxos_db,
                    )?
                    .is_empty()
                    {
                        return Err(ChainStorageError::InvalidOperation(
                            "Cannot delete header because there are utxos linked to it".to_string(),
                        ));
                    }

                    lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                    lmdb_delete(&txn, &self.header_accumulated_data_db, &k)?;
                    lmdb_delete(&txn, &self.kernel_mmr_size_index, &v.kernel_mmr_size.to_be_bytes())?;
                    lmdb_delete(&txn, &self.output_mmr_size_index, &v.output_mmr_size.to_be_bytes())?;
                }
            },
            DbKey::BlockHash(_) => {
                unimplemented!("Not supported. Use delete by height");
            },
            DbKey::OrphanBlock(k) => {
                if let Some(orphan) = lmdb_get::<_, Block>(&txn, &self.orphans_db, &k)? {
                    let parent_hash = orphan.header.prev_hash;
                    lmdb_delete_key_value(&txn, &self.orphan_parent_map_index, parent_hash.as_slice(), &k)?;
                    let tip: Option<Vec<u8>> = lmdb_get(&txn, &self.orphan_chain_tips_db, &k)?;
                    if tip.is_some() {
                        if lmdb_get::<_, Block>(&txn, &self.orphans_db, parent_hash.as_slice())?.is_some() {
                            lmdb_insert(
                                &txn,
                                &self.orphan_chain_tips_db,
                                parent_hash.as_slice(),
                                &parent_hash,
                                "orphan_chain_tips_db",
                            )?;
                        }
                        lmdb_delete(&txn, &self.orphan_chain_tips_db, &k)?;
                    }
                    lmdb_delete(&txn, &self.orphans_db, k.as_slice())?;
                }
            },
        }

        Ok(())
    }

    fn insert_block_body(
        &mut self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        body: AggregateBody,
    ) -> Result<(), ChainStorageError>
    {
        let block_hash = header.hash();
        debug!(
            target: LOG_TARGET,
            "Inserting block body for header `{}`: {}",
            block_hash.to_hex(),
            body.to_counts_string()
        );

        let (inputs, outputs, kernels) = body.dissolve();

        let data = if header.height == 0 {
            BlockAccumulatedData::default()
        } else {
            self.fetch_block_accumulated_data(&*txn, header.height - 1)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData".to_string(),
                    field: "prev_hash".to_string(),
                    value: header.prev_hash.to_hex(),
                })?
        };

        let mut total_kernel_sum = Commitment::from_bytes(&[0u8; 32]).expect("Could not create commitment");
        let mut total_utxo_sum = Commitment::from_bytes(&[0u8; 32]).expect("Could not create commitment");
        let BlockAccumulatedData {
            kernels: pruned_kernel_set,
            outputs: pruned_output_set,
            deleted,
            range_proofs: pruned_proof_set,
            ..
        } = data;

        let mut kernel_mmr = MerkleMountainRange::<HashDigest, _>::new(pruned_kernel_set);

        for kernel in kernels {
            total_kernel_sum = &total_kernel_sum + &kernel.excess;
            let pos = kernel_mmr.push(kernel.hash())?;
            trace!(
                target: LOG_TARGET,
                "Inserting kernel `{}`",
                kernel.excess_sig.get_signature().to_hex()
            );
            self.insert_kernel(txn, block_hash.clone(), kernel, pos as u32)?;
        }

        let mut output_mmr = MutableMmr::<HashDigest, _>::new(pruned_output_set, deleted.deleted)?;
        let mut proof_mmr = MerkleMountainRange::<HashDigest, _>::new(pruned_proof_set);
        for output in outputs {
            total_utxo_sum = &total_utxo_sum + &output.commitment;
            output_mmr.push(output.hash())?;
            proof_mmr.push(output.proof().hash())?;
            trace!(
                target: LOG_TARGET,
                "Inserting output `{}`",
                to_hex(&output.commitment.as_bytes())
            );
            self.insert_output(
                txn,
                block_hash.clone(),
                output,
                (proof_mmr.get_leaf_count()? - 1) as u32,
            )?;
        }

        for input in inputs {
            total_utxo_sum = &total_utxo_sum - &input.commitment;
            let index = self
                .fetch_mmr_leaf_index(&**txn, MmrTree::Utxo, &input.hash())?
                .ok_or_else(|| ChainStorageError::UnspendableInput)?;
            if !output_mmr.delete(index) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Could not delete index {} from the output MMR",
                    index
                )));
            }
            trace!(
                target: LOG_TARGET,
                "Inserting input `{}`",
                to_hex(&input.commitment.as_bytes())
            );
            self.insert_input(txn, block_hash.clone(), input, index)?;
        }
        output_mmr.compress();

        self.update_block_accumulated_data(
            txn,
            header.height,
            &BlockAccumulatedData::new(
                kernel_mmr.get_pruned_hash_set()?,
                output_mmr.mmr().get_pruned_hash_set()?,
                proof_mmr.get_pruned_hash_set()?,
                output_mmr.deleted().clone(),
                total_kernel_sum,
            ),
        )?;

        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    fn update_block_accumulated_data(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_height: u64,
        data: &BlockAccumulatedData,
    ) -> Result<(), ChainStorageError>
    {
        lmdb_replace(&txn, &self.block_accumulated_data_db, &header_height, data)
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_mmr_leaf_index(
        &self,
        txn: &ConstTransaction<'_>,
        tree: MmrTree,
        hash: &Hash,
    ) -> Result<Option<u32>, ChainStorageError>
    {
        match tree {
            MmrTree::Utxo => {
                Ok(lmdb_get::<_, (u32, String)>(txn, &self.txos_hash_to_index_db, hash)?.map(|(index, _)| index))
            },
            _ => unimplemented!(),
        }
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_orphan(&self, txn: &ConstTransaction<'_>, hash: &HashOutput) -> Result<Option<Block>, ChainStorageError> {
        let val: Option<Block> = lmdb_get(txn, &self.orphans_db, hash)?;
        Ok(val)
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_block_accumulated_data(
        &self,
        txn: &ConstTransaction<'_>,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        lmdb_get(&txn, &self.block_accumulated_data_db, &height).map_err(Into::into)
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_height_from_hash(
        &self,
        txn: &ConstTransaction<'_>,
        header_hash: &HashOutput,
    ) -> Result<Option<u64>, ChainStorageError>
    {
        lmdb_get(&txn, &self.block_hashes_db, header_hash.as_slice()).map_err(Into::into)
    }

    fn fetch_header_accumulated_data_by_height(
        &self,
        height: u64,
        txn: &ReadTransaction,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>
    {
        lmdb_get(&txn, &self.header_accumulated_data_db, &height)
    }
}

pub fn create_lmdb_database<P: AsRef<Path>>(path: P, config: LMDBConfig) -> Result<LMDBDatabase, ChainStorageError> {
    let flags = db::CREATE;
    let _ = std::fs::create_dir_all(&path);

    let file_lock = acquire_exclusive_file_lock(&path.as_ref().to_path_buf())?;

    let lmdb_store = LMDBBuilder::new()
        .set_path(path)
        .set_env_config(config)
        .set_max_number_of_databases(15)
        .add_database(LMDB_DB_METADATA, flags)
        .add_database(LMDB_DB_HEADERS, flags | db::INTEGERKEY)
        .add_database(LMDB_DB_HEADER_ACCUMULATED_DATA, flags | db::INTEGERKEY)
        .add_database(LMDB_DB_BLOCK_ACCUMULATED_DATA, flags | db::INTEGERKEY)
        .add_database(LMDB_DB_BLOCK_HASHES, flags)
        .add_database(LMDB_DB_UTXOS, flags)
        .add_database(LMDB_DB_INPUTS, flags)
        .add_database(LMDB_DB_TXOS_HASH_TO_INDEX, flags)
        .add_database(LMDB_DB_KERNELS, flags)
        .add_database(LMDB_DB_KERNEL_EXCESS_INDEX, flags)
        .add_database(LMDB_DB_KERNEL_EXCESS_SIG_INDEX, flags)
        .add_database(LMDB_DB_KERNEL_MMR_SIZE_INDEX, flags)
        .add_database(LMDB_DB_UTXO_MMR_SIZE_INDEX, flags)
        .add_database(LMDB_DB_ORPHANS, flags)
        .add_database(LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_MONERO_SEED_HEIGHT, flags)
        .add_database(LMDB_DB_ORPHAN_CHAIN_TIPS, flags)
        .add_database(LMDB_DB_ORPHAN_PARENT_MAP_INDEX, flags | db::DUPSORT)
        .build()
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not create LMDB store:{}", err)))?;
    LMDBDatabase::new(lmdb_store, file_lock)
}

pub fn create_recovery_lmdb_database<P: AsRef<Path>>(path: P) -> Result<(), ChainStorageError> {
    let new_path = path.as_ref().join("temp_recovery");
    let _ = fs::create_dir_all(&new_path);

    let data_file = path.as_ref().join("data.mdb");
    let lock_file = path.as_ref().join("lock.mdb");

    let new_data_file = new_path.join("data.mdb");
    let new_lock_file = new_path.join("lock.mdb");

    fs::rename(data_file, new_data_file)
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not copy LMDB store:{}", err)))?;
    fs::rename(lock_file, new_lock_file)
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not copy LMDB store:{}", err)))?;
    Ok(())
}

fn acquire_exclusive_file_lock(db_path: &PathBuf) -> Result<File, ChainStorageError> {
    let lock_file_path = db_path.join(".chain_storage_file.lock");

    let file = File::create(lock_file_path)?;
    // Attempt to acquire exclusive OS level Write Lock
    if let Err(e) = file.try_lock_exclusive() {
        error!(
            target: LOG_TARGET,
            "Could not acquire exclusive write lock on database lock file: {:?}", e
        );
        return Err(ChainStorageError::CannotAcquireFileLock);
    }

    Ok(file)
}

impl BlockchainBackend for LMDBDatabase {
    fn write(&mut self, txn: DbTransaction) -> Result<(), ChainStorageError> {
        if txn.operations().is_empty() {
            return Ok(());
        }
        LMDBStore::resize_if_required(&self.env, &self.env_config)?;

        let mark = Instant::now();
        let num_operations = txn.operations().len();
        match self.apply_db_transaction(txn) {
            Ok(_) => {
                trace!(
                    target: LOG_TARGET,
                    "Database completed {} operation(s) in {:.0?}",
                    num_operations,
                    mark.elapsed()
                );
                Ok(())
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Failed to apply DB transaction: {}", e);
                Err(e)
            },
        }
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let res = match key {
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(&txn, &self.headers_db, k)?;
                val.map(|val| DbValue::BlockHeader(Box::new(val)))
            },
            DbKey::BlockHash(hash) => {
                // TODO: investigate making BlockHash a `[u8;32]`
                if hash.len() != BLOCK_HASH_LENGTH {
                    return Err(ChainStorageError::InvalidQuery(format!(
                        "Invalid block hash length. Expected length: {} Got: {}",
                        BLOCK_HASH_LENGTH,
                        hash.len()
                    )));
                }
                let k: Option<u64> = self.fetch_height_from_hash(&txn, hash)?;
                match k {
                    Some(k) => {
                        trace!(
                            target: LOG_TARGET,
                            "Header with hash:{} found at height:{}",
                            hash.to_hex(),
                            k
                        );
                        let val: Option<BlockHeader> = lmdb_get(&txn, &self.headers_db, &k)?;
                        val.map(|val| DbValue::BlockHash(Box::new(val)))
                    },
                    None => {
                        trace!(
                            target: LOG_TARGET,
                            "Header with hash:{} not found in block_hashes_db",
                            hash.to_hex()
                        );
                        None
                    },
                }
            },
            DbKey::OrphanBlock(k) => self
                .fetch_orphan(&txn, k)?
                .map(|val| DbValue::OrphanBlock(Box::new(val))),
        };
        Ok(res)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        Ok(match key {
            DbKey::BlockHeader(k) => lmdb_exists(&txn, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&txn, &self.block_hashes_db, h)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&txn, &self.orphans_db, k)?,
        })
    }

    fn fetch_header_and_accumulated_data(
        &self,
        height: u64,
    ) -> Result<(BlockHeader, BlockHeaderAccumulatedData), ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let header: BlockHeader =
            lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeader".to_string(),
                field: "height".to_string(),
                value: height.to_string(),
            })?;

        let accum_data = self
            .fetch_header_accumulated_data_by_height(height, &txn)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData".to_string(),
                field: "height".to_string(),
                value: height.to_string(),
            })?;

        Ok((header, accum_data))
    }

    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let height: Option<u64> = self.fetch_height_from_hash(&txn, hash)?;

        if let Some(h) = height {
            self.fetch_header_accumulated_data_by_height(h, &txn)
        } else {
            Ok(None)
        }
    }

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let height: Option<u64> = self.fetch_height_from_hash(&txn, hash)?;
        if let Some(h) = height {
            let (header, accum) = self.fetch_header_and_accumulated_data(h)?;

            return Ok(Some(ChainHeader {
                header,
                accumulated_data: accum,
            }));
        }
        let orphan_accum: Option<BlockHeaderAccumulatedData> =
            lmdb_get(&txn, &self.orphan_header_accumulated_data_db, hash.as_slice())?;
        if let Some(accum) = orphan_accum {
            if let Some(orphan) = self.fetch_orphan(&txn, hash)? {
                return Ok(Some(ChainHeader {
                    header: orphan.header,
                    accumulated_data: accum,
                }));
            }
        }
        Ok(None)
    }

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        if let Some(height) =
            lmdb_first_after::<_, u64>(&txn, &self.kernel_mmr_size_index, &mmr_position.to_be_bytes())?
        {
            let header: BlockHeader =
                lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeader".to_string(),
                    field: "height".to_string(),
                    value: height.to_string(),
                })?;

            let accum_data = self
                .fetch_header_accumulated_data_by_height(height, &txn)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeaderAccumulatedData".to_string(),
                    field: "height".to_string(),
                    value: height.to_string(),
                })?;

            Ok(ChainHeader {
                header,
                accumulated_data: accum_data,
            })
        } else {
            Err(ChainStorageError::ValueNotFound {
                entity: "BlockHeader".to_string(),
                field: "mmr_position".to_string(),
                value: mmr_position.to_string(),
            })
        }
    }

    // TODO: Can be merged with the method above
    fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        if let Some((height, _hash)) =
            lmdb_first_after::<_, (u64, Vec<u8>)>(&txn, &self.output_mmr_size_index, &mmr_position.to_be_bytes())?
        {
            let header: BlockHeader =
                lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeader".to_string(),
                    field: "height".to_string(),
                    value: height.to_string(),
                })?;

            let accum_data = self
                .fetch_header_accumulated_data_by_height(height, &txn)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeaderAccumulatedData".to_string(),
                    field: "height".to_string(),
                    value: height.to_string(),
                })?;

            Ok(ChainHeader {
                header,
                accumulated_data: accum_data,
            })
        } else {
            Err(ChainStorageError::ValueNotFound {
                entity: "BlockHeader".to_string(),
                field: "mmr_position".to_string(),
                value: mmr_position.to_string(),
            })
        }
    }

    fn is_empty(&self) -> Result<bool, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;

        Ok(lmdb_len(&txn, &self.headers_db)? == 0)
    }

    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        if let Some(height) = self.fetch_height_from_hash(&txn, header_hash)? {
            self.fetch_block_accumulated_data(&txn, height)
        } else {
            Ok(None)
        }
    }

    fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        self.fetch_block_accumulated_data(&txn, height)
    }

    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        Ok(
            lmdb_fetch_keys_starting_with(header_hash.to_hex().as_str(), &txn, &self.kernels_db)?
                .into_iter()
                .map(|f: TransactionKernelRowData| f.kernel)
                .collect(),
        )
    }

    fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        if let Some((header_hash, mmr_position, hash)) =
            lmdb_get::<_, (HashOutput, u32, HashOutput)>(&txn, &self.kernel_excess_index, excess)?
        {
            let key = format!("{}-{:010}-{}", header_hash.to_hex(), mmr_position, hash.to_hex());
            Ok(lmdb_get(&txn, &self.kernels_db, key.as_str())?
                .map(|kernel: TransactionKernelRowData| (kernel.kernel, header_hash)))
        } else {
            Ok(None)
        }
    }

    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        let mut key = Vec::<u8>::new();
        key.extend(excess_sig.get_public_nonce().as_bytes());
        key.extend(excess_sig.get_signature().as_bytes());
        if let Some((header_hash, mmr_position, hash)) =
            lmdb_get::<_, (HashOutput, u32, HashOutput)>(&txn, &self.kernel_excess_sig_index, key.as_slice())?
        {
            let key = format!("{}-{:010}-{}", header_hash.to_hex(), mmr_position, hash.to_hex());
            Ok(lmdb_get(&txn, &self.kernels_db, key.as_str())?
                .map(|kernel: TransactionKernelRowData| (kernel.kernel, header_hash)))
        } else {
            Ok(None)
        }
    }

    fn fetch_kernels_by_mmr_position(&self, start: u64, end: u64) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        if let Some(start_height) = lmdb_first_after(&txn, &self.kernel_mmr_size_index, &(start + 1).to_be_bytes())? {
            let end_height: u64 =
                lmdb_first_after(&txn, &self.kernel_mmr_size_index, &(end + 1).to_be_bytes())?.unwrap_or(start_height);

            let previous_mmr_count = if start_height == 0 {
                0
            } else {
                let header: BlockHeader =
                    lmdb_get(&txn, &self.headers_db, &(start_height - 1))?.expect("Header should exist");
                debug!(target: LOG_TARGET, "Previous header:{}", header);
                header.kernel_mmr_size
            };

            let total_size = (end - start) as usize + 1;
            let mut result = Vec::with_capacity(total_size);

            let mut skip_amount = (start - previous_mmr_count) as usize;
            debug!(
                target: LOG_TARGET,
                "Fetching kernels by MMR position. Start {}, end {}, in headers at height {}-{},  prev mmr count: {}, \
                 skipping the first:{}",
                start,
                end,
                start_height,
                end_height,
                previous_mmr_count,
                skip_amount
            );

            for height in start_height..=end_height {
                let hash = lmdb_get::<_, BlockHeaderAccumulatedData>(&txn, &self.header_accumulated_data_db, &height)?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "BlockHeader".to_string(),
                        field: "height".to_string(),
                        value: height.to_string(),
                    })?
                    .hash;

                result.extend(
                    lmdb_fetch_keys_starting_with::<TransactionKernelRowData>(
                        hash.to_hex().as_str(),
                        &txn,
                        &self.kernels_db,
                    )?
                    .into_iter()
                    .skip(skip_amount)
                    .take(total_size - result.len())
                    .map(|f| f.kernel),
                );

                skip_amount = 0;
            }
            Ok(result)
        } else {
            Ok(vec![])
        }
    }

    fn fetch_utxos_by_mmr_position(
        &self,
        start: u64,
        end: u64,
        deleted: &Bitmap,
    ) -> Result<(Vec<PrunedOutput>, Vec<Bitmap>), ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        if let Some(start_height) = lmdb_first_after(&txn, &self.output_mmr_size_index, &(start + 1).to_be_bytes())? {
            let end_height: u64 =
                lmdb_first_after(&txn, &self.output_mmr_size_index, &(end + 1).to_be_bytes())?.unwrap_or(start_height);

            let previous_mmr_count = if start_height == 0 {
                0
            } else {
                let header: BlockHeader =
                    lmdb_get(&txn, &self.headers_db, &(start_height - 1))?.expect("Header should exist");
                debug!(target: LOG_TARGET, "Previous header:{}", header);
                header.output_mmr_size
            };

            let total_size = (end - start) as usize + 1;
            let mut result = Vec::with_capacity(total_size);
            let mut deleted_result = vec![];

            let mut skip_amount = (start - previous_mmr_count) as usize;
            debug!(
                target: LOG_TARGET,
                "Fetching outputs by MMR position. Start {}, end {}, starting in header at height {},  prev mmr \
                 count: {}, skipping the first:{}",
                start,
                end,
                start_height,
                previous_mmr_count,
                skip_amount
            );

            for height in start_height..=end_height {
                let accum_data =
                    lmdb_get::<_, BlockHeaderAccumulatedData>(&txn, &self.header_accumulated_data_db, &height)?
                        .ok_or_else(|| ChainStorageError::ValueNotFound {
                            entity: "BlockHeader".to_string(),
                            field: "height".to_string(),
                            value: height.to_string(),
                        })?;

                result.extend(
                    lmdb_fetch_keys_starting_with::<TransactionOutputRowData>(
                        accum_data.hash.to_hex().as_str(),
                        &txn,
                        &self.utxos_db,
                    )?
                    .into_iter()
                    .skip(skip_amount)
                    .take(total_size - result.len())
                    .map(|row| {
                        if deleted.contains(row.mmr_position) {
                            return PrunedOutput::Pruned {
                                output_hash: row.hash,
                                range_proof_hash: row.range_proof_hash,
                            };
                        }
                        if let Some(output) = &row.output {
                            PrunedOutput::NotPruned { output: output.clone() }
                        } else {
                            PrunedOutput::Pruned {
                                output_hash: row.hash,
                                range_proof_hash: row.range_proof_hash,
                            }
                        }
                    }),
                );

                let block_accum_data = self
                    .fetch_block_accumulated_data(&txn, height)
                    .or_not_found("BlockAccumulatedData", "height", height.to_string())?
                    .deleted()
                    .clone();
                let prev_block_accum_data = if height == 0 {
                    Bitmap::create()
                } else {
                    self.fetch_block_accumulated_data(&txn, height - 1)
                        .or_not_found("BlockAccumulatedData", "height", height.to_string())?
                        .deleted()
                        .clone()
                };
                let diff_bitmap = block_accum_data.xor(&prev_block_accum_data);
                deleted_result.push(diff_bitmap);

                skip_amount = 0;
            }
            Ok((result, deleted_result))
        } else {
            Ok((vec![], vec![]))
        }
    }

    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<(TransactionOutput, u32)>, ChainStorageError> {
        debug!(target: LOG_TARGET, "Fetch output: {}", output_hash.to_hex());
        let txn = ReadTransaction::new(&*self.env)?;
        if let Some((index, key)) =
            lmdb_get::<_, (u32, String)>(&txn, &self.txos_hash_to_index_db, output_hash.as_slice())?
        {
            debug!(
                target: LOG_TARGET,
                "Fetch output: {} Found ({}, {})",
                output_hash.to_hex(),
                index,
                key
            );
            if let Some(output) = lmdb_get::<_, TransactionOutputRowData>(&txn, &self.utxos_db, key.as_str())? {
                if output.output.is_none() {
                    error!(
                        target: LOG_TARGET,
                        "Tried to fetch pruned output: {} ({}, {})",
                        output_hash.to_hex(),
                        index,
                        key
                    );
                    unimplemented!("Output has been pruned");
                }
                Ok(Some((output.output.unwrap(), output.mmr_position)))
            } else {
                Ok(None)
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Fetch output: {} NOT found in index",
                output_hash.to_hex()
            );
            Ok(None)
        }
    }

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<PrunedOutput>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        Ok(
            lmdb_fetch_keys_starting_with(header_hash.to_hex().as_str(), &txn, &self.utxos_db)?
                .into_iter()
                .map(|f: TransactionOutputRowData| match f.output {
                    Some(o) => PrunedOutput::NotPruned { output: o },
                    None => PrunedOutput::Pruned {
                        output_hash: f.hash,
                        range_proof_hash: f.range_proof_hash,
                    },
                })
                .collect(),
        )
    }

    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        Ok(
            lmdb_fetch_keys_starting_with(header_hash.to_hex().as_str(), &txn, &self.inputs_db)?
                .into_iter()
                .map(|f: TransactionInputRowData| f.input)
                .collect(),
        )
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        match tree {
            MmrTree::Kernel => Ok(lmdb_len(&txn, &self.kernels_db)? as u64),
            MmrTree::Utxo => Ok(lmdb_len(&txn, &self.utxos_db)? as u64),
            MmrTree::RangeProof => {
                //  lmdb_len(&txn, &self.utxo)
                unimplemented!("Need to get rangeproof mmr size")
            },
        }
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        self.fetch_mmr_leaf_index(&*txn, tree, hash)
    }

    /// Returns the number of blocks in the block orphan pool.
    fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        trace!(target: LOG_TARGET, "Get orphan count");
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_len(&txn, &self.orphans_db)
    }

    /// Finds and returns the last stored header.
    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_last(&txn, &self.headers_db)?.ok_or_else(|| {
            ChainStorageError::InvalidOperation("Cannot fetch last header because database is empty".to_string())
        })
    }

    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let metadata = self.fetch_chain_metadata()?;
        let height = metadata.height_of_longest_chain();
        let header = lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
            entity: "Header".to_string(),
            field: "height".to_string(),
            value: height.to_string(),
        })?;
        let accumulated_data = self
            .fetch_header_accumulated_data_by_height(metadata.height_of_longest_chain(), &txn)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData".to_string(),
                field: "height".to_string(),
                value: height.to_string(),
            })?;
        Ok(ChainHeader {
            header,
            accumulated_data,
        })
    }

    /// Returns the metadata of the chain.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        // This should only be None if the database is empty
        self.mem_metadata.as_ref().cloned().ok_or_else(|| {
            ChainStorageError::AccessError("Cannot retrieve chain metadata because the database is empty".to_string())
        })
    }

    fn utxo_count(&self) -> Result<usize, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_len(&txn, &self.utxos_db)
    }

    fn kernel_count(&self) -> Result<usize, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        lmdb_len(&txn, &self.kernels_db)
    }

    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        trace!(target: LOG_TARGET, "Call to fetch_orphan_chain_tips()");
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        if lmdb_get::<_, HashOutput>(&txn, &self.orphan_chain_tips_db, hash.as_slice())?.is_some() {
            let orphan: Block =
                lmdb_get(&txn, &self.orphans_db, hash.as_slice())?.ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "Orphan".to_string(),
                    field: "hash".to_string(),
                    value: hash.to_hex(),
                })?;
            let accum_data =
                lmdb_get(&txn, &self.orphan_header_accumulated_data_db, hash.as_slice())?.ok_or_else(|| {
                    ChainStorageError::ValueNotFound {
                        entity: "Orphan accumulated data".to_string(),
                        field: "hash".to_string(),
                        value: hash.to_hex(),
                    }
                })?;
            Ok(Some(ChainHeader {
                header: orphan.header,
                accumulated_data: accum_data,
            }))
        } else {
            Ok(None)
        }
    }

    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
        trace!(
            target: LOG_TARGET,
            "Call to fetch_orphan_children_of({})",
            hash.to_hex()
        );
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let orphan_hashes: Vec<HashOutput> = lmdb_get_multiple(&txn, &self.orphan_parent_map_index, hash.as_slice())?;
        let mut res = vec![];
        for hash in orphan_hashes {
            res.push(lmdb_get(&txn, &self.orphans_db, hash.as_slice())?.ok_or_else(|| {
                ChainStorageError::ValueNotFound {
                    entity: "Orphan".to_string(),
                    field: "hash".to_string(),
                    value: hash.to_hex(),
                }
            })?)
        }
        Ok(res)
    }

    fn fetch_orphan_header_accumulated_data(
        &self,
        hash: HashOutput,
    ) -> Result<BlockHeaderAccumulatedData, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_get(&txn, &self.orphan_header_accumulated_data_db, hash.as_slice())?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "Orphan accumulated data".to_string(),
                field: "hash".to_string(),
                value: hash.to_hex(),
            }
        })
    }

    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>
    {
        let orphan_count = self.orphan_count()?;
        let num_over_limit = orphan_count.saturating_sub(orphan_storage_capacity);
        if num_over_limit == 0 {
            return Ok(());
        }
        debug!(
            target: LOG_TARGET,
            "Orphan block storage limit of {} reached, performing cleanup of {} entries.",
            orphan_storage_capacity,
            num_over_limit,
        );

        let mut orphans;

        {
            let read_txn = ReadTransaction::new(&*self.env)?;

            orphans = lmdb_filter_map_values(&read_txn, &self.orphans_db, |block: Block| {
                Ok(Some((block.header.height, block.hash())))
            })?;
        }

        orphans.sort_by(|a, b| a.0.cmp(&b.0));
        let mut txn = DbTransaction::new();
        for (removed_count, (height, block_hash)) in orphans.into_iter().enumerate() {
            if height > horizon_height && removed_count >= num_over_limit {
                break;
            }
            debug!(
                target: LOG_TARGET,
                "Discarding orphan block #{} ({}).",
                height,
                block_hash.to_hex()
            );
            txn.delete(DbKey::OrphanBlock(block_hash.clone()));
        }
        self.write(txn)?;

        Ok(())
    }

    fn fetch_monero_seed_first_seen_height(&self, seed: &str) -> Result<u64, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        Ok(lmdb_get(&txn, &self.monero_seed_height_db, seed)?.unwrap_or(0))
    }

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        fetch_horizon_data(&txn, &self.metadata_db)
    }
}

// Fetch the chain metadata
fn fetch_metadata(txn: &ConstTransaction<'_>, db: &Database) -> Result<ChainMetadata, ChainStorageError> {
    Ok(ChainMetadata::new(
        fetch_chain_height(&txn, &db)?,
        fetch_best_block(&txn, &db)?,
        fetch_pruning_horizon(&txn, &db)?,
        fetch_pruned_height(&txn, &db)?,
        fetch_accumulated_work(&txn, &db)?,
    ))
}

// Fetches the chain height from the provided metadata db.
fn fetch_chain_height(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::ChainHeight;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::ChainHeight(height)) => Ok(height),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata".to_string(),
            field: "ChainHeight".to_string(),
            value: "".to_string(),
        }),
    }
}

// // Fetches the effective pruned height from the provided metadata db.
fn fetch_pruned_height(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PrunedHeight;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::PrunedHeight(height)) => Ok(height),
        _ => Ok(0),
    }
}
// Fetches the best block hash from the provided metadata db.
fn fetch_horizon_data(txn: &ConstTransaction<'_>, db: &Database) -> Result<Option<HorizonData>, ChainStorageError> {
    let k = MetadataKey::HorizonData;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::HorizonData(data)) => Ok(Some(data)),
        None => Ok(None),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata".to_string(),
            field: "HorizonData".to_string(),
            value: "".to_string(),
        }),
    }
}
// Fetches the best block hash from the provided metadata db.
fn fetch_best_block(txn: &ConstTransaction<'_>, db: &Database) -> Result<BlockHash, ChainStorageError> {
    let k = MetadataKey::BestBlock;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::BestBlock(best_block)) => Ok(best_block),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata".to_string(),
            field: "BestBlock".to_string(),
            value: "".to_string(),
        }),
    }
}

// Fetches the accumulated work from the provided metadata db.
fn fetch_accumulated_work(txn: &ConstTransaction<'_>, db: &Database) -> Result<u128, ChainStorageError> {
    let k = MetadataKey::AccumulatedWork;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::AccumulatedWork(accumulated_difficulty)) => Ok(accumulated_difficulty),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata".to_string(),
            field: "AccumulatedWork".to_string(),
            value: "".to_string(),
        }),
    }
}

// Fetches the pruning horizon from the provided metadata db.
fn fetch_pruning_horizon(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PruningHorizon;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::PruningHorizon(pruning_horizon)) => Ok(pruning_horizon),
        _ => Ok(0),
    }
}

fn get_database(store: &LMDBStore, name: &str) -> Result<DatabaseRef, ChainStorageError> {
    let handle = store
        .get_handle(name)
        .ok_or_else(|| ChainStorageError::CriticalError(format!("Could not get `{}` database", name)))?;
    Ok(handle.db())
}

#[derive(Debug, Clone, PartialEq, Copy)]
enum MetadataKey {
    ChainHeight,
    BestBlock,
    AccumulatedWork,
    PruningHorizon,
    PrunedHeight,
    HorizonData,
}

impl fmt::Display for MetadataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataKey::ChainHeight => f.write_str("Current chain height"),
            MetadataKey::AccumulatedWork => f.write_str("Total accumulated work"),
            MetadataKey::PruningHorizon => f.write_str("Pruning horizon"),
            MetadataKey::PrunedHeight => f.write_str("Effective pruned height"),
            MetadataKey::BestBlock => f.write_str("Chain tip block hash"),
            MetadataKey::HorizonData => f.write_str("Database info"),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
enum MetadataValue {
    ChainHeight(u64),
    BestBlock(BlockHash),
    AccumulatedWork(u128),
    PruningHorizon(u64),
    PrunedHeight(u64),
    HorizonData(HorizonData),
}

impl fmt::Display for MetadataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataValue::ChainHeight(h) => write!(f, "Chain height is {}", h),
            MetadataValue::AccumulatedWork(d) => write!(f, "Total accumulated work is {}", d),
            MetadataValue::PruningHorizon(h) => write!(f, "Pruning horizon is {}", h),
            MetadataValue::PrunedHeight(height) => write!(f, "Effective pruned height is {}", height),
            MetadataValue::BestBlock(hash) => write!(f, "Chain tip block hash is {}", hash.to_hex()),
            MetadataValue::HorizonData(_) => write!(f, "Horizon data"),
        }
    }
}

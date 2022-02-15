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

// Because we use dynamically sized u8 vectors for hash types through the type alias HashOutput,
// let's ignore this clippy error in this module

#![allow(clippy::ptr_arg)]

use std::{
    fmt,
    fmt::Formatter,
    fs,
    fs::File,
    mem,
    ops::{Deref, Range},
    path::Path,
    sync::Arc,
    time::Instant,
};

use blake2::Digest;
use croaring::Bitmap;
use fs2::FileExt;
use lmdb_zero::{open, ConstTransaction, Database, Environment, ReadTransaction, WriteTransaction};
use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{BlockHash, Commitment, HashDigest, HashOutput, PublicKey, Signature, BLOCK_HASH_LENGTH},
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex, ByteArray};
use tari_mmr::{Hash, MerkleMountainRange, MutableMmr};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBConfig, LMDBStore};
use tari_utilities::hex::to_hex;

use crate::{
    blocks::{
        Block,
        BlockAccumulatedData,
        BlockHeader,
        BlockHeaderAccumulatedData,
        ChainBlock,
        ChainHeader,
        DeletedBitmap,
        UpdateBlockAccumulatedData,
    },
    chain_storage::{
        db_transaction::{DbKey, DbTransaction, DbValue, WriteOperation},
        error::{ChainStorageError, OrNotFound},
        lmdb_db::{
            lmdb::{
                fetch_db_entry_sizes,
                lmdb_clear,
                lmdb_delete,
                lmdb_delete_each_where,
                lmdb_delete_key_value,
                lmdb_delete_keys_starting_with,
                lmdb_exists,
                lmdb_fetch_matching_after,
                lmdb_filter_map_values,
                lmdb_first_after,
                lmdb_get,
                lmdb_get_multiple,
                lmdb_get_prefix_cursor,
                lmdb_insert,
                lmdb_insert_dup,
                lmdb_last,
                lmdb_len,
                lmdb_replace,
            },
            TransactionInputRowData,
            TransactionInputRowDataRef,
            TransactionKernelRowData,
            TransactionOutputRowData,
        },
        stats::DbTotalSizeStats,
        utxo_mined_info::UtxoMinedInfo,
        BlockchainBackend,
        DbBasicStats,
        DbSize,
        HorizonData,
        MmrTree,
        PrunedOutput,
        Reorg,
    },
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
    },
};

type DatabaseRef = Arc<Database<'static>>;

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db";

const LMDB_DB_METADATA: &str = "metadata";
const LMDB_DB_HEADERS: &str = "headers";
const LMDB_DB_HEADER_ACCUMULATED_DATA: &str = "header_accumulated_data";
const LMDB_DB_BLOCK_ACCUMULATED_DATA: &str = "mmr_peak_data";
const LMDB_DB_BLOCK_HASHES: &str = "block_hashes";
const LMDB_DB_UTXOS: &str = "utxos";
const LMDB_DB_INPUTS: &str = "inputs";
const LMDB_DB_TXOS_HASH_TO_INDEX: &str = "txos_hash_to_index";
const LMDB_DB_KERNELS: &str = "kernels";
const LMDB_DB_KERNEL_EXCESS_INDEX: &str = "kernel_excess_index";
const LMDB_DB_KERNEL_EXCESS_SIG_INDEX: &str = "kernel_excess_sig_index";
const LMDB_DB_KERNEL_MMR_SIZE_INDEX: &str = "kernel_mmr_size_index";
const LMDB_DB_UTXO_MMR_SIZE_INDEX: &str = "utxo_mmr_size_index";
const LMDB_DB_DELETED_TXO_MMR_POSITION_TO_HEIGHT_INDEX: &str = "deleted_txo_mmr_position_to_height_index";
const LMDB_DB_UTXO_COMMITMENT_INDEX: &str = "utxo_commitment_index";
const LMDB_DB_UNIQUE_ID_INDEX: &str = "unique_id_index";
const LMDB_DB_ORPHANS: &str = "orphans";
const LMDB_DB_MONERO_SEED_HEIGHT: &str = "monero_seed_height";
const LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA: &str = "orphan_accumulated_data";
const LMDB_DB_ORPHAN_CHAIN_TIPS: &str = "orphan_chain_tips";
const LMDB_DB_ORPHAN_PARENT_MAP_INDEX: &str = "orphan_parent_map_index";
const LMDB_DB_BAD_BLOCK_LIST: &str = "bad_blocks";
const LMDB_DB_REORGS: &str = "reorgs";

pub fn create_lmdb_database<P: AsRef<Path>>(path: P, config: LMDBConfig) -> Result<LMDBDatabase, ChainStorageError> {
    let flags = db::CREATE;
    debug!(target: LOG_TARGET, "Creating LMDB database at {:?}", path.as_ref());
    std::fs::create_dir_all(&path)?;

    let file_lock = acquire_exclusive_file_lock(&path.as_ref().to_path_buf())?;

    let lmdb_store = LMDBBuilder::new()
        .set_path(path)
        // NOLOCK - No lock required because we manage the DB locking using a RwLock
        .set_env_flags(open::NOLOCK)
        .set_env_config(config)
        .set_max_number_of_databases(40)
        .add_database(LMDB_DB_METADATA, flags | db::INTEGERKEY)
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
        .add_database(LMDB_DB_UTXO_COMMITMENT_INDEX, flags)
        .add_database(LMDB_DB_UNIQUE_ID_INDEX, flags)
        .add_database(LMDB_DB_DELETED_TXO_MMR_POSITION_TO_HEIGHT_INDEX, flags | db::INTEGERKEY)
        .add_database(LMDB_DB_ORPHANS, flags)
        .add_database(LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_MONERO_SEED_HEIGHT, flags)
        .add_database(LMDB_DB_ORPHAN_CHAIN_TIPS, flags)
        .add_database(LMDB_DB_ORPHAN_PARENT_MAP_INDEX, flags | db::DUPSORT)
        .add_database(LMDB_DB_BAD_BLOCK_LIST, flags)
        .add_database(LMDB_DB_REORGS, flags | db::INTEGERKEY)
        .build()
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not create LMDB store:{}", err)))?;
    debug!(target: LOG_TARGET, "LMDB database creation successful");
    LMDBDatabase::new(lmdb_store, file_lock)
}

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase {
    env: Arc<Environment>,
    env_config: LMDBConfig,
    metadata_db: DatabaseRef,
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
    utxo_commitment_index: DatabaseRef,
    unique_id_index: DatabaseRef,
    deleted_txo_mmr_position_to_height_index: DatabaseRef,
    orphans_db: DatabaseRef,
    monero_seed_height_db: DatabaseRef,
    orphan_header_accumulated_data_db: DatabaseRef,
    orphan_chain_tips_db: DatabaseRef,
    orphan_parent_map_index: DatabaseRef,
    bad_blocks: DatabaseRef,
    reorgs: DatabaseRef,
    _file_lock: Arc<File>,
}

impl LMDBDatabase {
    pub fn new(store: LMDBStore, file_lock: File) -> Result<Self, ChainStorageError> {
        let env = store.env();

        let db = Self {
            metadata_db: get_database(&store, LMDB_DB_METADATA)?,
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
            utxo_commitment_index: get_database(&store, LMDB_DB_UTXO_COMMITMENT_INDEX)?,
            unique_id_index: get_database(&store, LMDB_DB_UNIQUE_ID_INDEX)?,
            deleted_txo_mmr_position_to_height_index: get_database(
                &store,
                LMDB_DB_DELETED_TXO_MMR_POSITION_TO_HEIGHT_INDEX,
            )?,
            orphans_db: get_database(&store, LMDB_DB_ORPHANS)?,
            orphan_header_accumulated_data_db: get_database(&store, LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA)?,
            monero_seed_height_db: get_database(&store, LMDB_DB_MONERO_SEED_HEIGHT)?,
            orphan_chain_tips_db: get_database(&store, LMDB_DB_ORPHAN_CHAIN_TIPS)?,
            orphan_parent_map_index: get_database(&store, LMDB_DB_ORPHAN_PARENT_MAP_INDEX)?,
            bad_blocks: get_database(&store, LMDB_DB_BAD_BLOCK_LIST)?,
            reorgs: get_database(&store, LMDB_DB_REORGS)?,
            env,
            env_config: store.env_config(),
            _file_lock: Arc::new(file_lock),
        };

        Ok(db)
    }

    /// Try to establish a read lock on the LMDB database. If an exclusive write lock has been previously acquired, this
    /// method will block until that lock is released.
    fn read_transaction(&self) -> Result<ReadTransaction<'_>, ChainStorageError> {
        ReadTransaction::new(&*self.env).map_err(Into::into)
    }

    /// Try to establish an exclusive write lock on the LMDB database. This method will block until an exclusive lock is
    /// obtained or an LMDB error is encountered (http://www.lmdb.tech/doc/group__mdb.html#gad7ea55da06b77513609efebd44b26920).
    fn write_transaction(&self) -> Result<WriteTransaction<'_>, ChainStorageError> {
        WriteTransaction::new(&*self.env).map_err(Into::into)
    }

    fn apply_db_transaction(&mut self, txn: &DbTransaction) -> Result<(), ChainStorageError> {
        use WriteOperation::*;
        let write_txn = self.write_transaction()?;
        for op in txn.operations() {
            trace!(target: LOG_TARGET, "[apply_db_transaction] WriteOperation: {}", op);
            match op {
                InsertOrphanBlock(block) => self.insert_orphan_block(&write_txn, block)?,
                InsertChainHeader { header } => {
                    self.insert_header(&write_txn, header.header(), header.accumulated_data())?;
                },
                InsertBlockBody { block } => {
                    self.insert_block_body(&write_txn, block.header(), block.block().body.clone())?;
                },
                InsertKernel {
                    header_hash,
                    kernel,
                    mmr_position,
                } => {
                    self.insert_kernel(&write_txn, header_hash, &**kernel, *mmr_position)?;
                },
                InsertOutput {
                    header_hash,
                    header_height,
                    output,
                    mmr_position,
                } => {
                    self.insert_output(&write_txn, header_hash, *header_height, &*output, *mmr_position)?;
                },
                InsertPrunedOutput {
                    header_hash,
                    header_height,
                    output_hash,
                    witness_hash,
                    mmr_position,
                } => {
                    self.insert_pruned_output(
                        &write_txn,
                        header_hash,
                        *header_height,
                        output_hash,
                        witness_hash,
                        *mmr_position,
                    )?;
                },
                DeleteHeader(height) => {
                    self.delete_header(&write_txn, *height)?;
                },
                DeleteOrphan(hash) => {
                    self.delete_orphan(&write_txn, hash)?;
                },
                DeleteOrphanChainTip(hash) => {
                    lmdb_delete(&write_txn, &self.orphan_chain_tips_db, hash, "orphan_chain_tips_db")?;
                },
                InsertOrphanChainTip(hash) => {
                    lmdb_insert(
                        &write_txn,
                        &self.orphan_chain_tips_db,
                        hash,
                        hash,
                        "orphan_chain_tips_db",
                    )?;
                },
                DeleteBlock(hash) => {
                    self.delete_block_body(&write_txn, hash)?;
                },
                InsertMoneroSeedHeight(data, height) => {
                    self.insert_monero_seed_height(&write_txn, data, *height)?;
                },
                SetAccumulatedDataForOrphan(accumulated_data) => {
                    self.set_accumulated_data_for_orphan(&write_txn, accumulated_data)?;
                },
                InsertChainOrphanBlock(chain_block) => {
                    self.insert_orphan_block(&write_txn, chain_block.block())?;
                    self.set_accumulated_data_for_orphan(&write_txn, chain_block.accumulated_data())?;
                },
                UpdateBlockAccumulatedData { header_hash, values } => {
                    self.update_block_accumulated_data(&write_txn, header_hash, values.clone())?;
                },
                UpdateDeletedBitmap { deleted } => {
                    let mut bitmap = self.load_deleted_bitmap_model(&write_txn)?;
                    bitmap.merge(deleted)?;
                    bitmap.save()?;
                },
                PruneOutputsAtMmrPositions { output_positions } => {
                    self.prune_outputs_at_positions(&write_txn, output_positions)?;
                },
                DeleteAllInputsInBlock { block_hash } => {
                    self.delete_all_inputs_in_block(&write_txn, block_hash)?;
                },
                SetBestBlock {
                    height,
                    hash,
                    accumulated_difficulty,
                    expected_prev_best_block,
                } => {
                    // for security we check that the best block does exist, and we check the previous value
                    // we dont want to check this if the prev block has never been set, this means a empty hash of 32
                    // bytes.
                    if *height > 0 {
                        let prev = fetch_best_block(&write_txn, &self.metadata_db)?;
                        if *expected_prev_best_block != prev {
                            return Err(ChainStorageError::InvalidOperation(format!(
                                "There was a change in best_block, the best block is suppose to be: ({}), but it \
                                 currently is: ({})",
                                expected_prev_best_block.to_hex(),
                                prev.to_hex(),
                            )));
                        };
                    }
                    if !lmdb_exists(&write_txn, &self.block_hashes_db, hash.as_slice())? {
                        // we dont care about the header or the height, we just want to know its there.
                        return Err(ChainStorageError::InvalidOperation(format!(
                            "There is no Blockheader hash ({}) in db",
                            expected_prev_best_block.to_hex(),
                        )));
                    };
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::ChainHeight,
                        MetadataValue::ChainHeight(*height),
                    )?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::BestBlock,
                        MetadataValue::BestBlock(hash.clone()),
                    )?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::AccumulatedWork,
                        MetadataValue::AccumulatedWork(*accumulated_difficulty),
                    )?;
                },
                SetPruningHorizonConfig(pruning_horizon) => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PruningHorizon,
                        MetadataValue::PruningHorizon(*pruning_horizon),
                    )?;
                },
                SetPrunedHeight { height } => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PrunedHeight,
                        MetadataValue::PrunedHeight(*height),
                    )?;
                },
                SetHorizonData { horizon_data } => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::HorizonData,
                        MetadataValue::HorizonData(horizon_data.clone()),
                    )?;
                },
                InsertBadBlock { hash, height } => {
                    self.insert_bad_block_and_cleanup(&write_txn, hash, *height)?;
                },
                InsertReorg { reorg } => {
                    lmdb_replace(&write_txn, &self.reorgs, &reorg.local_time.timestamp(), &reorg)?;
                },
                ClearAllReorgs => {
                    lmdb_clear(&write_txn, &self.reorgs)?;
                },
            }
        }
        write_txn.commit()?;

        Ok(())
    }

    fn all_dbs(&self) -> [(&'static str, &DatabaseRef); 23] {
        [
            ("metadata_db", &self.metadata_db),
            ("headers_db", &self.headers_db),
            ("header_accumulated_data_db", &self.header_accumulated_data_db),
            ("block_accumulated_data_db", &self.block_accumulated_data_db),
            ("block_hashes_db", &self.block_hashes_db),
            ("utxos_db", &self.utxos_db),
            ("inputs_db", &self.inputs_db),
            ("txos_hash_to_index_db", &self.txos_hash_to_index_db),
            ("kernels_db", &self.kernels_db),
            ("kernel_excess_index", &self.kernel_excess_index),
            ("kernel_excess_sig_index", &self.kernel_excess_sig_index),
            ("kernel_mmr_size_index", &self.kernel_mmr_size_index),
            ("output_mmr_size_index", &self.output_mmr_size_index),
            ("utxo_commitment_index", &self.utxo_commitment_index),
            ("unique_id_index", &self.unique_id_index),
            (
                "deleted_txo_mmr_position_to_height_index",
                &self.deleted_txo_mmr_position_to_height_index,
            ),
            ("orphans_db", &self.orphans_db),
            (
                "orphan_header_accumulated_data_db",
                &self.orphan_header_accumulated_data_db,
            ),
            ("monero_seed_height_db", &self.monero_seed_height_db),
            ("orphan_chain_tips_db", &self.orphan_chain_tips_db),
            ("orphan_parent_map_index", &self.orphan_parent_map_index),
            ("bad_blocks", &self.bad_blocks),
            ("reorgs", &self.reorgs),
        ]
    }

    fn prune_output(
        &self,
        txn: &WriteTransaction<'_>,
        key: &OutputKey,
    ) -> Result<TransactionOutput, ChainStorageError> {
        let mut output: TransactionOutputRowData =
            lmdb_get(txn, &self.utxos_db, key.as_bytes()).or_not_found("TransactionOutput", "key", key.to_hex())?;
        let pruned_output = output
            .output
            .take()
            .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                function: "prune_output",
                details: format!(
                    "Attempt to prune output that has already been pruned for key {}",
                    key.to_hex()
                ),
            })?;
        // output.output is None
        lmdb_replace(txn, &self.utxos_db, key.as_bytes(), &output)?;
        Ok(pruned_output)
    }

    fn insert_output(
        &self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        header_height: u64,
        output: &TransactionOutput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError> {
        let output_hash = output.hash();
        let witness_hash = output.witness_hash();

        let key = OutputKey::new(header_hash, mmr_position, &[]);

        lmdb_insert(
            txn,
            &*self.utxo_commitment_index,
            output.commitment.as_bytes(),
            &output_hash,
            "utxo_commitment_index",
        )?;

        if let Some(ref unique_id) = output.features.unique_id {
            let parent_public_key = output.features.parent_public_key.as_ref();
            let key = UniqueIdIndexKey::new(parent_public_key, unique_id.as_slice());
            debug!(
                target: LOG_TARGET,
                "inserting index for unique_id <{}, {}> in output {}. Key is {}",
                parent_public_key
                    .map(|p| p.to_hex())
                    .unwrap_or_else(|| "<none>".to_string()),
                unique_id.to_hex(),
                output_hash.to_hex(),
                key,
            );
            lmdb_insert(
                txn,
                &*self.unique_id_index,
                key.as_bytes(),
                &output_hash,
                "unique_id_index",
            )?;
        }

        lmdb_insert(
            txn,
            &*self.txos_hash_to_index_db,
            output_hash.as_slice(),
            &(mmr_position, key.as_bytes()),
            "txos_hash_to_index_db",
        )?;
        lmdb_insert(
            txn,
            &*self.utxos_db,
            key.as_bytes(),
            &TransactionOutputRowData {
                output: Some(output.clone()),
                header_hash: header_hash.clone(),
                mmr_position,
                hash: output_hash,
                witness_hash,
                mined_height: header_height,
            },
            "utxos_db",
        )?;

        Ok(())
    }

    fn insert_pruned_output(
        &self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        header_height: u64,
        output_hash: &HashOutput,
        witness_hash: &HashOutput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError> {
        if !lmdb_exists(txn, &self.block_hashes_db, header_hash.as_slice())? {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Unable to insert pruned output because header {} does not exist",
                header_hash.to_hex(),
            )));
        }
        let key = OutputKey::new(header_hash, mmr_position, &[]);
        lmdb_insert(
            txn,
            &*self.txos_hash_to_index_db,
            output_hash.as_slice(),
            &(mmr_position, key.as_bytes()),
            "txos_hash_to_index_db",
        )?;
        lmdb_insert(
            txn,
            &*self.utxos_db,
            key.as_bytes(),
            &TransactionOutputRowData {
                output: None,
                header_hash: header_hash.clone(),
                mmr_position,
                hash: output_hash.clone(),
                witness_hash: witness_hash.clone(),
                mined_height: header_height,
            },
            "utxos_db",
        )
    }

    fn insert_kernel(
        &self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        kernel: &TransactionKernel,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError> {
        let hash = kernel.hash();
        let key = KernelKey::new(header_hash, mmr_position, &hash);

        lmdb_insert(
            txn,
            &*self.kernel_excess_index,
            kernel.excess.as_bytes(),
            &(header_hash.clone(), mmr_position, hash.clone()),
            "kernel_excess_index",
        )?;

        let mut excess_sig_key = Vec::<u8>::with_capacity(32 * 2);
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
            key.as_bytes(),
            &TransactionKernelRowData {
                kernel: kernel.clone(),
                header_hash: header_hash.clone(),
                mmr_position,
                hash,
            },
            "kernels_db",
        )
    }

    fn insert_input(
        &self,
        txn: &WriteTransaction<'_>,
        height: u64,
        header_hash: &HashOutput,
        input: &TransactionInput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError> {
        lmdb_delete(
            txn,
            &self.utxo_commitment_index,
            input.commitment()?.as_bytes(),
            "utxo_commitment_index",
        )
        .or_else(|err| match err {
            // The commitment may not yet be included in the DB in the 0-conf transaction case
            ChainStorageError::ValueNotFound { .. } => Ok(()),
            _ => Err(err),
        })?;
        lmdb_insert(
            txn,
            &self.deleted_txo_mmr_position_to_height_index,
            &mmr_position,
            &(height, header_hash),
            "deleted_txo_mmr_position_to_height_index",
        )?;

        if let Some(ref unique_id) = input.features()?.unique_id {
            let parent_public_key = input.features()?.parent_public_key.as_ref();
            // Move the "current" UTXO entry to a key containing the spend height
            let mut key = UniqueIdIndexKey::new(parent_public_key, unique_id.as_slice());
            let expected_output_hash = lmdb_get::<_, HashOutput>(txn, &self.unique_id_index, key.as_bytes())?
                .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                    function: "insert_input",
                    details: format!("unique token ID with key {} does not exist in index", key.to_hex()),
                })?;

            let output_hash = input.output_hash();
            if expected_output_hash != output_hash {
                // This should have been checked by an upstream validator
                return Err(ChainStorageError::DataInconsistencyDetected {
                    function: "insert_input",
                    details: format!(
                        "output hash for unique id key {} did not match the output hash this input is spending. \
                         output hash in index {}, hash of spent output: {}",
                        key.to_hex(),
                        expected_output_hash.to_hex(),
                        output_hash.to_hex()
                    ),
                });
            }

            // TODO: 0-conf is not currently supported for transactions with unique_id set
            lmdb_delete(txn, &self.unique_id_index, key.as_bytes(), "unique_id_index")?;
            key.set_deleted_height(height);
            debug!(
                target: LOG_TARGET,
                "moving index for unique_id <{}, {}> in output {} to key {}",
                parent_public_key
                    .map(|p| p.to_hex())
                    .unwrap_or_else(|| "<none>".to_string()),
                unique_id.to_hex(),
                output_hash.to_hex(),
                key,
            );
            lmdb_insert(
                txn,
                &self.unique_id_index,
                key.as_bytes(),
                &output_hash,
                "unique_id_index",
            )?;
        }

        let hash = input.canonical_hash()?;
        let key = InputKey::new(header_hash, mmr_position, &hash);
        lmdb_insert(
            txn,
            &*self.inputs_db,
            key.as_bytes(),
            &TransactionInputRowDataRef {
                input: &input.to_compact(),
                header_hash,
                mmr_position,
                hash: &hash,
            },
            "inputs_db",
        )
    }

    fn set_metadata(
        &self,
        txn: &WriteTransaction<'_>,
        k: MetadataKey,
        v: MetadataValue,
    ) -> Result<(), ChainStorageError> {
        lmdb_replace(txn, &self.metadata_db, &k.as_u32(), &v)?;
        Ok(())
    }

    fn insert_orphan_block(&self, txn: &WriteTransaction<'_>, block: &Block) -> Result<(), ChainStorageError> {
        let k = block.hash();
        lmdb_insert_dup(txn, &self.orphan_parent_map_index, &block.header.prev_hash, &k)?;
        lmdb_insert(txn, &self.orphans_db, k.as_slice(), &block, "orphans_db")?;

        Ok(())
    }

    fn set_accumulated_data_for_orphan(
        &self,
        txn: &WriteTransaction<'_>,
        accumulated_data: &BlockHeaderAccumulatedData,
    ) -> Result<(), ChainStorageError> {
        if !lmdb_exists(txn, &self.orphans_db, accumulated_data.hash.as_slice())? {
            return Err(ChainStorageError::InvalidOperation(format!(
                "set_accumulated_data_for_orphan: orphan {} does not exist",
                accumulated_data.hash.to_hex()
            )));
        }

        lmdb_insert(
            txn,
            &self.orphan_header_accumulated_data_db,
            accumulated_data.hash.as_slice(),
            &accumulated_data,
            "orphan_header_accumulated_data_db",
        )?;

        Ok(())
    }

    /// Inserts the header and header accumulated data.
    fn insert_header(
        &self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        accum_data: &BlockHeaderAccumulatedData,
    ) -> Result<(), ChainStorageError> {
        if let Some(current_header_at_height) = lmdb_get::<_, BlockHeader>(txn, &self.headers_db, &header.height)? {
            let hash = current_header_at_height.hash();
            if hash != accum_data.hash {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "There is a different header stored at height {} already. New header ({}), current header: ({})",
                    header.height,
                    accum_data.hash.to_hex(),
                    current_header_at_height.hash().to_hex(),
                )));
            }
            return Err(ChainStorageError::InvalidOperation(format!(
                "The header at height {} already exists. Existing header hash: {}",
                header.height,
                hash.to_hex()
            )));
        }

        // Check that the current height is still header.height - 1 and that no other threads have inserted
        if let Some(ref last_header) = self.fetch_last_header_in_txn(txn)? {
            if last_header.height != header.height.saturating_sub(1) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Attempted to insert a header out of order. The last header height is {} but attempted to insert \
                     a header with height {}",
                    last_header.height, header.height,
                )));
            }

            // Possibly remove this check later
            let hash = last_header.hash();
            if hash != header.prev_hash {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Attempted to insert a block header at height {} that didn't form a chain. Previous block \
                     hash:{}, new block's previous hash:{}",
                    header.height,
                    hash.to_hex(),
                    header.prev_hash.to_hex()
                )));
            }
        } else if header.height != 0 {
            return Err(ChainStorageError::InvalidOperation(format!(
                "The first header inserted must have height 0. Height provided: {}",
                header.height
            )));
        }

        lmdb_insert(
            txn,
            &self.header_accumulated_data_db,
            &header.height,
            &accum_data,
            "header_accumulated_data_db",
        )?;
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
        Ok(())
    }

    fn delete_header(&self, txn: &WriteTransaction<'_>, height: u64) -> Result<(), ChainStorageError> {
        if self.fetch_block_accumulated_data(txn, height)?.is_some() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Attempted to delete header at height {} while block accumulated data still exists",
                height
            )));
        }

        let header =
            self.fetch_last_header_in_txn(txn)
                .or_not_found("BlockHeader", "height", "last_header".to_string())?;
        if header.height != height {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Attempted to delete a header at height {} that was not the last header (which is at height {}). \
                 Headers must be deleted in reverse order.",
                height, header.height
            )));
        }

        // TODO: This can maybe be removed for performance if the check for block_accumulated_data existing is
        // sufficient

        let hash = header.hash();

        // Check that there are no utxos or kernels linked to this.
        if !lmdb_fetch_matching_after::<TransactionKernelRowData>(txn, &self.kernels_db, &hash)?.is_empty() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Cannot delete header {} ({}) because there are kernels linked to it",
                header.height,
                hash.to_hex()
            )));
        }
        if !lmdb_fetch_matching_after::<TransactionOutputRowData>(txn, &self.utxos_db, &hash)?.is_empty() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Cannot delete header at height {} ({}) because there are UTXOs linked to it",
                height,
                hash.to_hex()
            )));
        }

        lmdb_delete(txn, &self.block_hashes_db, &hash, "block_hashes_db")?;
        lmdb_delete(txn, &self.headers_db, &height, "headers_db")?;
        lmdb_delete(
            txn,
            &self.header_accumulated_data_db,
            &height,
            "header_accumulated_data_db",
        )?;
        lmdb_delete(
            txn,
            &self.kernel_mmr_size_index,
            &header.kernel_mmr_size.to_be_bytes(),
            "kernel_mmr_size_index",
        )?;
        lmdb_delete(
            txn,
            &self.output_mmr_size_index,
            &header.output_mmr_size.to_be_bytes(),
            "output_mmr_size_index",
        )?;

        Ok(())
    }

    fn delete_block_body(
        &self,
        write_txn: &WriteTransaction<'_>,
        block_hash: &HashOutput,
    ) -> Result<(), ChainStorageError> {
        let hash_hex = block_hash.to_hex();
        debug!(target: LOG_TARGET, "Deleting block `{}`", hash_hex);
        debug!(target: LOG_TARGET, "Deleting UTXOs...");
        let height = self
            .fetch_height_from_hash(write_txn, block_hash)
            .or_not_found("Block", "hash", hash_hex)?;
        let block_accum_data =
            self.fetch_block_accumulated_data(write_txn, height)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData",
                    field: "height",
                    value: height.to_string(),
                })?;
        let mut bitmap = self.load_deleted_bitmap_model(write_txn)?;
        bitmap.remove(block_accum_data.deleted())?;
        bitmap.save()?;

        lmdb_delete(
            write_txn,
            &self.block_accumulated_data_db,
            &height,
            "block_accumulated_data_db",
        )?;

        self.delete_block_inputs_outputs(write_txn, height, block_hash)?;
        self.delete_block_kernels(write_txn, block_hash)?;

        Ok(())
    }

    fn delete_block_inputs_outputs(
        &self,
        txn: &WriteTransaction<'_>,
        height: u64,
        hash: &[u8],
    ) -> Result<(), ChainStorageError> {
        let output_rows = lmdb_delete_keys_starting_with::<TransactionOutputRowData>(txn, &self.utxos_db, hash)?;
        debug!(target: LOG_TARGET, "Deleted {} outputs...", output_rows.len());
        let inputs = lmdb_delete_keys_starting_with::<TransactionInputRowData>(txn, &self.inputs_db, hash)?;
        debug!(target: LOG_TARGET, "Deleted {} input(s)...", inputs.len());

        for utxo in &output_rows {
            trace!(target: LOG_TARGET, "Deleting UTXO `{}`", to_hex(&utxo.hash));
            lmdb_delete(
                txn,
                &self.txos_hash_to_index_db,
                utxo.hash.as_slice(),
                "txos_hash_to_index_db",
            )?;
            if let Some(ref output) = utxo.output {
                let output_hash = output.hash();
                // if an output was already spent in the block, it was never created as unspent, so dont delete it as it
                // does not exist here
                if inputs.iter().any(|r| r.input.output_hash() == output_hash) {
                    continue;
                }
                lmdb_delete(
                    txn,
                    &*self.utxo_commitment_index,
                    output.commitment.as_bytes(),
                    "utxo_commitment_index",
                )?;
                if let Some(unique_id) = output.features.unique_asset_id() {
                    let key = UniqueIdIndexKey::new(output.features.parent_public_key.as_ref(), unique_id);
                    lmdb_delete(txn, &self.unique_id_index, key.as_bytes(), "unique_id_index")?;
                }
            }
        }
        // Move inputs in this block back into the unspent set, any outputs spent within this block they will be removed
        // by deleting all the block's outputs below
        for row in inputs {
            // If input spends an output in this block, don't add it to the utxo set
            let output_hash = row.input.output_hash();
            if output_rows.iter().any(|r| r.hash == output_hash) {
                continue;
            }
            let mut input = row.input.clone();

            let utxo_mined_info =
                self.fetch_output_in_txn(txn, &output_hash)?
                    .ok_or_else(|| ChainStorageError::ValueNotFound {
                        entity: "UTXO",
                        field: "hash",
                        value: output_hash.to_hex(),
                    })?;

            match utxo_mined_info.output {
                PrunedOutput::Pruned { .. } => {
                    debug!(target: LOG_TARGET, "Output Transaction Input is spending is pruned");
                    return Err(ChainStorageError::TransactionError(
                        TransactionError::MissingTransactionInputData,
                    ));
                },
                PrunedOutput::NotPruned { output } => {
                    input.add_output_data(
                        output.version,
                        output.features,
                        output.commitment,
                        output.script,
                        output.sender_offset_public_key,
                        output.covenant,
                    );
                },
            }

            trace!(target: LOG_TARGET, "Input moved to UTXO set: {}", input);
            lmdb_insert(
                txn,
                &*self.utxo_commitment_index,
                input.commitment()?.as_bytes(),
                &input.output_hash(),
                "utxo_commitment_index",
            )?;
            lmdb_delete(
                txn,
                &self.deleted_txo_mmr_position_to_height_index,
                &row.mmr_position,
                "deleted_txo_mmr_position_to_height_index",
            )?;

            if let Some(unique_id) = input.features()?.unique_asset_id() {
                let mut key = UniqueIdIndexKey::new(input.features()?.parent_public_key.as_ref(), unique_id);
                // The output that made this input that is being unspent is now at the head
                lmdb_replace(txn, &self.unique_id_index, key.as_bytes(), &output_hash)?;

                // Remove the checkpoint key at current height
                key.set_deleted_height(height);
                lmdb_delete(txn, &self.unique_id_index, key.as_bytes(), "unique_id_index")?;
            }
        }
        Ok(())
    }

    fn delete_block_kernels(&self, txn: &WriteTransaction<'_>, block_hash: &[u8]) -> Result<(), ChainStorageError> {
        let kernels = lmdb_delete_keys_starting_with::<TransactionKernelRowData>(txn, &self.kernels_db, block_hash)?;
        debug!(target: LOG_TARGET, "Deleted {} kernels...", kernels.len());
        for kernel in kernels {
            trace!(
                target: LOG_TARGET,
                "Deleting excess `{}`",
                kernel.kernel.excess.to_hex()
            );
            lmdb_delete(
                txn,
                &self.kernel_excess_index,
                kernel.kernel.excess.as_bytes(),
                "kernel_excess_index",
            )?;
            let mut excess_sig_key = Vec::<u8>::new();
            excess_sig_key.extend(kernel.kernel.excess_sig.get_public_nonce().as_bytes());
            excess_sig_key.extend(kernel.kernel.excess_sig.get_signature().as_bytes());
            trace!(
                target: LOG_TARGET,
                "Deleting excess signature `{}`",
                to_hex(&excess_sig_key)
            );
            lmdb_delete(
                txn,
                &self.kernel_excess_sig_index,
                excess_sig_key.as_slice(),
                "kernel_excess_sig_index",
            )?;
        }
        Ok(())
    }

    fn delete_orphan(&self, txn: &WriteTransaction<'_>, hash: &HashOutput) -> Result<(), ChainStorageError> {
        let orphan = match lmdb_get::<_, Block>(txn, &self.orphans_db, hash.as_slice())? {
            Some(orphan) => orphan,
            None => {
                // delete_orphan is idempotent
                debug!(
                    target: LOG_TARGET,
                    "delete_orphan: request to delete orphan block {} that was not found.",
                    hash.to_hex()
                );
                return Ok(());
            },
        };

        let parent_hash = orphan.header.prev_hash;
        lmdb_delete_key_value(txn, &self.orphan_parent_map_index, parent_hash.as_slice(), &hash)?;

        // Orphan is a tip hash
        if lmdb_exists(txn, &self.orphan_chain_tips_db, hash.as_slice())? {
            lmdb_delete(txn, &self.orphan_chain_tips_db, hash.as_slice(), "orphan_chain_tips_db")?;

            // Parent becomes a tip hash
            if lmdb_exists(txn, &self.orphans_db, parent_hash.as_slice())? {
                lmdb_insert(
                    txn,
                    &self.orphan_chain_tips_db,
                    parent_hash.as_slice(),
                    &parent_hash,
                    "orphan_chain_tips_db",
                )?;
            }
        }

        if lmdb_exists(txn, &self.orphan_header_accumulated_data_db, hash.as_slice())? {
            lmdb_delete(
                txn,
                &self.orphan_header_accumulated_data_db,
                hash.as_slice(),
                "orphan_header_accumulated_data_db",
            )?;
        }

        if lmdb_exists(txn, &self.orphan_header_accumulated_data_db, hash.as_slice())? {
            lmdb_delete(
                txn,
                &self.orphan_header_accumulated_data_db,
                hash.as_slice(),
                "orphan_header_accumulated_data_db",
            )?;
        }
        lmdb_delete(txn, &self.orphans_db, hash.as_slice(), "orphans_db")?;
        Ok(())
    }

    fn insert_block_body(
        &self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        body: AggregateBody,
    ) -> Result<(), ChainStorageError> {
        let block_hash = header.hash();
        debug!(
            target: LOG_TARGET,
            "Inserting block body for header `{}`: {}",
            block_hash.to_hex(),
            body.to_counts_string()
        );

        // Check that the database has not been changed by another thread
        // 1. The header we are inserting for matches the header at that height
        let current_header_at_height = lmdb_get::<_, BlockHeader>(txn, &self.headers_db, &header.height).or_not_found(
            "BlockHeader",
            "height",
            header.height.to_string(),
        )?;
        let hash = current_header_at_height.hash();
        if hash != block_hash {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Could not insert this block body because there is a different header stored at height {}. New header \
                 ({}), current header: ({})",
                header.height,
                hash.to_hex(),
                block_hash.to_hex()
            )));
        }

        let (inputs, outputs, kernels) = body.dissolve();

        let data = if header.height == 0 {
            BlockAccumulatedData::default()
        } else {
            self.fetch_block_accumulated_data(&*txn, header.height - 1)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData",
                    field: "height",
                    value: (header.height - 1).to_string(),
                })?
        };

        let mut total_kernel_sum = Commitment::default();
        let BlockAccumulatedData {
            kernels: pruned_kernel_set,
            outputs: pruned_output_set,
            witness: pruned_proof_set,
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
            self.insert_kernel(txn, &block_hash, &kernel, pos as u32)?;
        }
        let mut output_mmr = MutableMmr::<HashDigest, _>::new(pruned_output_set, Bitmap::create())?;
        let mut witness_mmr = MerkleMountainRange::<HashDigest, _>::new(pruned_proof_set);

        let leaf_count = witness_mmr.get_leaf_count()?;

        // Output hashes added before inputs so that inputs can spend outputs in this transaction (0-conf and combined)
        let outputs = outputs
            .into_iter()
            .enumerate()
            .map(|(i, output)| {
                output_mmr.push(output.hash())?;
                witness_mmr.push(output.witness_hash())?;
                Ok((output, leaf_count + i + 1))
            })
            .collect::<Result<Vec<_>, ChainStorageError>>()?;

        let mut spent_zero_conf_commitments = Vec::new();
        // unique_id_index expects inputs to be inserted before outputs
        for input in &inputs {
            let output_hash = input.output_hash();
            let index = match self.fetch_mmr_leaf_index(&**txn, MmrTree::Utxo, &output_hash)? {
                Some(index) => index,
                None => match output_mmr.find_leaf_index(&output_hash)? {
                    Some(index) => {
                        debug!(
                            target: LOG_TARGET,
                            "Input {} spends output from current block (0-conf)", input
                        );
                        spent_zero_conf_commitments.push(input.commitment()?);
                        index
                    },
                    None => return Err(ChainStorageError::UnspendableInput),
                },
            };
            if !output_mmr.delete(index) {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "Could not delete index {} from the output MMR",
                    index
                )));
            }
            trace!(target: LOG_TARGET, "Inserting input `{}`", input.commitment()?.to_hex());
            self.insert_input(txn, current_header_at_height.height, &block_hash, input, index)?;
        }

        for (output, mmr_count) in outputs {
            trace!(target: LOG_TARGET, "Inserting output `{}`", output.commitment.to_hex());
            self.insert_output(txn, &block_hash, header.height, &output, mmr_count as u32 - 1)?;
        }

        for commitment in spent_zero_conf_commitments {
            lmdb_delete(
                txn,
                &self.utxo_commitment_index,
                commitment.as_bytes(),
                "utxo_commitment_index",
            )?;
        }
        // Merge current deletions with the tip bitmap
        let deleted_at_current_height = output_mmr.deleted().clone();
        // Merge the new indexes with the blockchain deleted bitmap
        let mut deleted_bitmap = self.load_deleted_bitmap_model(txn)?;
        deleted_bitmap.merge(&deleted_at_current_height)?;

        // Set the output MMR to the complete map so that the complete state can be committed to in the final MR
        output_mmr.set_deleted(deleted_bitmap.get().clone().into_bitmap());
        output_mmr.compress();

        // Save the bitmap
        deleted_bitmap.save()?;

        self.insert_block_accumulated_data(
            txn,
            header.height,
            &BlockAccumulatedData::new(
                kernel_mmr.get_pruned_hash_set()?,
                output_mmr.mmr().get_pruned_hash_set()?,
                witness_mmr.get_pruned_hash_set()?,
                deleted_at_current_height,
                total_kernel_sum,
            ),
        )?;

        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    fn insert_block_accumulated_data(
        &self,
        txn: &WriteTransaction<'_>,
        header_height: u64,
        data: &BlockAccumulatedData,
    ) -> Result<(), ChainStorageError> {
        lmdb_insert(
            txn,
            &self.block_accumulated_data_db,
            &header_height,
            data,
            "block_accumulated_data_db",
        )
    }

    fn update_block_accumulated_data(
        &self,
        write_txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        values: UpdateBlockAccumulatedData,
    ) -> Result<(), ChainStorageError> {
        let height = self.fetch_height_from_hash(write_txn, header_hash).or_not_found(
            "BlockHash",
            "hash",
            header_hash.to_hex(),
        )?;

        let mut block_accum_data = self
            .fetch_block_accumulated_data(&*write_txn, height)?
            .unwrap_or_default();

        if let Some(deleted_diff) = values.deleted_diff {
            block_accum_data.deleted = deleted_diff;
        }
        if let Some(kernel_sum) = values.kernel_sum {
            block_accum_data.kernel_sum = kernel_sum;
        }
        if let Some(kernel_hash_set) = values.kernel_hash_set {
            block_accum_data.kernels = kernel_hash_set;
        }
        if let Some(utxo_hash_set) = values.utxo_hash_set {
            block_accum_data.outputs = utxo_hash_set;
        }
        if let Some(witness_hash_set) = values.witness_hash_set {
            block_accum_data.witness = witness_hash_set;
        }

        lmdb_replace(write_txn, &self.block_accumulated_data_db, &height, &block_accum_data)?;
        Ok(())
    }

    fn load_deleted_bitmap_model<'a, 'b, T>(
        &'a self,
        txn: &'a T,
    ) -> Result<DeletedBitmapModel<'a, T>, ChainStorageError>
    where
        T: Deref<Target = ConstTransaction<'b>>,
    {
        DeletedBitmapModel::load(txn, &self.metadata_db)
    }

    fn insert_monero_seed_height(
        &self,
        write_txn: &WriteTransaction<'_>,
        seed: &[u8],
        height: u64,
    ) -> Result<(), ChainStorageError> {
        let current_height = lmdb_get(write_txn, &self.monero_seed_height_db, seed)?.unwrap_or(std::u64::MAX);
        if height < current_height {
            lmdb_replace(write_txn, &self.monero_seed_height_db, seed, &height)?;
        };
        Ok(())
    }

    fn delete_all_inputs_in_block(
        &self,
        txn: &WriteTransaction<'_>,
        block_hash: &BlockHash,
    ) -> Result<(), ChainStorageError> {
        let inputs = lmdb_delete_keys_starting_with::<TransactionInput>(txn, &self.inputs_db, block_hash)?;
        debug!(target: LOG_TARGET, "Deleted {} input(s)", inputs.len());
        Ok(())
    }

    fn prune_outputs_at_positions(
        &self,
        write_txn: &WriteTransaction<'_>,
        output_positions: &[u32],
    ) -> Result<(), ChainStorageError> {
        for pos in output_positions {
            let (_height, hash) = lmdb_first_after::<_, (u64, Vec<u8>)>(
                write_txn,
                &self.output_mmr_size_index,
                &((pos + 1) as u64).to_be_bytes(),
            )
            .or_not_found("BlockHeader", "mmr_position", pos.to_string())?;
            let key = OutputKey::new(&hash, *pos, &[]);
            debug!(target: LOG_TARGET, "Pruning output: {}", key.to_hex());
            self.prune_output(write_txn, &key)?;
        }

        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_mmr_leaf_index(
        &self,
        txn: &ConstTransaction<'_>,
        tree: MmrTree,
        hash: &Hash,
    ) -> Result<Option<u32>, ChainStorageError> {
        match tree {
            MmrTree::Utxo => {
                Ok(lmdb_get::<_, (u32, Vec<u8>)>(txn, &self.txos_hash_to_index_db, hash)?.map(|(index, _)| index))
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
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
        lmdb_get(txn, &self.block_accumulated_data_db, &height).map_err(Into::into)
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_height_from_hash(
        &self,
        txn: &ConstTransaction<'_>,
        header_hash: &HashOutput,
    ) -> Result<Option<u64>, ChainStorageError> {
        lmdb_get(txn, &self.block_hashes_db, header_hash.as_slice()).map_err(Into::into)
    }

    fn fetch_header_accumulated_data_by_height(
        &self,
        txn: &ReadTransaction,
        height: u64,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError> {
        lmdb_get(txn, &self.header_accumulated_data_db, &height)
    }

    fn fetch_last_header_in_txn(&self, txn: &ConstTransaction<'_>) -> Result<Option<BlockHeader>, ChainStorageError> {
        lmdb_last(txn, &self.headers_db)
    }

    fn insert_bad_block_and_cleanup(
        &self,
        txn: &WriteTransaction<'_>,
        hash: &HashOutput,
        height: u64,
    ) -> Result<(), ChainStorageError> {
        const CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT: u64 = 10000;

        lmdb_replace(txn, &self.bad_blocks, hash, &height)?;
        // Clean up bad blocks that are far from the tip
        let metadata = fetch_metadata(&*txn, &self.metadata_db)?;
        let deleted_before_height = metadata
            .height_of_longest_chain()
            .saturating_sub(CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT);
        if deleted_before_height == 0 {
            return Ok(());
        }

        let num_deleted =
            lmdb_delete_each_where::<[u8], u64, _>(txn, &self.bad_blocks, |_, v| Some(v < deleted_before_height))?;
        debug!(target: LOG_TARGET, "Cleaned out {} stale bad blocks", num_deleted);

        Ok(())
    }

    fn fetch_output_in_txn(
        &self,
        txn: &ConstTransaction<'_>,
        output_hash: &HashOutput,
    ) -> Result<Option<UtxoMinedInfo>, ChainStorageError> {
        if let Some((index, key)) =
            lmdb_get::<_, (u32, Vec<u8>)>(txn, &self.txos_hash_to_index_db, output_hash.as_slice())?
        {
            debug!(
                target: LOG_TARGET,
                "Fetch output: {} Found ({}, {})",
                output_hash.to_hex(),
                index,
                key.to_hex()
            );
            match lmdb_get::<_, TransactionOutputRowData>(txn, &self.utxos_db, &key)? {
                Some(TransactionOutputRowData {
                    output: Some(o),
                    mmr_position,
                    mined_height,
                    header_hash,
                    ..
                }) => Ok(Some(UtxoMinedInfo {
                    output: PrunedOutput::NotPruned { output: o },
                    mmr_position,
                    mined_height,
                    header_hash,
                })),
                Some(TransactionOutputRowData {
                    output: None,
                    mmr_position,
                    mined_height,
                    hash,
                    witness_hash,
                    header_hash,
                    ..
                }) => Ok(Some(UtxoMinedInfo {
                    output: PrunedOutput::Pruned {
                        output_hash: hash,
                        witness_hash,
                    },
                    mmr_position,
                    mined_height,
                    header_hash,
                })),
                _ => Ok(None),
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
}

pub fn create_recovery_lmdb_database<P: AsRef<Path>>(path: P) -> Result<(), ChainStorageError> {
    let new_path = path.as_ref().join("temp_recovery");
    let _ = fs::create_dir_all(&new_path);

    let data_file = path.as_ref().join("data.mdb");

    let new_data_file = new_path.join("data.mdb");

    fs::rename(data_file, new_data_file)
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not copy LMDB store:{}", err)))?;
    Ok(())
}

fn acquire_exclusive_file_lock(db_path: &Path) -> Result<File, ChainStorageError> {
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

        let mark = Instant::now();
        // Resize this many times before assuming something is not right
        const MAX_RESIZES: usize = 5;
        for i in 0..MAX_RESIZES {
            let num_operations = txn.operations().len();
            match self.apply_db_transaction(&txn) {
                Ok(_) => {
                    trace!(
                        target: LOG_TARGET,
                        "Database completed {} operation(s) in {:.0?}",
                        num_operations,
                        mark.elapsed()
                    );

                    return Ok(());
                },
                Err(ChainStorageError::DbResizeRequired) => {
                    info!(
                        target: LOG_TARGET,
                        "Database resize required (resized {} time(s) in this transaction)",
                        i + 1
                    );
                    // SAFETY: This depends on the thread safety of the caller. Technically, `write` is unsafe too
                    // however we happen to know that `LmdbDatabase` is wrapped in an exclusive write lock in
                    // BlockchainDatabase, so we know there are no other threads taking out LMDB transactions when this
                    // is called.
                    unsafe {
                        LMDBStore::resize(&self.env, &self.env_config)?;
                    }
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Failed to apply DB transaction: {:?}", e);
                    return Err(e);
                },
            }
        }

        Err(ChainStorageError::DbTransactionTooLarge(txn.operations().len()))
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        let txn = self.read_transaction()?;
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
        let txn = self.read_transaction()?;
        Ok(match key {
            DbKey::BlockHeader(k) => lmdb_exists(&txn, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&txn, &self.block_hashes_db, h)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&txn, &self.orphans_db, k)?,
        })
    }

    fn fetch_chain_header_by_height(&self, height: u64) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;

        let header: BlockHeader =
            lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeader",
                field: "height",
                value: height.to_string(),
            })?;

        let accum_data = self
            .fetch_header_accumulated_data_by_height(&txn, height)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "height",
                value: height.to_string(),
            })?;

        let height = header.height;
        let chain_header = ChainHeader::try_construct(header, accum_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_chain_header_by_height",
                details: format!("Mismatch in accumulated data at height #{}", height),
            }
        })?;

        Ok(chain_header)
    }

    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let height = self.fetch_height_from_hash(&txn, hash)?;
        if let Some(h) = height {
            self.fetch_header_accumulated_data_by_height(&txn, h)
        } else {
            Ok(None)
        }
    }

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;

        let height: Option<u64> = self.fetch_height_from_hash(&txn, hash)?;
        if let Some(h) = height {
            let chain_header = self.fetch_chain_header_by_height(h)?;
            return Ok(chain_header);
        }

        let orphan_accum: Option<BlockHeaderAccumulatedData> =
            lmdb_get(&txn, &self.orphan_header_accumulated_data_db, hash.as_slice())?;

        if let Some(accum) = orphan_accum {
            let orphan =
                self.fetch_orphan(&txn, hash)?
                    .ok_or_else(|| ChainStorageError::DataInconsistencyDetected {
                        function: "fetch_chain_header_in_all_chains",
                        details: format!(
                            "Orphan accumulated data exists but the corresponding orphan header {} does not",
                            hash.to_hex()
                        ),
                    })?;
            let chain_header = ChainHeader::try_construct(orphan.header, accum).ok_or_else(|| {
                ChainStorageError::DataInconsistencyDetected {
                    function: "fetch_chain_header_in_all_chains",
                    details: format!("accumulated data mismatch for orphan header {}", hash.to_hex()),
                }
            })?;
            return Ok(chain_header);
        }

        Err(ChainStorageError::ValueNotFound {
            entity: "chain header (in chain_header_in_all_chains)",
            field: "hash",
            value: hash.to_hex(),
        })
    }

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;
        // LMDB returns the height at the position, so we have to offset the position by 1 so that the mmr_position arg
        // is an index starting from 0
        let mmr_position = mmr_position + 1;

        let height = lmdb_first_after::<_, u64>(&txn, &self.kernel_mmr_size_index, &mmr_position.to_be_bytes())?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "kernel_mmr_size_index",
                field: "mmr_position",
                value: mmr_position.to_string(),
            })?;

        let header: BlockHeader =
            lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeader",
                field: "height",
                value: height.to_string(),
            })?;

        let accum_data = self
            .fetch_header_accumulated_data_by_height(&txn, height)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "height",
                value: height.to_string(),
            })?;

        let chain_header = ChainHeader::try_construct(header, accum_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_header_containing_kernel_mmr",
                details: format!("Accumulated data mismatch at height #{}", height),
            }
        })?;
        Ok(chain_header)
    }

    // TODO: Can be merged with the method above
    fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;
        // LMDB returns the height at the position, so we have to offset the position by 1 so that the mmr_position arg
        // is an index starting from 0
        let mmr_position = mmr_position + 1;

        let (height, _hash) =
            lmdb_first_after::<_, (u64, Vec<u8>)>(&txn, &self.output_mmr_size_index, &mmr_position.to_be_bytes())?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "output_mmr_size_index",
                    field: "mmr_position",
                    value: mmr_position.to_string(),
                })?;

        let header: BlockHeader =
            lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeader",
                field: "height",
                value: height.to_string(),
            })?;
        let accum_data = self
            .fetch_header_accumulated_data_by_height(&txn, height)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "height",
                value: height.to_string(),
            })?;

        let chain_header = ChainHeader::try_construct(header, accum_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_header_containing_utxo_mmr",
                details: format!("Accumulated data mismatch at height #{}", height),
            }
        })?;
        Ok(chain_header)
    }

    fn is_empty(&self) -> Result<bool, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(lmdb_len(&txn, &self.headers_db)? == 0)
    }

    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
        let txn = self.read_transaction()?;
        if let Some(height) = self.fetch_height_from_hash(&txn, header_hash)? {
            self.fetch_block_accumulated_data(&txn, height)
        } else {
            Ok(None)
        }
    }

    fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
        let txn = self.read_transaction()?;
        self.fetch_block_accumulated_data(&txn, height)
    }

    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(lmdb_fetch_matching_after(&txn, &self.kernels_db, header_hash)?
            .into_iter()
            .map(|f: TransactionKernelRowData| f.kernel)
            .collect())
    }

    fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        let txn = self.read_transaction()?;
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
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let mut key = Vec::<u8>::new();
        key.extend(excess_sig.get_public_nonce().as_bytes());
        key.extend(excess_sig.get_signature().as_bytes());
        if let Some((header_hash, mmr_position, hash)) =
            lmdb_get::<_, (HashOutput, u32, HashOutput)>(&txn, &self.kernel_excess_sig_index, key.as_slice())?
        {
            let key = KernelKey::new(&header_hash, mmr_position, &hash);
            Ok(lmdb_get(&txn, &self.kernels_db, key.as_bytes())?
                .map(|kernel: TransactionKernelRowData| (kernel.kernel, header_hash)))
        } else {
            Ok(None)
        }
    }

    fn fetch_utxos_in_block(
        &self,
        header_hash: &HashOutput,
        deleted: Option<&Bitmap>,
    ) -> Result<(Vec<PrunedOutput>, Bitmap), ChainStorageError> {
        let txn = self.read_transaction()?;

        let utxos = lmdb_fetch_matching_after::<TransactionOutputRowData>(&txn, &self.utxos_db, header_hash)?
            .into_iter()
            .map(|row| {
                if deleted.map(|b| b.contains(row.mmr_position)).unwrap_or(false) {
                    return PrunedOutput::Pruned {
                        output_hash: row.hash,
                        witness_hash: row.witness_hash,
                    };
                }
                if let Some(output) = row.output {
                    PrunedOutput::NotPruned { output }
                } else {
                    PrunedOutput::Pruned {
                        output_hash: row.hash,
                        witness_hash: row.witness_hash,
                    }
                }
            })
            .collect();

        let height =
            self.fetch_height_from_hash(&txn, header_hash)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockHeader",
                    field: "hash",
                    value: header_hash.to_hex(),
                })?;

        // Builds a BitMap of the deleted UTXO MMR indexes that occurred at the current height
        let acc_data =
            self.fetch_block_accumulated_data(&txn, height)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData",
                    field: "height",
                    value: height.to_string(),
                })?;

        let mut difference_bitmap = Bitmap::create();
        difference_bitmap.or_inplace(acc_data.deleted());

        Ok((utxos, difference_bitmap))
    }

    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<UtxoMinedInfo>, ChainStorageError> {
        debug!(target: LOG_TARGET, "Fetch output: {}", output_hash.to_hex());
        let txn = self.read_transaction()?;
        self.fetch_output_in_txn(&*txn, output_hash)
    }

    fn fetch_unspent_output_hash_by_commitment(
        &self,
        commitment: &Commitment,
    ) -> Result<Option<HashOutput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_get::<_, HashOutput>(&*txn, &*self.utxo_commitment_index, commitment.as_bytes())
    }

    fn fetch_utxo_by_unique_id(
        &self,
        parent_public_key: Option<&PublicKey>,
        unique_id: &[u8],
        deleted_height: Option<u64>,
    ) -> Result<Option<UtxoMinedInfo>, ChainStorageError> {
        let txn = self.read_transaction()?;

        let mut key = UniqueIdIndexKey::new(parent_public_key, unique_id);
        if let Some(height) = deleted_height {
            key.set_deleted_height(height);
        }

        let output_hash = {
            let mut cursor = lmdb_get_prefix_cursor(&txn, &self.unique_id_index, key.as_prefix_bytes())?;
            // Seek to the exact matching key or greater
            let r = cursor.seek_gte(key.as_bytes())?;
            match r {
                Some((k, output_hash)) if k == key.as_bytes() => Some(output_hash),
                _ => {
                    // Either return the output hash that matches the key exactly, or the key before the given key.
                    // Since a key without a deleted height is `0xFF x 8 (u64::MAX)` that key will always return the
                    // output in the UTxO set or the last TxO matching the <parent_pk, unique_id>
                    // tuple
                    cursor.prev()?.map(|(_, v)| v)
                },
            }
        };

        match output_hash {
            Some(output_hash) => self.fetch_output_in_txn(&txn, &output_hash),
            None => Ok(None),
        }
    }

    fn fetch_all_unspent_by_parent_public_key(
        &self,
        parent_public_key: &PublicKey,
        range: Range<usize>,
    ) -> Result<Vec<UtxoMinedInfo>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let key = parent_public_key.as_bytes();
        let values: Vec<HashOutput> = lmdb_fetch_matching_after(&txn, &self.unique_id_index, key)?;
        let mut result = vec![];
        for hash in values.into_iter().skip(range.start).take(range.len()) {
            if let Some(s) = self.fetch_output_in_txn(&txn, &hash)? {
                result.push(s);
            }
        }
        Ok(result)
    }

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<PrunedOutput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(lmdb_fetch_matching_after(&txn, &self.utxos_db, header_hash)?
            .into_iter()
            .map(|f: TransactionOutputRowData| match f.output {
                Some(o) => PrunedOutput::NotPruned { output: o },
                None => PrunedOutput::Pruned {
                    output_hash: f.hash,
                    witness_hash: f.witness_hash,
                },
            })
            .collect())
    }

    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(lmdb_fetch_matching_after(&txn, &self.inputs_db, header_hash)?
            .into_iter()
            .map(|f: TransactionInputRowData| f.input)
            .collect())
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        let txn = self.read_transaction()?;
        match tree {
            MmrTree::Kernel => Ok(lmdb_len(&txn, &self.kernels_db)? as u64),
            MmrTree::Witness | MmrTree::Utxo => Ok(lmdb_len(&txn, &self.utxos_db)? as u64),
        }
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        let txn = self.read_transaction()?;
        self.fetch_mmr_leaf_index(&*txn, tree, hash)
    }

    /// Returns the number of blocks in the block orphan pool.
    fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        trace!(target: LOG_TARGET, "Get orphan count");
        let txn = self.read_transaction()?;
        lmdb_len(&txn, &self.orphans_db)
    }

    /// Finds and returns the last stored header.
    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        let txn = self.read_transaction()?;
        self.fetch_last_header_in_txn(&txn)?.ok_or_else(|| {
            ChainStorageError::InvalidOperation("Cannot fetch last header because database is empty".to_string())
        })
    }

    /// Finds and returns the last stored header.
    fn fetch_last_chain_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;
        let header = self.fetch_last_header_in_txn(&txn)?.ok_or_else(|| {
            ChainStorageError::InvalidOperation("Cannot fetch last header because database is empty".to_string())
        })?;
        let height = header.height;
        let accumulated_data = self
            .fetch_header_accumulated_data_by_height(&txn, height)?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "height",
                value: height.to_string(),
            })?;

        let chain_header = ChainHeader::try_construct(header, accumulated_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_tip_header",
                details: format!("Accumulated data mismatch at height #{}", height),
            }
        })?;

        Ok(chain_header)
    }

    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        let txn = self.read_transaction()?;

        let metadata = self.fetch_chain_metadata()?;
        let height = metadata.height_of_longest_chain();
        let header = lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
            entity: "Header",
            field: "height",
            value: height.to_string(),
        })?;
        let accumulated_data = self
            .fetch_header_accumulated_data_by_height(&txn, metadata.height_of_longest_chain())?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "height",
                value: height.to_string(),
            })?;
        let chain_header = ChainHeader::try_construct(header, accumulated_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_tip_header",
                details: format!("Accumulated data mismatch at height #{}", height),
            }
        })?;
        Ok(chain_header)
    }

    /// Returns the metadata of the chain.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        let txn = self.read_transaction()?;
        let metadata = fetch_metadata(&txn, &self.metadata_db)?;
        Ok(metadata)
    }

    fn utxo_count(&self) -> Result<usize, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_len(&txn, &self.utxos_db)
    }

    fn kernel_count(&self) -> Result<usize, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_len(&txn, &self.kernels_db)
    }

    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        trace!(target: LOG_TARGET, "Call to fetch_orphan_chain_tips()");
        let txn = self.read_transaction()?;
        if !lmdb_exists(&txn, &self.orphan_chain_tips_db, hash.as_slice())? {
            return Ok(None);
        }

        let orphan: Block =
            lmdb_get(&txn, &self.orphans_db, hash.as_slice())?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "Orphan",
                field: "hash",
                value: hash.to_hex(),
            })?;

        let accumulated_data =
            lmdb_get(&txn, &self.orphan_header_accumulated_data_db, hash.as_slice())?.ok_or_else(|| {
                ChainStorageError::ValueNotFound {
                    entity: "Orphan accumulated data",
                    field: "hash",
                    value: hash.to_hex(),
                }
            })?;

        let height = orphan.header.height;
        let chain_header = ChainHeader::try_construct(orphan.header, accumulated_data).ok_or_else(|| {
            ChainStorageError::DataInconsistencyDetected {
                function: "fetch_orphan_chain_tip_by_hash",
                details: format!("Accumulated data mismatch at height #{}", height),
            }
        })?;
        Ok(Some(chain_header))
    }

    fn fetch_orphan_children_of(&self, parent_hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
        trace!(
            target: LOG_TARGET,
            "Call to fetch_orphan_children_of({})",
            parent_hash.to_hex()
        );
        let txn = self.read_transaction()?;
        let orphan_hashes: Vec<HashOutput> =
            lmdb_get_multiple(&txn, &self.orphan_parent_map_index, parent_hash.as_slice())?;
        let mut res = Vec::with_capacity(orphan_hashes.len());
        for hash in orphan_hashes {
            res.push(lmdb_get(&txn, &self.orphans_db, hash.as_slice())?.ok_or_else(|| {
                ChainStorageError::ValueNotFound {
                    entity: "Orphan",
                    field: "hash",
                    value: hash.to_hex(),
                }
            })?)
        }
        Ok(res)
    }

    fn fetch_orphan_chain_block(&self, hash: HashOutput) -> Result<Option<ChainBlock>, ChainStorageError> {
        let txn = self.read_transaction()?;
        match lmdb_get::<_, Block>(&txn, &self.orphans_db, hash.as_slice())? {
            Some(block) => {
                match lmdb_get::<_, BlockHeaderAccumulatedData>(
                    &txn,
                    &self.orphan_header_accumulated_data_db,
                    hash.as_slice(),
                )? {
                    Some(accumulated_data) => {
                        let chain_block =
                            ChainBlock::try_construct(Arc::new(block), accumulated_data).ok_or_else(|| {
                                ChainStorageError::DataInconsistencyDetected {
                                    function: "fetch_orphan_chain_block",
                                    details: format!("Accumulated data mismatch for hash {}", hash.to_hex()),
                                }
                            })?;
                        Ok(Some(chain_block))
                    },
                    None => Ok(None),
                }
            },
            None => Ok(None),
        }
    }

    fn fetch_deleted_bitmap(&self) -> Result<DeletedBitmap, ChainStorageError> {
        let txn = self.read_transaction()?;
        let deleted_bitmap = self.load_deleted_bitmap_model(&txn)?;
        Ok(deleted_bitmap.get().clone())
    }

    fn fetch_header_hash_by_deleted_mmr_positions(
        &self,
        mmr_positions: Vec<u32>,
    ) -> Result<Vec<Option<(u64, HashOutput)>>, ChainStorageError> {
        let txn = self.read_transaction()?;

        mmr_positions
            .iter()
            .map(|pos| lmdb_get(&txn, &self.deleted_txo_mmr_position_to_height_index, pos))
            .collect()
    }

    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError> {
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
            let read_txn = self.read_transaction()?;

            orphans = lmdb_filter_map_values(&read_txn, &self.orphans_db, |block: Block| {
                Some((block.header.height, block.hash()))
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
            txn.delete_orphan(block_hash.clone());
        }
        self.write(txn)?;

        Ok(())
    }

    fn fetch_monero_seed_first_seen_height(&self, seed: &[u8]) -> Result<u64, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(lmdb_get(&txn, &self.monero_seed_height_db, seed)?.unwrap_or(0))
    }

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(Some(fetch_horizon_data(&txn, &self.metadata_db)?))
    }

    fn get_stats(&self) -> Result<DbBasicStats, ChainStorageError> {
        let global = self.env.stat()?;
        let env_info = self.env.info()?;

        let txn = self.read_transaction()?;
        let db_stats = self
            .all_dbs()
            .iter()
            .map(|(name, db)| txn.db_stat(db).map(|s| (*name, s)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DbBasicStats::new(global, env_info, db_stats))
    }

    fn fetch_total_size_stats(&self) -> Result<DbTotalSizeStats, ChainStorageError> {
        let txn = self.read_transaction()?;
        self.all_dbs()
            .iter()
            .map(|(name, db)| {
                fetch_db_entry_sizes(&txn, db).map(|(num_entries, total_key_size, total_value_size)| DbSize {
                    name,
                    num_entries,
                    total_key_size,
                    total_value_size,
                })
            })
            .collect()
    }

    fn bad_block_exists(&self, block_hash: HashOutput) -> Result<bool, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_exists(&txn, &self.bad_blocks, &block_hash)
    }

    fn clear_all_pending_headers(&self) -> Result<usize, ChainStorageError> {
        let txn = self.write_transaction()?;
        let last_header = match self.fetch_last_header_in_txn(&txn)? {
            Some(h) => h,
            None => {
                return Ok(0);
            },
        };
        let metadata = fetch_metadata(&txn, &self.metadata_db)?;

        if metadata.height_of_longest_chain() == last_header.height {
            return Ok(0);
        }

        let start = metadata.height_of_longest_chain() + 1;
        let end = last_header.height;

        let mut num_deleted = 0;
        for h in (start..=end).rev() {
            self.delete_header(&txn, h)?;
            num_deleted += 1;
        }
        txn.commit()?;
        Ok(num_deleted)
    }

    fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_filter_map_values(&txn, &self.reorgs, Some)
    }
}

// Fetch the chain metadata
fn fetch_metadata(txn: &ConstTransaction<'_>, db: &Database) -> Result<ChainMetadata, ChainStorageError> {
    Ok(ChainMetadata::new(
        fetch_chain_height(txn, db)?,
        fetch_best_block(txn, db)?,
        fetch_pruning_horizon(txn, db)?,
        fetch_pruned_height(txn, db)?,
        fetch_accumulated_work(txn, db)?,
    ))
}

// Fetches the chain height from the provided metadata db.
fn fetch_chain_height(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::ChainHeight;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::ChainHeight(height)) => Ok(height),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata",
            field: "ChainHeight",
            value: "".to_string(),
        }),
    }
}

/// Fetches the effective pruned height from the provided metadata db.
fn fetch_pruned_height(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PrunedHeight;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::PrunedHeight(height)) => Ok(height),
        _ => Ok(0),
    }
}

/// Fetches the horizon data from the provided metadata db.
fn fetch_horizon_data(txn: &ConstTransaction<'_>, db: &Database) -> Result<HorizonData, ChainStorageError> {
    let k = MetadataKey::HorizonData;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::HorizonData(data)) => Ok(data),
        None => Err(ChainStorageError::ValueNotFound {
            entity: "HorizonData",
            field: "metadata",
            value: "".to_string(),
        }),
        Some(k) => Err(ChainStorageError::DataInconsistencyDetected {
            function: "fetch_horizon_data",
            details: format!("Received incorrect value {:?} for key horizon data", k),
        }),
    }
}
// Fetches the best block hash from the provided metadata db.
fn fetch_best_block(txn: &ConstTransaction<'_>, db: &Database) -> Result<BlockHash, ChainStorageError> {
    let k = MetadataKey::BestBlock;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::BestBlock(best_block)) => Ok(best_block),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata",
            field: "BestBlock",
            value: "".to_string(),
        }),
    }
}

// Fetches the accumulated work from the provided metadata db.
fn fetch_accumulated_work(txn: &ConstTransaction<'_>, db: &Database) -> Result<u128, ChainStorageError> {
    let k = MetadataKey::AccumulatedWork;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::AccumulatedWork(accumulated_difficulty)) => Ok(accumulated_difficulty),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata",
            field: "AccumulatedWork",
            value: "".to_string(),
        }),
    }
}

// Fetches the deleted bitmap from the provided metadata db.
fn fetch_deleted_bitmap(txn: &ConstTransaction<'_>, db: &Database) -> Result<DeletedBitmap, ChainStorageError> {
    let k = MetadataKey::DeletedBitmap.as_u32();
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k)?;
    match val {
        Some(MetadataValue::DeletedBitmap(bitmap)) => Ok(bitmap),
        None => Ok(Bitmap::create().into()),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata",
            field: "DeletedBitmap",
            value: "".to_string(),
        }),
    }
}

// Fetches the pruning horizon from the provided metadata db.
fn fetch_pruning_horizon(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::PruningHorizon;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
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
    DeletedBitmap,
}

impl MetadataKey {
    #[inline]
    pub fn as_u32(self) -> u32 {
        self as u32
    }
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
            MetadataKey::DeletedBitmap => f.write_str("Deleted bitmap"),
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
    DeletedBitmap(DeletedBitmap),
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
            MetadataValue::DeletedBitmap(deleted) => {
                write!(f, "Deleted Bitmap ({} indexes)", deleted.bitmap().cardinality())
            },
        }
    }
}

#[derive(Debug, Clone)]
struct UniqueIdIndexKey {
    inner: Vec<u8>,
    prefix_len: usize,
}

impl UniqueIdIndexKey {
    /// Construct a key for the unique_id_index db.
    ///
    /// # Arguments
    /// `parent_public_key` - the parent asset public key to which the token is assigned
    /// `unique_id` - a series of bytes representing the token uniquely for the asset
    pub fn new(parent_public_key: Option<&PublicKey>, unique_id: &[u8]) -> Self {
        let unique_id_hash = HashDigest::default().chain(unique_id).finalize();
        Self::from_raw_parts(
            parent_public_key.map(|p| p.as_bytes()).unwrap_or(&[0; 32][..]),
            &unique_id_hash,
            // u64::MAX
            &[0xff; 8][..],
        )
    }

    /// Convert the key to a deleted at height key
    /// `deleted_height` - The height that the UTXO was deleted
    pub fn set_deleted_height(&mut self, deleted_height: u64) -> &mut Self {
        let n = self.inner.len() - mem::size_of::<u64>();
        self.inner[n..].copy_from_slice(&deleted_height.to_be_bytes());
        self
    }

    fn from_raw_parts(parent_public_key_bytes: &[u8], unique_id_bytes: &[u8], deleted_height_bytes: &[u8]) -> Self {
        let prefix_len = parent_public_key_bytes.len() + unique_id_bytes.len();
        let mut key = Vec::with_capacity(prefix_len + deleted_height_bytes.len());

        key.extend_from_slice(parent_public_key_bytes);
        key.extend_from_slice(unique_id_bytes);
        key.extend_from_slice(deleted_height_bytes);

        Self { inner: key, prefix_len }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.inner.as_slice()
    }

    pub fn as_prefix_bytes(&self) -> &[u8] {
        &self.inner[..self.prefix_len][..]
    }

    pub fn to_hex(&self) -> String {
        self.inner.to_hex()
    }
}

impl fmt::Display for UniqueIdIndexKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A struct that wraps a LMDB transaction and provides an interface to valid operations that can be performed
/// on the current deleted bitmap state of the blockchain.
/// A deleted bitmap contains the MMR leaf indexes of spent TXOs.
struct DeletedBitmapModel<'a, T> {
    txn: &'a T,
    db: &'a Database<'static>,
    bitmap: DeletedBitmap,
    is_dirty: bool,
}

impl<'a, 'b, T> DeletedBitmapModel<'a, T>
where T: Deref<Target = ConstTransaction<'b>>
{
    pub fn load(txn: &'a T, db: &'a Database<'static>) -> Result<Self, ChainStorageError> {
        let bitmap = fetch_deleted_bitmap(txn, db)?;
        Ok(Self {
            txn,
            db,
            bitmap,
            is_dirty: false,
        })
    }

    /// Returns a reference to the `DeletedBitmap`
    pub fn get(&self) -> &DeletedBitmap {
        &self.bitmap
    }
}

impl<'a, 'b> DeletedBitmapModel<'a, WriteTransaction<'b>> {
    /// Merge (union) the given bitmap into this instance.
    /// `finish` must be called to persist the bitmap.
    pub fn merge(&mut self, deleted: &Bitmap) -> Result<&mut Self, ChainStorageError> {
        self.bitmap.bitmap_mut().or_inplace(deleted);
        self.is_dirty = true;
        Ok(self)
    }

    /// Remove (difference) the given bitmap from this instance.
    /// `finish` must be called to persist the bitmap.
    pub fn remove(&mut self, deleted: &Bitmap) -> Result<&mut Self, ChainStorageError> {
        self.bitmap.bitmap_mut().andnot_inplace(deleted);
        self.is_dirty = true;
        Ok(self)
    }

    /// Persist the bitmap if required. This is a no-op if the bitmap has not been modified.
    pub fn save(mut self) -> Result<(), ChainStorageError> {
        if !self.is_dirty {
            return Ok(());
        }

        self.bitmap.bitmap_mut().run_optimize();
        lmdb_replace(
            self.txn,
            self.db,
            &MetadataKey::DeletedBitmap.as_u32(),
            &MetadataValue::DeletedBitmap(self.bitmap),
        )?;
        Ok(())
    }
}

struct CompositeKey {
    key: Vec<u8>,
}

impl CompositeKey {
    pub fn new(header_hash: &[u8], mmr_position: u32, hash: &[u8]) -> OutputKey {
        let mut key = Vec::with_capacity(header_hash.len() + mem::size_of::<u32>() + hash.len());
        key.extend_from_slice(header_hash);
        key.extend_from_slice(&mmr_position.to_be_bytes());
        key.extend_from_slice(hash);

        OutputKey { key }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }

    pub fn to_hex(&self) -> String {
        self.key.to_hex()
    }
}

type InputKey = CompositeKey;
type KernelKey = CompositeKey;
type OutputKey = CompositeKey;

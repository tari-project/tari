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

use std::{convert::TryFrom, fmt, fs, fs::File, ops::Deref, path::Path, sync::Arc, time::Instant};

use fs2::FileExt;
use lmdb_zero::{open, ConstTransaction, Database, Environment, ReadTransaction, WriteTransaction};
use log::*;
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    epoch::VnEpoch,
    types::{BlockHash, Commitment, FixedHash, HashOutput, PublicKey, Signature},
};
use tari_mmr::sparse_merkle_tree::{DeleteResult, NodeKey, ValueHash};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBConfig, LMDBStore};
use tari_utilities::{
    hex::{to_hex, Hex},
    ByteArray,
};

use super::{cursors::KeyPrefixCursor, lmdb::lmdb_get_prefix_cursor};
use crate::{
    blocks::{
        Block,
        BlockAccumulatedData,
        BlockHeader,
        BlockHeaderAccumulatedData,
        ChainBlock,
        ChainHeader,
        UpdateBlockAccumulatedData,
    },
    chain_storage::{
        db_transaction::{DbKey, DbTransaction, DbValue, WriteOperation},
        error::{ChainStorageError, OrNotFound},
        lmdb_db::{
            composite_key::{CompositeKey, InputKey, OutputKey},
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
                lmdb_insert,
                lmdb_insert_dup,
                lmdb_last,
                lmdb_len,
                lmdb_replace,
            },
            validator_node_store::ValidatorNodeStore,
            TransactionInputRowData,
            TransactionInputRowDataRef,
            TransactionKernelRowData,
            TransactionOutputRowData,
        },
        stats::DbTotalSizeStats,
        utxo_mined_info::OutputMinedInfo,
        BlockchainBackend,
        ChainTipData,
        DbBasicStats,
        DbSize,
        HorizonData,
        InputMinedInfo,
        MmrTree,
        Reorg,
        TemplateRegistrationEntry,
        ValidatorNodeEntry,
    },
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{
            OutputType,
            SpentOutput,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
            ValidatorNodeRegistration,
        },
    },
    OutputSmt,
    PrunedKernelMmr,
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
const LMDB_DB_DELETED_TXO_HASH_TO_HEADER_INDEX: &str = "deleted_txo_hash_to_header_index";
const LMDB_DB_UTXO_COMMITMENT_INDEX: &str = "utxo_commitment_index";
const LMDB_DB_UNIQUE_ID_INDEX: &str = "unique_id_index";
const LMDB_DB_CONTRACT_ID_INDEX: &str = "contract_index";
const LMDB_DB_ORPHANS: &str = "orphans";
const LMDB_DB_MONERO_SEED_HEIGHT: &str = "monero_seed_height";
const LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA: &str = "orphan_accumulated_data";
const LMDB_DB_ORPHAN_CHAIN_TIPS: &str = "orphan_chain_tips";
const LMDB_DB_ORPHAN_PARENT_MAP_INDEX: &str = "orphan_parent_map_index";
const LMDB_DB_BAD_BLOCK_LIST: &str = "bad_blocks";
const LMDB_DB_REORGS: &str = "reorgs";
const LMDB_DB_VALIDATOR_NODES: &str = "validator_nodes";
const LMDB_DB_VALIDATOR_NODES_MAPPING: &str = "validator_nodes_mapping";
const LMDB_DB_TEMPLATE_REGISTRATIONS: &str = "template_registrations";
const LMDB_DB_TIP_UTXO_SMT: &str = "tip_utxo_smt";

/// HeaderHash(32), mmr_pos(8), hash(32)
type KernelKey = CompositeKey<72>;
/// Height(8), Hash(32)
type ValidatorNodeRegistrationKey = CompositeKey<40>;

pub fn create_lmdb_database<P: AsRef<Path>>(
    path: P,
    config: LMDBConfig,
    consensus_manager: ConsensusManager,
) -> Result<LMDBDatabase, ChainStorageError> {
    let flags = db::CREATE;
    debug!(target: LOG_TARGET, "Creating LMDB database at {:?}", path.as_ref());
    fs::create_dir_all(&path)?;

    let file_lock = acquire_exclusive_file_lock(path.as_ref())?;

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
        .add_database(LMDB_DB_UTXO_COMMITMENT_INDEX, flags)
        .add_database(LMDB_DB_UNIQUE_ID_INDEX, flags)
        .add_database(LMDB_DB_CONTRACT_ID_INDEX, flags)
        .add_database(LMDB_DB_DELETED_TXO_HASH_TO_HEADER_INDEX, flags)
        .add_database(LMDB_DB_ORPHANS, flags)
        .add_database(LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_MONERO_SEED_HEIGHT, flags)
        .add_database(LMDB_DB_ORPHAN_CHAIN_TIPS, flags)
        .add_database(LMDB_DB_ORPHAN_PARENT_MAP_INDEX, flags | db::DUPSORT)
        .add_database(LMDB_DB_BAD_BLOCK_LIST, flags)
        .add_database(LMDB_DB_REORGS, flags | db::INTEGERKEY)
        .add_database(LMDB_DB_VALIDATOR_NODES, flags)
        .add_database(LMDB_DB_VALIDATOR_NODES_MAPPING, flags)
        .add_database(LMDB_DB_TEMPLATE_REGISTRATIONS, flags | db::DUPSORT)
        .add_database(LMDB_DB_TIP_UTXO_SMT, flags)
        .build()
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not create LMDB store:{}", err)))?;
    debug!(target: LOG_TARGET, "LMDB database creation successful");
    LMDBDatabase::new(&lmdb_store, file_lock, consensus_manager)
}

/// This is a lmdb-based blockchain database for persistent storage of the chain state.
pub struct LMDBDatabase {
    env: Arc<Environment>,
    env_config: LMDBConfig,
    metadata_db: DatabaseRef,
    /// Maps height -> BlockHeader
    headers_db: DatabaseRef,
    /// Maps height -> BlockHeaderAccumulatedData
    header_accumulated_data_db: DatabaseRef,
    /// Maps height -> BlockAccumulatedData
    block_accumulated_data_db: DatabaseRef,
    /// Maps block_hash -> height
    block_hashes_db: DatabaseRef,
    /// Maps OutputKey -> TransactionOutputRowData
    utxos_db: DatabaseRef,
    /// Maps InputKey -> TransactionInputRowData
    inputs_db: DatabaseRef,
    /// Maps OutputHash -> <mmr_pos, OutputKey>
    txos_hash_to_index_db: DatabaseRef,
    /// Maps KernelKey -> TransactionKernelRowData
    kernels_db: DatabaseRef,
    /// Maps excess -> <block_hash, mmr_pos, kernel_hash>
    kernel_excess_index: DatabaseRef,
    /// Maps excess_sig -> <block_hash, mmr_pos, kernel_hash>
    kernel_excess_sig_index: DatabaseRef,
    /// Maps kernel_mmr_size -> height
    kernel_mmr_size_index: DatabaseRef,
    /// Maps commitment -> output_hash
    utxo_commitment_index: DatabaseRef,
    /// Maps unique_id -> output_hash
    unique_id_index: DatabaseRef,
    /// Maps <contract_id, output_type> -> (block_hash, output_hash)
    /// and  <block_hash, output_type, contract_id> -> output_hash
    contract_index: DatabaseRef,
    /// Maps output hash-> <block_hash, input_hash>
    deleted_txo_hash_to_header_index: DatabaseRef,
    /// Maps block_hash -> Block
    orphans_db: DatabaseRef,
    /// Maps randomx_seed -> height
    monero_seed_height_db: DatabaseRef,
    /// Maps block_hash -> BlockHeaderAccumulatedData
    orphan_header_accumulated_data_db: DatabaseRef,
    /// Stores the orphan tip block hashes
    orphan_chain_tips_db: DatabaseRef,
    /// Maps parent_block_hash -> block_hash
    orphan_parent_map_index: DatabaseRef,
    /// Stores bad blocks by block_hash and height
    bad_blocks: DatabaseRef,
    /// Stores reorgs by epochtime and Reorg
    reorgs: DatabaseRef,
    /// Maps <Height, VN PK> -> ActiveValidatorNode
    validator_nodes: DatabaseRef,
    /// Stores the sparse merkle tree of the utxo set on tip
    tip_utxo_smt: DatabaseRef,
    /// Maps <Epoch, VN Public Key> -> VN Shard Key
    validator_nodes_mapping: DatabaseRef,
    /// Maps CodeTemplateRegistration <block_height, hash> -> TemplateRegistration
    template_registrations: DatabaseRef,
    _file_lock: Arc<File>,
    consensus_manager: ConsensusManager,
}

impl LMDBDatabase {
    pub fn new(
        store: &LMDBStore,
        file_lock: File,
        consensus_manager: ConsensusManager,
    ) -> Result<Self, ChainStorageError> {
        let env = store.env();

        let db = Self {
            metadata_db: get_database(store, LMDB_DB_METADATA)?,
            headers_db: get_database(store, LMDB_DB_HEADERS)?,
            header_accumulated_data_db: get_database(store, LMDB_DB_HEADER_ACCUMULATED_DATA)?,
            block_accumulated_data_db: get_database(store, LMDB_DB_BLOCK_ACCUMULATED_DATA)?,
            block_hashes_db: get_database(store, LMDB_DB_BLOCK_HASHES)?,
            utxos_db: get_database(store, LMDB_DB_UTXOS)?,
            inputs_db: get_database(store, LMDB_DB_INPUTS)?,
            txos_hash_to_index_db: get_database(store, LMDB_DB_TXOS_HASH_TO_INDEX)?,
            kernels_db: get_database(store, LMDB_DB_KERNELS)?,
            kernel_excess_index: get_database(store, LMDB_DB_KERNEL_EXCESS_INDEX)?,
            kernel_excess_sig_index: get_database(store, LMDB_DB_KERNEL_EXCESS_SIG_INDEX)?,
            kernel_mmr_size_index: get_database(store, LMDB_DB_KERNEL_MMR_SIZE_INDEX)?,
            utxo_commitment_index: get_database(store, LMDB_DB_UTXO_COMMITMENT_INDEX)?,
            unique_id_index: get_database(store, LMDB_DB_UNIQUE_ID_INDEX)?,
            contract_index: get_database(store, LMDB_DB_CONTRACT_ID_INDEX)?,
            deleted_txo_hash_to_header_index: get_database(store, LMDB_DB_DELETED_TXO_HASH_TO_HEADER_INDEX)?,
            orphans_db: get_database(store, LMDB_DB_ORPHANS)?,
            orphan_header_accumulated_data_db: get_database(store, LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA)?,
            monero_seed_height_db: get_database(store, LMDB_DB_MONERO_SEED_HEIGHT)?,
            orphan_chain_tips_db: get_database(store, LMDB_DB_ORPHAN_CHAIN_TIPS)?,
            orphan_parent_map_index: get_database(store, LMDB_DB_ORPHAN_PARENT_MAP_INDEX)?,
            bad_blocks: get_database(store, LMDB_DB_BAD_BLOCK_LIST)?,
            reorgs: get_database(store, LMDB_DB_REORGS)?,
            validator_nodes: get_database(store, LMDB_DB_VALIDATOR_NODES)?,
            validator_nodes_mapping: get_database(store, LMDB_DB_VALIDATOR_NODES_MAPPING)?,
            tip_utxo_smt: get_database(store, LMDB_DB_TIP_UTXO_SMT)?,
            template_registrations: get_database(store, LMDB_DB_TEMPLATE_REGISTRATIONS)?,
            env,
            env_config: store.env_config(),
            _file_lock: Arc::new(file_lock),
            consensus_manager,
        };

        run_migrations(&db)?;

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

    #[allow(clippy::too_many_lines)]
    fn apply_db_transaction(&mut self, txn: &DbTransaction) -> Result<(), ChainStorageError> {
        #[allow(clippy::enum_glob_use)]
        use WriteOperation::*;
        let write_txn = self.write_transaction()?;
        for op in txn.operations() {
            trace!(target: LOG_TARGET, "[apply_db_transaction] WriteOperation: {}", op);
            match op {
                InsertOrphanBlock(block) => self.insert_orphan_block(&write_txn, block)?,
                InsertChainHeader { header } => {
                    self.insert_header(&write_txn, header.header(), header.accumulated_data())?;
                },
                InsertTipBlockBody { block } => {
                    self.insert_tip_block_body(&write_txn, block.header(), block.block().body.clone())?;
                },
                InsertKernel {
                    header_hash,
                    kernel,
                    mmr_position,
                } => {
                    self.insert_kernel(&write_txn, header_hash, kernel, *mmr_position)?;
                },
                InsertOutput {
                    header_hash,
                    header_height,
                    timestamp,
                    output,
                } => {
                    self.insert_output(&write_txn, header_hash, *header_height, *timestamp, output)?;
                },
                DeleteHeader(height) => {
                    self.delete_header(&write_txn, *height)?;
                },
                DeleteOrphan(hash) => {
                    self.delete_orphan(&write_txn, hash)?;
                },
                DeleteOrphanChainTip(hash) => {
                    lmdb_delete(
                        &write_txn,
                        &self.orphan_chain_tips_db,
                        hash.deref(),
                        "orphan_chain_tips_db",
                    )?;
                },
                InsertOrphanChainTip(hash, total_accumulated_difficulty) => {
                    lmdb_insert(
                        &write_txn,
                        &self.orphan_chain_tips_db,
                        hash.deref(),
                        &ChainTipData {
                            hash: *hash,
                            total_accumulated_difficulty: *total_accumulated_difficulty,
                        },
                        "orphan_chain_tips_db",
                    )?;
                },
                DeleteTipBlock(hash) => {
                    self.delete_tip_block_body(&write_txn, hash)?;
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
                PruneOutputsSpentAtHash { block_hash } => {
                    self.prune_outputs_spent_at_hash(&write_txn, block_hash)?;
                },
                PruneOutputFromAllDbs {
                    output_hash,
                    commitment,
                    output_type,
                } => {
                    self.prune_output_from_all_dbs(&write_txn, output_hash, commitment, *output_type)?;
                },
                DeleteAllKernelsInBlock { block_hash } => {
                    self.delete_all_kernels_in_block(&write_txn, block_hash)?;
                },
                DeleteAllInputsInBlock { block_hash } => {
                    self.delete_all_inputs_in_block(&write_txn, block_hash)?;
                },
                SetBestBlock {
                    height,
                    hash,
                    accumulated_difficulty,
                    expected_prev_best_block,
                    timestamp,
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
                        &MetadataValue::ChainHeight(*height),
                    )?;
                    self.set_metadata(&write_txn, MetadataKey::BestBlock, &MetadataValue::BestBlock(*hash))?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::AccumulatedWork,
                        &MetadataValue::AccumulatedWork(*accumulated_difficulty),
                    )?;
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::BestBlockTimestamp,
                        &MetadataValue::BestBlockTimestamp(*timestamp),
                    )?;
                },
                SetPruningHorizonConfig(pruning_horizon) => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PruningHorizon,
                        &MetadataValue::PruningHorizon(*pruning_horizon),
                    )?;
                },
                SetPrunedHeight { height } => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::PrunedHeight,
                        &MetadataValue::PrunedHeight(*height),
                    )?;
                },
                SetHorizonData { horizon_data } => {
                    self.set_metadata(
                        &write_txn,
                        MetadataKey::HorizonData,
                        &MetadataValue::HorizonData(horizon_data.clone()),
                    )?;
                },
                InsertBadBlock { hash, height, reason } => {
                    self.insert_bad_block_and_cleanup(&write_txn, hash, *height, reason.to_string())?;
                },
                InsertReorg { reorg } => {
                    lmdb_replace(&write_txn, &self.reorgs, &reorg.local_time.timestamp(), &reorg)?;
                },
                ClearAllReorgs => {
                    lmdb_clear(&write_txn, &self.reorgs)?;
                },
                InsertTipSmt { smt } => {
                    self.insert_tip_smt(&write_txn, smt)?;
                },
            }
        }
        write_txn.commit()?;

        Ok(())
    }

    fn all_dbs(&self) -> [(&'static str, &DatabaseRef); 26] {
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
            ("utxo_commitment_index", &self.utxo_commitment_index),
            ("contract_index", &self.contract_index),
            ("unique_id_index", &self.unique_id_index),
            (
                "deleted_txo_hash_to_header_index",
                &self.deleted_txo_hash_to_header_index,
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
            ("validator_nodes", &self.validator_nodes),
            ("validator_nodes_mapping", &self.validator_nodes_mapping),
            ("template_registrations", &self.template_registrations),
        ]
    }

    fn insert_output(
        &self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        header_height: u64,
        header_timestamp: u64,
        output: &TransactionOutput,
    ) -> Result<(), ChainStorageError> {
        let output_hash = output.hash();

        let output_key = OutputKey::new(header_hash, &output_hash)?;

        if !output.is_burned() {
            lmdb_insert(
                txn,
                &self.utxo_commitment_index,
                output.commitment.as_bytes(),
                &output_hash,
                "utxo_commitment_index",
            )?;
        }

        lmdb_insert(
            txn,
            &self.txos_hash_to_index_db,
            output_hash.as_slice(),
            &(output_key.clone().convert_to_comp_key().to_vec()),
            "txos_hash_to_index_db",
        )?;
        lmdb_insert(
            txn,
            &self.utxos_db,
            &output_key.convert_to_comp_key(),
            &TransactionOutputRowData {
                output: output.clone(),
                header_hash: *header_hash,
                hash: output_hash,
                mined_height: header_height,
                mined_timestamp: header_timestamp,
            },
            "utxos_db",
        )?;

        Ok(())
    }

    fn insert_kernel(
        &self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        kernel: &TransactionKernel,
        mmr_position: u64,
    ) -> Result<(), ChainStorageError> {
        let hash = kernel.hash();
        let key = KernelKey::try_from_parts(&[
            header_hash.as_slice(),
            mmr_position.to_be_bytes().as_slice(),
            hash.as_slice(),
        ])?;

        lmdb_insert(
            txn,
            &self.kernel_excess_index,
            kernel.excess.as_bytes(),
            &(*header_hash, mmr_position, hash),
            "kernel_excess_index",
        )?;

        let mut excess_sig_key = Vec::<u8>::with_capacity(32 * 2);
        excess_sig_key.extend(kernel.excess_sig.get_public_nonce().as_bytes());
        excess_sig_key.extend(kernel.excess_sig.get_signature().as_bytes());
        lmdb_insert(
            txn,
            &self.kernel_excess_sig_index,
            excess_sig_key.as_slice(),
            &(*header_hash, mmr_position, hash),
            "kernel_excess_sig_index",
        )?;

        lmdb_insert(
            txn,
            &self.kernels_db,
            &key,
            &TransactionKernelRowData {
                kernel: kernel.clone(),
                header_hash: *header_hash,
                mmr_position,
                hash,
            },
            "kernels_db",
        )
    }

    fn input_with_output_data(
        &self,
        txn: &WriteTransaction<'_>,
        input: TransactionInput,
    ) -> Result<TransactionInput, ChainStorageError> {
        let input_with_output_data = match input.spent_output {
            SpentOutput::OutputData { .. } => input,
            SpentOutput::OutputHash(output_hash) => match self.fetch_output_in_txn(txn, output_hash.as_slice()) {
                Ok(Some(utxo_mined_info)) => TransactionInput {
                    version: input.version,
                    spent_output: SpentOutput::create_from_output(utxo_mined_info.output),
                    input_data: input.input_data,
                    script_signature: input.script_signature,
                },
                Ok(None) => {
                    error!(
                        target: LOG_TARGET,
                        "Could not retrieve output data from input's output_hash `{}`",
                        output_hash.to_hex()
                    );
                    return Err(ChainStorageError::ValueNotFound {
                        entity: "UTXO",
                        field: "hash",
                        value: output_hash.to_hex(),
                    });
                },
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Could not retrieve output data from input's output_hash `{}` ({})",
                        output_hash.to_hex(), e
                    );
                    return Err(e);
                },
            },
        };
        Ok(input_with_output_data)
    }

    fn insert_input(
        &self,
        txn: &WriteTransaction<'_>,
        height: u64,
        header_timestamp: u64,
        header_hash: &HashOutput,
        input: TransactionInput,
    ) -> Result<(), ChainStorageError> {
        let input_with_output_data = self.input_with_output_data(txn, input)?;
        lmdb_delete(
            txn,
            &self.utxo_commitment_index,
            input_with_output_data.commitment()?.as_bytes(),
            "utxo_commitment_index",
        )?;

        let hash = input_with_output_data.canonical_hash();
        let output_hash = input_with_output_data.output_hash();
        let key = InputKey::new(header_hash, &hash)?;
        lmdb_insert(
            txn,
            &self.deleted_txo_hash_to_header_index,
            output_hash.as_slice(),
            &(key.clone().convert_to_comp_key().to_vec()),
            "deleted_txo_hash_to_header_index",
        )?;

        lmdb_insert(
            txn,
            &self.inputs_db,
            &key.convert_to_comp_key(),
            &TransactionInputRowDataRef {
                input: &input_with_output_data.to_compact(),
                header_hash,
                spent_timestamp: header_timestamp,
                spent_height: height,
                hash: &hash,
            },
            "inputs_db",
        )
    }

    fn set_metadata(
        &self,
        txn: &WriteTransaction<'_>,
        k: MetadataKey,
        v: &MetadataValue,
    ) -> Result<(), ChainStorageError> {
        lmdb_replace(txn, &self.metadata_db, &k.as_u32(), v)?;
        Ok(())
    }

    fn insert_orphan_block(&self, txn: &WriteTransaction<'_>, block: &Block) -> Result<(), ChainStorageError> {
        let k = block.hash();
        lmdb_insert_dup(txn, &self.orphan_parent_map_index, block.header.prev_hash.deref(), &k)?;
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
        } else {
            // we can continue
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

        let hash = header.hash();

        // Check that there are no utxos or kernels linked to this.
        if !lmdb_fetch_matching_after::<TransactionKernelRowData>(txn, &self.kernels_db, hash.as_slice())?.is_empty() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Cannot delete header {} ({}) because there are kernels linked to it",
                header.height,
                hash.to_hex()
            )));
        }
        if !lmdb_fetch_matching_after::<TransactionOutputRowData>(txn, &self.utxos_db, hash.as_slice())?.is_empty() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Cannot delete header at height {} ({}) because there are UTXOs linked to it",
                height,
                hash.to_hex()
            )));
        }

        lmdb_delete(txn, &self.block_hashes_db, hash.as_slice(), "block_hashes_db")?;
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

        Ok(())
    }

    fn delete_tip_block_body(
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
        let next_height = height.saturating_add(1);
        let prev_height = height.saturating_sub(1);
        if self.fetch_block_accumulated_data(write_txn, next_height)?.is_some() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Attempted to delete block at height {} while next block still exists",
                height
            )));
        }

        lmdb_delete(
            write_txn,
            &self.block_accumulated_data_db,
            &height,
            "block_accumulated_data_db",
        )?;
        let mut smt = self.fetch_tip_smt()?;

        self.delete_block_inputs_outputs(write_txn, block_hash.as_slice(), &mut smt)?;

        let new_tip_header = self.fetch_chain_header_by_height(prev_height)?;
        let root = FixedHash::try_from(smt.hash().as_slice())?;
        if root != new_tip_header.header().output_mr {
            error!(
                target: LOG_TARGET,
                "Deleting block, new smt root(#{}) did not match expected (#{}) smt root",
                    root.to_hex(),
                    new_tip_header.header().output_mr.to_hex(),
            );
            return Err(ChainStorageError::InvalidOperation(
                "Deleting block, new smt root did not match expected smt root".to_string(),
            ));
        }

        self.insert_tip_smt(write_txn, &smt)?;
        self.delete_block_kernels(write_txn, block_hash.as_slice())?;

        Ok(())
    }

    fn delete_block_inputs_outputs(
        &self,
        txn: &WriteTransaction<'_>,
        block_hash: &[u8],
        output_smt: &mut OutputSmt,
    ) -> Result<(), ChainStorageError> {
        let output_rows = lmdb_delete_keys_starting_with::<TransactionOutputRowData>(txn, &self.utxos_db, block_hash)?;
        debug!(target: LOG_TARGET, "Deleted {} outputs...", output_rows.len());
        let inputs = lmdb_delete_keys_starting_with::<TransactionInputRowData>(txn, &self.inputs_db, block_hash)?;
        debug!(target: LOG_TARGET, "Deleted {} input(s)...", inputs.len());

        for utxo in &output_rows {
            trace!(target: LOG_TARGET, "Deleting UTXO `{}`", to_hex(utxo.hash.as_slice()));
            lmdb_delete(
                txn,
                &self.txos_hash_to_index_db,
                utxo.hash.as_slice(),
                "txos_hash_to_index_db",
            )?;

            let output_hash = utxo.output.hash();
            // if an output was already spent in the block, it was never created as unspent, so dont delete it as it
            // does not exist here
            if inputs.iter().any(|r| r.input.output_hash() == output_hash) {
                continue;
            }
            // if an output was burned, it was never created as an unspent utxo
            if utxo.output.is_burned() {
                continue;
            }
            let smt_key = NodeKey::try_from(utxo.output.commitment.as_bytes())?;
            match output_smt.delete(&smt_key)? {
                DeleteResult::Deleted(_value_hash) => {},
                DeleteResult::KeyNotFound => {
                    error!(
                        target: LOG_TARGET,
                        "Could not find input({}) in SMT",
                        utxo.output.commitment.to_hex(),
                    );
                    return Err(ChainStorageError::UnspendableInput);
                },
            };
            lmdb_delete(
                txn,
                &self.utxo_commitment_index,
                utxo.output.commitment.as_bytes(),
                "utxo_commitment_index",
            )?;
        }
        // Move inputs in this block back into the unspent set, any outputs spent within this block they will be removed
        // by deleting all the block's outputs below
        for row in inputs {
            // If input spends an output in this block, don't add it to the utxo set
            let output_hash = row.input.output_hash();

            lmdb_delete(
                txn,
                &self.deleted_txo_hash_to_header_index,
                output_hash.as_slice(),
                "deleted_txo_hash_to_header_index",
            )?;
            if output_rows.iter().any(|r| r.hash == output_hash) {
                continue;
            }

            let mut input = row.input.clone();

            let utxo_mined_info = self.fetch_output_in_txn(txn, output_hash.as_slice())?.ok_or_else(|| {
                ChainStorageError::ValueNotFound {
                    entity: "UTXO",
                    field: "hash",
                    value: output_hash.to_hex(),
                }
            })?;

            let rp_hash = match utxo_mined_info.output.proof {
                Some(proof) => proof.hash(),
                None => FixedHash::zero(),
            };
            input.add_output_data(
                utxo_mined_info.output.version,
                utxo_mined_info.output.features,
                utxo_mined_info.output.commitment,
                utxo_mined_info.output.script,
                utxo_mined_info.output.sender_offset_public_key,
                utxo_mined_info.output.covenant,
                utxo_mined_info.output.encrypted_data,
                utxo_mined_info.output.metadata_signature,
                rp_hash,
                utxo_mined_info.output.minimum_value_promise,
            );
            let smt_key = NodeKey::try_from(input.commitment()?.as_bytes())?;
            let smt_node = ValueHash::try_from(input.smt_hash(utxo_mined_info.mined_height).as_slice())?;
            if let Err(e) = output_smt.insert(smt_key, smt_node) {
                error!(
                    target: LOG_TARGET,
                    "Output commitment({}) already in SMT",
                    input.commitment()?.to_hex(),
                );
                return Err(e.into());
            }

            trace!(target: LOG_TARGET, "Input moved to UTXO set: {}", input);
            lmdb_insert(
                txn,
                &self.utxo_commitment_index,
                input.commitment()?.as_bytes(),
                &input.output_hash(),
                "utxo_commitment_index",
            )?;
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
            // We get rid of the orphan tip
            lmdb_delete(txn, &self.orphan_chain_tips_db, hash.as_slice(), "orphan_chain_tips_db")?;
            // If an orphan parent exists, it must be promoted
            match (
                lmdb_exists(txn, &self.orphans_db, parent_hash.as_slice())?,
                lmdb_exists(txn, &self.orphan_header_accumulated_data_db, parent_hash.as_slice())?,
            ) {
                (true, true) => {
                    // Parent becomes a tip hash
                    let orphan_parent_accum: Option<BlockHeaderAccumulatedData> =
                        lmdb_get(txn, &self.orphan_header_accumulated_data_db, parent_hash.as_slice())?;
                    match orphan_parent_accum {
                        Some(val) => {
                            lmdb_insert(
                                txn,
                                &self.orphan_chain_tips_db,
                                parent_hash.as_slice(),
                                &ChainTipData {
                                    hash: parent_hash,
                                    total_accumulated_difficulty: val.total_accumulated_difficulty,
                                },
                                "orphan_chain_tips_db",
                            )?;
                        },
                        None => {
                            warn!(
                                target: LOG_TARGET,
                                "Empty 'BlockHeaderAccumulatedData' for parent hash '{}'",
                                parent_hash.to_hex()
                            );
                        },
                    }
                },
                (false, false) => {
                    // No entries, nothing here
                },
                _ => {
                    // Some previous database operations were not atomic
                    warn!(
                        target: LOG_TARGET,
                        "'orphans_db' ({}) and 'orphan_header_accumulated_data_db' ({}) out of sync, missing parent hash '{}' entry",
                        lmdb_exists(txn, &self.orphans_db, parent_hash.as_slice())?,
                        lmdb_exists(txn, &self.orphan_header_accumulated_data_db, parent_hash.as_slice())?,
                        parent_hash.to_hex()
                    );
                },
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
        lmdb_delete(txn, &self.orphans_db, hash.as_slice(), "orphans_db")?;
        Ok(())
    }

    // Break function up into smaller pieces
    #[allow(clippy::too_many_lines)]
    fn insert_tip_block_body(
        &self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        body: AggregateBody,
    ) -> Result<(), ChainStorageError> {
        if self.fetch_block_accumulated_data(txn, header.height + 1)?.is_some() {
            return Err(ChainStorageError::InvalidOperation(format!(
                "Attempted to insert block at height {} while next block already exists",
                header.height
            )));
        }
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
            self.fetch_block_accumulated_data(txn, header.height - 1)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData",
                    field: "height",
                    value: (header.height - 1).to_string(),
                })?
        };

        let mut total_kernel_sum = Commitment::default();
        let BlockAccumulatedData {
            kernels: pruned_kernel_set,
            ..
        } = data;

        let mut kernel_mmr = PrunedKernelMmr::new(pruned_kernel_set);

        for kernel in kernels {
            total_kernel_sum = &total_kernel_sum + &kernel.excess;
            let pos =
                u64::try_from(kernel_mmr.push(kernel.hash().to_vec())?).map_err(|_| ChainStorageError::OutOfRange)?;
            trace!(
                target: LOG_TARGET,
                "Inserting kernel `{}`",
                kernel.excess_sig.get_signature().to_hex()
            );
            self.insert_kernel(txn, &block_hash, &kernel, pos)?;
        }
        let k = MetadataKey::TipSmt;
        let mut output_smt: OutputSmt =
            lmdb_get(txn, &self.tip_utxo_smt, &k.as_u32())?.ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "Output_smt",
                field: "tip",
                value: "".to_string(),
            })?;

        for output in outputs {
            trace!(
                target: LOG_TARGET,
                "Inserting output (`{}`, `{}`)",
                output.commitment.to_hex(),
                output.hash()
            );
            if !output.is_burned() {
                let smt_key = NodeKey::try_from(output.commitment.as_bytes())?;
                let smt_node = ValueHash::try_from(output.smt_hash(header.height).as_slice())?;
                if let Err(e) = output_smt.insert(smt_key, smt_node) {
                    error!(
                        target: LOG_TARGET,
                        "Output commitment({}) already in SMT",
                        output.commitment.to_hex(),
                    );
                    return Err(e.into());
                }
            }

            let output_hash = output.hash();
            if let Some(vn_reg) = output
                .features
                .sidechain_feature
                .as_ref()
                .and_then(|f| f.validator_node_registration())
            {
                self.insert_validator_node(txn, header, &output.commitment, vn_reg)?;
            }
            if let Some(template_reg) = output
                .features
                .sidechain_feature
                .as_ref()
                .and_then(|f| f.code_template_registration())
            {
                let record = TemplateRegistrationEntry {
                    registration_data: template_reg.clone(),
                    output_hash,
                    block_height: header.height,
                    block_hash,
                };

                self.insert_template_registration(txn, &record)?;
            }
            self.insert_output(txn, &block_hash, header.height, header.timestamp().as_u64(), &output)?;
        }

        // unique_id_index expects inputs to be inserted before outputs
        for input in inputs {
            let input_with_output_data = self.input_with_output_data(txn, input)?;
            let smt_key = NodeKey::try_from(input_with_output_data.commitment()?.as_bytes())?;
            match output_smt.delete(&smt_key)? {
                DeleteResult::Deleted(_value_hash) => {},
                DeleteResult::KeyNotFound => {
                    error!(
                        target: LOG_TARGET,
                        "Could not find input({}) in SMT",
                        input_with_output_data.commitment()?.to_hex(),
                    );
                    return Err(ChainStorageError::UnspendableInput);
                },
            };

            let features = input_with_output_data.features()?;
            if let Some(vn_reg) = features
                .sidechain_feature
                .as_ref()
                .and_then(|f| f.validator_node_registration())
            {
                self.validator_node_store(txn).delete(
                    header.height,
                    vn_reg.public_key(),
                    input_with_output_data.commitment()?,
                )?;
            }
            trace!(
                target: LOG_TARGET,
                "Inserting input (`{}`, `{}`)",
                input_with_output_data.commitment()?.to_hex(),
                input_with_output_data.output_hash().to_hex()
            );
            self.insert_input(
                txn,
                current_header_at_height.height,
                current_header_at_height.timestamp.as_u64(),
                &block_hash,
                input_with_output_data,
            )?;
        }

        self.insert_block_accumulated_data(
            txn,
            header.height,
            &BlockAccumulatedData::new(kernel_mmr.get_pruned_hash_set()?, total_kernel_sum),
        )?;
        self.insert_tip_smt(txn, &output_smt)?;

        Ok(())
    }

    fn validator_node_store<'a, T: Deref<Target = ConstTransaction<'a>>>(
        &'a self,
        txn: &'a T,
    ) -> ValidatorNodeStore<'a, T> {
        ValidatorNodeStore::new(txn, self.validator_nodes.clone(), self.validator_nodes_mapping.clone())
    }

    fn insert_validator_node(
        &self,
        txn: &WriteTransaction<'_>,
        header: &BlockHeader,
        commitment: &Commitment,
        vn_reg: &ValidatorNodeRegistration,
    ) -> Result<(), ChainStorageError> {
        let store = self.validator_node_store(txn);
        let constants = self.get_consensus_constants(header.height);
        let current_epoch = constants.block_height_to_epoch(header.height);

        let prev_shard_key = store.get_shard_key(
            current_epoch
                .as_u64()
                .saturating_sub(constants.validator_node_validity_period_epochs().as_u64()) *
                constants.epoch_length(),
            current_epoch.as_u64() * constants.epoch_length(),
            vn_reg.public_key(),
        )?;
        let shard_key = vn_reg.derive_shard_key(
            prev_shard_key,
            current_epoch,
            constants.validator_node_registration_shuffle_interval(),
            &header.prev_hash,
        );

        let next_epoch = constants.block_height_to_epoch(header.height) + VnEpoch(1);
        let validator_node = ValidatorNodeEntry {
            shard_key,
            start_epoch: next_epoch,
            end_epoch: next_epoch + constants.validator_node_validity_period_epochs(),
            public_key: vn_reg.public_key().clone(),
            commitment: commitment.clone(),
        };

        store.insert(header.height, &validator_node)?;
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

    fn insert_tip_smt(&self, txn: &WriteTransaction<'_>, smt: &OutputSmt) -> Result<(), ChainStorageError> {
        let k = MetadataKey::TipSmt;
        lmdb_replace(txn, &self.tip_utxo_smt, &k.as_u32(), smt)
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
            .fetch_block_accumulated_data(write_txn, height)?
            .unwrap_or_default();

        if let Some(kernel_sum) = values.kernel_sum {
            block_accum_data.kernel_sum = kernel_sum;
        }
        if let Some(kernel_hash_set) = values.kernel_hash_set {
            block_accum_data.kernels = kernel_hash_set;
        }

        lmdb_replace(write_txn, &self.block_accumulated_data_db, &height, &block_accum_data)?;
        Ok(())
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
        let inputs = lmdb_delete_keys_starting_with::<TransactionInput>(txn, &self.inputs_db, block_hash.as_slice())?;
        debug!(target: LOG_TARGET, "Deleted {} input(s)", inputs.len());
        Ok(())
    }

    fn prune_outputs_spent_at_hash(
        &self,
        write_txn: &WriteTransaction<'_>,
        block_hash: &HashOutput,
    ) -> Result<(), ChainStorageError> {
        let inputs =
            lmdb_fetch_matching_after::<TransactionInputRowData>(write_txn, &self.inputs_db, block_hash.as_slice())?;

        for input_data in inputs {
            let input = input_data.input;
            // From 'utxo_commitment_index::utxo_commitment_index'
            if let SpentOutput::OutputData { commitment, .. } = input.spent_output.clone() {
                debug!(target: LOG_TARGET, "Pruning output from 'utxo_commitment_index': key '{}'", commitment.to_hex());
                lmdb_delete(
                    write_txn,
                    &self.utxo_commitment_index,
                    commitment.as_bytes(),
                    "utxo_commitment_index",
                )?;
            }
            // From 'utxos_db::utxos_db'
            if let Some(key_bytes) =
                lmdb_get::<_, Vec<u8>>(write_txn, &self.txos_hash_to_index_db, input.output_hash().as_slice())?
            {
                let mut buffer = [0u8; 32];
                buffer.copy_from_slice(&key_bytes[0..32]);
                let key = OutputKey::new(&FixedHash::from(buffer), &input.output_hash())?;
                debug!(target: LOG_TARGET, "Pruning output from 'utxos_db': key '{}'", key.0);
                lmdb_delete(write_txn, &self.utxos_db, &key.convert_to_comp_key(), "utxos_db")?;
            };
            // From 'txos_hash_to_index_db::utxos_db'
            debug!(
                target: LOG_TARGET,
                "Pruning output from 'txos_hash_to_index_db': key '{}'",
                input.output_hash().to_hex()
            );
            lmdb_delete(
                write_txn,
                &self.txos_hash_to_index_db,
                input.output_hash().as_slice(),
                "utxos_db",
            )?;
        }

        Ok(())
    }

    fn prune_output_from_all_dbs(
        &self,
        write_txn: &WriteTransaction<'_>,
        output_hash: &HashOutput,
        commitment: &Commitment,
        output_type: OutputType,
    ) -> Result<(), ChainStorageError> {
        match lmdb_get::<_, Vec<u8>>(write_txn, &self.txos_hash_to_index_db, output_hash.as_slice())? {
            Some(key_bytes) => {
                if !matches!(output_type, OutputType::Burn) {
                    debug!(target: LOG_TARGET, "Pruning output from 'utxo_commitment_index': key '{}'", commitment.to_hex());
                    lmdb_delete(
                        write_txn,
                        &self.utxo_commitment_index,
                        commitment.as_bytes(),
                        "utxo_commitment_index",
                    )?;
                }
                debug!(target: LOG_TARGET, "Pruning output from 'txos_hash_to_index_db': key '{}'", output_hash.to_hex());
                lmdb_delete(
                    write_txn,
                    &self.txos_hash_to_index_db,
                    output_hash.as_slice(),
                    "utxos_db",
                )?;

                let mut buffer = [0u8; 32];
                buffer.copy_from_slice(&key_bytes[0..32]);
                let key = OutputKey::new(&FixedHash::from(buffer), output_hash)?;
                debug!(target: LOG_TARGET, "Pruning output from 'utxos_db': key '{}'", key.0);
                lmdb_delete(write_txn, &self.utxos_db, &key.convert_to_comp_key(), "utxos_db")?;
            },
            None => return Err(ChainStorageError::InvalidOperation("Output key not found".to_string())),
        }

        Ok(())
    }

    fn delete_all_kernels_in_block(
        &self,
        txn: &WriteTransaction<'_>,
        block_hash: &BlockHash,
    ) -> Result<(), ChainStorageError> {
        self.delete_block_kernels(txn, block_hash.as_slice())?;
        debug!(target: LOG_TARGET, "Deleted kernels in block {}", block_hash.to_hex());
        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    fn fetch_orphan(&self, txn: &ConstTransaction<'_>, hash: &HashOutput) -> Result<Option<Block>, ChainStorageError> {
        let val: Option<Block> = lmdb_get(txn, &self.orphans_db, hash.deref())?;
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
        reason: String,
    ) -> Result<(), ChainStorageError> {
        #[cfg(test)]
        const CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT: u64 = 10000;
        #[cfg(not(test))]
        const CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT: u64 = 0;

        lmdb_replace(txn, &self.bad_blocks, hash.deref(), &(height, reason))?;
        // Clean up bad blocks that are far from the tip
        let metadata = fetch_metadata(txn, &self.metadata_db)?;
        let deleted_before_height = metadata
            .best_block_height()
            .saturating_sub(CLEAN_BAD_BLOCKS_BEFORE_REL_HEIGHT);
        if deleted_before_height == 0 {
            return Ok(());
        }

        let num_deleted = lmdb_delete_each_where::<[u8], (u64, String), _>(txn, &self.bad_blocks, |_, (v, _)| {
            Some(v < deleted_before_height)
        })?;
        debug!(target: LOG_TARGET, "Cleaned out {} stale bad blocks", num_deleted);

        Ok(())
    }

    fn insert_template_registration(
        &self,
        txn: &WriteTransaction<'_>,
        template_registration: &TemplateRegistrationEntry,
    ) -> Result<(), ChainStorageError> {
        let key = ValidatorNodeRegistrationKey::try_from_parts(&[
            template_registration.block_height.to_le_bytes().as_slice(),
            template_registration.output_hash.as_slice(),
        ])?;
        lmdb_insert(
            txn,
            &self.template_registrations,
            &key,
            template_registration,
            "template_registrations",
        )
    }

    fn fetch_output_in_txn(
        &self,
        txn: &ConstTransaction<'_>,
        output_hash: &[u8],
    ) -> Result<Option<OutputMinedInfo>, ChainStorageError> {
        if let Some(key) = lmdb_get::<_, Vec<u8>>(txn, &self.txos_hash_to_index_db, output_hash)? {
            debug!(
                target: LOG_TARGET,
                "Fetch output: {} Found ({})",
                to_hex(output_hash),
                key.to_hex()
            );
            match lmdb_get::<_, TransactionOutputRowData>(txn, &self.utxos_db, &key)? {
                Some(TransactionOutputRowData {
                    output: o,
                    mined_height,
                    header_hash,
                    mined_timestamp,
                    ..
                }) => Ok(Some(OutputMinedInfo {
                    output: o,
                    mined_height,
                    header_hash,
                    mined_timestamp,
                })),

                _ => Ok(None),
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Fetch output: {} NOT found in index",
                to_hex(output_hash)
            );
            Ok(None)
        }
    }

    fn fetch_input_in_txn(
        &self,
        txn: &ConstTransaction<'_>,
        output_hash: &[u8],
    ) -> Result<Option<InputMinedInfo>, ChainStorageError> {
        if let Some(key) = lmdb_get::<_, Vec<u8>>(txn, &self.deleted_txo_hash_to_header_index, output_hash)? {
            debug!(
                target: LOG_TARGET,
                "Fetch input: {} Found ({})",
                to_hex(output_hash),
                key.to_hex()
            );
            match lmdb_get::<_, TransactionInputRowData>(txn, &self.inputs_db, &key)? {
                Some(TransactionInputRowData {
                    input: i,
                    spent_height: height,
                    header_hash,
                    spent_timestamp,
                    ..
                }) => Ok(Some(InputMinedInfo {
                    input: i,
                    spent_height: height,
                    header_hash,
                    spent_timestamp,
                })),

                _ => Ok(None),
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Fetch input: {} NOT found in index",
                to_hex(output_hash)
            );
            Ok(None)
        }
    }

    fn get_consensus_constants(&self, height: u64) -> &ConsensusConstants {
        self.consensus_manager.consensus_constants(height)
    }
}

pub fn create_recovery_lmdb_database<P: AsRef<Path>>(path: P) -> Result<(), ChainStorageError> {
    let new_path = path.as_ref().join("temp_recovery");
    let _result = fs::create_dir_all(&new_path);

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
            DbKey::HeaderHeight(k) => {
                let val: Option<BlockHeader> = lmdb_get(&txn, &self.headers_db, k)?;
                val.map(|val| DbValue::HeaderHeight(Box::new(val)))
            },
            DbKey::HeaderHash(hash) => {
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
                        val.map(|val| DbValue::HeaderHash(Box::new(val)))
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
            DbKey::HeaderHeight(k) => lmdb_exists(&txn, &self.headers_db, k)?,
            DbKey::HeaderHash(h) => lmdb_exists(&txn, &self.block_hashes_db, h.deref())?,
            DbKey::OrphanBlock(k) => lmdb_exists(&txn, &self.orphans_db, k.deref())?,
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
        Ok(lmdb_fetch_matching_after(&txn, &self.kernels_db, header_hash.deref())?
            .into_iter()
            .map(|f: TransactionKernelRowData| f.kernel)
            .collect())
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
            lmdb_get::<_, (HashOutput, u64, HashOutput)>(&txn, &self.kernel_excess_sig_index, key.as_slice())?
        {
            let key = KernelKey::try_from_parts(&[
                header_hash.as_slice(),
                mmr_position.to_be_bytes().as_slice(),
                hash.as_slice(),
            ])?;
            Ok(lmdb_get(&txn, &self.kernels_db, &key)?
                .map(|kernel: TransactionKernelRowData| (kernel.kernel, header_hash)))
        } else {
            Ok(None)
        }
    }

    fn fetch_outputs_in_block_with_spend_state(
        &self,
        previous_header_hash: &HashOutput,
        spend_status_at_header: Option<HashOutput>,
    ) -> Result<Vec<(TransactionOutput, bool)>, ChainStorageError> {
        let txn = self.read_transaction()?;

        let mut outputs: Vec<(TransactionOutput, bool)> =
            lmdb_fetch_matching_after::<TransactionOutputRowData>(&txn, &self.utxos_db, previous_header_hash.deref())?
                .into_iter()
                .map(|row| (row.output, false))
                .collect();
        if let Some(header_hash) = spend_status_at_header {
            let header_height =
                self.fetch_height_from_hash(&txn, &header_hash)?
                    .ok_or(ChainStorageError::ValueNotFound {
                        entity: "Header",
                        field: "hash",
                        value: header_hash.to_hex(),
                    })?;
            for output in &mut outputs {
                let hash = output.0.hash();
                if let Some(key) =
                    lmdb_get::<_, Vec<u8>>(&txn, &self.deleted_txo_hash_to_header_index, hash.as_slice())?
                {
                    let input = lmdb_get::<_, TransactionInputRowData>(&txn, &self.inputs_db, &key)?.ok_or(
                        ChainStorageError::ValueNotFound {
                            entity: "input",
                            field: "hash",
                            value: header_hash.to_hex(),
                        },
                    )?;
                    if input.spent_height <= header_height {
                        // we know its spend at the header height specified as optional in the fn
                        output.1 = true;
                    }
                }
            }
        }

        Ok(outputs)
    }

    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<OutputMinedInfo>, ChainStorageError> {
        debug!(target: LOG_TARGET, "Fetch output: {}", output_hash.to_hex());
        let txn = self.read_transaction()?;
        self.fetch_output_in_txn(&txn, output_hash.as_slice())
    }

    fn fetch_input(&self, output_hash: &HashOutput) -> Result<Option<InputMinedInfo>, ChainStorageError> {
        debug!(target: LOG_TARGET, "Fetch input: {}", output_hash.to_hex());
        let txn = self.read_transaction()?;
        self.fetch_input_in_txn(&txn, output_hash.as_slice())
    }

    fn fetch_unspent_output_hash_by_commitment(
        &self,
        commitment: &Commitment,
    ) -> Result<Option<HashOutput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_get::<_, HashOutput>(&txn, &self.utxo_commitment_index, commitment.as_bytes())
    }

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        lmdb_fetch_matching_after(&txn, &self.utxos_db, header_hash.as_slice())
    }

    fn fetch_inputs_in_block(
        &self,
        previous_header_hash: &HashOutput,
    ) -> Result<Vec<TransactionInput>, ChainStorageError> {
        let txn = self.read_transaction()?;
        Ok(
            lmdb_fetch_matching_after(&txn, &self.inputs_db, previous_header_hash.as_slice())?
                .into_iter()
                .map(|f: TransactionInputRowData| f.input)
                .collect(),
        )
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        let txn = self.read_transaction()?;
        match tree {
            MmrTree::Kernel => Ok(lmdb_len(&txn, &self.kernels_db)? as u64),
        }
    }

    /// Returns the number of blocks in the block orphan pool.
    fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        let txn = self.read_transaction()?;
        let count = lmdb_len(&txn, &self.orphans_db)?;
        trace!(target: LOG_TARGET, "Get orphan count ...({})", count);
        Ok(count)
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
        let height = metadata.best_block_height();
        let header = lmdb_get(&txn, &self.headers_db, &height)?.ok_or_else(|| ChainStorageError::ValueNotFound {
            entity: "Header",
            field: "height",
            value: height.to_string(),
        })?;
        let accumulated_data = self
            .fetch_header_accumulated_data_by_height(&txn, metadata.best_block_height())?
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
        lmdb_len(&txn, &self.utxo_commitment_index)
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

    fn fetch_strongest_orphan_chain_tips(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
        trace!(target: LOG_TARGET, "Call to fetch_strongest_orphan_chain_tips() ...");
        let timer = Instant::now();
        let txn = self.read_transaction()?;
        let tips: Vec<ChainTipData> = lmdb_filter_map_values(&txn, &self.orphan_chain_tips_db, Some)?;
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let max_value = tips.iter().map(|tip| tip.total_accumulated_difficulty).max();
        let strongest_tips = if let Some(val) = max_value {
            tips.iter()
                .filter(|tip| tip.total_accumulated_difficulty == val)
                .collect::<Vec<_>>()
        } else {
            // This branch should not be possible
            return Ok(Vec::new());
        };

        let tips_len = strongest_tips.len();
        let mut chain_tips = Vec::new();
        for chain_tip in strongest_tips {
            let orphan: Block = lmdb_get(&txn, &self.orphans_db, chain_tip.hash.as_slice())?.ok_or_else(|| {
                ChainStorageError::ValueNotFound {
                    entity: "Orphan",
                    field: "hash",
                    value: chain_tip.hash.to_hex(),
                }
            })?;
            let accumulated_data = lmdb_get(&txn, &self.orphan_header_accumulated_data_db, chain_tip.hash.as_slice())?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "Orphan accumulated data",
                    field: "hash",
                    value: chain_tip.hash.to_hex(),
                })?;

            let height = orphan.header.height;
            let chain_header = ChainHeader::try_construct(orphan.header, accumulated_data).ok_or_else(|| {
                ChainStorageError::DataInconsistencyDetected {
                    function: "fetch_orphan_chain_tip_by_hash",
                    details: format!("Accumulated data mismatch at height #{}", height),
                }
            })?;
            chain_tips.push(chain_header);
        }
        trace!(target: LOG_TARGET, "Call to fetch_strongest_orphan_chain_tips() ({}) completed in {:.2?}", tips_len, timer.elapsed());
        Ok(chain_tips)
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

        // Sort the orphans by age, oldest first
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
            txn.delete_orphan(block_hash);
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

    fn bad_block_exists(&self, block_hash: HashOutput) -> Result<(bool, String), ChainStorageError> {
        let txn = self.read_transaction()?;
        // We do this to ensure backwards compatibility on older exising dbs that did not store a reason
        let exist = lmdb_exists(&txn, &self.bad_blocks, block_hash.deref())?;
        match lmdb_get::<_, (u64, String)>(&txn, &self.bad_blocks, block_hash.deref()) {
            Ok(Some((_height, reason))) => Ok((true, reason)),
            Ok(None) => Ok((false, "".to_string())),
            Err(ChainStorageError::AccessError(e)) => {
                if exist {
                    Ok((true, "No reason recorded".to_string()))
                } else {
                    Err(ChainStorageError::AccessError(e))
                }
            },
            Err(e) => Err(e),
        }
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

        if metadata.best_block_height() == last_header.height {
            return Ok(0);
        }

        let start = metadata.best_block_height() + 1;
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

    fn fetch_active_validator_nodes(&self, height: u64) -> Result<Vec<(PublicKey, [u8; 32])>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let vn_store = self.validator_node_store(&txn);
        let constants = self.consensus_manager.consensus_constants(height);

        // Get the current epoch for the height
        let end_epoch = constants.block_height_to_epoch(height);
        // Subtract the registration validaty period to get the start epoch
        let start_epoch = end_epoch.saturating_sub(constants.validator_node_validity_period_epochs());
        // Convert these back to height as validators regs are indexed by height
        let start_height = start_epoch.as_u64() * constants.epoch_length();
        let end_height = end_epoch.as_u64() * constants.epoch_length();
        let nodes = vn_store.get_vn_set(start_height, end_height)?;
        Ok(nodes)
    }

    fn get_shard_key(&self, height: u64, public_key: PublicKey) -> Result<Option<[u8; 32]>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let store = self.validator_node_store(&txn);
        let constants = self.get_consensus_constants(height);

        // Get the epoch height boundaries for our query
        let current_epoch = constants.block_height_to_epoch(height);
        let start_epoch = current_epoch.saturating_sub(constants.validator_node_validity_period_epochs());
        let start_height = start_epoch.as_u64() * constants.epoch_length();
        let end_height = current_epoch.as_u64() * constants.epoch_length();
        let maybe_shard_id = store.get_shard_key(start_height, end_height, &public_key)?;
        Ok(maybe_shard_id)
    }

    fn fetch_template_registrations(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<TemplateRegistrationEntry>, ChainStorageError> {
        let txn = self.read_transaction()?;
        let mut result = vec![];
        for _ in start_height..=end_height {
            let height = start_height.to_le_bytes();
            let mut cursor: KeyPrefixCursor<TemplateRegistrationEntry> =
                lmdb_get_prefix_cursor(&txn, &self.template_registrations, &height)?;
            while let Some((_, val)) = cursor.next()? {
                result.push(val);
            }
        }
        Ok(result)
    }

    fn fetch_tip_smt(&self) -> Result<OutputSmt, ChainStorageError> {
        let txn = self.read_transaction()?;
        let k = MetadataKey::TipSmt;
        let val: Option<OutputSmt> = lmdb_get(&txn, &self.tip_utxo_smt, &k.as_u32())?;
        match val {
            Some(smt) => Ok(smt),
            _ => Err(ChainStorageError::ValueNotFound {
                entity: "TipSmt",
                field: "TipSmt",
                value: "".to_string(),
            }),
        }
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
        fetch_best_block_timestamp(txn, db)?,
    )?)
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

// Fetches the timestamp of the best block from the provided metadata db.
fn fetch_best_block_timestamp(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::BestBlockTimestamp;
    let val: Option<MetadataValue> = lmdb_get(txn, db, &k.as_u32())?;
    match val {
        Some(MetadataValue::BestBlockTimestamp(timestamp)) => Ok(timestamp),
        _ => Err(ChainStorageError::ValueNotFound {
            entity: "ChainMetadata",
            field: "BestBlockTimestamp",
            value: "".to_string(),
        }),
    }
}

// Fetches the accumulated work from the provided metadata db.
fn fetch_accumulated_work(txn: &ConstTransaction<'_>, db: &Database) -> Result<U256, ChainStorageError> {
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

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
enum MetadataKey {
    ChainHeight,
    BestBlock,
    AccumulatedWork,
    PruningHorizon,
    PrunedHeight,
    HorizonData,
    BestBlockTimestamp,
    MigrationVersion,
    TipSmt,
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
            MetadataKey::ChainHeight => write!(f, "Current chain height"),
            MetadataKey::AccumulatedWork => write!(f, "Total accumulated work"),
            MetadataKey::PruningHorizon => write!(f, "Pruning horizon"),
            MetadataKey::PrunedHeight => write!(f, "Effective pruned height"),
            MetadataKey::BestBlock => write!(f, "Chain tip block hash"),
            MetadataKey::HorizonData => write!(f, "Database info"),
            MetadataKey::BestBlockTimestamp => write!(f, "Chain tip block timestamp"),
            MetadataKey::MigrationVersion => write!(f, "Migration version"),
            MetadataKey::TipSmt => write!(f, "Chain tip Sparse Merkle Tree version"),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
enum MetadataValue {
    ChainHeight(u64),
    BestBlock(BlockHash),
    AccumulatedWork(U256),
    PruningHorizon(u64),
    PrunedHeight(u64),
    HorizonData(HorizonData),
    BestBlockTimestamp(u64),
    MigrationVersion(u64),
}

impl fmt::Display for MetadataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataValue::ChainHeight(h) => write!(f, "Chain height is {}", h),
            MetadataValue::AccumulatedWork(d) => write!(f, "Total accumulated work is {}", d),
            MetadataValue::PruningHorizon(h) => write!(f, "Pruning horizon is {}", h),
            MetadataValue::PrunedHeight(height) => write!(f, "Effective pruned height is {}", height),
            MetadataValue::BestBlock(hash) => write!(f, "Chain tip block hash is {}", hash),
            MetadataValue::HorizonData(_) => write!(f, "Horizon data"),
            MetadataValue::BestBlockTimestamp(timestamp) => write!(f, "Chain tip block timestamp is {}", timestamp),
            MetadataValue::MigrationVersion(n) => write!(f, "Migration version {}", n),
        }
    }
}

fn run_migrations(db: &LMDBDatabase) -> Result<(), ChainStorageError> {
    const MIGRATION_VERSION: u64 = 1;
    let txn = db.read_transaction()?;

    let k = MetadataKey::MigrationVersion;
    let val = lmdb_get::<_, MetadataValue>(&txn, &db.metadata_db, &k.as_u32())?;
    let n = match val {
        Some(MetadataValue::MigrationVersion(n)) => n,
        Some(_) | None => 0,
    };
    info!(
        target: LOG_TARGET,
        "Blockchain database is at v{} (required version: {})", n, MIGRATION_VERSION
    );
    drop(txn);

    if n < MIGRATION_VERSION {
        // Add migrations here
        info!(target: LOG_TARGET, "Migrated database to version {}", MIGRATION_VERSION);
        let txn = db.write_transaction()?;
        lmdb_replace(
            &txn,
            &db.metadata_db,
            &k.as_u32(),
            &MetadataValue::MigrationVersion(MIGRATION_VERSION),
        )?;
        txn.commit()?;
    }

    Ok(())
}

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
    blocks::{blockheader::BlockHeader, Block},
    chain_storage::{
        blockchain_database::BlockchainBackend,
        db_transaction::{DbKey, DbKeyValuePair, DbTransaction, DbValue, MetadataValue, MmrTree, WriteOperation},
        error::ChainStorageError,
        lmdb_db::{
            lmdb::{
                lmdb_delete,
                lmdb_delete_key_value,
                lmdb_delete_keys_starting_with,
                lmdb_exists,
                lmdb_fetch_keys_starting_with,
                lmdb_filter_map_values,
                lmdb_get,
                lmdb_get_multiple,
                lmdb_insert,
                lmdb_insert_dup,
                lmdb_last,
                lmdb_len,
                lmdb_list_keys,
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
            LMDB_DB_METADATA,
            LMDB_DB_ORPHANS,
            LMDB_DB_ORPHAN_CHAIN_TIPS,
            LMDB_DB_ORPHAN_PARENT_MAP_INDEX,
            LMDB_DB_TXOS_HASH_TO_INDEX,
            LMDB_DB_UTXOS,
        },
        BlockAccumulatedData,
        BlockHeaderAccumulatedData,
        ChainMetadata,
        MetadataKey,
    },
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::HashOutput,
    },
};
use fs2::FileExt;
use lmdb_zero::{ConstTransaction, Database, Environment, ReadTransaction, WriteTransaction};
use log::*;
use std::{
    collections::VecDeque,
    fs,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tari_common_types::types::BlockHash;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hash::Hashable, hex::Hex};
use tari_mmr::Hash;
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
    orphans_db: DatabaseRef,
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
            orphans_db: get_database(&store, LMDB_DB_ORPHANS)?,
            orphan_chain_tips_db: get_database(&store, LMDB_DB_ORPHAN_CHAIN_TIPS)?,
            orphan_parent_map_index: get_database(&store, LMDB_DB_ORPHAN_PARENT_MAP_INDEX)?,
            env,
            env_config: store.env_config(),
            is_mem_metadata_dirty: true,
            _file_lock: Arc::new(file_lock),
        };
        if !res.is_empty()? {
            res.refresh_chain_metadata()?;
        }
        Ok(res)
    }

    fn apply_db_transaction(&mut self, txn: &DbTransaction) -> Result<(), ChainStorageError> {
        let write_txn =
            WriteTransaction::new(self.env.clone()).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        for op in txn.operations.iter() {
            trace!(target: LOG_TARGET, "[apply_db_transaction] WriteOperation: {}", op);
            match op {
                WriteOperation::Insert(insert) => self.op_insert(&write_txn, insert)?,
                WriteOperation::Delete(delete) => self.op_delete(&write_txn, delete)?,
                WriteOperation::UpdateBlockAccumulatedData(header_hash, data) => {
                    self.op_update_block_accumulated_data(&write_txn, header_hash, data)?;
                },
                WriteOperation::InsertKernel {
                    header_hash,
                    kernel,
                    mmr_position,
                } => {
                    self.insert_kernel(&write_txn, header_hash.clone(), kernel.as_ref().clone(), *mmr_position)?;
                },
                WriteOperation::InsertOutput {
                    header_hash,
                    output,
                    mmr_position,
                } => {
                    self.insert_output(&write_txn, header_hash, output, *mmr_position)?;
                },
                WriteOperation::InsertInput {
                    header_hash,
                    input,
                    mmr_position,
                } => {
                    self.insert_input(&write_txn, header_hash.clone(), input.as_ref().clone(), *mmr_position)?;
                },
                WriteOperation::DeleteOrphanChainTip(hash) => {
                    lmdb_delete(&write_txn, &self.orphan_chain_tips_db, hash)?;
                },
                WriteOperation::InsertOrphanChainTip(hash) => {
                    lmdb_replace(&write_txn, &self.orphan_chain_tips_db, hash, hash)?;
                },
                WriteOperation::DeleteBlock(hash) => {
                    for utxo in lmdb_delete_keys_starting_with::<Option<TransactionOutputRowData>>(
                        &write_txn,
                        &self.utxos_db,
                        hash.to_hex().as_str(),
                    )? {
                        if let Some(u) = utxo {
                            lmdb_delete(&write_txn, &self.txos_hash_to_index_db, u.hash.as_slice())?;
                        } else {
                            // TODO: Replace when this node is pruned
                            unimplemented!();
                        }
                    }
                    lmdb_delete_keys_starting_with::<Option<TransactionKernelRowData>>(
                        &write_txn,
                        &self.kernels_db,
                        hash.to_hex().as_str(),
                    )?;
                    lmdb_delete_keys_starting_with::<TransactionInputRowData>(
                        &write_txn,
                        &self.inputs_db,
                        hash.to_hex().as_str(),
                    )?;
                },
                WriteOperation::InsertHeaderAccumulatedData(data) => {
                    lmdb_replace(
                        &write_txn,
                        &self.header_accumulated_data_db,
                        data.hash.clone().as_slice(),
                        data,
                    )?;
                },
            }
        }
        let metadata = fetch_metadata(&write_txn, &self.metadata_db)?;
        write_txn
            .commit()
            .map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        if self.is_mem_metadata_dirty {
            self.mem_metadata = Some(metadata);
            self.is_mem_metadata_dirty = false;
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

    #[allow(clippy::ptr_arg)]
    fn insert_output(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: &HashOutput,
        output: &TransactionOutput,
        mmr_position: u32,
    ) -> Result<(), ChainStorageError>
    {
        let output_hash = output.hash();
        let proof_hash = output.proof.hash();
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
        )?;
        lmdb_insert(
            txn,
            &*self.utxos_db,
            key.as_str(),
            &Some(TransactionOutputRowData {
                output: output.clone(),
                header_hash: header_hash.to_owned(),
                mmr_position,
                hash: output_hash,
                range_proof_hash: proof_hash,
            }),
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
            &*self.kernels_db,
            key.as_str(),
            &Some(TransactionKernelRowData {
                kernel,
                header_hash,
                mmr_position,
                hash,
            }),
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
        lmdb_insert(txn, &*self.inputs_db, key.as_str(), &TransactionInputRowData {
            input,
            header_hash,
            mmr_position,
            hash,
        })
    }

    fn op_insert(&mut self, txn: &WriteTransaction<'_>, kv_pair: &DbKeyValuePair) -> Result<(), ChainStorageError> {
        match kv_pair {
            DbKeyValuePair::Metadata(k, v) => {
                lmdb_replace(&txn, &self.metadata_db, &(*k as u32), &v)?;
                self.is_mem_metadata_dirty = true;
            },
            DbKeyValuePair::BlockHeader(k, v) => {
                if lmdb_exists(txn, &self.headers_db, k)? {
                    return Err(ChainStorageError::InvalidOperation(format!(
                        "Duplicate `BlockHeader` key `{}`",
                        k
                    )));
                }
                let hash = v.hash();
                lmdb_insert(&txn, &self.block_hashes_db, hash.as_slice(), k)?;
                lmdb_insert(&txn, &self.headers_db, k, &v)?;
            },
            DbKeyValuePair::OrphanBlock(k, v) => {
                if !lmdb_exists(&txn, &self.orphans_db, k.as_slice())? {
                    lmdb_insert_dup(&txn, &self.orphan_parent_map_index, &v.header.prev_hash, k)?;
                    lmdb_replace(&txn, &self.orphans_db, k.as_slice(), &**v)?;
                }
            },
        }
        Ok(())
    }

    fn op_delete(&mut self, txn: &WriteTransaction<'_>, key: &DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::Metadata(_key) => unimplemented!("Deleting of metadata keys not supported"),
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(txn, &self.headers_db, &*k)?;
                if let Some(v) = val {
                    let hash = v.hash();
                    lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                    lmdb_delete(&txn, &self.headers_db, k)?;
                }
            },
            DbKey::BlockHash(hash) => {
                let result: Option<u64> = lmdb_get(txn, &self.block_hashes_db, hash.as_slice())?;
                if let Some(k) = result {
                    lmdb_delete(&txn, &self.block_hashes_db, hash.as_slice())?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                }
            },
            DbKey::TransactionKernel(k) => {
                lmdb_delete(&txn, &self.kernels_db, k.as_slice())?;
            },
            DbKey::OrphanBlock(k) => {
                if let Some(orphan) = lmdb_get::<_, Block>(&txn, &self.orphans_db, k)? {
                    let parent_hash = orphan.header.prev_hash;
                    lmdb_delete_key_value(&txn, &self.orphan_parent_map_index, parent_hash.as_slice(), k)?;
                    let tip: Option<Vec<u8>> = lmdb_get(&txn, &self.orphan_chain_tips_db, k)?;
                    if tip.is_some() {
                        if lmdb_get::<_, Block>(&txn, &self.orphans_db, parent_hash.as_slice())?.is_some() {
                            lmdb_insert(&txn, &self.orphan_chain_tips_db, parent_hash.as_slice(), &parent_hash)?;
                        }
                        lmdb_delete(&txn, &self.orphan_chain_tips_db, k)?;
                    }
                    lmdb_delete(&txn, &self.orphans_db, k.as_slice())?;
                }
            },
        }

        Ok(())
    }

    fn op_update_block_accumulated_data(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: &[u8],
        data: &BlockAccumulatedData,
    ) -> Result<(), ChainStorageError>
    {
        lmdb_replace(&txn, &self.block_accumulated_data_db, header_hash, data)
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
        .add_database(LMDB_DB_HEADER_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_BLOCK_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_BLOCK_HASHES, flags)
        .add_database(LMDB_DB_UTXOS, flags)
        .add_database(LMDB_DB_INPUTS, flags)
        .add_database(LMDB_DB_TXOS_HASH_TO_INDEX, flags)
        .add_database(LMDB_DB_KERNELS, flags)
        .add_database(LMDB_DB_ORPHANS, flags)
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

pub fn remove_lmdb_database<P: AsRef<Path>>(path: P) -> Result<(), ChainStorageError> {
    fs::remove_dir_all(&path)
        .map_err(|err| ChainStorageError::CriticalError(format!("Could not remove LMDB store:{}", err)))?;
    Ok(())
}

pub fn acquire_exclusive_file_lock(db_path: &PathBuf) -> Result<File, ChainStorageError> {
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
        if txn.operations.is_empty() {
            return Ok(());
        }

        LMDBStore::resize_if_required(&self.env, &self.env_config)?;

        let mark = Instant::now();
        let num_operations = txn.operations.len();
        match self.apply_db_transaction(&txn) {
            Ok(_) => {
                debug!(
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
        let mark = Instant::now();

        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        let res = match key {
            DbKey::Metadata(k) => {
                let val: Option<MetadataValue> = lmdb_get(&txn, &self.metadata_db, &(*k as u32))?;
                val.map(DbValue::Metadata)
            },
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(&txn, &self.headers_db, k)?;
                val.map(|val| DbValue::BlockHeader(Box::new(val)))
            },
            DbKey::BlockHash(hash) => {
                let k: Option<u64> = lmdb_get(&txn, &self.block_hashes_db, hash)?;

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
            DbKey::TransactionKernel(k) => {
                let val: Option<TransactionKernel> = lmdb_get(&txn, &self.kernels_db, k)?;
                val.map(|val| DbValue::TransactionKernel(Box::new(val)))
            },
            DbKey::OrphanBlock(k) => {
                let val: Option<Block> = lmdb_get(&txn, &self.orphans_db, k)?;
                val.map(|val| DbValue::OrphanBlock(Box::new(val)))
            },
        };
        trace!(target: LOG_TARGET, "Fetched key {} in {:.0?}", key, mark.elapsed());
        Ok(res)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        Ok(match key {
            DbKey::Metadata(k) => lmdb_exists(&txn, &self.metadata_db, &(*k as u32))?,
            DbKey::BlockHeader(k) => lmdb_exists(&txn, &self.headers_db, k)?,
            DbKey::BlockHash(h) => lmdb_exists(&txn, &self.block_hashes_db, h)?,
            DbKey::TransactionKernel(k) => lmdb_exists(&txn, &self.kernels_db, k)?,
            DbKey::OrphanBlock(k) => lmdb_exists(&txn, &self.orphans_db, k)?,
        })
    }

    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_get(&txn, &self.header_accumulated_data_db, hash.as_slice())
    }

    fn is_empty(&self) -> Result<bool, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;

        Ok(lmdb_len(&txn, &self.headers_db)? == 0)
    }

    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<BlockAccumulatedData, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env)?;
        lmdb_get(&txn, &self.block_accumulated_data_db, header_hash.as_slice())?.ok_or_else(|| {
            ChainStorageError::ValueNotFound {
                entity: "MmrPeakData".to_string(),
                field: "header_hash".to_string(),
                value: header_hash.to_hex(),
            }
        })
    }

    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        Ok(
            lmdb_fetch_keys_starting_with(header_hash.to_hex().as_str(), &txn, &self.kernels_db)?
                .into_iter()
                .map(|f: Option<TransactionKernelRowData>| f.unwrap().kernel)
                .collect(),
        )
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
            if let Some(output) = lmdb_get::<_, Option<TransactionOutputRowData>>(&txn, &self.utxos_db, key.as_str())? {
                Ok(output.map(|o| (o.output, o.mmr_position)))
            } else {
                unimplemented!("Pruning of outputs not implemented")
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

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env)?;
        Ok(
            lmdb_fetch_keys_starting_with(header_hash.to_hex().as_str(), &txn, &self.utxos_db)?
                .into_iter()
                .map(|f: Option<TransactionOutputRowData>| f.unwrap().output)
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

    fn fetch_mmr_node_count(&self, _tree: MmrTree, _height: u64) -> Result<u32, ChainStorageError> {
        info!(target: LOG_TARGET, "Fetch MMR node count");
        unimplemented!();
        // let txn = ReadTransaction::new(&self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        // let tip_height = lmdb_len(&txn, &self.headers_db)?.saturating_sub(1) as u64;
        // match tree {
        //     MmrTree::Kernel => {
        //         checkpoint_utils::fetch_mmr_nodes_added_count(&self.kernel_checkpoints, tip_height, height)
        //     },
        //     MmrTree::Utxo => checkpoint_utils::fetch_mmr_nodes_added_count(&self.utxo_checkpoints, tip_height,
        // height),     MmrTree::RangeProof => {
        //         checkpoint_utils::fetch_mmr_nodes_added_count(&self.range_proof_checkpoints, tip_height, height)
        //     },
        // }
    }

    fn fetch_mmr_node(
        &self,
        _tree: MmrTree,
        _pos: u32,
        _hist_height: Option<u64>,
    ) -> Result<(Vec<u8>, bool), ChainStorageError>
    {
        info!(target: LOG_TARGET, "Fetch MMR node");
        unimplemented!();
        // let (hash, deleted) = match tree {
        //     MmrTree::Kernel => {
        //         self.kernel_mmr.fetch_mmr_node(pos)?
        //     },
        //     MmrTree::Utxo => {
        //         let (hash, mut deleted) = self.utxo_mmr.fetch_mmr_node(pos)?;
        //         // Check if the MMR node was deleted after the historic height then its deletion status should
        // change.         // TODO: Find a more efficient way to query the historic deletion status of an MMR
        // node.         if deleted {
        //             if let Some(hist_height) = hist_height {
        //                 let tip_height = lmdb_len(&self.env, &self.headers_db)?.saturating_sub(1) as u64;
        //                 for height in hist_height + 1..=tip_height {
        //                     let cp = self.fetch_checkpoint_at_height(MmrTree::Utxo, height)?;
        //                     if cp.nodes_deleted().contains(pos) {
        //                         deleted = false;
        //                     }
        //                 }
        //             }
        //         }
        //         (hash, deleted)
        //     },
        //     MmrTree::RangeProof => self.range_proof_mmr.fetch_mmr_node(pos)?,
        // };
        //
        // let hash = hash.ok_or_else(|| {
        //     ChainStorageError::UnexpectedResult(format!("A leaf node hash in the {} MMR tree was not found", tree))
        // })?;
        //
        // Ok((hash, deleted))
    }

    fn fetch_mmr_nodes(
        &self,
        tree: MmrTree,
        pos: u32,
        count: u32,
        hist_height: Option<u64>,
    ) -> Result<Vec<(Vec<u8>, bool)>, ChainStorageError>
    {
        let mut leaf_nodes = Vec::<(Vec<u8>, bool)>::with_capacity(count as usize);
        for pos in pos..pos + count {
            leaf_nodes.push(self.fetch_mmr_node(tree, pos, hist_height)?);
        }
        Ok(leaf_nodes)
    }

    fn insert_mmr_node(&mut self, _tree: MmrTree, _hash: Hash, _deleted: bool) -> Result<(), ChainStorageError> {
        info!(target: LOG_TARGET, "Insert MMR node");
        unimplemented!();
        // match tree {
        //     MmrTree::Kernel => self.curr_kernel_checkpoint.push_addition(hash),
        //     MmrTree::Utxo => {
        //         self.curr_utxo_checkpoint.push_addition(hash);
        //         if deleted {
        //             let leaf_index = self
        //                 .curr_utxo_checkpoint
        //                 .accumulated_nodes_added_count()
        //                 .saturating_sub(1);
        //             self.curr_utxo_checkpoint.push_deletion(leaf_index);
        //         }
        //     },
        //     MmrTree::RangeProof => self.curr_range_proof_checkpoint.push_addition(hash),
        // };
        // Ok(())
    }

    fn delete_mmr_node(&mut self, _tree: MmrTree, _hash: &Hash) -> Result<(), ChainStorageError> {
        info!(target: LOG_TARGET, "Delete MMR node");
        // match tree {
        //     MmrTree::Kernel | MmrTree::RangeProof => {},
        //     MmrTree::Utxo => {
        //         if let Some(leaf_index) = self.utxo_mmr.find_leaf_index(&hash)? {
        //             self.curr_utxo_checkpoint.push_deletion(leaf_index);
        //         }
        //     },
        // };
        // Ok(())
        unimplemented!()
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError> {
        info!(target: LOG_TARGET, "Fetch MMR leaf index");
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        match tree {
            MmrTree::Utxo => {
                Ok(lmdb_get::<_, (u32, String)>(&txn, &self.txos_hash_to_index_db, hash)?.map(|(index, _)| index))
            },
            _ => unimplemented!(),
        }
    }

    /// Returns the number of blocks in the block orphan pool.
    fn get_orphan_count(&self) -> Result<usize, ChainStorageError> {
        info!(target: LOG_TARGET, "Get orphan count");
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

    /// Returns the metadata of the chain.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        // This should only be None if the database is empty
        self.mem_metadata.as_ref().cloned().ok_or_else(|| {
            ChainStorageError::AccessError("Cannot retrieve chain metadata because the database is empty".to_string())
        })
    }

    /// Returns the set of target difficulties for the specified proof of work algorithm.
    fn fetch_target_difficulties(
        &self,
        pow_algo: PowAlgorithm,
        height: u64,
        block_window: usize,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ChainStorageError>
    {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let mut target_difficulties = VecDeque::<(EpochTime, Difficulty)>::with_capacity(block_window);
        for height in (0..=height).rev() {
            let header: BlockHeader = lmdb_get(&txn, &self.headers_db, &height)?
                .ok_or_else(|| ChainStorageError::InvalidQuery("Cannot retrieve header.".into()))?;
            if header.pow.pow_algo == pow_algo {
                target_difficulties.push_front((header.timestamp, header.pow.target_difficulty));
                if target_difficulties.len() >= block_window {
                    break;
                }
            }
        }
        Ok(target_difficulties
            .into_iter()
            .collect::<Vec<(EpochTime, Difficulty)>>())
    }

    fn count_utxos(&self) -> Result<usize, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_len(&txn, &self.utxos_db)
    }

    fn count_kernels(&self) -> Result<usize, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        lmdb_len(&txn, &self.kernels_db)
    }

    fn fetch_orphan_chain_tips(&self) -> Result<Vec<HashOutput>, ChainStorageError> {
        trace!(target: LOG_TARGET, "Call to fetch_orphan_chain_tips()");
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_list_keys(&txn, &self.orphan_chain_tips_db)
    }

    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<HashOutput>, ChainStorageError> {
        trace!(
            target: LOG_TARGET,
            "Call to fetch_orphan_children_of({})",
            hash.to_hex()
        );
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;
        lmdb_get_multiple(&txn, &self.orphan_parent_map_index, hash.as_slice())
    }

    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>
    {
        let orphan_count = self.get_orphan_count()?;
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
}

// Fetch the chain metadata
fn fetch_metadata(txn: &ConstTransaction<'_>, db: &Database) -> Result<ChainMetadata, ChainStorageError> {
    Ok(ChainMetadata::new(
        fetch_chain_height(&txn, &db)?,
        fetch_best_block(&txn, &db)?,
        fetch_pruning_horizon(&txn, &db)?,
        fetch_effective_pruned_height(&txn, &db)?,
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
fn fetch_effective_pruned_height(txn: &ConstTransaction<'_>, db: &Database) -> Result<u64, ChainStorageError> {
    let k = MetadataKey::EffectivePrunedHeight;
    let val: Option<MetadataValue> = lmdb_get(&txn, &db, &(k as u32))?;
    match val {
        Some(MetadataValue::EffectivePrunedHeight(height)) => Ok(height),
        _ => Ok(0),
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

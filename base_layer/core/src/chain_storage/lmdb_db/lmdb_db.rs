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
        accumulated_data::{BlockAccumulatedData, BlockHeaderAccumulatedData},
        db_transaction::{DbKey, DbTransaction, DbValue, MetadataValue, MmrTree, WriteOperation},
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
            LMDB_DB_METADATA,
            LMDB_DB_MONERO_SEED_HEIGHT,
            LMDB_DB_ORPHANS,
            LMDB_DB_ORPHAN_CHAIN_TIPS,
            LMDB_DB_ORPHAN_HEADER_ACCUMULATED_DATA,
            LMDB_DB_ORPHAN_PARENT_MAP_INDEX,
            LMDB_DB_TXOS_HASH_TO_INDEX,
            LMDB_DB_UTXOS,
        },
        BlockchainBackend,
        ChainHeader,
        MetadataKey,
    },
    crypto::tari_utilities::hex::to_hex,
    transactions::{
        aggregated_body::AggregateBody,
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{HashDigest, HashOutput, Signature},
    },
};
use fs2::FileExt;
use lmdb_zero::{ConstTransaction, Database, Environment, ReadTransaction, WriteTransaction};
use log::*;
use std::{
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
use uint::rustc_hex::ToHex;

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
                SetMetadata(key, value) => self.set_metadata(&write_txn, key, value)?,
                InsertOrphanBlock(block) => self.insert_orphan_block(&write_txn, &block)?,
                Delete(delete) => self.op_delete(&write_txn, delete)?,
                InsertHeader { header } => {
                    let height = header.header.height;
                    if !self.insert_header(&write_txn, &header.header, header.accumulated_data)? {
                        return Err(ChainStorageError::InvalidOperation(format!(
                            "Duplicate `BlockHeader` key `{}`",
                            height
                        )));
                    }
                },
                InsertBlock { block } => {
                    // TODO: Sort out clones
                    self.insert_header(&write_txn, &block.block.header, block.accumulated_data.clone())?;
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
                    let rows = lmdb_delete_keys_starting_with::<Option<TransactionOutputRowData>>(
                        &write_txn,
                        &self.utxos_db,
                        &hash_hex,
                    )?;

                    for utxo in rows {
                        if let Some(u) = utxo {
                            trace!(target: LOG_TARGET, "Deleting UTXO `{}`", to_hex(&u.hash));
                            lmdb_delete(&write_txn, &self.txos_hash_to_index_db, u.hash.as_slice())?;
                        } else {
                            // TODO: Replace when this node is pruned
                            unimplemented!();
                        }
                    }
                    debug!(target: LOG_TARGET, "Deleting kernels...");
                    let kernels = lmdb_delete_keys_starting_with::<Option<TransactionKernelRowData>>(
                        &write_txn,
                        &self.kernels_db,
                        &hash_hex,
                    )?;
                    for kernel in kernels {
                        if let Some(k) = kernel {
                            trace!(
                                target: LOG_TARGET,
                                "Deleting excess `{}`",
                                to_hex(k.kernel.excess.as_bytes())
                            );
                            lmdb_delete(&write_txn, &self.kernel_excess_index, k.kernel.excess.as_bytes())?;
                            let mut excess_sig_key = Vec::<u8>::new();
                            excess_sig_key.extend(k.kernel.excess_sig.get_public_nonce().as_bytes());
                            excess_sig_key.extend(k.kernel.excess_sig.get_signature().as_bytes());
                            trace!(
                                target: LOG_TARGET,
                                "Deleting excess signature `{}`",
                                to_hex(&excess_sig_key)
                            );
                            lmdb_delete(&write_txn, &self.kernel_excess_sig_index, excess_sig_key.as_slice())?;
                        } else {
                            unimplemented!("This option around kernels is unnecessary and should be removed")
                        }
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
                output,
                header_hash,
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
            &*self.kernel_excess_index,
            kernel.excess.as_bytes(),
            &(header_hash.clone(), mmr_position, hash.clone()),
        )?;

        let mut excess_sig_key = Vec::<u8>::new();
        excess_sig_key.extend(kernel.excess_sig.get_public_nonce().as_bytes());
        excess_sig_key.extend(kernel.excess_sig.get_signature().as_bytes());
        lmdb_insert(
            txn,
            &*self.kernel_excess_sig_index,
            excess_sig_key.as_slice(),
            &(header_hash.clone(), mmr_position, hash.clone()),
        )?;

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
        accum_data: BlockHeaderAccumulatedData,
    ) -> Result<bool, ChainStorageError>
    {
        if let Some(current_header_at_height) = lmdb_get::<_, BlockHeader>(txn, &self.headers_db, &header.height)? {
            let hash = current_header_at_height.hash();
            if current_header_at_height.hash() != accum_data.hash {
                return Err(ChainStorageError::InvalidOperation(format!(
                    "There is a different header stored at height {} already. New header ({}), current header: ({})",
                    header.height,
                    hash.to_hex(),
                    accum_data.hash.to_hex()
                )));
            }
            return Ok(false);
        }

        lmdb_replace(&txn, &self.header_accumulated_data_db, &header.height, &accum_data)?;
        lmdb_insert(txn, &self.block_hashes_db, header.hash().as_slice(), &header.height)?;
        lmdb_insert(txn, &self.headers_db, &header.height, header)?;
        Ok(true)
    }

    fn op_delete(&mut self, txn: &WriteTransaction<'_>, key: DbKey) -> Result<(), ChainStorageError> {
        match key {
            DbKey::Metadata(_key) => unimplemented!("Deleting of metadata keys not supported"),
            DbKey::BlockHeader(k) => {
                let val: Option<BlockHeader> = lmdb_get(txn, &self.headers_db, &k)?;
                if let Some(v) = val {
                    let hash = v.hash();
                    lmdb_delete(&txn, &self.block_hashes_db, &hash)?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                    lmdb_delete(&txn, &self.header_accumulated_data_db, &k)?;
                }
            },
            DbKey::BlockHash(hash) => {
                let result: Option<u64> = lmdb_get(txn, &self.block_hashes_db, hash.as_slice())?;
                if let Some(k) = result {
                    lmdb_delete(&txn, &self.block_hashes_db, hash.as_slice())?;
                    lmdb_delete(&txn, &self.headers_db, &k)?;
                }
            },
            DbKey::OrphanBlock(k) => {
                if let Some(orphan) = lmdb_get::<_, Block>(&txn, &self.orphans_db, &k)? {
                    let parent_hash = orphan.header.prev_hash;
                    lmdb_delete_key_value(&txn, &self.orphan_parent_map_index, parent_hash.as_slice(), &k)?;
                    let tip: Option<Vec<u8>> = lmdb_get(&txn, &self.orphan_chain_tips_db, &k)?;
                    if tip.is_some() {
                        if lmdb_get::<_, Block>(&txn, &self.orphans_db, parent_hash.as_slice())?.is_some() {
                            lmdb_insert(&txn, &self.orphan_chain_tips_db, parent_hash.as_slice(), &parent_hash)?;
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
            self.fetch_block_accumulated_data(&*txn, &header.prev_hash)?
                .ok_or_else(|| ChainStorageError::ValueNotFound {
                    entity: "BlockAccumulatedData".to_string(),
                    field: "prev_hash".to_string(),
                    value: header.prev_hash.to_hex(),
                })?
        };

        let BlockAccumulatedData {
            kernels: pruned_kernel_set,
            outputs: pruned_output_set,
            deleted,
            range_proofs: pruned_proof_set,
            mut total_kernel_sum,
            mut total_utxo_sum,
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

        let mut output_mmr = MutableMmr::<HashDigest, _>::new(pruned_output_set, deleted)?;
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
            &block_hash,
            &BlockAccumulatedData::new(
                kernel_mmr.get_pruned_hash_set()?,
                output_mmr.mmr().get_pruned_hash_set()?,
                proof_mmr.get_pruned_hash_set()?,
                output_mmr.deleted().clone(),
                total_kernel_sum,
                total_utxo_sum,
            ),
        )?;

        Ok(())
    }

    #[allow(clippy::ptr_arg)]
    fn update_block_accumulated_data(
        &mut self,
        txn: &WriteTransaction<'_>,
        header_hash: &Hash,
        data: &BlockAccumulatedData,
    ) -> Result<(), ChainStorageError>
    {
        lmdb_replace(&txn, &self.block_accumulated_data_db, header_hash.as_slice(), data)
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
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        lmdb_get(&txn, &self.block_accumulated_data_db, header_hash.as_slice()).map_err(Into::into)
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
        .add_database(LMDB_DB_BLOCK_ACCUMULATED_DATA, flags)
        .add_database(LMDB_DB_BLOCK_HASHES, flags)
        .add_database(LMDB_DB_UTXOS, flags)
        .add_database(LMDB_DB_INPUTS, flags)
        .add_database(LMDB_DB_TXOS_HASH_TO_INDEX, flags)
        .add_database(LMDB_DB_KERNELS, flags)
        .add_database(LMDB_DB_KERNEL_EXCESS_INDEX, flags)
        .add_database(LMDB_DB_KERNEL_EXCESS_SIG_INDEX, flags)
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
                // TODO: investigate making BlockHash a `[u8;32]`
                if hash.len() != BLOCK_HASH_LENGTH {
                    return Err(ChainStorageError::InvalidQuery(format!(
                        "Invalid block hash length. Expected length: {} Got: {}",
                        BLOCK_HASH_LENGTH,
                        hash.len()
                    )));
                }
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
            DbKey::OrphanBlock(k) => self
                .fetch_orphan(&txn, k)?
                .map(|val| DbValue::OrphanBlock(Box::new(val))),
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

        let height: Option<u64> = lmdb_get(&txn, &self.block_hashes_db, hash.as_slice())?;

        if let Some(h) = height {
            self.fetch_header_accumulated_data_by_height(h, &txn)
        } else {
            Ok(None)
        }
    }

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        let txn = ReadTransaction::new(&*self.env).map_err(|e| ChainStorageError::AccessError(e.to_string()))?;

        let height: Option<u64> = lmdb_get(&txn, &self.block_hashes_db, hash.as_slice())?;
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
        self.fetch_block_accumulated_data(&txn, header_hash)
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
                .map(|kernel: Option<TransactionKernelRowData>| (kernel.unwrap().kernel, header_hash)))
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
                .map(|kernel: Option<TransactionKernelRowData>| (kernel.unwrap().kernel, header_hash)))
        } else {
            Ok(None)
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
        debug!(target: LOG_TARGET, "Fetch MMR node count");
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
        debug!(target: LOG_TARGET, "Fetch MMR node");
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
        debug!(target: LOG_TARGET, "Insert MMR node");
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
        debug!(target: LOG_TARGET, "Delete MMR node");
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
        trace!(target: LOG_TARGET, "Fetch MMR leaf index");
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

// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{Commitment, HashOutput, PublicKey, Signature},
};

use super::TemplateRegistrationEntry;
use crate::{
    blocks::{Block, BlockAccumulatedData, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader},
    chain_storage::{
        ChainStorageError,
        DbBasicStats,
        DbKey,
        DbTotalSizeStats,
        DbTransaction,
        DbValue,
        HorizonData,
        InputMinedInfo,
        MmrTree,
        OutputMinedInfo,
        Reorg,
    },
    transactions::transaction_components::{TransactionInput, TransactionKernel, TransactionOutput},
    OutputSmt,
};

/// Identify behaviour for Blockchain database backends. Implementations must support `Send` and `Sync` so that
/// `BlockchainDatabase` can be thread-safe. The backend *must* also execute transactions atomically; i.e., every
/// operation within it must succeed, or they all fail. Failure to support this contract could lead to
/// synchronisation issues in your database backend.
///
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. This strategy allows
/// us to keep the reading and writing API extremely simple. Extending the types of data that the backends can handle
/// will entail adding to those enums, and the backends, while this trait can remain unchanged.
#[allow(clippy::ptr_arg)]
pub trait BlockchainBackend: Send + Sync {
    /// Commit the transaction given to the backend. If there is an error, the transaction must be rolled back, and
    /// the error condition returned. On success, every operation in the transaction will have been committed, and
    /// the function will return `Ok(())`.
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError>;
    /// Fetch a value from the backend corresponding to the given key. If the value is not found, `get` must return
    /// `Ok(None)`. It should only error if there is an access or integrity issue with the underlying backend.
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError>;
    /// Checks to see whether the given key exists in the backend. This function should only fail if there is an
    /// access or integrity issue with the backend.
    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError>;

    /// Fetches data that is calculated and accumulated for blocks that have been
    /// added to a chain of headers
    fn fetch_chain_header_by_height(&self, height: u64) -> Result<ChainHeader, ChainStorageError>;

    /// Fetches data that is calculated and accumulated for blocks that have been
    /// added to a chain of headers
    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>;

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError>;

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError>;

    /// Used to determine if the database is empty, i.e. a brand new database.
    /// This is called to decide if the genesis block should be created.
    fn is_empty(&self) -> Result<bool, ChainStorageError>;

    /// Fetch accumulated data like MMR peaks and deleted hashmap
    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>;

    fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>;

    /// Fetch all the kernels in a block
    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError>;

    /// Fetch a kernel with this excess signature  and returns a `TransactionKernel` and the hash of the block that it
    /// is in
    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>;

    /// Fetch all UTXOs and spends in the block
    fn fetch_outputs_in_block_with_spend_state(
        &self,
        header_hash: &HashOutput,
        spend_status_at_header: Option<HashOutput>,
    ) -> Result<Vec<(TransactionOutput, bool)>, ChainStorageError>;

    /// Fetch a specific output. Returns the output
    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<OutputMinedInfo>, ChainStorageError>;

    /// Fetch a specific input. Returns the input
    fn fetch_input(&self, output_hash: &HashOutput) -> Result<Option<InputMinedInfo>, ChainStorageError>;

    /// Returns the unspent TransactionOutput output that matches the given commitment if it exists in the current UTXO
    /// set, otherwise None is returned.
    fn fetch_unspent_output_hash_by_commitment(
        &self,
        commitment: &Commitment,
    ) -> Result<Option<HashOutput>, ChainStorageError>;

    /// Fetch all outputs in a block
    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError>;

    /// Fetch all inputs in a block
    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError>;

    /// Fetches the total merkle mountain range node count upto the specified height.
    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError>;

    /// Returns the number of blocks in the block orphan pool.
    fn orphan_count(&self) -> Result<usize, ChainStorageError>;
    /// Returns the stored header with the highest corresponding height.
    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError>;

    /// Clear all headers that are beyond the current height of longest chain, returning the number of headers that were
    /// deleted.
    fn clear_all_pending_headers(&self) -> Result<usize, ChainStorageError>;
    /// Returns the stored header and accumulated data with the highest height.
    fn fetch_last_chain_header(&self) -> Result<ChainHeader, ChainStorageError>;
    /// Returns the stored header with the highest corresponding height.
    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError>;
    /// Returns the stored chain metadata.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError>;
    /// Returns the UTXO count
    fn utxo_count(&self) -> Result<usize, ChainStorageError>;
    /// Returns the kernel count
    fn kernel_count(&self) -> Result<usize, ChainStorageError>;

    /// Fetches an current tip orphan by hash or returns None if the orphan is not found or is not a tip of any
    /// alternate chain
    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError>;
    /// Fetches strongest currently stored orphan tips, if none are stored, returns an empty vec.
    fn fetch_strongest_orphan_chain_tips(&self) -> Result<Vec<ChainHeader>, ChainStorageError>;
    /// Fetch all orphans that have `hash` as a previous hash
    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError>;

    fn fetch_orphan_chain_block(&self, hash: HashOutput) -> Result<Option<ChainBlock>, ChainStorageError>;

    /// Delete orphans according to age. Used to keep the orphan pool at a certain capacity
    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>;

    /// This gets the monero seed_height. This will return 0, if the seed is unkown
    fn fetch_monero_seed_first_seen_height(&self, seed: &[u8]) -> Result<u64, ChainStorageError>;

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError>;

    /// Returns basic database stats for each internal database, such as number of entries and page sizes. This call may
    /// not apply to every database implementation.
    fn get_stats(&self) -> Result<DbBasicStats, ChainStorageError>;
    /// Returns total size information about each internal database. This call may be very slow and will obtain a read
    /// lock for the duration.
    fn fetch_total_size_stats(&self) -> Result<DbTotalSizeStats, ChainStorageError>;

    /// Check if a block hash is in the bad block list
    fn bad_block_exists(&self, block_hash: HashOutput) -> Result<(bool, String), ChainStorageError>;

    /// Fetches all tracked reorgs
    fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError>;

    /// Fetches the validator node set for the given height ordered according to height of registration and canonical
    /// block body ordering.
    fn fetch_active_validator_nodes(&self, height: u64) -> Result<Vec<(PublicKey, [u8; 32])>, ChainStorageError>;
    /// Returns the shard key for the validator node if valid at the given height.
    fn get_shard_key(&self, height: u64, public_key: PublicKey) -> Result<Option<[u8; 32]>, ChainStorageError>;
    /// Returns all template registrations within (inclusive) the given height range.
    fn fetch_template_registrations(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<TemplateRegistrationEntry>, ChainStorageError>;
    /// Returns the tip utxo smt
    fn fetch_tip_smt(&self) -> Result<OutputSmt, ChainStorageError>;
}

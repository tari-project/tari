use crate::{
    blocks::{Block, BlockHeader},
    chain_storage::{
        pruned_output::PrunedOutput,
        BlockAccumulatedData,
        BlockHeaderAccumulatedData,
        ChainHeader,
        ChainStorageError,
        DbKey,
        DbTransaction,
        DbValue,
        HorizonData,
        MmrTree,
    },
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::Signature,
    },
};
use croaring::Bitmap;
use tari_common_types::{
    chain_metadata::{ChainMetadata, ReorgInfo},
    types::HashOutput,
};
use tari_mmr::Hash;

/// Identify behaviour for Blockchain database back ends. Implementations must support `Send` and `Sync` so that
/// `BlockchainDatabase` can be thread-safe. The backend *must* also execute transactions atomically; i.e., every
/// operation within it must succeed, or they all fail. Failure to support this contract could lead to
/// synchronisation issues in your database backend.
///
/// Data is passed to and from the backend via the [DbKey], [DbValue], and [DbValueKey] enums. This strategy allows
/// us to keep the reading and writing API extremely simple. Extending the types of data that the back ends can handle
/// will entail adding to those enums, and the back ends, while this trait can remain unchanged.
#[allow(clippy::ptr_arg)]
pub trait BlockchainBackend: Send + Sync {
    /// Commit the transaction given to the backend. If there is an error, the transaction must be rolled back, and
    /// the error condition returned. On success, every operation in the transaction will have been committed, and
    /// the function will return `Ok(())`.
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError>;
    /// Fetch a value from the back end corresponding to the given key. If the value is not found, `get` must return
    /// `Ok(None)`. It should only error if there is an access or integrity issue with the underlying back end.
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError>;
    /// Checks to see whether the given key exists in the back end. This function should only fail if there is an
    /// access or integrity issue with the back end.
    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError>;

    /// Fetches data that is calculated and accumulated for blocks that have been
    /// added to a chain of headers
    fn fetch_header_and_accumulated_data(
        &self,
        height: u64,
    ) -> Result<(BlockHeader, BlockHeaderAccumulatedData), ChainStorageError>;

    /// Fetches data that is calculated and accumulated for blocks that have been
    /// added to a chain of headers
    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>;

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError>;

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError>;

    fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError>;

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

    /// Fetch a kernel with this excess and returns a `TransactionKernel` and the hash of the block that it is in
    fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>;

    /// Fetch a kernel with this excess signature  and returns a `TransactionKernel` and the hash of the block that it
    /// is in
    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>;

    /// Fetch kernels by MMR position
    fn fetch_kernels_by_mmr_position(&self, start: u64, end: u64) -> Result<Vec<TransactionKernel>, ChainStorageError>;

    fn fetch_utxos_by_mmr_position(
        &self,
        start: u64,
        end: u64,
        deleted: &Bitmap,
    ) -> Result<(Vec<PrunedOutput>, Vec<Bitmap>), ChainStorageError>;

    /// Fetch a specific output. Returns the output and the leaf index in the output MMR
    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<(TransactionOutput, u32)>, ChainStorageError>;

    /// Fetch all outputs in a block
    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<PrunedOutput>, ChainStorageError>;

    /// Fetch all inputs in a block
    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError>;

    /// Fetches the total merkle mountain range node count upto the specified height.
    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError>;

    /// Fetches the leaf index of the provided leaf node hash in the given MMR tree.
    #[allow(clippy::ptr_arg)]
    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &Hash) -> Result<Option<u32>, ChainStorageError>;
    /// Returns the number of blocks in the block orphan pool.
    fn orphan_count(&self) -> Result<usize, ChainStorageError>;
    /// Returns the stored header with the highest corresponding height.
    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError>;
    /// Returns the stored header with the highest corresponding height.
    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError>;
    /// Returns the stored chain metadata.
    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError>;
    /// Returns the reorg info.
    fn fetch_reorg_info(&self) -> Result<Option<ReorgInfo>, ChainStorageError>;
    /// Returns the UTXO count
    fn utxo_count(&self) -> Result<usize, ChainStorageError>;
    /// Returns the kernel count
    fn kernel_count(&self) -> Result<usize, ChainStorageError>;

    /// Fetches all of the orphans (hash) that are currently at the tip of an alternate chain
    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError>;
    /// Fetch all orphans that have `hash` as a previous hash
    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError>;

    fn fetch_orphan_header_accumulated_data(
        &self,
        hash: HashOutput,
    ) -> Result<BlockHeaderAccumulatedData, ChainStorageError>;

    /// Delete orphans according to age. Used to keep the orphan pool at a certain capacity
    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>;

    /// This gets the monero seed_height. This will return 0, if the seed is unkown
    fn fetch_monero_seed_first_seen_height(&self, seed: &str) -> Result<u64, ChainStorageError>;

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError>;
}

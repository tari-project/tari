//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    blocks::{genesis_block::get_weatherwax_genesis_block, Block, BlockHeader},
    chain_storage::{
        create_lmdb_database,
        BlockAccumulatedData,
        BlockHeaderAccumulatedData,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainHeader,
        ChainStorageError,
        DbKey,
        DbTransaction,
        DbValue,
        HorizonData,
        LMDBDatabase,
        MmrTree,
        PrunedOutput,
        Validators,
    },
    consensus::{
        chain_strength_comparer::ChainStrengthComparerBuilder,
        ConsensusConstantsBuilder,
        ConsensusManager,
        ConsensusManagerBuilder,
        Network,
    },
    transactions::{
        transaction::{TransactionInput, TransactionKernel, TransactionOutput},
        types::{CryptoFactories, HashOutput, Signature},
    },
    validation::{
        block_validators::{BodyOnlyMinusHeightValidator, OrphanBlockValidator},
        mocks::MockValidator,
    },
};
use croaring::Bitmap;
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;

/// Create a new blockchain database containing no blocks.
pub fn create_new_blockchain() -> BlockchainDatabase<TempDatabase> {
    let network = Network::Weatherwax;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let genesis = get_weatherwax_genesis_block();
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(genesis)
        .on_ties(ChainStrengthComparerBuilder::new().by_height().build())
        .build();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    create_store_with_consensus_and_validators(&consensus_manager, validators)
}

pub fn create_store_with_consensus_and_validators(
    rules: &ConsensusManager,
    validators: Validators<TempDatabase>,
) -> BlockchainDatabase<TempDatabase>
{
    create_store_with_consensus_and_validators_and_config(&rules, validators, BlockchainDatabaseConfig::default())
}

pub fn create_store_with_consensus_and_validators_and_config(
    rules: &ConsensusManager,
    validators: Validators<TempDatabase>,
    config: BlockchainDatabaseConfig,
) -> BlockchainDatabase<TempDatabase>
{
    let backend = create_test_db();
    BlockchainDatabase::new(backend, &rules, validators, config, false).unwrap()
}

pub fn create_store_with_consensus(rules: &ConsensusManager) -> BlockchainDatabase<TempDatabase> {
    let factories = CryptoFactories::default();
    let validators = Validators::new(
        BodyOnlyMinusHeightValidator::default(),
        MockValidator::new(true),
        OrphanBlockValidator::new(rules.clone(), factories),
    );
    create_store_with_consensus_and_validators(rules, validators)
}
pub fn create_test_blockchain_db() -> BlockchainDatabase<TempDatabase> {
    let network = Network::Weatherwax;
    let rules = ConsensusManagerBuilder::new(network).build();
    create_store_with_consensus(&rules)
}

pub fn create_test_db() -> TempDatabase {
    TempDatabase::new()
}

pub struct TempDatabase {
    path: PathBuf,
    db: LMDBDatabase,
}

impl TempDatabase {
    fn new() -> Self {
        let temp_path = create_temporary_data_path();

        Self {
            db: create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap(),
            path: temp_path,
        }
    }
}

impl Deref for TempDatabase {
    type Target = LMDBDatabase;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        if Path::new(&self.path).exists() {
            if let Err(e) = fs::remove_dir_all(&self.path) {
                println!("\n{:?}\n", e);
            }
        }
    }
}

impl BlockchainBackend for TempDatabase {
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.write(tx)
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        self.db.fetch(key)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        self.db.contains(key)
    }

    fn fetch_header_and_accumulated_data(
        &self,
        height: u64,
    ) -> Result<(BlockHeader, BlockHeaderAccumulatedData), ChainStorageError>
    {
        self.db.fetch_header_and_accumulated_data(height)
    }

    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError>
    {
        self.db.fetch_header_accumulated_data(hash)
    }

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        self.db.fetch_chain_header_in_all_chains(hash)
    }

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        self.db.fetch_header_containing_kernel_mmr(mmr_position)
    }

    fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        self.db.fetch_header_containing_utxo_mmr(mmr_position)
    }

    fn is_empty(&self) -> Result<bool, ChainStorageError> {
        self.db.is_empty()
    }

    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        self.db.fetch_block_accumulated_data(header_hash)
    }

    fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError>
    {
        self.db.fetch_block_accumulated_data_by_height(height)
    }

    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        self.db.fetch_kernels_in_block(header_hash)
    }

    fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        self.db.fetch_kernel_by_excess(excess)
    }

    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError>
    {
        self.db.fetch_kernel_by_excess_sig(excess_sig)
    }

    fn fetch_kernels_by_mmr_position(&self, start: u64, end: u64) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        self.db.fetch_kernels_by_mmr_position(start, end)
    }

    fn fetch_utxos_by_mmr_position(
        &self,
        start: u64,
        end: u64,
        deleted: &Bitmap,
    ) -> Result<(Vec<PrunedOutput>, Vec<Bitmap>), ChainStorageError>
    {
        self.db.fetch_utxos_by_mmr_position(start, end, deleted)
    }

    fn fetch_output(
        &self,
        output_hash: &HashOutput,
    ) -> Result<Option<(TransactionOutput, u32, u64)>, ChainStorageError>
    {
        self.db.fetch_output(output_hash)
    }

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<PrunedOutput>, ChainStorageError> {
        self.db.fetch_outputs_in_block(header_hash)
    }

    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        self.db.fetch_inputs_in_block(header_hash)
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        self.db.fetch_mmr_size(tree)
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &HashOutput) -> Result<Option<u32>, ChainStorageError> {
        self.db.fetch_mmr_leaf_index(tree, hash)
    }

    fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        self.db.orphan_count()
    }

    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        self.db.fetch_last_header()
    }

    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        self.db.fetch_tip_header()
    }

    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        self.db.fetch_chain_metadata()
    }

    fn utxo_count(&self) -> Result<usize, ChainStorageError> {
        self.db.utxo_count()
    }

    fn kernel_count(&self) -> Result<usize, ChainStorageError> {
        self.db.kernel_count()
    }

    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        self.db.fetch_orphan_chain_tip_by_hash(hash)
    }

    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
        self.db.fetch_orphan_children_of(hash)
    }

    fn fetch_orphan_header_accumulated_data(
        &self,
        hash: HashOutput,
    ) -> Result<BlockHeaderAccumulatedData, ChainStorageError>
    {
        self.db.fetch_orphan_header_accumulated_data(hash)
    }

    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError>
    {
        self.db.delete_oldest_orphans(horizon_height, orphan_storage_capacity)
    }

    fn fetch_monero_seed_first_seen_height(&self, seed: &str) -> Result<u64, ChainStorageError> {
        self.db.fetch_monero_seed_first_seen_height(seed)
    }

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
        self.db.fetch_horizon_data()
    }
}

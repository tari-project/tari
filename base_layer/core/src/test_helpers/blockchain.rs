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

use std::{
    collections::HashMap,
    fs,
    ops::{Deref, Range},
    path::{Path, PathBuf},
    sync::Arc,
};

use croaring::Bitmap;
use tari_common::configuration::Network;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{Commitment, HashOutput, PublicKey, Signature},
};
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;
use tari_utilities::Hashable;

use super::{create_block, mine_to_difficulty};
use crate::{
    blocks::{
        genesis_block::get_genesis_block,
        Block,
        BlockAccumulatedData,
        BlockHeader,
        BlockHeaderAccumulatedData,
        ChainBlock,
        ChainHeader,
        DeletedBitmap,
    },
    chain_storage::{
        create_lmdb_database,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainStorageError,
        DbBasicStats,
        DbKey,
        DbTotalSizeStats,
        DbTransaction,
        DbValue,
        HorizonData,
        LMDBDatabase,
        MmrTree,
        PrunedOutput,
        Reorg,
        UtxoMinedInfo,
        Validators,
    },
    consensus::{chain_strength_comparer::ChainStrengthComparerBuilder, ConsensusConstantsBuilder, ConsensusManager},
    proof_of_work::{AchievedTargetDifficulty, Difficulty, PowAlgorithm},
    test_helpers::{block_spec::BlockSpecs, create_consensus_rules, BlockSpec},
    transactions::{
        transaction_components::{TransactionInput, TransactionKernel, UnblindedOutput},
        CryptoFactories,
    },
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        mocks::MockValidator,
        DifficultyCalculator,
    },
};

/// Create a new blockchain database containing the genesis block
pub fn create_new_blockchain() -> BlockchainDatabase<TempDatabase> {
    create_new_blockchain_with_network(Network::LocalNet)
}

pub fn create_new_blockchain_with_network(network: Network) -> BlockchainDatabase<TempDatabase> {
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let genesis = get_genesis_block(network);
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis)
        .on_ties(ChainStrengthComparerBuilder::new().by_height().build())
        .build();
    create_custom_blockchain(consensus_manager)
}

/// Create a new custom blockchain database containing no blocks.
pub fn create_custom_blockchain(rules: ConsensusManager) -> BlockchainDatabase<TempDatabase> {
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    create_store_with_consensus_and_validators(rules, validators)
}

pub fn create_store_with_consensus_and_validators(
    rules: ConsensusManager,
    validators: Validators<TempDatabase>,
) -> BlockchainDatabase<TempDatabase> {
    create_store_with_consensus_and_validators_and_config(rules, validators, BlockchainDatabaseConfig::default())
}

pub fn create_store_with_consensus_and_validators_and_config(
    rules: ConsensusManager,
    validators: Validators<TempDatabase>,
    config: BlockchainDatabaseConfig,
) -> BlockchainDatabase<TempDatabase> {
    let backend = create_test_db();
    BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        config,
        DifficultyCalculator::new(rules, Default::default()),
    )
    .unwrap()
}

pub fn create_store_with_consensus(rules: ConsensusManager) -> BlockchainDatabase<TempDatabase> {
    let factories = CryptoFactories::default();
    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        MockValidator::new(true),
        OrphanBlockValidator::new(rules.clone(), false, factories),
    );
    create_store_with_consensus_and_validators(rules, validators)
}
pub fn create_test_blockchain_db() -> BlockchainDatabase<TempDatabase> {
    let rules = create_consensus_rules();
    create_store_with_consensus(rules)
}

pub fn create_test_db() -> TempDatabase {
    TempDatabase::new()
}

pub struct TempDatabase {
    path: PathBuf,
    db: Option<LMDBDatabase>,
    delete_on_drop: bool,
}

impl TempDatabase {
    pub fn new() -> Self {
        let temp_path = create_temporary_data_path();

        Self {
            db: Some(create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap()),
            path: temp_path,
            delete_on_drop: true,
        }
    }

    pub fn from_path<P: AsRef<Path>>(temp_path: P) -> Self {
        Self {
            db: Some(create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap()),
            path: temp_path.as_ref().to_path_buf(),
            delete_on_drop: true,
        }
    }

    pub fn disable_delete_on_drop(&mut self) -> &mut Self {
        self.delete_on_drop = false;
        self
    }
}

impl Default for TempDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for TempDatabase {
    type Target = LMDBDatabase;

    fn deref(&self) -> &Self::Target {
        self.db.as_ref().unwrap()
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        // force a drop on the LMDB db
        self.db = None;
        if self.delete_on_drop && Path::new(&self.path).exists() {
            fs::remove_dir_all(&self.path).expect("Could not delete temporary file");
        }
    }
}

impl BlockchainBackend for TempDatabase {
    fn write(&mut self, tx: DbTransaction) -> Result<(), ChainStorageError> {
        self.db.as_mut().unwrap().write(tx)
    }

    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch(key)
    }

    fn contains(&self, key: &DbKey) -> Result<bool, ChainStorageError> {
        self.db.as_ref().unwrap().contains(key)
    }

    fn fetch_chain_header_by_height(&self, height: u64) -> Result<ChainHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_chain_header_by_height(height)
    }

    fn fetch_header_accumulated_data(
        &self,
        hash: &HashOutput,
    ) -> Result<Option<BlockHeaderAccumulatedData>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_header_accumulated_data(hash)
    }

    fn fetch_chain_header_in_all_chains(&self, hash: &HashOutput) -> Result<ChainHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_chain_header_in_all_chains(hash)
    }

    fn fetch_header_containing_kernel_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_header_containing_kernel_mmr(mmr_position)
    }

    fn fetch_header_containing_utxo_mmr(&self, mmr_position: u64) -> Result<ChainHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_header_containing_utxo_mmr(mmr_position)
    }

    fn is_empty(&self) -> Result<bool, ChainStorageError> {
        self.db.as_ref().unwrap().is_empty()
    }

    fn fetch_block_accumulated_data(
        &self,
        header_hash: &HashOutput,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_block_accumulated_data(header_hash)
    }

    fn fetch_block_accumulated_data_by_height(
        &self,
        height: u64,
    ) -> Result<Option<BlockAccumulatedData>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_block_accumulated_data_by_height(height)
    }

    fn fetch_kernels_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionKernel>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_kernels_in_block(header_hash)
    }

    fn fetch_kernel_by_excess(
        &self,
        excess: &[u8],
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_kernel_by_excess(excess)
    }

    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_kernel_by_excess_sig(excess_sig)
    }

    fn fetch_utxos_in_block(
        &self,
        header_hash: &HashOutput,
        deleted: Option<&Bitmap>,
    ) -> Result<(Vec<PrunedOutput>, Bitmap), ChainStorageError> {
        self.db.as_ref().unwrap().fetch_utxos_in_block(header_hash, deleted)
    }

    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<UtxoMinedInfo>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_output(output_hash)
    }

    fn fetch_unspent_output_hash_by_commitment(
        &self,
        commitment: &Commitment,
    ) -> Result<Option<HashOutput>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_unspent_output_hash_by_commitment(commitment)
    }

    fn fetch_utxo_by_unique_id(
        &self,
        parent_public_key: Option<&PublicKey>,
        unique_id: &[u8],
        deleted_at: Option<u64>,
    ) -> Result<Option<UtxoMinedInfo>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_utxo_by_unique_id(parent_public_key, unique_id, deleted_at)
    }

    fn fetch_all_unspent_by_parent_public_key(
        &self,
        parent_public_key: &PublicKey,
        range: Range<usize>,
    ) -> Result<Vec<UtxoMinedInfo>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_all_unspent_by_parent_public_key(parent_public_key, range)
    }

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<PrunedOutput>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_outputs_in_block(header_hash)
    }

    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_inputs_in_block(header_hash)
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_mmr_size(tree)
    }

    fn fetch_mmr_leaf_index(&self, tree: MmrTree, hash: &HashOutput) -> Result<Option<u32>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_mmr_leaf_index(tree, hash)
    }

    fn orphan_count(&self) -> Result<usize, ChainStorageError> {
        self.db.as_ref().unwrap().orphan_count()
    }

    fn fetch_last_header(&self) -> Result<BlockHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_last_header()
    }

    fn clear_all_pending_headers(&self) -> Result<usize, ChainStorageError> {
        self.db.as_ref().unwrap().clear_all_pending_headers()
    }

    fn fetch_last_chain_header(&self) -> Result<ChainHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_last_chain_header()
    }

    fn fetch_tip_header(&self) -> Result<ChainHeader, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_tip_header()
    }

    fn fetch_chain_metadata(&self) -> Result<ChainMetadata, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_chain_metadata()
    }

    fn utxo_count(&self) -> Result<usize, ChainStorageError> {
        self.db.as_ref().unwrap().utxo_count()
    }

    fn kernel_count(&self) -> Result<usize, ChainStorageError> {
        self.db.as_ref().unwrap().kernel_count()
    }

    fn fetch_orphan_chain_tip_by_hash(&self, hash: &HashOutput) -> Result<Option<ChainHeader>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_orphan_chain_tip_by_hash(hash)
    }

    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_orphan_children_of(hash)
    }

    fn fetch_orphan_chain_block(&self, hash: HashOutput) -> Result<Option<ChainBlock>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_orphan_chain_block(hash)
    }

    fn fetch_deleted_bitmap(&self) -> Result<DeletedBitmap, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_deleted_bitmap()
    }

    fn delete_oldest_orphans(
        &mut self,
        horizon_height: u64,
        orphan_storage_capacity: usize,
    ) -> Result<(), ChainStorageError> {
        self.db
            .as_mut()
            .unwrap()
            .delete_oldest_orphans(horizon_height, orphan_storage_capacity)
    }

    fn fetch_monero_seed_first_seen_height(&self, seed: &[u8]) -> Result<u64, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_monero_seed_first_seen_height(seed)
    }

    fn fetch_horizon_data(&self) -> Result<Option<HorizonData>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_horizon_data()
    }

    fn get_stats(&self) -> Result<DbBasicStats, ChainStorageError> {
        self.db.as_ref().unwrap().get_stats()
    }

    fn fetch_total_size_stats(&self) -> Result<DbTotalSizeStats, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_total_size_stats()
    }

    fn fetch_header_hash_by_deleted_mmr_positions(
        &self,
        mmr_positions: Vec<u32>,
    ) -> Result<Vec<Option<(u64, HashOutput)>>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_header_hash_by_deleted_mmr_positions(mmr_positions)
    }

    fn bad_block_exists(&self, block_hash: HashOutput) -> Result<bool, ChainStorageError> {
        self.db.as_ref().unwrap().bad_block_exists(block_hash)
    }

    fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_all_reorgs()
    }
}

pub fn create_chained_blocks<T: Into<BlockSpecs>>(
    blocks: T,
    genesis_block: Arc<ChainBlock>,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let mut block_hashes = HashMap::new();
    block_hashes.insert("GB".to_string(), genesis_block);
    let rules = ConsensusManager::builder(Network::LocalNet).build();
    let blocks: BlockSpecs = blocks.into();
    let mut block_names = Vec::with_capacity(blocks.len());
    for block_spec in blocks {
        let prev_block = block_hashes
            .get(block_spec.prev_block)
            .unwrap_or_else(|| panic!("Could not find block {}", block_spec.prev_block));
        let name = block_spec.name;
        let difficulty = block_spec.difficulty;
        let (block, _) = create_block(&rules, prev_block.block(), block_spec);
        let block = mine_block(block, prev_block.accumulated_data(), difficulty);
        block_names.push(name.to_string());
        block_hashes.insert(name.to_string(), block);
    }
    (block_names, block_hashes)
}

fn mine_block(block: Block, prev_block_accum: &BlockHeaderAccumulatedData, difficulty: Difficulty) -> Arc<ChainBlock> {
    let block = mine_to_difficulty(block, difficulty).unwrap();
    let accum = BlockHeaderAccumulatedData::builder(prev_block_accum)
        .with_hash(block.hash())
        .with_achieved_target_difficulty(
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3, (difficulty.as_u64() - 1).into(), difficulty)
                .unwrap(),
        )
        .with_total_kernel_offset(block.header.total_kernel_offset.clone())
        .build()
        .unwrap();
    Arc::new(ChainBlock::try_construct(Arc::new(block), accum).unwrap())
}

pub fn create_main_chain<T: Into<BlockSpecs>>(
    db: &BlockchainDatabase<TempDatabase>,
    blocks: T,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let genesis_block = db.fetch_block(0).unwrap().try_into_chain_block().map(Arc::new).unwrap();
    let (names, chain) = create_chained_blocks(blocks, genesis_block);
    names.iter().for_each(|name| {
        let block = chain.get(name).unwrap();
        db.add_block(block.to_arc_block()).unwrap();
    });

    (names, chain)
}

pub fn create_orphan_chain<T: Into<BlockSpecs>>(
    db: &BlockchainDatabase<TempDatabase>,
    blocks: T,
    root_block: Arc<ChainBlock>,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let (names, chain) = create_chained_blocks(blocks, root_block);
    let mut txn = DbTransaction::new();
    for name in &names {
        let block = chain.get(name).unwrap().clone();
        txn.insert_chained_orphan(block);
    }
    db.write(txn).unwrap();

    (names, chain)
}

pub struct TestBlockchain {
    db: BlockchainDatabase<TempDatabase>,
    chain: Vec<(&'static str, Arc<ChainBlock>)>,
    rules: ConsensusManager,
}

impl TestBlockchain {
    pub fn new(db: BlockchainDatabase<TempDatabase>, rules: ConsensusManager) -> Self {
        let genesis = db.fetch_block(0).unwrap().try_into_chain_block().map(Arc::new).unwrap();
        let mut blockchain = Self {
            db,
            chain: Default::default(),
            rules,
        };

        blockchain.chain.push(("GB", genesis));
        blockchain
    }

    pub fn create(rules: ConsensusManager) -> Self {
        Self::new(create_custom_blockchain(rules.clone()), rules)
    }

    pub fn rules(&self) -> &ConsensusManager {
        &self.rules
    }

    pub fn db(&self) -> &BlockchainDatabase<TempDatabase> {
        &self.db
    }

    pub fn add_block(
        &mut self,
        name: &'static str,
        child_of: &'static str,
        block_spec: BlockSpec,
    ) -> (Arc<ChainBlock>, UnblindedOutput) {
        let (block, coinbase) = self.create_chained_block(child_of, block_spec);
        self.append_block(name, block.clone());
        (block, coinbase)
    }

    pub fn add_next_tip(&mut self, name: &'static str, spec: BlockSpec) -> (Arc<ChainBlock>, UnblindedOutput) {
        let (block, coinbase) = self.create_next_tip(spec);
        self.append_block(name, block.clone());
        (block, coinbase)
    }

    pub fn append_block(&mut self, name: &'static str, block: Arc<ChainBlock>) {
        let result = self.db.add_block(block.to_arc_block()).unwrap();
        assert!(result.is_added());
        let _ = self.chain.push((name, block));
    }

    pub fn get_block_by_name(&self, name: &'static str) -> Option<Arc<ChainBlock>> {
        self.chain.iter().find(|(n, _)| *n == name).map(|(_, ch)| ch.clone())
    }

    pub fn get_tip_block(&self) -> (&'static str, Arc<ChainBlock>) {
        self.chain.last().cloned().unwrap()
    }

    pub fn create_chained_block(
        &self,
        parent_name: &'static str,
        block_spec: BlockSpec,
    ) -> (Arc<ChainBlock>, UnblindedOutput) {
        let parent = self.get_block_by_name(parent_name).unwrap();
        let difficulty = block_spec.difficulty;
        let (block, coinbase) = create_block(&self.rules, parent.block(), block_spec);
        let block = mine_block(block, parent.accumulated_data(), difficulty);
        (block, coinbase)
    }

    pub fn create_unmined_block(&self, parent_name: &'static str, block_spec: BlockSpec) -> (Block, UnblindedOutput) {
        let parent = self.get_block_by_name(parent_name).unwrap();
        create_block(&self.rules, parent.block(), block_spec)
    }

    pub fn mine_block(&self, parent_name: &'static str, block: Block, difficulty: Difficulty) -> Arc<ChainBlock> {
        let parent = self.get_block_by_name(parent_name).unwrap();
        mine_block(block, parent.accumulated_data(), difficulty)
    }

    pub fn create_next_tip(&self, spec: BlockSpec) -> (Arc<ChainBlock>, UnblindedOutput) {
        let (name, _) = self.get_tip_block();
        self.create_chained_block(name, spec)
    }

    pub fn get_genesis_block(&self) -> Arc<ChainBlock> {
        self.chain.first().map(|(_, block)| block).unwrap().clone()
    }
}

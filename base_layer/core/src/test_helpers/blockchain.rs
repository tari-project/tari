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
    convert::TryFrom,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use tari_common::configuration::Network;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    tari_address::TariAddress,
    types::{Commitment, FixedHash, HashOutput, PublicKey, Signature},
};
use tari_mmr::sparse_merkle_tree::{NodeKey, ValueHash};
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;
use tari_utilities::ByteArray;

use super::{create_block, mine_to_difficulty};
use crate::{
    blocks::{Block, BlockAccumulatedData, BlockHeader, BlockHeaderAccumulatedData, ChainBlock, ChainHeader},
    chain_storage::{
        create_lmdb_database,
        BlockAddResult,
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
        InputMinedInfo,
        LMDBDatabase,
        MmrTree,
        OutputMinedInfo,
        Reorg,
        TemplateRegistrationEntry,
        Validators,
    },
    consensus::{chain_strength_comparer::ChainStrengthComparerBuilder, ConsensusConstantsBuilder, ConsensusManager},
    proof_of_work::{AchievedTargetDifficulty, Difficulty, PowAlgorithm},
    test_helpers::{block_spec::BlockSpecs, create_consensus_rules, default_coinbase_entities, BlockSpec},
    transactions::{
        key_manager::{create_memory_db_key_manager, MemoryDbKeyManager, TariKeyId},
        transaction_components::{
            RangeProofType,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
            WalletOutput,
        },
        CryptoFactories,
    },
    validation::{
        block_body::{BlockBodyFullValidator, BlockBodyInternalConsistencyValidator},
        mocks::MockValidator,
        DifficultyCalculator,
    },
    OutputSmt,
};

/// Create a new blockchain database containing the genesis block
pub fn create_new_blockchain() -> BlockchainDatabase<TempDatabase> {
    create_new_blockchain_with_network(Network::LocalNet)
}

pub fn create_new_blockchain_with_network(network: Network) -> BlockchainDatabase<TempDatabase> {
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let consensus_manager = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .on_ties(ChainStrengthComparerBuilder::new().by_height().build())
        .build()
        .unwrap();
    create_custom_blockchain(consensus_manager)
}

/// Create a new custom blockchain database containing no blocks.
pub fn create_custom_blockchain(rules: ConsensusManager) -> BlockchainDatabase<TempDatabase> {
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let smt = Arc::new(RwLock::new(OutputSmt::new()));
    create_store_with_consensus_and_validators(rules, validators, smt)
}

pub fn create_store_with_consensus_and_validators(
    rules: ConsensusManager,
    validators: Validators<TempDatabase>,
    smt: Arc<RwLock<OutputSmt>>,
) -> BlockchainDatabase<TempDatabase> {
    create_store_with_consensus_and_validators_and_config(rules, validators, BlockchainDatabaseConfig::default(), smt)
}

pub fn create_store_with_consensus_and_validators_and_config(
    rules: ConsensusManager,
    validators: Validators<TempDatabase>,
    config: BlockchainDatabaseConfig,
    smt: Arc<RwLock<OutputSmt>>,
) -> BlockchainDatabase<TempDatabase> {
    let backend = create_test_db();
    BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        config,
        DifficultyCalculator::new(rules, Default::default()),
        smt,
    )
    .unwrap()
}

pub fn create_store_with_consensus(rules: ConsensusManager) -> BlockchainDatabase<TempDatabase> {
    let factories = CryptoFactories::default();
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        MockValidator::new(true),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories),
    );
    let smt = Arc::new(RwLock::new(OutputSmt::new()));
    create_store_with_consensus_and_validators(rules, validators, smt)
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
        let rules = create_consensus_rules();

        Self {
            db: Some(create_lmdb_database(&temp_path, LMDBConfig::default(), rules).unwrap()),
            path: temp_path,
            delete_on_drop: true,
        }
    }

    pub fn from_path<P: AsRef<Path>>(temp_path: P) -> Self {
        let rules = create_consensus_rules();
        Self {
            db: Some(create_lmdb_database(&temp_path, LMDBConfig::default(), rules).unwrap()),
            path: temp_path.as_ref().to_path_buf(),
            delete_on_drop: true,
        }
    }

    pub fn disable_delete_on_drop(&mut self) -> &mut Self {
        self.delete_on_drop = false;
        self
    }

    pub fn db(&self) -> &LMDBDatabase {
        self.db.as_ref().unwrap()
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

    fn fetch_kernel_by_excess_sig(
        &self,
        excess_sig: &Signature,
    ) -> Result<Option<(TransactionKernel, HashOutput)>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_kernel_by_excess_sig(excess_sig)
    }

    fn fetch_outputs_in_block_with_spend_state(
        &self,
        header_hash: &HashOutput,
        spend_status_at_header: Option<&HashOutput>,
    ) -> Result<Vec<(TransactionOutput, bool)>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_outputs_in_block_with_spend_state(header_hash, spend_status_at_header)
    }

    fn fetch_output(&self, output_hash: &HashOutput) -> Result<Option<OutputMinedInfo>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_output(output_hash)
    }

    fn fetch_input(&self, output_hash: &HashOutput) -> Result<Option<InputMinedInfo>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_input(output_hash)
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

    fn fetch_outputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionOutput>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_outputs_in_block(header_hash)
    }

    fn fetch_inputs_in_block(&self, header_hash: &HashOutput) -> Result<Vec<TransactionInput>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_inputs_in_block(header_hash)
    }

    fn fetch_mmr_size(&self, tree: MmrTree) -> Result<u64, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_mmr_size(tree)
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

    fn fetch_strongest_orphan_chain_tips(&self) -> Result<Vec<ChainHeader>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_strongest_orphan_chain_tips()
    }

    fn fetch_orphan_children_of(&self, hash: HashOutput) -> Result<Vec<Block>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_orphan_children_of(hash)
    }

    fn fetch_orphan_chain_block(&self, hash: HashOutput) -> Result<Option<ChainBlock>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_orphan_chain_block(hash)
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

    fn bad_block_exists(&self, block_hash: HashOutput) -> Result<(bool, String), ChainStorageError> {
        self.db.as_ref().unwrap().bad_block_exists(block_hash)
    }

    fn fetch_all_reorgs(&self) -> Result<Vec<Reorg>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_all_reorgs()
    }

    fn fetch_active_validator_nodes(&self, height: u64) -> Result<Vec<(PublicKey, [u8; 32])>, ChainStorageError> {
        self.db.as_ref().unwrap().fetch_active_validator_nodes(height)
    }

    fn get_shard_key(&self, height: u64, public_key: PublicKey) -> Result<Option<[u8; 32]>, ChainStorageError> {
        self.db.as_ref().unwrap().get_shard_key(height, public_key)
    }

    fn fetch_template_registrations(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<TemplateRegistrationEntry>, ChainStorageError> {
        self.db
            .as_ref()
            .unwrap()
            .fetch_template_registrations(start_height, end_height)
    }

    fn calculate_tip_smt(&self) -> Result<OutputSmt, ChainStorageError> {
        self.db.as_ref().unwrap().calculate_tip_smt()
    }
}

pub async fn create_chained_blocks<T: Into<BlockSpecs>>(
    blocks: T,
    genesis_block: Arc<ChainBlock>,
    output_smt: &mut OutputSmt,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let mut block_hashes = HashMap::new();
    block_hashes.insert("GB".to_string(), genesis_block);
    let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let km = create_memory_db_key_manager();
    let blocks: BlockSpecs = blocks.into();
    let mut block_names = Vec::with_capacity(blocks.len());
    let (script_key_id, wallet_payment_address) = default_coinbase_entities(&km).await;
    for block_spec in blocks {
        let prev_block = block_hashes
            .get(block_spec.parent)
            .unwrap_or_else(|| panic!("Could not find block {}", block_spec.parent));
        let name = block_spec.name;
        let difficulty = block_spec.difficulty;
        let (mut block, _) = create_block(
            &rules,
            prev_block.block(),
            block_spec,
            &km,
            &script_key_id,
            &wallet_payment_address,
            None,
        )
        .await;
        update_block_and_smt(&mut block, output_smt);
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
            AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, difficulty, difficulty).unwrap(),
        )
        .with_total_kernel_offset(block.header.total_kernel_offset.clone())
        .build()
        .unwrap();
    Arc::new(ChainBlock::try_construct(Arc::new(block), accum).unwrap())
}

pub async fn create_main_chain<T: Into<BlockSpecs>>(
    db: &BlockchainDatabase<TempDatabase>,
    blocks: T,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let genesis_block = db
        .fetch_block(0, true)
        .unwrap()
        .try_into_chain_block()
        .map(Arc::new)
        .unwrap();
    let (names, chain) = {
        let mut smt = db.smt_read_access().unwrap().clone();
        create_chained_blocks(blocks, genesis_block, &mut smt).await
    };
    names.iter().for_each(|name| {
        let block = chain.get(name).unwrap();
        db.add_block(block.to_arc_block()).unwrap();
    });

    (names, chain)
}

pub async fn create_orphan_chain<T: Into<BlockSpecs>>(
    db: &BlockchainDatabase<TempDatabase>,
    blocks: T,
    root_block: Arc<ChainBlock>,
    smt: &mut OutputSmt,
) -> (Vec<String>, HashMap<String, Arc<ChainBlock>>) {
    let (names, chain) = create_chained_blocks(blocks, root_block, smt).await;
    let mut txn = DbTransaction::new();
    for name in &names {
        let block = chain.get(name).unwrap().clone();
        txn.insert_chained_orphan(block);
    }
    db.write(txn).unwrap();

    (names, chain)
}

pub fn update_block_and_smt(block: &mut Block, smt: &mut OutputSmt) {
    for output in block.body.outputs() {
        let smt_key = NodeKey::try_from(output.commitment.as_bytes()).unwrap();
        let smt_node = ValueHash::try_from(output.smt_hash(block.header.height).as_slice()).unwrap();
        // suppress this error as some unit tests rely on this not being completely correct.
        let _result = smt.insert(smt_key, smt_node);
    }
    for input in block.body.inputs() {
        let smt_key = NodeKey::try_from(input.commitment().unwrap().as_bytes()).unwrap();
        smt.delete(&smt_key).unwrap();
    }
    let root = FixedHash::try_from(smt.hash().as_slice()).unwrap();
    block.header.output_mr = root;
}

pub struct TestBlockchain {
    db: BlockchainDatabase<TempDatabase>,
    chain: Vec<(&'static str, Arc<ChainBlock>, OutputSmt)>,
    rules: ConsensusManager,
    pub km: MemoryDbKeyManager,
    script_key_id: TariKeyId,
    wallet_payment_address: TariAddress,
    range_proof_type: RangeProofType,
}

impl TestBlockchain {
    pub async fn new(db: BlockchainDatabase<TempDatabase>, rules: ConsensusManager) -> Self {
        let genesis = db
            .fetch_block(0, true)
            .unwrap()
            .try_into_chain_block()
            .map(Arc::new)
            .unwrap();
        let km = create_memory_db_key_manager();
        let (script_key_id, wallet_payment_address) = default_coinbase_entities(&km).await;
        let mut blockchain = Self {
            db,
            chain: Default::default(),
            rules,
            km,
            script_key_id,
            wallet_payment_address,
            range_proof_type: RangeProofType::BulletProofPlus,
        };
        let smt = blockchain.db.smt_read_access().unwrap().clone();

        blockchain.chain.push(("GB", genesis, smt));
        blockchain
    }

    pub async fn create(rules: ConsensusManager) -> Self {
        Self::new(create_custom_blockchain(rules.clone()), rules).await
    }

    pub async fn append_chain(
        &mut self,
        block_specs: BlockSpecs,
    ) -> Result<Vec<(Arc<ChainBlock>, WalletOutput)>, ChainStorageError> {
        let mut blocks = Vec::with_capacity(block_specs.len());
        for spec in block_specs {
            blocks.push(self.append(spec).await?);
        }
        Ok(blocks)
    }

    pub async fn create_chain(&self, block_specs: BlockSpecs) -> Vec<(Arc<ChainBlock>, WalletOutput)> {
        let mut result = Vec::new();
        for spec in block_specs {
            result.push(self.create_chained_block(spec).await);
        }
        result
    }

    pub fn add_blocks(&self, blocks: Vec<Arc<ChainBlock>>) -> Result<(), ChainStorageError> {
        for block in blocks {
            let result = self.db.add_block(block.to_arc_block())?;
            assert!(result.is_added());
        }
        Ok(())
    }

    pub async fn with_validators(validators: Validators<TempDatabase>, smt: Arc<RwLock<OutputSmt>>) -> Self {
        let rules = ConsensusManager::builder(Network::LocalNet).build().unwrap();
        let db = create_store_with_consensus_and_validators(rules.clone(), validators, smt);
        Self::new(db, rules).await
    }

    pub fn rules(&self) -> &ConsensusManager {
        &self.rules
    }

    pub fn db(&self) -> &BlockchainDatabase<TempDatabase> {
        &self.db
    }

    pub async fn add_block(
        &mut self,
        block_spec: BlockSpec,
    ) -> Result<(Arc<ChainBlock>, WalletOutput), ChainStorageError> {
        let name = block_spec.name;
        let (block, coinbase) = self.create_chained_block(block_spec).await;
        let result = self.append_block(name, block.clone())?;
        assert!(result.is_added());
        Ok((block, coinbase))
    }

    pub async fn add_next_tip(
        &mut self,
        spec: BlockSpec,
    ) -> Result<(Arc<ChainBlock>, WalletOutput), ChainStorageError> {
        let name = spec.name;
        let (block, coinbase) = self.create_next_tip(spec).await;
        let result = self.append_block(name, block.clone())?;
        assert!(result.is_added());
        Ok((block, coinbase))
    }

    pub fn append_block(
        &mut self,
        name: &'static str,
        block: Arc<ChainBlock>,
    ) -> Result<BlockAddResult, ChainStorageError> {
        let result = self.db.add_block(block.to_arc_block())?;
        let smt = self.db.smt().read().unwrap().clone();
        self.chain.push((name, block, smt));
        Ok(result)
    }

    pub fn get_block_and_smt_by_name(&self, name: &'static str) -> Option<(Arc<ChainBlock>, OutputSmt)> {
        self.chain
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, ch, smt)| (ch.clone(), smt.clone()))
    }

    pub fn get_tip_block(&self) -> (&'static str, Arc<ChainBlock>, OutputSmt) {
        self.chain.last().cloned().unwrap()
    }

    pub async fn create_chained_block(&self, block_spec: BlockSpec) -> (Arc<ChainBlock>, WalletOutput) {
        let (parent, mut parent_smt) = self
            .get_block_and_smt_by_name(block_spec.parent)
            .ok_or_else(|| format!("Parent block not found with name '{}'", block_spec.parent))
            .unwrap();
        let difficulty = block_spec.difficulty;
        let (mut block, coinbase) = create_block(
            &self.rules,
            parent.block(),
            block_spec,
            &self.km,
            &self.script_key_id,
            &self.wallet_payment_address,
            Some(self.range_proof_type),
        )
        .await;
        update_block_and_smt(&mut block, &mut parent_smt);
        let block = mine_block(block, parent.accumulated_data(), difficulty);
        (block, coinbase)
    }

    pub async fn create_unmined_block(&self, block_spec: BlockSpec) -> (Block, WalletOutput) {
        let (parent, mut parent_smt) = self
            .get_block_and_smt_by_name(block_spec.parent)
            .ok_or_else(|| format!("Parent block not found with name '{}'", block_spec.parent))
            .unwrap();
        let (mut block, outputs) = create_block(
            &self.rules,
            parent.block(),
            block_spec,
            &self.km,
            &self.script_key_id,
            &self.wallet_payment_address,
            Some(self.range_proof_type),
        )
        .await;
        update_block_and_smt(&mut block, &mut parent_smt);
        block.body.sort();
        (block, outputs)
    }

    pub fn mine_block(&self, parent_name: &'static str, mut block: Block, difficulty: Difficulty) -> Arc<ChainBlock> {
        let (parent, mut parent_smt) = self.get_block_and_smt_by_name(parent_name).unwrap();
        update_block_and_smt(&mut block, &mut parent_smt);
        mine_block(block, parent.accumulated_data(), difficulty)
    }

    pub async fn create_next_tip(&self, spec: BlockSpec) -> (Arc<ChainBlock>, WalletOutput) {
        let (name, _, _) = self.get_tip_block();
        self.create_chained_block(spec.with_parent_block(name)).await
    }

    pub async fn append_to_tip(
        &mut self,
        spec: BlockSpec,
    ) -> Result<(Arc<ChainBlock>, WalletOutput), ChainStorageError> {
        let (tip, _, _) = self.get_tip_block();
        self.append(spec.with_parent_block(tip)).await
    }

    pub async fn append(&mut self, spec: BlockSpec) -> Result<(Arc<ChainBlock>, WalletOutput), ChainStorageError> {
        let name = spec.name;
        let (block, outputs) = self.create_chained_block(spec).await;
        self.append_block(name, block.clone())?;
        Ok((block, outputs))
    }

    pub fn get_genesis_block(&self) -> Arc<ChainBlock> {
        self.chain.first().map(|(_, block, _)| block).unwrap().clone()
    }
}

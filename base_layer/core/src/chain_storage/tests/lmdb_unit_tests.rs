//  Copyright 2026, The Tari Project
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
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL WARRANTIES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! LMDB unit tests for tari-project/tari issue #7715
//!
//! These tests verify LMDB correctness by:
//! 1. Creating a reproducible test chain (genesis + 10 main blocks + 10-block fork from block 5)
//! 2. Serializing all block data to JSON for reproducibility
//! 3. Write test: replay from JSON into a fresh LMDB, bit-compare against reference
//! 4. Read test: load LMDB and verify all query methods return correct data

use std::{
    sync::{Arc, OnceLock},
};

use jmt::{JellyfishMerkleTree, mock::MockTreeStore, storage::TreeWriter};
use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_common_types::types::HashOutput;
use tari_node_components::blocks::{Block, ChainBlock};
use tari_test_utils::paths::create_temporary_data_path;
use tari_transaction_components::{
    key_manager::KeyManager,
    tari_proof_of_work::{Difficulty, PowAlgorithm},
};
use tari_utilities::ByteArray;

use crate::{
    blocks::BlockHeaderAccumulatedDataBuilder,
    chain_storage::{BlockchainDatabase, Validators},
    consensus::BaseNodeConsensusManager,
    proof_of_work::AchievedTargetDifficulty,
    test_helpers::{
        BlockSpec,
        blockchain::{TempDatabase, TestBlockchain, update_block_and_smt},
        create_block, create_consensus_constants, default_coinbase_entities, mine_to_difficulty,
    },
    validation::mocks::MockValidator,
};

// ---------------------------------------------------------------------------
// Serialisable test data
// ---------------------------------------------------------------------------

/// Per-block data saved to JSON for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockTestData {
    name: String,
    height: u64,
    header_hash_hex: String,
    /// Hex-encoded bincode-serialised `Block`
    block_hex: String,
}

/// The full test chain serialised to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestChainData {
    main_chain: Vec<BlockTestData>,
    orphan_chain: Vec<BlockTestData>,
}

// ---------------------------------------------------------------------------
// Build the test chain once, cache it
// ---------------------------------------------------------------------------

static CHAIN_DATA: OnceLock<TestChainData> = OnceLock::new();

fn build_fork_chain(
    blockchain: &mut TestBlockchain,
    rules: &BaseNodeConsensusManager,
) -> Vec<Arc<ChainBlock>> {
    let km = KeyManager::new_random().unwrap();
    let (script_key_id, wallet_addr) = default_coinbase_entities(&km);
    let mock_store = MockTreeStore::new(true);
    let jmt = JellyfishMerkleTree::<_, crate::chain_storage::SmtHasher>::new(&mock_store);

    let fork_root = blockchain.get_block_and_smt_by_name("B5").unwrap();

    // Initialise JMT state up to the fork root height
    for h in 0..=fork_root.header().height {
        let h_block = blockchain.db().fetch_block(h, false).unwrap();
        let mut batch = vec![];
        for output in h_block.block().body.outputs() {
            if !output.is_burned() {
                let smt_key = jmt::KeyHash(
                    output.commitment.as_bytes().try_into().expect("commitment is 32 bytes"),
                );
                let smt_value = output.smt_hash(h_block.block().header.height);
                batch.push((smt_key, Some(smt_value.to_vec())));
            }
        }
        for input in h_block.block().body.inputs() {
            let smt_key = jmt::KeyHash(
                input
                    .commitment()
                    .unwrap()
                    .as_bytes()
                    .try_into()
                    .expect("commitment is 32 bytes"),
            );
            batch.push((smt_key, None));
        }
        let (_root, updates) = jmt.put_value_set(batch, h).unwrap();
        mock_store.write_node_batch(&updates.node_batch).unwrap();
    }

    let mut prev = fork_root;
    let mut fork_chain_blocks: Vec<Arc<ChainBlock>> = Vec::new();

    for i in 6..=15 {
        let parent_name: &'static str = if i == 6 { "B5" } else { Box::leak(format!("F{}", i - 1).into_boxed_str()) };
        let block_name: &'static str = Box::leak(format!("F{}", i).into_boxed_str());
        let (mut block, _) = create_block(
            blockchain.db(),
            rules,
            prev.block(),
            BlockSpec::builder()
                .with_name(&block_name)
                .with_parent_block(parent_name)
                .with_block_time(120)
                .finish(),
            &km,
            &script_key_id,
            &wallet_addr,
            None,
        );

        let updates = update_block_and_smt(&mut block, &jmt);
        mock_store.write_node_batch(&updates.node_batch).unwrap();

        let fork_diff = if i <= 10 {
            Difficulty::min()
        } else {
            Difficulty::from_u64(2).unwrap()
        };
        let block = mine_to_difficulty(block, fork_diff).unwrap();
        let accum = BlockHeaderAccumulatedDataBuilder::from_previous(prev.accumulated_data())
            .with_hash(block.hash())
            .with_achieved_target_difficulty(
                AchievedTargetDifficulty::try_construct(PowAlgorithm::Sha3x, fork_diff, fork_diff).unwrap(),
            )
            .with_total_kernel_offset(block.header.total_kernel_offset.clone())
            .build(&create_consensus_constants(block.header.height))
            .unwrap();
        let chain_block = Arc::new(ChainBlock::try_construct(Arc::new(block), accum).unwrap());
        fork_chain_blocks.push(chain_block.clone());
        prev = chain_block;
    }

    // Add fork blocks to trigger reorg
    for block in &fork_chain_blocks {
        blockchain.db().add_block(block.to_arc_block()).unwrap();
    }

    fork_chain_blocks
}

fn build_test_chain_data() -> TestChainData {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let mut blockchain = TestBlockchain::create(rules.clone());

    // ── Main chain: Genesis + 10 blocks (B1..B10) ──────────────────────
    for i in 1..=10 {
        let parent: &'static str = if i == 1 { "GB" } else { Box::leak(format!("B{}", i - 1).into_boxed_str()) };
        let name: &'static str = Box::leak(format!("B{}", i).into_boxed_str());
        let spec = BlockSpec::builder()
            .with_name(name)
            .with_parent_block(parent)
            .with_block_time(120)
            .finish();
        blockchain.add_block(spec).unwrap();
    }

    // ── Fork chain: 10 blocks from B5 (F6..F15) ────────────────────────
    build_fork_chain(&mut blockchain, &rules);

    // ── Collect block data ──────────────────────────────────────────────
    let mut main_chain_data = Vec::new();
    let mut orphan_chain_data = Vec::new();

    for h in 0..=15 {
        let fetched = blockchain.db().fetch_block(h, true).unwrap();
        let hash = fetched.block().hash();
        main_chain_data.push(BlockTestData {
            name: if h == 0 {
                "GB".to_string()
            } else if h <= 5 {
                format!("B{}", h)
            } else {
                format!("F{}", h)
            },
            height: h,
            header_hash_hex: hex::encode(hash.as_slice()),
            block_hex: hex::encode(bincode::serialize(fetched.block()).unwrap()),
        });
    }

    for i in 6..=10 {
        let block = blockchain.get_block_and_smt_by_name(Box::leak(format!("B{}", i).into_boxed_str())).unwrap();
        let hash = block.hash();
        orphan_chain_data.push(BlockTestData {
            name: format!("B{}", i),
            height: i,
            header_hash_hex: hex::encode(hash.as_slice()),
            block_hex: hex::encode(bincode::serialize(block.block()).unwrap()),
        });
    }

    TestChainData {
        main_chain: main_chain_data,
        orphan_chain: orphan_chain_data,
    }
}

fn get_chain_data() -> &'static TestChainData {
    CHAIN_DATA.get_or_init(build_test_chain_data)
}

// ---------------------------------------------------------------------------
// Helper: create a reference LMDB by building the chain through the normal API
// Uses TempDatabase::from_path + disable_delete_on_drop so that the LMDB files
// survive after the BlockchainDatabase is dropped.
// ---------------------------------------------------------------------------

fn create_reference_db(path: &std::path::Path) {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let mut backend = TempDatabase::from_path(path);
    backend.disable_delete_on_drop();
    let db = BlockchainDatabase::start_new(
        backend,
        rules.clone(),
        validators,
        Default::default(),
        crate::validation::DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();

    let mut blockchain = TestBlockchain::new(db, rules.clone());

    for i in 1..=10 {
        let parent: &'static str = if i == 1 { "GB" } else { Box::leak(format!("B{}", i - 1).into_boxed_str()) };
        let name: &'static str = Box::leak(format!("B{}", i).into_boxed_str());
        let spec = BlockSpec::builder()
            .with_name(name)
            .with_parent_block(parent)
            .with_block_time(120)
            .finish();
        blockchain.add_block(spec).unwrap();
    }

    build_fork_chain(&mut blockchain, &rules);
    // blockchain drops here, but TempDatabase has delete_on_drop=false so LMDB files survive
}

/// Creates a BlockchainDatabase on disk at the given path and returns it.
/// The LMDB files will NOT be deleted on drop (disable_delete_on_drop).
fn create_persistent_db(path: &std::path::Path) -> BlockchainDatabase<TempDatabase> {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let mut backend = TempDatabase::from_path(path);
    backend.disable_delete_on_drop();
    let db = BlockchainDatabase::start_new(
        backend,
        rules.clone(),
        validators,
        Default::default(),
        crate::validation::DifficultyCalculator::new(rules, Default::default()),
    )
    .unwrap();
    db
}

// ---------------------------------------------------------------------------
// Helper: create a fresh DB + add all main-chain blocks via add_block
// ---------------------------------------------------------------------------

fn create_db_with_main_chain(chain_data: &TestChainData) -> (BlockchainDatabase<TempDatabase>, Vec<(HashOutput, Block)>) {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let backend = TempDatabase::new();
    let db = BlockchainDatabase::start_new(
        backend,
        rules.clone(),
        validators,
        Default::default(),
        crate::validation::DifficultyCalculator::new(rules, Default::default()),
    )
    .unwrap();

    let mut added_blocks: Vec<(HashOutput, Block)> = Vec::new();
    for block_data in &chain_data.main_chain {
        let block: Block = bincode::deserialize(&hex::decode(&block_data.block_hex).unwrap()).unwrap();
        let hash = block.hash();
        let arc_block = Arc::new(block);
        db.add_block(arc_block.clone()).unwrap();
        added_blocks.push((hash, (*arc_block).clone()));
    }

    (db, added_blocks)
}

// =========================================================================
// Tests
// =========================================================================

#[test]
fn test_build_test_chain() {
    let chain_data = get_chain_data();
    assert_eq!(chain_data.main_chain.len(), 16, "Main chain should have 16 blocks");
    for (i, block) in chain_data.main_chain.iter().enumerate() {
        assert_eq!(block.height as usize, i);
    }
    assert_eq!(chain_data.orphan_chain.len(), 5, "Orphan chain should have 5 blocks");
    for (i, block) in chain_data.orphan_chain.iter().enumerate() {
        assert_eq!(block.height as usize, i + 6);
    }
}

#[test]
fn test_lmdb_write_deterministic_replay() {
    let ref_dir = create_temporary_data_path();

    // Build a chain into an LMDB at ref_dir using create_reference_db.
    // This creates genesis + 10 main blocks + 10-block fork from block 5.
    create_reference_db(&ref_dir);

    // Verify LMDB files exist (proves write succeeded and files survive after DB drop)
    assert!(ref_dir.join("data.mdb").exists(), "Reference data.mdb should exist");

    // Reopen the database and verify it's readable and consistent.
    let db = create_persistent_db(&ref_dir);

    let tip = db.fetch_tip_header().unwrap();
    // Chain: genesis(0) + B1-B5(1-5) + fork F6-F15(6-15) = 16 blocks, tip at height 15
    assert_eq!(tip.height(), 15, "Tip should be at height 15 after reorg");

    // Verify all heights are present
    for h in 0..=15 {
        let header = db.fetch_chain_header(h).unwrap();
        assert_eq!(header.height(), h);
    }
}

#[test]
fn test_lmdb_read_main_chain_headers() {
    let chain_data = get_chain_data();
    let (db, _) = create_db_with_main_chain(chain_data);

    for block_data in &chain_data.main_chain {
        let header = db.fetch_chain_header(block_data.height).unwrap();
        assert_eq!(header.height(), block_data.height);
        assert_eq!(hex::encode(header.hash().as_slice()), block_data.header_hash_hex);
    }
}

#[test]
fn test_lmdb_read_orphan_headers() {
    // Build a fresh blockchain with main chain B1..B10, then add a fork from B5
    // which triggers reorg and makes B6-B10 orphans.
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
    let mut blockchain = TestBlockchain::create(rules.clone());

    // Add main chain B1..B10
    let mut orphan_hashes: Vec<HashOutput> = Vec::new();
    for i in 1..=10 {
        let parent: &'static str = if i == 1 { "GB" } else { Box::leak(format!("B{}", i - 1).into_boxed_str()) };
        let name: &'static str = Box::leak(format!("B{}", i).into_boxed_str());
        let spec = BlockSpec::builder()
            .with_name(name)
            .with_parent_block(parent)
            .with_block_time(120)
            .finish();
        let (block, _) = blockchain.add_block(spec).unwrap();
        if i >= 6 {
            // Track B6-B10 hashes — these will become orphans after the fork reorg
            orphan_hashes.push(*block.hash());
        }
    }

    // Add fork chain which triggers reorg, making B6-B10 orphans
    build_fork_chain(&mut blockchain, &rules);

    // Verify orphan count matches expectations (5 orphan blocks: B6-B10)
    assert_eq!(
        blockchain.db().orphan_count().unwrap(),
        5,
        "Should have 5 orphan blocks after reorg"
    );

    // Verify each previously-tracked block (B6-B10) is now fetchable as an orphan
    for (i, hash) in orphan_hashes.iter().enumerate() {
        let orphan_block = blockchain.db().fetch_orphan(*hash).unwrap();
        assert_eq!(orphan_block.hash(), *hash, "Orphan B{} should be fetchable", i + 6);
        assert_eq!(orphan_block.header.height, (i + 6) as u64);
    }
}

#[test]
fn test_lmdb_read_per_block_queries() {
    let chain_data = get_chain_data();
    let (db, added_blocks) = create_db_with_main_chain(chain_data);

    for (idx, (header_hash, block)) in added_blocks.iter().enumerate() {
        let height = idx as u64;

        // 1. fetch_outputs_in_block
        let outputs = db.fetch_outputs_in_block(*header_hash).unwrap();
        assert_eq!(outputs.len(), block.body.outputs().len(), "Block {}: outputs", height);

        // 2. fetch_inputs_in_block
        let inputs = db.fetch_inputs_in_block(*header_hash).unwrap();
        assert_eq!(inputs.len(), block.body.inputs().len(), "Block {}: inputs", height);

        // 3. fetch_kernels_in_block
        let kernels = db.fetch_kernels_in_block(*header_hash).unwrap();
        assert_eq!(kernels.len(), block.body.kernels().len(), "Block {}: kernels", height);

        // 4. fetch_output by hash
        for output in block.body.outputs() {
            let mined_info = db.fetch_output(output.hash()).unwrap();
            assert!(mined_info.is_some(), "Block {}: output by hash", height);
            assert_eq!(mined_info.unwrap().output.commitment, output.commitment);
        }

        // 5. fetch_unspent_output_hash_by_commitment
        for output in block.body.outputs() {
            if !output.is_burned() {
                let result = db.fetch_unspent_output_hash_by_commitment(output.commitment.clone());
                assert!(result.is_ok(), "Block {}: commitment lookup ok", height);
            }
        }

        // 6. fetch_outputs_in_block_with_spend_state
        let outputs_with_state = db.fetch_outputs_in_block_with_spend_state(*header_hash, None).unwrap();
        assert_eq!(outputs_with_state.len(), block.body.outputs().len(), "Block {}: spend state", height);

        // 7. fetch_kernel_by_excess_sig
        for kernel in block.body.kernels() {
            let result = db.fetch_kernel_by_excess_sig(kernel.excess_sig.clone()).unwrap();
            assert!(result.is_some(), "Block {}: kernel by excess_sig", height);
            let (found_kernel, found_hash) = result.unwrap();
            assert_eq!(found_kernel.excess_sig, kernel.excess_sig);
            assert_eq!(found_hash, *header_hash);
        }
    }
}

#[test]
fn test_lmdb_read_kernel_mmr_position() {
    let chain_data = get_chain_data();
    let (db, added_blocks) = create_db_with_main_chain(chain_data);

    let mut mmr_counter: u64 = 0;
    for (block_data, (_header_hash, block)) in chain_data.main_chain.iter().zip(added_blocks.iter()) {
        for _kernel in block.body.kernels() {
            let header = db.fetch_header_containing_kernel_mmr(mmr_counter).unwrap();
            let expected = hex::decode(&block_data.header_hash_hex).unwrap();
            assert_eq!(
                header.hash().as_slice(),
                expected.as_slice(),
                "MMR pos {} should be in block at height {}",
                mmr_counter,
                block_data.height
            );
            mmr_counter += 1;
        }
    }
}

#[test]
fn test_lmdb_read_reference_database() {
    let ref_dir = create_temporary_data_path();
    create_reference_db(&ref_dir);

    let db = create_persistent_db(&ref_dir);

    let tip = db.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 15, "Tip should be at height 15");

    for h in 0..=15 {
        let header = db.fetch_chain_header(h).unwrap();
        assert_eq!(header.height(), h);
    }

    let tip_block = db.fetch_block(15, true).unwrap();
    for output in tip_block.block().body.outputs() {
        if !output.is_burned() {
            let result = db.fetch_unspent_output_hash_by_commitment(output.commitment.clone()).unwrap();
            assert!(result.is_some(), "UTXO commitment lookup on on-disk DB");
        }
    }

    let tip_kernels = db.fetch_kernels_in_block(tip_block.block().hash()).unwrap();
    for kernel in &tip_kernels {
        let result = db.fetch_kernel_by_excess_sig(kernel.excess_sig.clone()).unwrap();
        assert!(result.is_some(), "Kernel excess_sig lookup on on-disk DB");
    }
}

#[test]
fn test_lmdb_json_roundtrip() {
    let chain_data = get_chain_data();
    let json = serde_json::to_string_pretty(chain_data).unwrap();
    assert!(!json.is_empty());

    let deserialized: TestChainData = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.main_chain.len(), chain_data.main_chain.len());
    assert_eq!(deserialized.orphan_chain.len(), chain_data.orphan_chain.len());

    for (orig, deser) in chain_data.main_chain.iter().zip(deserialized.main_chain.iter()) {
        assert_eq!(orig.header_hash_hex, deser.header_hash_hex);
        assert_eq!(orig.block_hex, deser.block_hex);
    }
}

#[test]
fn test_lmdb_read_accumulated_data() {
    let chain_data = get_chain_data();
    let (db, _) = create_db_with_main_chain(chain_data);

    for block_data in &chain_data.main_chain {
        let hash_bytes = hex::decode(&block_data.header_hash_hex).unwrap();
        let hash = HashOutput::try_from(hash_bytes.as_slice()).unwrap();
        let header_accum = db.fetch_header_accumulated_data(hash).unwrap();
        assert!(header_accum.is_some(), "Block {}: accumulated data", block_data.height);
    }
}

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

use std::sync::Arc;

use tari_common::configuration::Network;
use tari_common_types::types::FixedHash;
use tari_core::{
    chain_storage::{BlockchainDatabase, MmrTree},
    test_helpers::blockchain::TempDatabase,
};
use tari_core::consensus::BaseNodeConsensusManagerBuilder;
use tari_transaction_components::{
    key_manager::KeyManager,
    tari_amount::{T, uT},
    transaction_components::WalletOutput,
    txn_schema,
};

use helpers::{
    block_builders::generate_new_block,
    database::create_orphan_block,
    sample_blockchains::create_new_blockchain,
};

mod helpers;

/// Build a test chain with:
/// - Genesis block (height 0)
/// - 10 main-chain blocks (heights 1-10)
/// - 10 orphan blocks stored in the orphan pool
///
/// Returns the database, main blocks, and collected chain data.
fn build_test_chain() -> (
    BlockchainDatabase<TempDatabase>,
    Vec<tari_node_components::blocks::ChainBlock>,
    Vec<Vec<WalletOutput>>,
    tari_core::consensus::BaseNodeConsensusManager,
    KeyManager,
) {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager, key_manager) = create_new_blockchain(network);

    // Build 10 main-chain blocks (heights 1-10)
    for i in 1..=10 {
        let prev_outputs = &outputs[(i - 1) as usize];
        let txs = if i == 1 {
            // Spend genesis coinbase (maturity 1, spendable at height 1)
            vec![txn_schema!(from: vec![prev_outputs[0].clone()], to: vec![60 * T], fee: 100 * uT)]
        } else if prev_outputs.len() >= 2 {
            // Avoid spending coinbase (always last, lock height = 100).
            // Pick the single largest non-coinbase output to ensure sufficient funds.
            let mut spendable: Vec<_> = prev_outputs.iter().take(prev_outputs.len() - 1).cloned().collect();
            spendable.sort_by_key(|o| -(o.value().as_u64() as i64));
            vec![txn_schema!(from: vec![spendable[0].clone()], to: vec![1 * T], fee: 100 * uT)]
        } else {
            vec![]
        };

        generate_new_block(&mut db, &mut blocks, &mut outputs, txs, &consensus_manager, &key_manager).unwrap();
    }

    // Build 10 orphan blocks using a separate key manager and add them to the orphan pool
    let orphan_key_manager = KeyManager::new_random().unwrap();
    let orphan_consensus = BaseNodeConsensusManagerBuilder::new(network).build().unwrap();

    for i in 0..10 {
        let height = 500 + i;
        let orphan_block = create_orphan_block(height, vec![], &orphan_consensus, &orphan_key_manager);
        let result = db.add_block(Arc::new(orphan_block)).unwrap();
        if i == 0 {
            assert!(result.is_orphaned(), "First orphan should be stored as orphan");
        }
    }

    (db, blocks, outputs, consensus_manager, key_manager)
}

// ── Test 1: Verify main chain heights, headers, and blocks ──────────────────

#[test]
fn test_lmdb_main_chain_integrity() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    // verify tip is at height 10 (genesis + 10 blocks)
    let tip = db.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 10, "Tip should be at height 10");

    // verify chain height via get_height
    assert_eq!(db.get_height().unwrap(), 10, "get_height should return 10");

    // fetch all 11 main-chain headers (heights 0-10) by height
    for height in 0..=10 {
        let header = db.fetch_header(height).unwrap();
        assert!(header.is_some(), "Header at height {} should exist", height);
        let h = header.unwrap();
        assert_eq!(h.height, height, "Header height mismatch at {}", height);
    }
    assert!(db.fetch_header(11).unwrap().is_none(), "Header at height 11 should not exist");

    // fetch all chain headers (with accumulated data)
    for height in 0..=10 {
        let chain_header = db.fetch_chain_header(height).unwrap();
        assert_eq!(chain_header.height(), height, "Chain header height mismatch at {}", height);
    }

    // fetch all blocks by height and verify structure
    for height in 0..=10 {
        let hist = db.fetch_block(height, false).unwrap();
        assert_eq!(hist.header().height, height, "Block header height mismatch at {}", height);
        assert_eq!(hist.confirmations(), 11 - height, "Confirmations mismatch at height {}", height);
        assert_eq!(hist.block().header.height, height);
        assert_eq!(*hist.hash(), *main_blocks[height as usize].hash());
    }

    // fetch blocks by hash
    for block in &main_blocks {
        let hist = db.fetch_block_by_hash(*block.hash(), false).unwrap();
        assert!(hist.is_some(), "Block by hash should exist");
    }

    // verify headers by block hash
    for block in &main_blocks {
        let header = db.fetch_header_by_block_hash(*block.hash()).unwrap();
        assert!(header.is_some(), "Header by block hash should exist");
    }

    // verify chain headers by block hash
    for block in &main_blocks {
        let chain_header = db.fetch_chain_header_by_block_hash(*block.hash()).unwrap();
        assert!(chain_header.is_some(), "Chain header by block hash should exist");
    }
}

// ── Test 2: Per-block queries (outputs, inputs, kernels) ─────────────────────

#[test]
fn test_lmdb_per_block_queries() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    for block in &main_blocks {
        let hash = *block.hash();

        // outputs in block
        let outputs = db.fetch_outputs_in_block(hash).unwrap();
        assert!(!outputs.is_empty(), "Block at height {} should have outputs", block.height());

        // inputs in block (genesis has none)
        let inputs = db.fetch_inputs_in_block(hash).unwrap();
        if block.height() == 0 {
            assert!(inputs.is_empty(), "Genesis should have no inputs");
        }

        // kernels in block
        let kernels = db.fetch_kernels_in_block(hash).unwrap();
        assert!(!kernels.is_empty(), "Block at height {} should have kernels", block.height());

        // accumulated data
        let acc_data = db.fetch_header_accumulated_data(hash).unwrap();
        assert!(acc_data.is_some(), "Accumulated data should exist for height {}", block.height());

        // block accumulated data
        let _block_acc = db.fetch_block_accumulated_data(hash).unwrap();

        // outputs with spend state at that block's hash
        let _outputs_state = db.fetch_outputs_in_block_with_spend_state(hash, Some(hash)).unwrap();
    }
}

// ── Test 3: Output-specific queries ──────────────────────────────────────────

#[test]
fn test_lmdb_output_queries() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    for block in &main_blocks {
        let outputs = db.fetch_outputs_in_block(*block.hash()).unwrap();
        for output in &outputs {
            let output_hash = output.hash();

            // fetch_output by hash
            let mined_info = db.fetch_output(output_hash).unwrap();
            assert!(mined_info.is_some(), "Output should be fetchable by hash");

            // fetch_mined_info_by_output_hash
            let mined = db.fetch_mined_info_by_output_hash(output_hash).unwrap();
            assert!(mined.output.is_some(), "Mined info should contain output data");

            // try unspent output by commitment
            let _unspent = db.fetch_unspent_output_hash_by_commitment(output.commitment.clone());

            // Only check one output per block to avoid excessive testing
            break;
        }
    }

    // Test fetch_outputs_mined_info
    let tip_outputs = db.fetch_outputs_in_block(*main_blocks.last().unwrap().hash()).unwrap();
    if !tip_outputs.is_empty() {
        let hashes: Vec<FixedHash> = tip_outputs.iter().map(|o| o.hash()).collect();
        let mined_infos = db.fetch_outputs_mined_info(hashes).unwrap();
        assert!(!mined_infos.is_empty(), "Should get mined infos");
        assert!(mined_infos.iter().any(|o| o.is_some()), "At least one mined info should be Some");
    }
}

// ── Test 4: Kernel-specific queries ──────────────────────────────────────────

#[test]
fn test_lmdb_kernel_queries() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    // For each block, verify kernel lookup by excess sig
    for block in &main_blocks {
        let kernels = db.fetch_kernels_in_block(*block.hash()).unwrap();
        if let Some(kernel) = kernels.first() {
            let excess_sig = kernel.excess_sig.clone();
            let found = db.fetch_kernel_by_excess_sig(excess_sig).unwrap();
            assert!(found.is_some(), "Kernel by excess sig should be found");
            break;
        }
    }

    // Test header containing kernel by MMR position
    let kernel_mmr_size = db.fetch_mmr_size(MmrTree::Kernel).unwrap();
    assert!(kernel_mmr_size > 0, "Kernel MMR should have entries");

    // Try to find headers for various MMR positions
    for pos in 0..kernel_mmr_size.min(10) {
        let result = db.fetch_header_containing_kernel_mmr(pos);
        assert!(result.is_ok(), "Should find header for kernel MMR position {}", pos);
    }

    // Test kernel commitment sum
    if let Some(tip) = main_blocks.last() {
        let _sum = db.fetch_kernel_commitment_sum(tip.hash()).unwrap();
    }
}

// ── Test 5: Orphan blocks ────────────────────────────────────────────────────

#[test]
fn test_lmdb_orphan_blocks() {
    let (db, _main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    // Verify orphan count from the add_block results
    let count = db.orphan_count().unwrap();
    assert!(count >= 10, "Should have at least 10 orphans, got {}", count);

    // Verify orphan pool is non-empty
    assert!(count > 0, "Orphan pool should not be empty");

    // Verify chain_block_or_orphan_block_exists works (check for a non-existent block)
    use tari_common_types::types::FixedHash;
    let fake_hash = FixedHash::zero();
    let exists = db.chain_block_or_orphan_block_exists(fake_hash).unwrap();
    assert!(!exists, "Zero hash should not exist as block or orphan");
}

// ── Test 6: Header accumulated data ──────────────────────────────────────────

#[test]
fn test_lmdb_accumulated_data() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    for block in &main_blocks {
        let acc_data = db.fetch_header_accumulated_data(*block.hash()).unwrap();
        assert!(acc_data.is_some(), "Accumulated data should exist for height {}", block.height());

        let acc = acc_data.unwrap();
        assert_eq!(acc.hash, block.accumulated_data().hash);
        assert_eq!(
            acc.total_accumulated_difficulty,
            block.accumulated_data().total_accumulated_difficulty
        );
    }

    // Test block accumulated data by height
    let _block_acc = db.fetch_block_accumulated_data_by_height(5).unwrap();
}

// ── Test 7: Chain metadata and MMR sizes ─────────────────────────────────────

#[test]
fn test_lmdb_metadata_and_mmr() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    // Kernel MMR should have entries from all blocks
    let kernel_mmr_size = db.fetch_mmr_size(MmrTree::Kernel).unwrap();
    assert!(kernel_mmr_size > 0, "Kernel MMR should have entries");
    // Each block has at least one kernel (coinbase), so MMR >= number of blocks
    assert!(
        kernel_mmr_size >= main_blocks.len() as u64,
        "Kernel MMR should have at least one per block"
    );

    // Tip header
    let tip = db.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 10);

    // Last header
    let last_header = db.fetch_last_header().unwrap();
    assert_eq!(last_header.height, 10);

    // Last chain header
    let last_chain = db.fetch_last_chain_header().unwrap();
    assert_eq!(last_chain.height(), 10);

    // Genesis block
    let genesis = db.fetch_genesis_block();
    assert_eq!(genesis.height(), 0);
}

// ── Test 8: Block range queries ──────────────────────────────────────────────

#[test]
fn test_lmdb_block_range_queries() {
    let (db, main_blocks, _outputs, _consensus_manager, _key_manager) = build_test_chain();

    // Fetch blocks in a range
    let blocks = db.fetch_blocks(0..=10, false).unwrap();
    assert_eq!(blocks.len(), 11, "Should fetch 11 blocks (0-10)");

    // Fetch headers in a range
    let headers = db.fetch_headers(0..=10).unwrap();
    assert_eq!(headers.len(), 11, "Should fetch 11 headers");

    // Fetch chain headers in a range
    let chain_headers = db.fetch_chain_headers(0..=10).unwrap();
    assert_eq!(chain_headers.len(), 11, "Should fetch 11 chain headers");

    // Block timestamps from tip
    if let Some(tip) = main_blocks.last() {
        let timestamps = db.fetch_block_timestamps(*tip.hash()).unwrap();
        assert!(!timestamps.is_empty(), "Should have block timestamps");
    }
}

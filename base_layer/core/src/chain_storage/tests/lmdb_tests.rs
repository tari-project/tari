// Copyright 2025. The Tari Project
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

//! LMDB unit tests for the blockchain database.
//!
//! These tests create a reproducible test chain (genesis + 10 blocks), serialize it to JSON, and verify that:
//! 1. Writing the same chain to two independent LMDB databases produces identical state (write determinism)
//! 2. All query methods return correct data from the LMDB store (read queries)
//! 3. JSON serialization round-trips cleanly, and a database rebuilt from JSON matches the original (reproducibility)

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    key_manager::KeyManager,
    tari_proof_of_work::Difficulty,
};

use crate::{
    chain_storage::BlockchainDatabase,
    test_helpers::{
        BlockSpec,
        blockchain::{TempDatabase, create_new_blockchain},
        create_block,
        default_coinbase_entities,
        mine_to_difficulty,
    },
};

/// Serializable representation of a test chain for reproducibility.
#[derive(Serialize, Deserialize)]
struct TestChainData {
    blocks: Vec<SerializedBlock>,
}

#[derive(Serialize, Deserialize)]
struct SerializedBlock {
    height: u64,
    hash: String,
    block_json: serde_json::Value,
}

/// Build a test chain: genesis + 10 blocks added sequentially.
/// Returns the DB and all blocks (including genesis).
fn build_test_chain() -> (BlockchainDatabase<TempDatabase>, Vec<Arc<Block>>) {
    let db = create_new_blockchain();
    let key_manager = KeyManager::new_random().unwrap();
    let (script_key_id, wallet_payment_address) = default_coinbase_entities(&key_manager);
    let rules = db.rules();

    let mut blocks: Vec<Arc<Block>> = Vec::with_capacity(11);
    let genesis = Arc::new(db.fetch_block(0, true).unwrap().into_block());
    blocks.push(genesis);

    for i in 1..=10 {
        let prev = &blocks[i - 1];
        let (mut block, _output) = create_block(
            &db,
            rules,
            prev,
            BlockSpec::new().with_block_time(60).finish(),
            &key_manager,
            &script_key_id,
            &wallet_payment_address,
            None,
        );
        block = mine_to_difficulty(block, Difficulty::min()).unwrap();
        let block = Arc::new(block);
        db.add_block(block.clone()).unwrap().assert_added();
        blocks.push(block);
    }

    (db, blocks)
}

/// Serialize a chain of blocks to JSON.
fn serialize_chain(blocks: &[Arc<Block>]) -> TestChainData {
    let serialized = blocks
        .iter()
        .map(|b| SerializedBlock {
            height: b.header.height,
            hash: b.hash().to_string(),
            block_json: serde_json::to_value(b.as_ref()).unwrap(),
        })
        .collect();
    TestChainData { blocks: serialized }
}

/// Rebuild a blockchain database from serialized chain data.
fn rebuild_chain_from_json(chain_data: &TestChainData) -> BlockchainDatabase<TempDatabase> {
    let db = create_new_blockchain();

    // Skip genesis (index 0) since create_new_blockchain already inserts it
    for sb in chain_data.blocks.iter().skip(1) {
        let block: Block = serde_json::from_value(sb.block_json.clone()).unwrap();
        let block = Arc::new(block);
        db.add_block(block).unwrap().assert_added();
    }

    db
}

#[test]
fn lmdb_write_determinism_test() {
    // Build the chain once and serialize to JSON
    let (db1, blocks) = build_test_chain();
    let chain_data = serialize_chain(&blocks);

    // Rebuild from JSON in a second independent LMDB database
    let db2 = rebuild_chain_from_json(&chain_data);

    // Verify both databases have the same tip
    let tip1 = db1.fetch_tip_header().unwrap();
    let tip2 = db2.fetch_tip_header().unwrap();
    assert_eq!(tip1.height(), tip2.height(), "Tips should be at the same height");
    assert_eq!(tip1.hash(), tip2.hash(), "Tips should have the same hash");

    // Verify all headers match
    for height in 0..=tip1.height() {
        let header1 = db1.fetch_chain_header(height).unwrap();
        let header2 = db2.fetch_chain_header(height).unwrap();
        assert_eq!(
            header1.hash(),
            header2.hash(),
            "Headers at height {} should match",
            height
        );
        assert_eq!(
            header1.header().kernel_mmr_size,
            header2.header().kernel_mmr_size,
            "Kernel MMR size at height {} should match",
            height
        );
        assert_eq!(
            header1.header().output_smt_size,
            header2.header().output_smt_size,
            "Output SMT size at height {} should match",
            height
        );
    }

    // Verify all blocks match
    for height in 0..=tip1.height() {
        let block1 = db1.fetch_block(height, true).unwrap();
        let block2 = db2.fetch_block(height, true).unwrap();
        assert_eq!(
            block1.block().hash(),
            block2.block().hash(),
            "Blocks at height {} should have same hash",
            height
        );
        assert_eq!(
            block1.block().body.outputs().len(),
            block2.block().body.outputs().len(),
            "Block {} should have same number of outputs",
            height
        );
        assert_eq!(
            block1.block().body.kernels().len(),
            block2.block().body.kernels().len(),
            "Block {} should have same number of kernels",
            height
        );
    }
}

#[test]
fn lmdb_read_queries_test() {
    let (db, _blocks) = build_test_chain();
    let tip = db.fetch_tip_header().unwrap();
    let tip_height = tip.height();

    assert_eq!(tip_height, 10, "Chain should have 10 blocks after genesis");

    // --- Verify all chain headers can be fetched ---
    for height in 0..=tip_height {
        let chain_header = db.fetch_chain_header(height).unwrap();
        assert_eq!(chain_header.height(), height);
    }

    // --- Per-block queries on the active chain (skip genesis which has no stored kernels/outputs) ---
    for height in 1..=tip_height {
        let chain_header = db.fetch_chain_header(height).unwrap();
        let header_hash = *chain_header.hash();

        // Fetch kernels in block
        let kernels = db.fetch_kernels_in_block(header_hash).unwrap();
        assert!(
            !kernels.is_empty(),
            "Block at height {} should have at least one kernel (coinbase)",
            height
        );

        // Fetch outputs in block
        let outputs = db.fetch_outputs_in_block(header_hash).unwrap();
        assert!(
            !outputs.is_empty(),
            "Block at height {} should have at least one output (coinbase)",
            height
        );

        // Fetch outputs with spend state
        let outputs_with_state = db
            .fetch_outputs_in_block_with_spend_state(header_hash, Some(header_hash))
            .unwrap();
        assert_eq!(
            outputs_with_state.len(),
            outputs.len(),
            "Output count with spend state should match output count at height {}",
            height
        );

        // Verify output by hash
        for output in &outputs {
            let output_hash = output.hash();
            let fetched = db.fetch_output(output_hash).unwrap();
            assert!(
                fetched.is_some(),
                "Output with hash {} at height {} should be fetchable",
                output_hash,
                height
            );
            let mined_info = fetched.unwrap();
            assert_eq!(
                mined_info.header_hash, header_hash,
                "Output header hash should match at height {}",
                height
            );
        }

        // Verify unspent output by commitment (for coinbase outputs at the tip which are unspent)
        if height == tip_height {
            for output in &outputs {
                let commitment = output.commitment().clone();
                let result = db.fetch_unspent_output_hash_by_commitment(commitment);
                assert!(
                    result.is_ok(),
                    "Should be able to query unspent output by commitment at tip height"
                );
            }
        }

        // Verify kernel by excess sig
        for kernel in &kernels {
            let excess_sig = kernel.excess_sig.clone();
            let fetched = db.fetch_kernel_by_excess_sig(excess_sig).unwrap();
            assert!(
                fetched.is_some(),
                "Kernel should be fetchable by excess sig at height {}",
                height
            );
            let (fetched_kernel, fetched_header_hash) = fetched.unwrap();
            assert_eq!(
                fetched_header_hash, header_hash,
                "Kernel header hash should match at height {}",
                height
            );
            assert_eq!(
                fetched_kernel.excess_sig, kernel.excess_sig,
                "Fetched kernel should match at height {}",
                height
            );
        }

        // Verify header containing kernel by MMR position
        if height > 0 {
            let prev_header = db.fetch_chain_header(height - 1).unwrap();
            let kernel_pos = prev_header.header().kernel_mmr_size;
            let header_at_pos = db.fetch_header_containing_kernel_mmr(kernel_pos).unwrap();
            assert_eq!(
                header_at_pos.height(),
                height,
                "Header containing kernel MMR position {} should be at height {}",
                kernel_pos,
                height
            );
        }
    }

    // --- Verify fetch_inputs_in_block ---
    for height in 0..=tip_height {
        let chain_header = db.fetch_chain_header(height).unwrap();
        let header_hash = *chain_header.hash();
        // Coinbase-only blocks have no inputs; just verify the method doesn't error
        let _inputs = db.fetch_inputs_in_block(header_hash).unwrap();
    }
}

#[test]
fn lmdb_json_reproducibility_test() {
    // Build chain and serialize to JSON
    let (_db, blocks) = build_test_chain();
    let chain_data = serialize_chain(&blocks);

    // Serialize to JSON string and back
    let json = serde_json::to_string(&chain_data).unwrap();
    let restored: TestChainData = serde_json::from_str(&json).unwrap();

    // Verify all block metadata survived serialization
    assert_eq!(chain_data.blocks.len(), restored.blocks.len());
    for (orig, rest) in chain_data.blocks.iter().zip(restored.blocks.iter()) {
        assert_eq!(orig.height, rest.height);
        assert_eq!(orig.hash, rest.hash);
    }

    // Verify deserialized blocks produce the same hashes when reconstructed
    for (i, sb) in restored.blocks.iter().enumerate() {
        let block: Block = serde_json::from_value(sb.block_json.clone()).unwrap();
        assert_eq!(
            block.hash().to_string(),
            sb.hash,
            "Block {} hash mismatch after JSON round-trip",
            i
        );
    }

    // Rebuild the DB from JSON and verify it matches the original
    let db2 = rebuild_chain_from_json(&restored);
    let tip = db2.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 10, "Rebuilt chain should have height 10");

    // Verify every block in the rebuilt DB
    for height in 0..=10 {
        let block = db2.fetch_block(height, true).unwrap();
        assert_eq!(
            block.block().hash().to_string(),
            chain_data.blocks[height as usize].hash,
            "Block at height {} should match original hash after rebuild",
            height
        );
    }
}

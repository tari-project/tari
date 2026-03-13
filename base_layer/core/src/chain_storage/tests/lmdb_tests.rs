// Copyright 2025. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following disclaimer.
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

//! LMDB blockchain unit tests.
//!
//! These tests verify that the LMDB backend correctly stores and retrieves blockchain data across two test scenarios:
//!   1. `test_lmdb_write` – creates a test blockchain, serialises all blocks to JSON, replays them into a fresh LMDB
//!      database, and verifies that both databases contain identical chain data.
//!   2. `test_lmdb_read`  – creates a test blockchain (main chain + fork that causes a reorg) and exhaustively
//!      exercises every read operation defined in the `BlockchainBackend` trait.
//!
//! # Test-chain layout
//!
//! ```text
//! Height:   0    1    2    3    4    5    6    7    8    9   10   11   12   13   14   15
//!          GB – B1 – B2 – B3 – B4 – B5 – B6 – B7 – B8 – B9 – B10   (original main chain)
//!                              └──── F6 – F7 – F8 – F9 – F10– F11– F12– F13– F14– F15
//!                                    (fork from B5; causes reorg when F15 is added)
//! ```
//!
//! After the reorg:
//!   - Main chain  : GB, B1, B2, B3, B4, B5, F6 … F15  (16 blocks, heights 0–15)
//!   - Orphan pool : B6, B7, B8, B9, B10               (5 blocks, heights 6–10)

#![allow(clippy::indexing_slicing)]

use std::sync::Arc;

use jmt::{JellyfishMerkleTree, mock::MockTreeStore, storage::TreeWriter};
use serde::{Deserialize, Serialize};
use tari_common_types::{
    payment_reference::generate_payment_reference,
    types::{FixedHash, HashOutput},
};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    tari_amount::uT,
    tari_proof_of_work::Difficulty,
    test_helpers::schema_to_transaction,
    transaction_components::WalletOutput,
    txn_schema,
};

use crate::{
    chain_storage::{BlockchainBackend, BlockchainDatabase, MmrTree},
    test_helpers::{
        BlockSpec,
        blockchain::{TempDatabase, TestBlockchain, create_new_blockchain, update_block_and_smt},
    },
};

// ──────────────────────────────────────────────────────────────────────────────
// Data structures
// ──────────────────────────────────────────────────────────────────────────────

/// All data that is needed to replay the test chain into a fresh LMDB and to
/// drive the read-operations tests.
#[derive(Serialize, Deserialize)]
struct TestChainData {
    /// Blocks added to the main chain, heights 1–10 (genesis is re-derived from
    /// the consensus rules and is therefore not stored here).
    main_chain_blocks: Vec<Block>,
    /// Wallet outputs (one coinbase per main-chain block, index 0 == B1).
    main_chain_coinbases: Vec<WalletOutput>,
    /// Blocks added to the fork chain, heights 6–15 (fork from B5).
    fork_chain_blocks: Vec<Block>,
    /// Wallet outputs for the fork chain coinbases (index 0 == F6 coinbase).
    fork_chain_coinbases: Vec<WalletOutput>,
    /// Hashes of the blocks that were removed from the main chain during the
    /// reorg (the original B6–B10).
    reorged_block_hashes: Vec<FixedHash>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Test-chain builder
// ──────────────────────────────────────────────────────────────────────────────

/// Creates the test blockchain described in the module-level doc-comment and
/// returns both the populated database and a snapshot of every block/output.
///
/// Main chain blocks B2–B5 contain spending transactions (each spending the
/// previous block's coinbase).  Fork chain blocks F6–F15 contain coinbase
/// outputs only.
fn build_test_blockchain() -> (BlockchainDatabase<TempDatabase>, TestChainData) {
    // `create_new_blockchain` uses LocalNet rules with MockValidators, so no
    // coinbase-maturity or script checks are performed.
    let db = create_new_blockchain();
    let rules = db.rules().clone();
    let mut chain = TestBlockchain::new(db, rules);

    // ── Main chain – B1 through B10 ──────────────────────────────────────────

    let (b1, cb1) = chain
        .add_block(BlockSpec::builder().with_name("B1->GB").finish())
        .unwrap();

    let (tx_b2, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb1.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b2, cb2) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B2->B1")
                .with_transactions(tx_b2.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b3, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb2.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b3, cb3) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B3->B2")
                .with_transactions(tx_b3.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b4, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb3.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b4, cb4) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B4->B3")
                .with_transactions(tx_b4.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b5, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb4.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b5, cb5) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B5->B4")
                .with_transactions(tx_b5.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b6, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb5.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b6, cb6) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B6->B5")
                .with_transactions(tx_b6.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b7, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb6.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b7, cb7) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B7->B6")
                .with_transactions(tx_b7.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b8, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb7.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b8, cb8) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B8->B7")
                .with_transactions(tx_b8.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b9, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb8.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b9, cb9) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B9->B8")
                .with_transactions(tx_b9.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    let (tx_b10, _) =
        schema_to_transaction(&[txn_schema!(from: vec![cb9.clone()], to: vec![5_000 * uT])], &chain.km);
    let (b10, cb10) = chain
        .add_block(
            BlockSpec::builder()
                .with_name("B10->B9")
                .with_transactions(tx_b10.into_iter().map(|t| (*t).clone()).collect())
                .finish(),
        )
        .unwrap();

    // Hashes of the blocks that will become orphans after the reorg.
    let reorged_block_hashes: Vec<FixedHash> = [&b6, &b7, &b8, &b9, &b10]
        .iter()
        .map(|b| *b.hash())
        .collect();

    // ── Fork chain – F6 through F15 (parent = B5) ────────────────────────────
    //
    // Fork blocks contain only coinbase outputs.  The output Merkle root (SMT
    // root) must be computed correctly for the blocks to be accepted during the
    // chain reorg.  We do this using a separate MockTreeStore that is
    // initialised with the UTXO set at height 5 (block B5).

    use crate::chain_storage::SmtHasher;

    let mock_store = MockTreeStore::new(true);
    let jmt = JellyfishMerkleTree::<_, SmtHasher>::new(&mock_store);

    // Replay blocks 0–5 into the mock JMT to capture the UTXO set at B5.
    for h in 0..=5u64 {
        let h_block = chain.db().fetch_block(h, false).unwrap();
        let mut block_clone = h_block.block().clone();
        let updates = update_block_and_smt(&mut block_clone, &jmt);
        mock_store.write_node_batch(&updates.node_batch).unwrap();
    }

    // Helper macro: build a coinbase-only fork block, set its correct SMT
    // root, mine it, and return the block + coinbase wallet output.
    macro_rules! build_fork_block {
        ($spec:expr, $parent:expr) => {{
            let (mut block, coinbase) = chain.create_unmined_block($spec);
            let updates = update_block_and_smt(&mut block, &jmt);
            mock_store.write_node_batch(&updates.node_batch).unwrap();
            let arc_block = chain.mine_block($parent, block, Difficulty::min());
            (arc_block, coinbase)
        }};
    }

    let (f6, fcb6) = build_fork_block!(BlockSpec::builder().with_name("F6->B5").finish(), "B5");
    let f6_result = chain.append_block("F6", f6.clone()).unwrap();
    assert!(f6_result.is_orphaned(), "F6 should be an orphan");

    let (f7, fcb7) = build_fork_block!(BlockSpec::builder().with_name("F7->F6").finish(), "F6");
    let f7_result = chain.append_block("F7", f7.clone()).unwrap();
    assert!(f7_result.is_orphaned(), "F7 should be an orphan");

    let (f8, fcb8) = build_fork_block!(BlockSpec::builder().with_name("F8->F7").finish(), "F7");
    let f8_result = chain.append_block("F8", f8.clone()).unwrap();
    assert!(f8_result.is_orphaned(), "F8 should be an orphan");

    let (f9, fcb9) = build_fork_block!(BlockSpec::builder().with_name("F9->F8").finish(), "F8");
    let f9_result = chain.append_block("F9", f9.clone()).unwrap();
    assert!(f9_result.is_orphaned(), "F9 should be an orphan");

    let (f10, fcb10) = build_fork_block!(BlockSpec::builder().with_name("F10->F9").finish(), "F9");
    let f10_result = chain.append_block("F10", f10.clone()).unwrap();
    assert!(f10_result.is_orphaned(), "F10 should be an orphan");

    let (f11, fcb11) = build_fork_block!(BlockSpec::builder().with_name("F11->F10").finish(), "F10");
    let f11_result = chain.append_block("F11", f11.clone()).unwrap();
    assert!(f11_result.is_orphaned(), "F11 should be an orphan");

    let (f12, fcb12) = build_fork_block!(BlockSpec::builder().with_name("F12->F11").finish(), "F11");
    let f12_result = chain.append_block("F12", f12.clone()).unwrap();
    assert!(f12_result.is_orphaned(), "F12 should be an orphan");

    let (f13, fcb13) = build_fork_block!(BlockSpec::builder().with_name("F13->F12").finish(), "F12");
    let f13_result = chain.append_block("F13", f13.clone()).unwrap();
    assert!(f13_result.is_orphaned(), "F13 should be an orphan");

    let (f14, fcb14) = build_fork_block!(BlockSpec::builder().with_name("F14->F13").finish(), "F13");
    let f14_result = chain.append_block("F14", f14.clone()).unwrap();
    assert!(f14_result.is_orphaned(), "F14 should be an orphan");

    // F15: adding this block makes the fork chain longer than the original main
    // chain (height 15 > 10), triggering a reorg.
    let (f15, fcb15) = build_fork_block!(BlockSpec::builder().with_name("F15->F14").finish(), "F14");
    let f15_result = chain.append_block("F15", f15.clone()).unwrap();
    // F15 should trigger a reorg: 10 fork blocks added, 5 main-chain blocks removed.
    f15_result.assert_reorg(10, 5);

    let chain_data = TestChainData {
        main_chain_blocks: vec![
            b1.block().clone(),
            b2.block().clone(),
            b3.block().clone(),
            b4.block().clone(),
            b5.block().clone(),
            b6.block().clone(),
            b7.block().clone(),
            b8.block().clone(),
            b9.block().clone(),
            b10.block().clone(),
        ],
        main_chain_coinbases: vec![cb1, cb2, cb3, cb4, cb5, cb6, cb7, cb8, cb9, cb10],
        fork_chain_blocks: vec![
            f6.block().clone(),
            f7.block().clone(),
            f8.block().clone(),
            f9.block().clone(),
            f10.block().clone(),
            f11.block().clone(),
            f12.block().clone(),
            f13.block().clone(),
            f14.block().clone(),
            f15.block().clone(),
        ],
        fork_chain_coinbases: vec![
            fcb6, fcb7, fcb8, fcb9, fcb10, fcb11, fcb12, fcb13, fcb14, fcb15,
        ],
        reorged_block_hashes,
    };

    let db = chain.db().clone();
    (db, chain_data)
}

// ──────────────────────────────────────────────────────────────────────────────
// Helper: replay serialised blocks into a fresh LMDB
// ──────────────────────────────────────────────────────────────────────────────

/// Creates a brand-new `BlockchainDatabase<TempDatabase>` with the same
/// genesis block as the original and re-adds every block from `chain_data` in
/// the same order (main chain first, fork chain second).
fn replay_chain_data(chain_data: &TestChainData) -> BlockchainDatabase<TempDatabase> {
    let db = create_new_blockchain();

    // Replay main chain (B1–B10).
    for block in &chain_data.main_chain_blocks {
        let result = db.add_block(Arc::new(block.clone())).unwrap();
        assert!(
            result.was_chain_modified(),
            "Main-chain block at height {} was not added: {result}",
            block.header.height
        );
    }

    // Replay fork chain (F6–F14 as orphans, F15 triggers the reorg).
    for (i, block) in chain_data.fork_chain_blocks.iter().enumerate() {
        let result = db.add_block(Arc::new(block.clone())).unwrap();
        if i < 9 {
            // F6–F14 should be orphans
            assert!(
                result.is_orphaned(),
                "Fork block at index {i} (height {}) should be an orphan, got: {result}",
                block.header.height
            );
        } else {
            // F15 triggers the reorg
            result.assert_reorg(10, 5);
        }
    }

    db
}

// ──────────────────────────────────────────────────────────────────────────────
// Test 1 – write
// ──────────────────────────────────────────────────────────────────────────────

/// Verifies that serialising the blockchain to JSON and replaying it into a
/// fresh LMDB produces a database with identical content to the original.
///
/// Checks performed:
///   - All 16 main-chain block hashes match (heights 0–15).
///   - The 5 reorged blocks are present in the orphan pool of both databases.
///   - The UTXO count and kernel MMR size match between the two databases.
#[test]
fn test_lmdb_write() {
    let (original_db, chain_data) = build_test_blockchain();

    // Round-trip through JSON to ensure the replay uses identical Block objects.
    let json = serde_json::to_string(&chain_data).expect("serialise chain data to JSON");
    let chain_data_from_json: TestChainData =
        serde_json::from_str(&json).expect("deserialise chain data from JSON");

    let replayed_db = replay_chain_data(&chain_data_from_json);

    // ── Compare chain tip ─────────────────────────────────────────────────────
    let original_tip = original_db.get_height().unwrap();
    let replayed_tip = replayed_db.get_height().unwrap();
    assert_eq!(original_tip, replayed_tip, "chain tips differ");
    assert_eq!(original_tip, 15, "expected main chain tip at height 15");

    // ── Compare main-chain block hashes (heights 0–15) ────────────────────────
    for height in 0..=15u64 {
        let orig_header = original_db.fetch_chain_header(height).unwrap();
        let rep_header = replayed_db.fetch_chain_header(height).unwrap();
        assert_eq!(
            orig_header.hash(),
            rep_header.hash(),
            "block hash at height {height} differs between original and replayed DB"
        );
    }

    // ── Compare orphan pool ───────────────────────────────────────────────────
    let orig_orphans = original_db.fetch_all_orphans().unwrap();
    let rep_orphans = replayed_db.fetch_all_orphans().unwrap();
    assert_eq!(orig_orphans.len(), rep_orphans.len(), "orphan count differs");

    let mut orig_orphan_hashes: Vec<HashOutput> = orig_orphans.iter().map(|h| *h.hash()).collect();
    let mut rep_orphan_hashes: Vec<HashOutput> = rep_orphans.iter().map(|h| *h.hash()).collect();
    orig_orphan_hashes.sort();
    rep_orphan_hashes.sort();
    assert_eq!(
        orig_orphan_hashes, rep_orphan_hashes,
        "orphan hashes differ between original and replayed DB"
    );

    // ── Compare stored metric counts ─────────────────────────────────────────
    let orig_utxo = original_db.utxo_count().unwrap();
    let rep_utxo = replayed_db.utxo_count().unwrap();
    assert_eq!(orig_utxo, rep_utxo, "UTXO count differs");

    let orig_kernel_size = {
        let db = original_db.db_read_access().unwrap();
        db.fetch_mmr_size(MmrTree::Kernel).unwrap()
    };
    let rep_kernel_size = {
        let db = replayed_db.db_read_access().unwrap();
        db.fetch_mmr_size(MmrTree::Kernel).unwrap()
    };
    assert_eq!(orig_kernel_size, rep_kernel_size, "kernel MMR size differs");
}

// ──────────────────────────────────────────────────────────────────────────────
// Test 2 – read
// ──────────────────────────────────────────────────────────────────────────────

/// Exhaustively exercises every read operation required by the issue:
///
/// - Fetch all 15 main-chain headers (heights 1–15, not counting genesis)
/// - Fetch all 5 reorged headers
/// - Fetch all 15 main-chain blocks (heights 1–15)
/// - Fetch accumulated data for each main-chain block and each orphan
/// - Fetch all 5 reorged blocks
/// - For each main-chain block:
///     - `fetch_outputs_in_block`
///     - `fetch_inputs_in_block`
///     - `fetch_kernels_in_block`
///     - `fetch_output` (by hash)
///     - `fetch_unspent_output_hash_by_commitment`
///     - `fetch_mined_info_by_payref`
///     - `fetch_outputs_in_block_with_spend_state`
///     - `fetch_kernel_by_excess_sig`
///     - `fetch_header_containing_kernel_mmr`
#[test]
fn test_lmdb_read() {
    let (db, chain_data) = build_test_blockchain();

    // ── Fetch all 15 main-chain headers (heights 1–15) ────────────────────────
    for height in 1u64..=15 {
        let header = db
            .fetch_chain_header(height)
            .unwrap_or_else(|e| panic!("fetch_chain_header({height}) failed: {e}"));
        assert_eq!(header.height(), height);
    }

    // ── Fetch all 5 reorged (orphan) headers ──────────────────────────────────
    assert_eq!(
        chain_data.reorged_block_hashes.len(),
        5,
        "expected exactly 5 reorged blocks"
    );
    {
        let db_read = db.db_read_access().unwrap();
        for hash in &chain_data.reorged_block_hashes {
            let header = db_read
                .fetch_chain_header_in_all_chains(hash)
                .unwrap_or_else(|e| panic!("fetch_chain_header_in_all_chains({hash}) failed: {e}"));
            assert!(
                (6..=10).contains(&header.height()),
                "reorged block height {} not in range 6–10",
                header.height()
            );
        }
    }

    // ── Fetch all 15 main-chain blocks (heights 1–15) ─────────────────────────
    for height in 1u64..=15 {
        let block = db
            .fetch_block(height, true)
            .unwrap_or_else(|e| panic!("fetch_block({height}) failed: {e}"));
        assert_eq!(block.header().height, height);
    }

    // ── Fetch accumulated data for each main-chain block ──────────────────────
    for height in 0u64..=15 {
        let block = db.fetch_block(height, false).unwrap();
        db.fetch_block_accumulated_data(*block.hash())
            .unwrap_or_else(|e| panic!("fetch_block_accumulated_data at height {height} failed: {e}"));
    }

    // ── Fetch accumulated data for each orphan block ──────────────────────────
    {
        let db_read = db.db_read_access().unwrap();
        for hash in &chain_data.reorged_block_hashes {
            db_read
                .fetch_header_accumulated_data(hash)
                .unwrap_or_else(|e| panic!("fetch_header_accumulated_data({hash}) failed: {e}"));
        }
    }

    // ── Fetch all 5 reorged blocks (from orphan pool) ─────────────────────────
    for hash in &chain_data.reorged_block_hashes {
        db.fetch_orphan(*hash)
            .unwrap_or_else(|e| panic!("fetch_orphan({hash}) failed: {e}"));
    }

    // ── Per-block operations on the main chain (heights 0–15) ─────────────────
    let all_main_chain_blocks: Vec<_> = (0u64..=15)
        .map(|h| db.fetch_block(h, true).unwrap())
        .collect();

    let db_read = db.db_read_access().unwrap();

    for historical_block in &all_main_chain_blocks {
        let header = historical_block.header();
        let header_hash: HashOutput = header.hash();
        let height = header.height;
        let block_body = historical_block.block().body.clone();

        // ── fetch_outputs_in_block ────────────────────────────────────────────
        let outputs = db_read
            .fetch_outputs_in_block(&header_hash)
            .unwrap_or_else(|e| panic!("fetch_outputs_in_block at height {height} failed: {e}"));
        assert_eq!(
            outputs.len(),
            block_body.outputs().len(),
            "output count mismatch at height {height}"
        );

        // ── fetch_inputs_in_block ─────────────────────────────────────────────
        let inputs = db_read
            .fetch_inputs_in_block(&header_hash)
            .unwrap_or_else(|e| panic!("fetch_inputs_in_block at height {height} failed: {e}"));
        assert_eq!(
            inputs.len(),
            block_body.inputs().len(),
            "input count mismatch at height {height}"
        );

        // ── fetch_kernels_in_block ────────────────────────────────────────────
        let kernels = db_read
            .fetch_kernels_in_block(&header_hash)
            .unwrap_or_else(|e| panic!("fetch_kernels_in_block at height {height} failed: {e}"));
        assert_eq!(
            kernels.len(),
            block_body.kernels().len(),
            "kernel count mismatch at height {height}"
        );

        // Per-output assertions
        for output in block_body.outputs() {
            let output_hash: HashOutput = output.hash();

            // ── fetch_output (by hash) ────────────────────────────────────────
            let fetched = db_read
                .fetch_output(&output_hash)
                .unwrap_or_else(|e| panic!("fetch_output at height {height} failed: {e}"))
                .unwrap_or_else(|| panic!("fetch_output returned None for output at height {height}"));
            assert_eq!(fetched.output.hash(), output_hash);

            // ── fetch_mined_info_by_payref ────────────────────────────────────
            let payref = generate_payment_reference(&header_hash, &output_hash);
            let mined_info = db_read
                .fetch_mined_info_by_payref(&payref)
                .unwrap_or_else(|e| panic!("fetch_mined_info_by_payref at height {height} failed: {e}"));
            assert!(
                mined_info.output.is_some(),
                "fetch_mined_info_by_payref returned no output info at height {height}"
            );
        }

        // ── fetch_outputs_in_block_with_spend_state ───────────────────────────
        let outputs_with_state = db_read
            .fetch_outputs_in_block_with_spend_state(&header_hash, Some(&header_hash))
            .unwrap_or_else(|e| {
                panic!("fetch_outputs_in_block_with_spend_state at height {height} failed: {e}")
            });
        assert_eq!(
            outputs_with_state.len(),
            block_body.outputs().len(),
            "outputs_with_state count mismatch at height {height}"
        );

        // Per-kernel assertions
        for kernel in block_body.kernels() {
            // ── fetch_kernel_by_excess_sig ────────────────────────────────────
            let excess_sig = kernel.excess_sig.clone();
            let result = db_read
                .fetch_kernel_by_excess_sig(&excess_sig)
                .unwrap_or_else(|e| panic!("fetch_kernel_by_excess_sig at height {height} failed: {e}"));
            assert!(
                result.is_some(),
                "fetch_kernel_by_excess_sig returned None at height {height}"
            );
            let (fetched_kernel, _) = result.unwrap();
            assert_eq!(fetched_kernel.excess_sig, excess_sig);
        }
    }

    // ── fetch_header_containing_kernel_mmr ────────────────────────────────────
    let genesis_block = db.fetch_block(0, true).unwrap();
    let mut mmr_pos = genesis_block.block().body.kernels().len() as u64;

    for height in 1u64..=15 {
        let block = db.fetch_block(height, true).unwrap();
        let num_kernels = block.block().body.kernels().len() as u64;
        for i in 0..num_kernels {
            let header = db
                .fetch_header_containing_kernel_mmr(mmr_pos + i)
                .unwrap_or_else(|e| {
                    panic!(
                        "fetch_header_containing_kernel_mmr(mmr_pos={}) at height {height} failed: {e}",
                        mmr_pos + i
                    )
                });
            assert_eq!(
                header.height(),
                height,
                "kernel at mmr_pos {} expected height {height} but got {}",
                mmr_pos + i,
                header.height()
            );
        }
        mmr_pos += num_kernels;
    }

    // ── fetch_unspent_output_hash_by_commitment ────────────────────────────────
    //
    // After the reorg the last fork coinbase (F15, index 9) has never been
    // spent, so its commitment must still be in the UTXO set.
    let fcb15 = &chain_data.fork_chain_coinbases[9];
    let commitment = fcb15.commitment().clone();
    let unspent_hash = db
        .fetch_unspent_output_hash_by_commitment(commitment)
        .unwrap_or_else(|e| panic!("fetch_unspent_output_hash_by_commitment for F15 coinbase failed: {e}"));
    assert!(
        unspent_hash.is_some(),
        "F15 coinbase commitment should be unspent after reorg"
    );
    assert_eq!(
        unspent_hash.unwrap(),
        fcb15.output_hash(),
        "unspent hash for F15 coinbase does not match expected output hash"
    );

    // cb5 was spent by B6 on the original main chain.  After the reorg B6 was
    // reverted, so cb5 should once again appear as unspent.
    let cb5 = &chain_data.main_chain_coinbases[4]; // index 4 = B5's coinbase
    let cb5_commitment = cb5.commitment().clone();
    let cb5_unspent = db
        .fetch_unspent_output_hash_by_commitment(cb5_commitment)
        .unwrap_or_else(|e| panic!("fetch_unspent_output_hash_by_commitment for cb5 failed: {e}"));
    assert!(
        cb5_unspent.is_some(),
        "cb5 should be unspent after the reorg (B6 was reverted)"
    );
}

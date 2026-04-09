// Copyright 2024. The Tari Project
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

//! LMDB unit tests that exercise the `BlockchainBackend` read methods against a realistic chain
//! containing a fork and a reorg.
//!
//! ## Chain layout
//!
//! The test constructs the following blockchain topology:
//!
//! ```text
//!   Genesis -> B1 -> B2 -> B3 -> B4 -> B5 -> B6  -> B7  -> B8  -> B9  -> B10  (original main chain)
//!                                        \-> F6' -> F7' -> F8' -> F9' -> F10' -> F11' -> ... -> F15'
//! ```
//!
//! The fork branches from block 5 and extends to height 15 (10 fork blocks), which is longer than
//! the original main chain (height 10). When the fork blocks are added, the database triggers a
//! reorg. After the reorg the canonical chain is:
//!
//! ```text
//!   Genesis -> B1 -> B2 -> B3 -> B4 -> B5 -> F6' -> F7' -> F8' -> F9' -> F10' -> F11' -> ... -> F15'
//! ```
//!
//! The five original blocks B6..B10 become reorged (removed) blocks.
//!
//! Each `read_tests` sub-module targets a specific `BlockchainBackend` query method and verifies
//! that it returns correct data for blocks on the canonical chain, fork blocks, and (where
//! applicable) the reorged blocks.

#![allow(clippy::indexing_slicing)]
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tari_common_types::{
    payment_reference::generate_payment_reference,
    tari_address::TariAddress,
    types::{CompressedSignature, FixedHash},
};
use tari_node_components::blocks::{Block, BlockHeader};
use tari_transaction_components::{
    key_manager::{KeyManager, TariKeyId},
    transaction_components::{Transaction, TransactionInput, TransactionKernel, TransactionOutput, WalletOutput},
};

use crate::{
    chain_storage::BlockchainDatabase,
    test_helpers::{
        BlockSpec,
        blockchain::{TempDatabase, create_new_blockchain},
        create_block,
        default_coinbase_entities,
    },
};

// ---------------------------------------------------------------------------
// JSON serialization types
// ---------------------------------------------------------------------------

/// Holds all the serializable data about the test chain, making it possible to snapshot the chain
/// state to a JSON file for reproducibility and offline inspection.
#[derive(Debug, Serialize, Deserialize)]
struct TestChainSnapshot {
    /// Blocks on the canonical chain after the reorg (genesis through fork tip).
    canonical_blocks: Vec<SerializableBlock>,
    /// The original main-chain blocks that were removed by the reorg (B6..B10).
    reorged_blocks: Vec<SerializableBlock>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SerializableBlock {
    height: u64,
    hash: FixedHash,
    header: BlockHeader,
    outputs: Vec<TransactionOutput>,
    inputs: Vec<TransactionInput>,
    kernels: Vec<TransactionKernel>,
}

impl SerializableBlock {
    fn from_block(block: &Block) -> Self {
        Self {
            height: block.header.height,
            hash: block.hash(),
            header: block.header.clone(),
            outputs: block.body.outputs().clone(),
            inputs: block.body.inputs().clone(),
            kernels: block.body.kernels().clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Test chain builder helpers
// ---------------------------------------------------------------------------

/// Outcome of building the test chain. Contains the database and references to blocks that tests
/// can use for assertions.
struct TestChain {
    /// The blockchain database with all blocks applied (post-reorg state).
    db: BlockchainDatabase<TempDatabase>,
    /// Canonical chain blocks in height order, starting from genesis (index 0).
    /// After the reorg this is: [genesis, B1..B5, F6'..F15'].
    canonical_blocks: Vec<Arc<Block>>,
    /// Coinbase wallet outputs for canonical chain blocks (index 0 = genesis coinbase, etc.).
    canonical_outputs: Vec<WalletOutput>,
    /// The original main-chain blocks that were removed during the reorg (B6..B10).
    reorged_blocks: Vec<Arc<Block>>,
}

fn apply_mmr_to_block(db: &BlockchainDatabase<TempDatabase>, block: Block) -> Block {
    let (mut block, mmr_roots) = db.calculate_mmr_roots(block).unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;
    block
}

fn create_next_block(
    db: &BlockchainDatabase<TempDatabase>,
    prev_block: &Block,
    transactions: Vec<Arc<Transaction>>,
    key_manager: &KeyManager,
    script_key_id: &TariKeyId,
    wallet_payment_address: &TariAddress,
) -> (Arc<Block>, WalletOutput) {
    let rules = db.rules();
    let (block, output) = create_block(
        db,
        rules,
        prev_block,
        BlockSpec::new()
            .with_transactions(transactions.into_iter().map(|t| (*t).clone()).collect())
            .finish(),
        key_manager,
        script_key_id,
        wallet_payment_address,
        None,
    );
    let block = apply_mmr_to_block(db, block);
    (Arc::new(block), output)
}

/// Append `count` blocks to the tip of `db`, returning the blocks and their coinbase outputs.
fn add_chained_blocks(
    count: usize,
    db: &BlockchainDatabase<TempDatabase>,
    key_manager: &KeyManager,
) -> (Vec<Arc<Block>>, Vec<WalletOutput>) {
    let last_header = db.fetch_last_header().unwrap();
    let mut prev_block = Arc::new(db.fetch_block(last_header.height, true).unwrap().into_block());
    let mut blocks = Vec::with_capacity(count);
    let mut outputs = Vec::with_capacity(count);
    let (script_key_id, wallet_payment_address) = default_coinbase_entities(key_manager);
    for _ in 0..count {
        let (block, coinbase) =
            create_next_block(db, &prev_block, vec![], key_manager, &script_key_id, &wallet_payment_address);
        db.add_block(block.clone()).unwrap().assert_added();
        prev_block = block.clone();
        blocks.push(block);
        outputs.push(coinbase);
    }
    (blocks, outputs)
}

/// Build the complete test chain (see module-level docs for the topology).
///
/// Returns a `TestChain` with the database in post-reorg state and references to all relevant
/// blocks and outputs.
fn build_test_chain() -> TestChain {
    // --- Main chain: genesis + 10 blocks -------------------------------------------------------
    let db = create_new_blockchain();
    let key_manager = KeyManager::new_random().unwrap();

    let genesis = Arc::new(db.fetch_block(0, true).unwrap().into_block());

    let (main_blocks, main_outputs) = add_chained_blocks(10, &db, &key_manager);

    // Collect blocks 1..5 (indices 0..4 in main_blocks) which will stay in the canonical chain
    // after the reorg, and blocks 6..10 (indices 5..9) which will be reorged out.
    let shared_blocks: Vec<Arc<Block>> = main_blocks[..5].to_vec();
    let reorged_blocks: Vec<Arc<Block>> = main_blocks[5..].to_vec();

    // --- Fork chain: branches from block 5, extends 10 blocks (heights 6-15) ------------------
    // We build the fork on a separate database so that MMR roots are calculated against the fork
    // chain state. Then we add the fork blocks to the main database, triggering a reorg.
    let fork_db = create_new_blockchain();
    let fork_key_manager = KeyManager::new_random().unwrap();

    // Replay the shared prefix (genesis + blocks 1-5) onto the fork database.
    for block in shared_blocks.iter() {
        fork_db.add_block(block.clone()).unwrap().assert_added();
    }

    // Create 10 fork blocks on top of block 5.
    let (fork_blocks, fork_outputs) = add_chained_blocks(10, &fork_db, &fork_key_manager);

    // --- Trigger the reorg by adding fork blocks to the main database --------------------------
    // Fork blocks 1-5 extend from block 5 (which is already in the main db). Because the fork
    // will not immediately be longer, the first blocks go into the orphan pool. Once the fork
    // surpasses the main chain length, a reorg is triggered.
    let mut reorg_happened = false;
    for fork_block in fork_blocks.iter() {
        let result = db.add_block(fork_block.clone()).unwrap();
        if result.is_chain_reorg() {
            reorg_happened = true;
        }
    }
    assert!(reorg_happened, "Expected a chain reorg but it did not happen");

    // Verify the tip is now at the fork tip (height 15).
    let tip = db.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 15, "Tip should be at fork height 15 after reorg");

    // --- Assemble the canonical chain block list -----------------------------------------------
    // Canonical: genesis, B1..B5, F6'..F15'
    let mut canonical_blocks = Vec::with_capacity(16);
    canonical_blocks.push(genesis);
    canonical_blocks.extend(shared_blocks);
    canonical_blocks.extend(fork_blocks.clone());

    // Canonical outputs: we don't have the real genesis WalletOutput, so we store main_outputs
    // for the shared blocks and fork_outputs for the fork blocks.
    let mut canonical_outputs = Vec::new();
    canonical_outputs.extend(main_outputs[..5].to_vec());
    canonical_outputs.extend(fork_outputs);

    TestChain {
        db,
        canonical_blocks,
        canonical_outputs,
        reorged_blocks,
    }
}

/// Serialize the test chain data to a JSON string. This can be written to a file for
/// debugging or snapshot testing.
fn serialize_chain_to_json(chain: &TestChain) -> String {
    let snapshot = TestChainSnapshot {
        canonical_blocks: chain.canonical_blocks.iter().map(|b| SerializableBlock::from_block(b)).collect(),
        reorged_blocks: chain.reorged_blocks.iter().map(|b| SerializableBlock::from_block(b)).collect(),
    };
    serde_json::to_string_pretty(&snapshot).expect("Failed to serialize chain snapshot to JSON")
}

// ---------------------------------------------------------------------------
// Write / chain-construction tests
// ---------------------------------------------------------------------------

mod write_tests {
    use super::*;

    #[test]
    fn test_chain_builds_and_reorgs_correctly() {
        let chain = build_test_chain();

        // The canonical chain should have 16 blocks: genesis + 5 shared + 10 fork.
        assert_eq!(chain.canonical_blocks.len(), 16);

        // The reorged blocks should be the 5 original blocks that were replaced.
        assert_eq!(chain.reorged_blocks.len(), 5);

        // Verify heights of canonical blocks.
        for (i, block) in chain.canonical_blocks.iter().enumerate() {
            assert_eq!(
                block.header.height, i as u64,
                "Canonical block at index {} should have height {}",
                i, i
            );
        }

        // Verify reorged blocks had heights 6..10.
        for (i, block) in chain.reorged_blocks.iter().enumerate() {
            assert_eq!(
                block.header.height,
                (i + 6) as u64,
                "Reorged block at index {} should have height {}",
                i,
                i + 6
            );
        }

        // Verify the tip header matches the last canonical block.
        let tip = chain.db.fetch_tip_header().unwrap();
        assert_eq!(tip.height(), 15);
        assert_eq!(
            *tip.hash(),
            chain.canonical_blocks[15].hash(),
            "Tip hash should match the last canonical (fork) block"
        );
    }

    #[test]
    fn test_chain_serializes_to_json() {
        let chain = build_test_chain();
        let json = serialize_chain_to_json(&chain);

        // Basic sanity: the JSON should be parseable and contain the expected number of blocks.
        let snapshot: TestChainSnapshot = serde_json::from_str(&json).expect("Failed to parse chain JSON");
        assert_eq!(snapshot.canonical_blocks.len(), 16);
        assert_eq!(snapshot.reorged_blocks.len(), 5);
    }
}

// ---------------------------------------------------------------------------
// Read tests
// ---------------------------------------------------------------------------

mod read_tests {

    use super::*;

    // === Outputs ===

    mod outputs {
        use super::*;

        #[test]
        fn fetch_outputs_in_block_returns_correct_outputs_for_canonical_blocks() {
            let chain = build_test_chain();

            // Check every canonical block: the outputs stored in the DB should match the block body.
            for block in chain.canonical_blocks.iter() {
                let header_hash = block.hash();
                let db_outputs = chain
                    .db
                    .fetch_outputs_in_block(header_hash)
                    .unwrap_or_else(|e| panic!("Failed to fetch outputs for block at height {}: {}", block.header.height, e));

                let expected_output_hashes: Vec<FixedHash> =
                    block.body.outputs().iter().map(|o| o.hash()).collect();
                let actual_output_hashes: Vec<FixedHash> =
                    db_outputs.iter().map(|o| o.hash()).collect();

                assert_eq!(
                    expected_output_hashes.len(),
                    actual_output_hashes.len(),
                    "Output count mismatch for block at height {}",
                    block.header.height
                );

                for expected in &expected_output_hashes {
                    assert!(
                        actual_output_hashes.contains(expected),
                        "Missing output {} in block at height {}",
                        expected,
                        block.header.height
                    );
                }
            }
        }

        #[test]
        fn fetch_output_returns_mined_info_for_canonical_outputs() {
            let chain = build_test_chain();

            // Pick a few canonical blocks and verify that individual outputs can be fetched by hash.
            for block in chain.canonical_blocks.iter().skip(1).take(5) {
                for output in block.body.outputs().iter() {
                    let output_hash = output.hash();
                    let mined_info = chain
                        .db
                        .fetch_output(output_hash)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to fetch output {} from block at height {}: {}",
                                output_hash, block.header.height, e
                            )
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "Output {} from block at height {} not found",
                                output_hash, block.header.height
                            )
                        });

                    assert_eq!(
                        mined_info.output.hash(),
                        output_hash,
                        "Fetched output hash mismatch"
                    );
                    assert_eq!(
                        mined_info.mined_height, block.header.height,
                        "Mined height mismatch for output {}",
                        output_hash
                    );
                    assert_eq!(
                        mined_info.header_hash,
                        block.hash(),
                        "Header hash mismatch for output {}",
                        output_hash
                    );
                }
            }
        }

        #[test]
        fn fetch_output_returns_mined_info_for_fork_block_outputs() {
            let chain = build_test_chain();

            // Fork blocks are canonical blocks at indices 6..15 (heights 6..15).
            for block in chain.canonical_blocks.iter().skip(6) {
                for output in block.body.outputs().iter() {
                    let output_hash = output.hash();
                    let mined_info = chain
                        .db
                        .fetch_output(output_hash)
                        .expect("fetch_output should not error for fork block outputs")
                        .unwrap_or_else(|| {
                            panic!(
                                "Output {} from fork block at height {} should exist",
                                output_hash, block.header.height
                            )
                        });

                    assert_eq!(mined_info.mined_height, block.header.height);
                }
            }
        }

        #[test]
        fn fetch_unspent_output_hash_by_commitment_finds_canonical_outputs() {
            let chain = build_test_chain();

            // For each canonical block's outputs (skipping genesis which may have special handling),
            // verify the commitment-based lookup returns the correct output hash.
            for block in chain.canonical_blocks.iter().skip(1) {
                for output in block.body.outputs().iter() {
                    let commitment = output.commitment().clone();
                    let result = chain
                        .db
                        .fetch_unspent_output_hash_by_commitment(commitment.clone())
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to fetch by commitment for output in block {}: {}",
                                block.header.height, e
                            )
                        });

                    // Coinbase outputs that haven't been spent should be findable.
                    // Some outputs may have been spent if they were inputs to later blocks,
                    // so we only assert that unspent ones return the correct hash.
                    if let Some(found_hash) = result {
                        assert_eq!(
                            found_hash,
                            output.hash(),
                            "Commitment lookup returned wrong output hash for block {}",
                            block.header.height
                        );
                    }
                }
            }
        }

        #[test]
        fn fetch_outputs_in_block_with_spend_state_returns_unspent_for_tip() {
            let chain = build_test_chain();
            let tip_hash = chain.canonical_blocks.last().unwrap().hash();

            // For the tip block, all outputs should be unspent (nothing has spent them yet).
            let outputs_with_state = chain
                .db
                .fetch_outputs_in_block_with_spend_state(tip_hash, Some(tip_hash))
                .expect("fetch_outputs_in_block_with_spend_state should succeed for tip");

            assert!(
                !outputs_with_state.is_empty(),
                "Tip block should have at least one output (coinbase)"
            );

            for (output, is_spent) in &outputs_with_state {
                assert!(
                    !is_spent,
                    "Output {} in tip block should be unspent",
                    output.hash()
                );
            }
        }

        #[test]
        fn fetch_outputs_in_block_with_spend_state_no_spend_header() {
            let chain = build_test_chain();

            // When spend_status_at_header is None, we should still get outputs back.
            let block = &chain.canonical_blocks[3];
            let header_hash = block.hash();
            let outputs_with_state = chain
                .db
                .fetch_outputs_in_block_with_spend_state(header_hash, None)
                .expect("fetch_outputs_in_block_with_spend_state should succeed with None spend header");

            assert_eq!(
                outputs_with_state.len(),
                block.body.outputs().len(),
                "Should return all outputs from block at height {}",
                block.header.height
            );
        }
    }

    // === Inputs ===

    mod inputs {
        use super::*;

        #[test]
        fn fetch_inputs_in_block_returns_empty_for_coinbase_only_blocks() {
            let chain = build_test_chain();

            // Block 1 (the first block after genesis) should have no inputs because it only
            // contains a coinbase transaction. Coinbase transactions have no inputs.
            let block = &chain.canonical_blocks[1];
            let inputs = chain
                .db
                .fetch_inputs_in_block(block.hash())
                .expect("fetch_inputs_in_block should succeed");

            // The number of inputs in the db should match the block body.
            assert_eq!(
                inputs.len(),
                block.body.inputs().len(),
                "Input count mismatch for block at height {}",
                block.header.height
            );
        }

        #[test]
        fn fetch_inputs_in_block_matches_block_body_for_all_canonical() {
            let chain = build_test_chain();

            for block in chain.canonical_blocks.iter() {
                let inputs = chain
                    .db
                    .fetch_inputs_in_block(block.hash())
                    .unwrap_or_else(|e| {
                        panic!(
                            "Failed to fetch inputs for block at height {}: {}",
                            block.header.height, e
                        )
                    });

                assert_eq!(
                    inputs.len(),
                    block.body.inputs().len(),
                    "Input count mismatch for block at height {}",
                    block.header.height
                );
            }
        }

        #[test]
        fn fetch_inputs_in_block_returns_empty_for_unknown_hash() {
            let chain = build_test_chain();
            let bogus_hash = FixedHash::zero();
            let inputs = chain.db.fetch_inputs_in_block(bogus_hash).unwrap();
            assert!(inputs.is_empty(), "Should return empty vec for unknown block hash");
        }
    }

    // === Kernels ===

    mod kernels {
        use super::*;

        #[test]
        fn fetch_kernels_in_block_matches_block_body() {
            let chain = build_test_chain();

            for block in chain.canonical_blocks.iter() {
                let kernels = chain
                    .db
                    .fetch_kernels_in_block(block.hash())
                    .unwrap_or_else(|e| {
                        panic!(
                            "Failed to fetch kernels for block at height {}: {}",
                            block.header.height, e
                        )
                    });

                assert_eq!(
                    kernels.len(),
                    block.body.kernels().len(),
                    "Kernel count mismatch for block at height {}",
                    block.header.height
                );

                // Verify each kernel matches by excess signature.
                let expected_sigs: Vec<&CompressedSignature> =
                    block.body.kernels().iter().map(|k| &k.excess_sig).collect();
                for kernel in &kernels {
                    assert!(
                        expected_sigs.contains(&&kernel.excess_sig),
                        "Unexpected kernel signature in block at height {}",
                        block.header.height
                    );
                }
            }
        }

        #[test]
        fn fetch_kernels_in_block_returns_empty_for_unknown_hash() {
            let chain = build_test_chain();
            let bogus_hash = FixedHash::zero();
            let kernels = chain.db.fetch_kernels_in_block(bogus_hash).unwrap();
            assert!(kernels.is_empty(), "Should return empty vec for unknown block hash");
        }

        #[test]
        fn fetch_kernel_by_excess_sig_finds_canonical_kernels() {
            let chain = build_test_chain();

            // For several canonical blocks, look up each kernel by its excess signature.
            for block in chain.canonical_blocks.iter().skip(1).take(8) {
                for kernel in block.body.kernels().iter() {
                    let result = chain
                        .db
                        .fetch_kernel_by_excess_sig(kernel.excess_sig.clone())
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to fetch kernel by excess sig in block at height {}: {}",
                                block.header.height, e
                            )
                        });

                    let (found_kernel, found_block_hash) = result.unwrap_or_else(|| {
                        panic!(
                            "Kernel with excess_sig {:?} from block at height {} not found",
                            kernel.excess_sig, block.header.height
                        )
                    });

                    assert_eq!(
                        found_kernel.excess_sig, kernel.excess_sig,
                        "Excess sig mismatch"
                    );
                    assert_eq!(
                        found_block_hash,
                        block.hash(),
                        "Block hash mismatch for kernel lookup at height {}",
                        block.header.height
                    );
                }
            }
        }

        #[test]
        fn fetch_kernel_by_excess_sig_returns_none_for_unknown_sig() {
            let chain = build_test_chain();
            let bogus_sig = CompressedSignature::default();
            let result = chain.db.fetch_kernel_by_excess_sig(bogus_sig).unwrap();
            assert!(result.is_none(), "Should return None for unknown excess signature");
        }
    }

    // === Headers ===

    mod headers {
        use super::*;

        #[test]
        fn fetch_header_containing_kernel_mmr_returns_correct_header() {
            let chain = build_test_chain();

            // The genesis block contains some kernels. After that each block adds at least one
            // kernel (the coinbase). We verify that the mmr position lookup returns the correct
            // header for kernels in the first few blocks after genesis.
            let genesis = &chain.canonical_blocks[0];
            let num_genesis_kernels = genesis.body.kernels().len() as u64;

            // Kernel mmr positions are 0-indexed. Genesis kernels occupy positions 0..num_genesis_kernels-1.
            // Block 1 kernels start at position num_genesis_kernels.
            if num_genesis_kernels > 0 {
                let header = chain
                    .db
                    .fetch_header_containing_kernel_mmr(0)
                    .expect("Should find header for mmr position 0");
                assert_eq!(
                    header.height(),
                    0,
                    "MMR position 0 should be in the genesis block"
                );
            }

            // Block 1's first kernel is at mmr position = num_genesis_kernels.
            let header = chain
                .db
                .fetch_header_containing_kernel_mmr(num_genesis_kernels)
                .expect("Should find header for block 1's kernel mmr position");
            assert_eq!(
                header.height(),
                1,
                "First kernel after genesis should be in block 1"
            );

            // Verify a kernel position in a fork block (post-reorg). The fork blocks start at
            // height 6. We accumulate kernel counts up to height 5, then check height 6.
            let mut accumulated_kernels = num_genesis_kernels;
            for i in 1..=5 {
                accumulated_kernels += chain.canonical_blocks[i].body.kernels().len() as u64;
            }

            // The first kernel in the fork's block at height 6 should be at `accumulated_kernels`.
            let header = chain
                .db
                .fetch_header_containing_kernel_mmr(accumulated_kernels)
                .expect("Should find header for fork block 6 kernel position");
            assert_eq!(
                header.height(),
                6,
                "Kernel at accumulated position {} should be in block 6",
                accumulated_kernels
            );
        }

        #[test]
        fn fetch_header_containing_kernel_mmr_fails_for_out_of_range() {
            let chain = build_test_chain();

            // A very large mmr position should fail.
            let result = chain.db.fetch_header_containing_kernel_mmr(999_999);
            assert!(
                result.is_err(),
                "Should error for mmr position beyond the chain"
            );
        }
    }

    // === Indices (commitment, payref) ===

    mod indices {
        use super::*;

        #[test]
        fn fetch_mined_info_by_payref_returns_output_for_canonical_blocks() {
            let chain = build_test_chain();

            // For each canonical block (skip genesis due to potential special handling), compute
            // the PayRef for each output and verify the lookup returns the correct output.
            for block in chain.canonical_blocks.iter().skip(1).take(8) {
                let block_hash = block.hash();
                for output in block.body.outputs().iter() {
                    let output_hash = output.hash();
                    let payref = generate_payment_reference(&block_hash, &output_hash);

                    let mined_info = chain
                        .db
                        .fetch_mined_info_by_payref(payref)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to fetch by payref for output in block {}: {}",
                                block.header.height, e
                            )
                        });

                    // The MinedInfo should have the output populated.
                    let output_info = mined_info
                        .output
                        .as_ref()
                        .unwrap_or_else(|| {
                            panic!(
                                "MinedInfo.output should be Some for payref lookup at block {}",
                                block.header.height
                            )
                        });

                    assert_eq!(
                        output_info.output.hash(),
                        output_hash,
                        "PayRef lookup returned wrong output for block {}",
                        block.header.height
                    );
                    assert_eq!(
                        output_info.mined_height, block.header.height,
                        "PayRef lookup returned wrong mined height for block {}",
                        block.header.height
                    );
                }
            }
        }

        #[test]
        fn fetch_mined_info_by_payref_works_for_fork_blocks() {
            let chain = build_test_chain();

            // Verify PayRef lookups work for outputs in the fork blocks (heights 6-15).
            for block in chain.canonical_blocks.iter().skip(6) {
                let block_hash = block.hash();
                for output in block.body.outputs().iter() {
                    let output_hash = output.hash();
                    let payref = generate_payment_reference(&block_hash, &output_hash);

                    let mined_info = chain
                        .db
                        .fetch_mined_info_by_payref(payref)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to fetch by payref for fork block at height {}: {}",
                                block.header.height, e
                            )
                        });

                    assert!(
                        mined_info.output.is_some(),
                        "PayRef lookup should return output for fork block at height {}",
                        block.header.height
                    );
                }
            }
        }

        #[test]
        fn fetch_unspent_output_hash_by_commitment_returns_none_for_unknown() {
            let chain = build_test_chain();

            // A commitment that doesn't exist in the UTXO set should return None.
            use tari_common_types::types::CompressedCommitment;
            let bogus_commitment = CompressedCommitment::default();
            let result = chain
                .db
                .fetch_unspent_output_hash_by_commitment(bogus_commitment)
                .unwrap();
            assert!(
                result.is_none(),
                "Should return None for a commitment not in the UTXO set"
            );
        }

        #[test]
        fn canonical_tip_outputs_are_unspent_via_commitment_lookup() {
            let chain = build_test_chain();

            // Outputs in the very last (tip) block should all be unspent, so the commitment
            // lookup should return their hashes.
            let tip_block = chain.canonical_blocks.last().unwrap();
            for output in tip_block.body.outputs().iter() {
                let commitment = output.commitment().clone();
                let result = chain
                    .db
                    .fetch_unspent_output_hash_by_commitment(commitment)
                    .expect("Commitment lookup should not error for tip output");

                assert_eq!(
                    result,
                    Some(output.hash()),
                    "Tip output should be found as unspent by commitment"
                );
            }
        }
    }
}

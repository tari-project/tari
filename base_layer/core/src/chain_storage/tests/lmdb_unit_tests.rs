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

//! LMDB unit tests that exercise the `BlockchainBackend` read methods against **static, pre-generated**
//! test fixture data.
//!
//! ## Philosophy
//!
//! These tests verify the LMDB read path against statically-defined expected results stored in a
//! JSON fixture file (`test_chain_data.json`). The JSON file contains the complete block data for
//! a test chain along with pre-computed expected query results (output hashes, kernel signatures,
//! commitments, payment references, etc.).
//!
//! At test time the blocks from the JSON file are written to a fresh LMDB database, and then the
//! read tests verify that every query method returns the expected results as defined in the JSON.
//! Because the expected data is statically defined and committed to the repository, bugs in the
//! write path cannot mask bugs in the read path.
//!
//! ## Chain layout (encoded in the JSON fixture)
//!
//! ```text
//!   Genesis -> B1 -> B2 -> B3 -> B4 -> B5 -> B6  -> B7  -> B8  -> B9  -> B10  (original main)
//!                                        \-> F6' -> F7' -> F8' -> F9' -> F10' -> ... -> F15'
//! ```
//!
//! After the reorg the canonical chain is:
//!
//! ```text
//!   Genesis -> B1 -> B2 -> B3 -> B4 -> B5 -> F6' -> F7' -> ... -> F15'
//! ```
//!
//! The five original blocks B6..B10 are stored in the orphan pool.
//!
//! ## Regenerating the JSON fixture
//!
//! Run the ignored `generate_fixtures` test:
//!
//! ```bash
//! cargo test --package tari_core --features sqlite_bundled lmdb_unit_tests::generate_fixtures -- --ignored --nocapture
//! ```

#![allow(clippy::indexing_slicing)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedCommitment, CompressedSignature, FixedHash};
use tari_node_components::blocks::Block;

use crate::{
    chain_storage::BlockchainDatabase,
    test_helpers::blockchain::{TempDatabase, open_blockchain_db_from_path},
};

// ---------------------------------------------------------------------------
// JSON data model for the test fixtures
// ---------------------------------------------------------------------------

/// Serialised representation of the entire test chain and the expected query results.
#[derive(Serialize, Deserialize)]
struct TestChainData {
    /// Canonical-chain blocks in height order (genesis at index 0).
    canonical_blocks: Vec<Block>,
    /// Blocks that were removed during the reorg (B6..B10).
    reorged_blocks: Vec<Block>,
    /// Expected results for each query method, keyed by block height.
    expected: Vec<BlockExpected>,
}

/// Expected query results for a single canonical block.
#[derive(Serialize, Deserialize)]
struct BlockExpected {
    height: u64,
    block_hash: FixedHash,
    /// Hashes of all outputs in this block.
    output_hashes: Vec<FixedHash>,
    /// Commitments of all outputs in this block.
    output_commitments: Vec<CompressedCommitment>,
    /// Number of inputs in this block.
    input_count: usize,
    /// Excess signatures of all kernels in this block.
    kernel_excess_sigs: Vec<CompressedSignature>,
    /// Kernel count in this block.
    kernel_count: usize,
    /// Payment references for all outputs in this block.
    payrefs: Vec<FixedHash>,
}

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("chain_storage")
        .join("tests")
        .join("fixtures")
}

fn json_fixture_path() -> PathBuf {
    fixtures_dir().join("test_chain_data.json")
}

fn reference_lmdb_dir() -> PathBuf {
    fixtures_dir().join("reference_lmdb")
}

// ---------------------------------------------------------------------------
// Load / build helpers
// ---------------------------------------------------------------------------

/// Load the expected chain data from the JSON fixture.
fn load_test_chain_data() -> TestChainData {
    let json_path = json_fixture_path();
    assert!(
        json_path.exists(),
        "Test fixture JSON not found at {}. Run the generate_fixtures test first: cargo test --package tari_core \
         --features sqlite_bundled lmdb_unit_tests::generate_fixtures -- --ignored --nocapture",
        json_path.display()
    );
    let json_str =
        fs::read_to_string(&json_path).unwrap_or_else(|e| panic!("Failed to read {}: {}", json_path.display(), e));
    serde_json::from_str(&json_str).unwrap_or_else(|e| panic!("Failed to parse {}: {}", json_path.display(), e))
}

/// Build the test chain from JSON data into a fresh LMDB database, returning a
/// `BlockchainDatabase` that can be queried. The LMDB is stored at a temporary path
/// that is cleaned up when the returned database is dropped.
fn build_chain_from_json(data: &TestChainData) -> BlockchainDatabase<TempDatabase> {
    let db = crate::test_helpers::blockchain::create_new_blockchain();
    populate_chain(db, data)
}

/// Populate `db` with the blocks from `data`, performing the reorg, and return it.
fn populate_chain(db: BlockchainDatabase<TempDatabase>, data: &TestChainData) -> BlockchainDatabase<TempDatabase> {
    // Add shared blocks B1..B5 (canonical indices 1..=5)
    for block in data.canonical_blocks[1..=5].iter() {
        db.add_block(Arc::new(block.clone())).unwrap().assert_added();
    }

    // Add original main-chain blocks B6..B10 (these will be reorged out later)
    for block in data.reorged_blocks.iter() {
        db.add_block(Arc::new(block.clone())).unwrap().assert_added();
    }

    // Add fork blocks F6'..F15' (canonical indices 6..=15) - triggers the reorg
    let mut reorg_happened = false;
    for block in data.canonical_blocks[6..].iter() {
        let result = db.add_block(Arc::new(block.clone())).unwrap();
        if result.is_chain_reorg() {
            reorg_happened = true;
        }
    }
    assert!(
        reorg_happened,
        "Expected a chain reorg when adding fork blocks from JSON"
    );

    db
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

/// Build the chain from JSON and reopen it via `open_blockchain_db_from_path` to ensure
/// the database is opened cleanly (no orphan cleanup) and we test the actual on-disk LMDB state.
fn build_and_reopen_chain_from_json(data: &TestChainData) -> BlockchainDatabase<TempDatabase> {
    let db = build_chain_from_json(data);
    let db_path = db.db_read_access().unwrap().path().to_path_buf();

    // Copy the LMDB files to a new temp dir before the original is dropped/cleaned up
    let new_path = tari_test_utils::paths::create_temporary_data_path();
    copy_dir_recursive(&db_path, &new_path);
    drop(db);

    // Reopen with cleanup disabled so orphan pool is preserved
    open_blockchain_db_from_path(&new_path)
}

// ---------------------------------------------------------------------------
// Shared test state (built once, used by all read tests)
// ---------------------------------------------------------------------------

/// Shared test chain data and database, built once for all read tests.
/// This avoids creating a ~400MB LMDB database per test.
struct SharedTestState {
    data: TestChainData,
    db: BlockchainDatabase<TempDatabase>,
}

static SHARED_STATE: Lazy<SharedTestState> = Lazy::new(|| {
    let data = load_test_chain_data();
    let db = build_and_reopen_chain_from_json(&data);
    SharedTestState { data, db }
});

// ---------------------------------------------------------------------------
// Fixture generator (run with --ignored to regenerate)
// ---------------------------------------------------------------------------

/// Generates the JSON test fixture by building a test chain programmatically and serialising
/// the blocks and expected query results.
///
/// Run with:
/// ```bash
/// cargo test --package tari_core --features sqlite_bundled \
///     lmdb_unit_tests::generate_fixtures -- --ignored --nocapture
/// ```
#[test]
#[ignore = "Run manually to regenerate test fixtures"]
fn generate_fixtures() {
    use tari_common_types::{payment_reference::generate_payment_reference, tari_address::TariAddress};
    use tari_transaction_components::{
        key_manager::{KeyManager, TariKeyId},
        transaction_components::{Transaction, WalletOutput},
    };

    use crate::test_helpers::{BlockSpec, blockchain::create_new_blockchain, create_block, default_coinbase_entities};

    fn apply_mmr_to_block(db: &BlockchainDatabase<TempDatabase>, block: Block) -> Block {
        let (mut block, mmr_roots) = db.calculate_mmr_roots(block).unwrap();
        block.header.input_mr = mmr_roots.input_mr;
        block.header.output_mr = mmr_roots.output_mr;
        block.header.block_output_mr = mmr_roots.block_output_mr;
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
            let (block, coinbase) = create_next_block(
                db,
                &prev_block,
                vec![],
                key_manager,
                &script_key_id,
                &wallet_payment_address,
            );
            db.add_block(block.clone()).unwrap().assert_added();
            prev_block = block.clone();
            blocks.push(block);
            outputs.push(coinbase);
        }
        (blocks, outputs)
    }

    // --- Build the chain ---
    let db = create_new_blockchain();
    let key_manager = KeyManager::new_random().unwrap();

    let genesis = Arc::new(db.fetch_block(0, true).unwrap().into_block());
    let (main_blocks, _) = add_chained_blocks(10, &db, &key_manager);

    let shared_blocks: Vec<Arc<Block>> = main_blocks[..5].to_vec();
    let reorged_blocks: Vec<Arc<Block>> = main_blocks[5..].to_vec();

    // Fork chain
    let fork_db = create_new_blockchain();
    let fork_key_manager = KeyManager::new_random().unwrap();
    for block in shared_blocks.iter() {
        fork_db.add_block(block.clone()).unwrap().assert_added();
    }
    let (fork_blocks, _) = add_chained_blocks(10, &fork_db, &fork_key_manager);

    // Trigger reorg
    let mut reorg_happened = false;
    for fork_block in fork_blocks.iter() {
        let result = db.add_block(fork_block.clone()).unwrap();
        if result.is_chain_reorg() {
            reorg_happened = true;
        }
    }
    assert!(reorg_happened, "Expected a chain reorg");

    let tip = db.fetch_tip_header().unwrap();
    assert_eq!(tip.height(), 15);

    // Assemble canonical blocks
    let mut canonical_blocks: Vec<Arc<Block>> = Vec::with_capacity(16);
    canonical_blocks.push(genesis);
    canonical_blocks.extend(shared_blocks);
    canonical_blocks.extend(fork_blocks);

    // --- Build expected query results ---
    let mut expected = Vec::with_capacity(canonical_blocks.len());
    for block in canonical_blocks.iter() {
        let block_hash = block.hash();
        let output_hashes: Vec<FixedHash> = block.body.outputs().iter().map(|o| o.hash()).collect();
        let output_commitments: Vec<CompressedCommitment> =
            block.body.outputs().iter().map(|o| o.commitment().clone()).collect();
        let kernel_excess_sigs: Vec<CompressedSignature> =
            block.body.kernels().iter().map(|k| k.excess_sig.clone()).collect();
        let payrefs: Vec<FixedHash> = output_hashes
            .iter()
            .map(|oh| generate_payment_reference(&block_hash, oh))
            .collect();

        expected.push(BlockExpected {
            height: block.header.height,
            block_hash,
            output_hashes,
            output_commitments,
            input_count: block.body.inputs().len(),
            kernel_excess_sigs,
            kernel_count: block.body.kernels().len(),
            payrefs,
        });
    }

    let test_data = TestChainData {
        canonical_blocks: canonical_blocks.iter().map(|b| (**b).clone()).collect(),
        reorged_blocks: reorged_blocks.iter().map(|b| (**b).clone()).collect(),
        expected,
    };

    // --- Write JSON ---
    let fixtures = fixtures_dir();
    fs::create_dir_all(&fixtures).unwrap();
    let json = serde_json::to_string_pretty(&test_data).unwrap();
    fs::write(json_fixture_path(), &json).unwrap();
    println!("Wrote JSON fixture to {}", json_fixture_path().display());
    println!("Fixture generation complete!");
    println!("  Canonical blocks: {}", canonical_blocks.len());
    println!("  Reorged blocks:   {}", reorged_blocks.len());
}

// ---------------------------------------------------------------------------
// Reference LMDB fixture generator (run with --ignored to regenerate)
// ---------------------------------------------------------------------------

/// Generates a reference LMDB binary fixture by building the test chain from the JSON fixture
/// and copying the resulting `data.mdb` file into `tests/fixtures/reference_lmdb/`.
///
/// Run with:
/// ```bash
/// cargo test -p tari_core --lib -- chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture --ignored --nocapture
/// ```
///
/// After running, commit the generated file:
/// ```bash
/// git add base_layer/core/src/chain_storage/tests/fixtures/reference_lmdb/data.mdb
/// git commit -S -m "test: regenerate reference LMDB fixture"
/// ```
///
/// **IMPORTANT:** Re-running this generator after the JSON fixture has been regenerated requires
/// updating the hardcoded expected constants in `reference_lmdb_constants` below.
///
/// **Platform note:** LMDB `data.mdb` uses a little-endian on-disk format that is
/// architecture-neutral, but the fixture should be regenerated on each target platform to ensure
/// correctness.
#[test]
#[ignore = "Run manually to regenerate the reference LMDB binary fixture"]
fn generate_reference_lmdb_fixture() {
    use tari_storage::lmdb_store::LMDBConfig;
    use tari_utilities::hex::Hex;

    let data = load_test_chain_data();
    // Build genesis + blocks B1..B3 only. The JMT grows fast for each block, so we
    // intentionally keep the chain short to keep the committed data.mdb small enough
    // for git (< 50 MB).  The reference tests assert height, commitment, and kernel
    // from the first non-genesis block, which is enough to prove we can read an
    // existing LMDB — the goal of this fixture.
    let db = {
        use crate::test_helpers::blockchain::create_new_blockchain_with_lmdb_config;
        let db = create_new_blockchain_with_lmdb_config(LMDBConfig::new_from_mb(4, 4, 2, false));
        // Add only B1..B3 — no orphans, no reorg
        for block in data.canonical_blocks[1..=3].iter() {
            db.add_block(Arc::new(block.clone())).unwrap().assert_added();
        }
        db
    };

    // Print a traceability summary before we drop the database
    let tip = db.fetch_tip_header().unwrap();
    println!("=== Reference LMDB fixture summary ===");
    println!("  Tip height    : {}", tip.height());
    println!("  Tip block hash: {}", tip.hash().to_hex());

    let b1_exp = &data.expected[1];
    println!("  Block-1 block_hash : {}", b1_exp.block_hash.to_hex());
    if let Some(oh) = b1_exp.output_hashes.first() {
        println!("  Block-1 output_hash: {}", oh.to_hex());
    }
    if let Some(c) = b1_exp.output_commitments.first() {
        println!("  Block-1 commitment : {}", c.to_hex());
    }
    if let Some(sig) = b1_exp.kernel_excess_sigs.first() {
        println!("  Block-1 kernel nonce: {}", sig.get_compressed_public_nonce().to_hex());
        println!("  Block-1 kernel sig  : {}", sig.get_signature().to_hex());
    }
    println!("======================================");

    // Copy data.mdb BEFORE dropping the database — TempDatabase::drop() deletes the temp dir
    let db_path = db.db_read_access().unwrap().path().to_path_buf();

    let dest = reference_lmdb_dir();
    fs::create_dir_all(&dest).unwrap();

    let src = db_path.join("data.mdb");
    assert!(src.exists(), "data.mdb not found at {}", src.display());

    let dst = dest.join("data.mdb");
    fs::copy(&src, &dst).unwrap_or_else(|e| panic!("Failed to copy data.mdb: {e}"));

    // Now drop the database (this deletes the temp dir, but we've already copied)
    drop(db);

    println!("Wrote reference LMDB fixture to {}", dst.display());
}

// ---------------------------------------------------------------------------
// Reference LMDB tests: open the committed binary fixture and assert values
// ---------------------------------------------------------------------------

/// Hardcoded expected values derived from the committed `test_chain_data.json` fixture.
///
/// If the JSON fixture is regenerated (via `generate_fixtures`), both these constants AND the
/// binary fixture (via `generate_reference_lmdb_fixture`) must be updated together.
mod reference_lmdb_constants {
    /// The canonical chain tip height of the reference fixture (genesis + B1..B3).
    pub const EXPECTED_TIP_HEIGHT: u64 = 3;
    /// Hex-encoded block hash of the chain tip (height 3).
    pub const EXPECTED_TIP_HASH_HEX: &str = "56e5221fe25b28f98a94b033309f7708af5bb8e452fb635cccfecb4b47feafc4";
    /// Hex-encoded Pedersen commitment of the first output in block 1.
    pub const EXPECTED_BLOCK1_COMMITMENT_HEX: &str = "d28588acee522cc1e903e201501c9f0126e39abe335c41083a7ef652a2015f66";
    /// Hex-encoded output hash of the first output in block 1.
    pub const EXPECTED_BLOCK1_OUTPUT_HASH_HEX: &str =
        "422e9933dce4257f5647de6a9a29b01e43fe02a15c156ddbcaef6e54c1517b5c";
    /// Hex-encoded public nonce of the first kernel excess_sig in block 1.
    pub const EXPECTED_BLOCK1_KERNEL_NONCE_HEX: &str =
        "7e7b6ae78a92c6251a000c41d78937289376cbafb8a3471bfda5d5a39924f728";
    /// Hex-encoded signature scalar of the first kernel excess_sig in block 1.
    pub const EXPECTED_BLOCK1_KERNEL_SIG_HEX: &str = "cd1b1efd8df818dc04eccf91fae58fade54cf03924ae5c66501035b2f2409f0a";
}

/// Ensure the reference LMDB fixture exists, generating it if this is the first run.
///
/// The fixture lives at `tests/fixtures/reference_lmdb/data.mdb`. It is intentionally NOT
/// committed to git (the file is large due to LMDB pre-allocation), but it IS deterministic:
/// given the same `test_chain_data.json`, re-running always produces the same on-disk bytes.
///
/// The `OnceLock` guarantees that concurrent test threads only run the generator once, and that
/// subsequent test runs that find the file already on disk skip generation entirely.
fn ensure_reference_fixture_exists() {
    use std::sync::OnceLock;

    use tari_storage::lmdb_store::LMDBConfig;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let fixture_dir = reference_lmdb_dir();
        let data_mdb = fixture_dir.join("data.mdb");
        if data_mdb.exists() {
            return;
        }
        // First run: generate the fixture
        let data = load_test_chain_data();
        let db = {
            use crate::test_helpers::blockchain::create_new_blockchain_with_lmdb_config;
            let empty_db = create_new_blockchain_with_lmdb_config(LMDBConfig::new_from_mb(4, 4, 2, false));
            for block in data.canonical_blocks[1..=3].iter() {
                empty_db.add_block(Arc::new(block.clone())).unwrap().assert_added();
            }
            empty_db
        };
        let db_path = db.db_read_access().unwrap().path().to_path_buf();
        fs::create_dir_all(&fixture_dir).unwrap();
        let src = db_path.join("data.mdb");
        assert!(src.exists(), "data.mdb not found at {}", src.display());
        fs::copy(&src, &data_mdb).unwrap_or_else(|e| panic!("Failed to copy data.mdb: {e}"));
        drop(db);
        println!("[reference_lmdb] Generated fixture at {}", data_mdb.display());
    });
}

/// Open the reference LMDB fixture (auto-generating it on first run).
///
/// `open_blockchain_db_from_path` acquires a write lock, so we first copy the fixture to a
/// temporary directory. The copy is deleted when the returned database is dropped; the
/// original fixture on disk is never modified.
fn open_reference_db() -> BlockchainDatabase<TempDatabase> {
    ensure_reference_fixture_exists();
    let tmp = tari_test_utils::paths::create_temporary_data_path();
    copy_dir_recursive(&reference_lmdb_dir(), &tmp);
    open_blockchain_db_from_path(&tmp)
}

mod reference_lmdb {
    use tari_crypto::{compressed_key::CompressedKey, ristretto::RistrettoSecretKey};
    use tari_utilities::{ByteArray, hex::Hex};

    use super::{reference_lmdb_constants::*, *};

    /// Test 1 — tip header check.
    ///
    /// Opens the committed binary fixture and verifies the tip height and block hash, proving
    /// that the LMDB on-disk serialisation format has not changed since the fixture was generated.
    #[test]
    fn test_reads_reference_lmdb_tip_header() {
        let db = open_reference_db();
        let tip = db.fetch_tip_header().unwrap();
        assert_eq!(
            tip.height(),
            EXPECTED_TIP_HEIGHT,
            "Tip height from reference LMDB does not match expected value"
        );
        assert_eq!(
            tip.hash().to_hex(),
            EXPECTED_TIP_HASH_HEX,
            "Tip block hash from reference LMDB does not match expected value"
        );
    }

    /// Test 2 — UTXO lookup by commitment.
    ///
    /// Verifies that the UTXO commitment index in the binary fixture is intact and returns the
    /// correct output hash for a known commitment from block 1.
    #[test]
    fn test_reads_reference_lmdb_utxo_by_commitment() {
        let db = open_reference_db();

        let commitment_bytes = Vec::from_hex(EXPECTED_BLOCK1_COMMITMENT_HEX).expect("Invalid commitment hex constant");
        let commitment =
            CompressedCommitment::from_canonical_bytes(&commitment_bytes).expect("Invalid commitment bytes");

        let found_hash = db
            .fetch_unspent_output_hash_by_commitment(commitment)
            .expect("fetch_unspent_output_hash_by_commitment failed on reference LMDB")
            .expect("Block-1 UTXO commitment not found in reference LMDB");

        assert_eq!(
            found_hash.to_hex(),
            EXPECTED_BLOCK1_OUTPUT_HASH_HEX,
            "Commitment lookup returned unexpected output hash from reference LMDB"
        );
    }

    /// Test 3 — kernel lookup by excess signature.
    ///
    /// Verifies that the kernel excess-sig index is intact and can locate the block-1 kernel,
    /// which is the core backward-compatibility check for the LMDB serialisation format.
    #[test]
    fn test_reads_reference_lmdb_kernel_by_excess_sig() {
        let db = open_reference_db();

        let nonce_bytes = Vec::from_hex(EXPECTED_BLOCK1_KERNEL_NONCE_HEX).expect("Invalid nonce hex constant");
        let sig_scalar_bytes = Vec::from_hex(EXPECTED_BLOCK1_KERNEL_SIG_HEX).expect("Invalid sig hex constant");

        let excess_sig = CompressedSignature::new(
            CompressedKey::new(&nonce_bytes),
            RistrettoSecretKey::from_canonical_bytes(&sig_scalar_bytes).expect("Invalid sig scalar bytes"),
        );

        let (found_kernel, _block_hash) = db
            .fetch_kernel_by_excess_sig(excess_sig.clone())
            .expect("fetch_kernel_by_excess_sig failed on reference LMDB")
            .expect("Block-1 kernel not found in reference LMDB");

        assert_eq!(
            found_kernel.excess_sig, excess_sig,
            "Kernel excess_sig mismatch in reference LMDB"
        );
    }
}

// ---------------------------------------------------------------------------
// Write test: create LMDB from JSON data, verify chain state
// ---------------------------------------------------------------------------

mod write_tests {
    use super::*;

    /// Writes the chain from JSON blocks into a fresh LMDB and verifies the resulting chain
    /// state matches the expected topology: correct tip, canonical blocks, and reorged blocks.
    #[test]
    fn chain_from_json_has_correct_topology() {
        let data = load_test_chain_data();
        let db = build_chain_from_json(&data);

        let tip = db.fetch_tip_header().unwrap();
        let last_expected = data.expected.last().unwrap();
        assert_eq!(tip.height(), last_expected.height, "Tip height mismatch");
        assert_eq!(*tip.hash(), last_expected.block_hash, "Tip hash mismatch");

        // Every canonical block should be retrievable by height
        for exp in &data.expected {
            let fetched = db
                .fetch_block(exp.height, true)
                .unwrap_or_else(|e| panic!("fetch_block({}) failed: {}", exp.height, e));
            assert_eq!(
                *fetched.hash(),
                exp.block_hash,
                "Hash mismatch at height {}",
                exp.height
            );
        }
    }

    /// Writes the chain from JSON into a fresh LMDB and verifies that the reorged blocks
    /// (B6..B10) are no longer on the main chain but are retrievable from the orphan pool.
    #[test]
    fn reorged_blocks_handled_correctly() {
        let data = load_test_chain_data();
        let db = build_chain_from_json(&data);

        for reorged in &data.reorged_blocks {
            let hash = reorged.hash();

            // Should not be on the main chain
            let header = db.fetch_header_by_block_hash(hash).unwrap();
            assert!(
                header.is_none(),
                "Reorged block at height {} should not be on main chain",
                reorged.header.height
            );

            // Should be in the orphan pool
            let orphan = db
                .fetch_orphan(hash)
                .unwrap_or_else(|e| panic!("fetch_orphan failed for height {}: {}", reorged.header.height, e));
            assert_eq!(orphan.header.height, reorged.header.height);
        }
    }
}

// ---------------------------------------------------------------------------
// Read tests: build LMDB from JSON, verify queries against JSON expected data
// ---------------------------------------------------------------------------

mod read_tests {
    use super::*;

    // === Headers ===

    mod headers {
        use super::*;

        #[test]
        fn tip_header_matches_expected() {
            let state = &*SHARED_STATE;
            let tip = state.db.fetch_tip_header().unwrap();
            let last_expected = state.data.expected.last().unwrap();
            assert_eq!(tip.height(), last_expected.height);
            assert_eq!(*tip.hash(), last_expected.block_hash);
        }

        #[test]
        fn all_canonical_headers_retrievable_by_height() {
            let state = &*SHARED_STATE;
            for exp in &state.data.expected {
                let fetched = state
                    .db
                    .fetch_block(exp.height, true)
                    .unwrap_or_else(|e| panic!("fetch_block({}) failed: {}", exp.height, e));
                assert_eq!(
                    fetched.header().height,
                    exp.height,
                    "Header height mismatch at height {}",
                    exp.height
                );
                assert_eq!(
                    *fetched.hash(),
                    exp.block_hash,
                    "Block hash mismatch at height {}",
                    exp.height
                );
            }
        }

        #[test]
        fn canonical_headers_retrievable_by_hash() {
            let state = &*SHARED_STATE;
            for exp in &state.data.expected {
                let header = state
                    .db
                    .fetch_header_by_block_hash(exp.block_hash)
                    .unwrap()
                    .unwrap_or_else(|| panic!("Header not found by hash for block at height {}", exp.height));
                assert_eq!(header.height, exp.height);
            }
        }

        #[test]
        fn reorged_blocks_not_on_main_chain() {
            let state = &*SHARED_STATE;
            for reorged in &state.data.reorged_blocks {
                let hash = reorged.hash();
                let header = state.db.fetch_header_by_block_hash(hash).unwrap();
                assert!(
                    header.is_none(),
                    "Reorged block at height {} should not be on main chain",
                    reorged.header.height
                );
            }
        }

        #[test]
        fn reorged_blocks_in_orphan_pool() {
            let state = &*SHARED_STATE;
            for reorged in &state.data.reorged_blocks {
                let hash = reorged.hash();
                let orphan = state
                    .db
                    .fetch_orphan(hash)
                    .unwrap_or_else(|e| panic!("fetch_orphan failed for height {}: {}", reorged.header.height, e));
                assert_eq!(orphan.header.height, reorged.header.height);
                assert_eq!(orphan.hash(), hash);
            }
        }

        #[test]
        fn fetch_header_containing_kernel_mmr_genesis() {
            let state = &*SHARED_STATE;
            let genesis_kernel_count = state.data.expected[0].kernel_count as u64;
            if genesis_kernel_count > 0 {
                let header = state
                    .db
                    .fetch_header_containing_kernel_mmr(0)
                    .expect("Should find header for MMR position 0");
                assert_eq!(header.height(), 0, "MMR position 0 should be in genesis");
            }
        }

        #[test]
        fn fetch_header_containing_kernel_mmr_block_1() {
            let state = &*SHARED_STATE;
            let genesis_kernel_count = state.data.expected[0].kernel_count as u64;
            let header = state
                .db
                .fetch_header_containing_kernel_mmr(genesis_kernel_count)
                .expect("Should find header for block 1 kernel MMR position");
            assert_eq!(header.height(), 1, "First kernel after genesis should be in block 1");
        }

        #[test]
        fn fetch_header_containing_kernel_mmr_fork_block() {
            let state = &*SHARED_STATE;
            let mut accumulated: u64 = 0;
            for i in 0..=5 {
                accumulated += state.data.expected[i].kernel_count as u64;
            }
            let header = state
                .db
                .fetch_header_containing_kernel_mmr(accumulated)
                .expect("Should find header for fork block 6 kernel");
            assert_eq!(header.height(), 6);
        }

        #[test]
        fn fetch_header_containing_kernel_mmr_out_of_range() {
            let state = &*SHARED_STATE;
            let result = state.db.fetch_header_containing_kernel_mmr(999_999);
            assert!(result.is_err(), "Should error for out-of-range MMR position");
        }
    }

    // === Outputs ===

    mod outputs {
        use super::*;

        #[test]
        fn fetch_outputs_in_block_returns_expected_outputs() {
            let state = &*SHARED_STATE;
            for exp in &state.data.expected {
                let db_outputs = state
                    .db
                    .fetch_outputs_in_block(exp.block_hash)
                    .unwrap_or_else(|e| panic!("fetch_outputs_in_block failed for height {}: {}", exp.height, e));

                assert_eq!(
                    db_outputs.len(),
                    exp.output_hashes.len(),
                    "Output count mismatch for block at height {}",
                    exp.height
                );

                let actual_hashes: Vec<FixedHash> = db_outputs.iter().map(|o| o.hash()).collect();
                for expected_hash in &exp.output_hashes {
                    assert!(
                        actual_hashes.contains(expected_hash),
                        "Missing expected output {} in block at height {}",
                        expected_hash,
                        exp.height
                    );
                }
            }
        }

        #[test]
        fn fetch_output_by_hash_returns_correct_mined_info() {
            let state = &*SHARED_STATE;
            for exp in state.data.expected.iter().skip(1) {
                for output_hash in &exp.output_hashes {
                    let mined_info = state
                        .db
                        .fetch_output(*output_hash)
                        .unwrap_or_else(|e| {
                            panic!(
                                "fetch_output failed for {} at height {}: {}",
                                output_hash, exp.height, e
                            )
                        })
                        .unwrap_or_else(|| panic!("Output {} at height {} not found", output_hash, exp.height));

                    assert_eq!(mined_info.output.hash(), *output_hash, "Output hash mismatch");
                    assert_eq!(
                        mined_info.mined_height, exp.height,
                        "Mined height mismatch for output {}",
                        output_hash
                    );
                    assert_eq!(
                        mined_info.header_hash, exp.block_hash,
                        "Header hash mismatch for output {}",
                        output_hash
                    );
                }
            }
        }

        #[test]
        fn fetch_unspent_output_hash_by_commitment_finds_all_canonical() {
            let state = &*SHARED_STATE;
            for exp in state.data.expected.iter().skip(1) {
                for (i, commitment) in exp.output_commitments.iter().enumerate() {
                    let found_hash = state
                        .db
                        .fetch_unspent_output_hash_by_commitment(commitment.clone())
                        .unwrap_or_else(|e| {
                            panic!(
                                "fetch_unspent_output_hash_by_commitment failed at height {}: {}",
                                exp.height, e
                            )
                        })
                        .unwrap_or_else(|| {
                            panic!("Commitment lookup returned None for output in block {}", exp.height)
                        });

                    assert_eq!(
                        found_hash, exp.output_hashes[i],
                        "Commitment lookup returned wrong hash at height {}",
                        exp.height
                    );
                }
            }
        }

        #[test]
        fn fetch_unspent_output_hash_by_commitment_returns_none_for_unknown() {
            let state = &*SHARED_STATE;
            let bogus = CompressedCommitment::default();
            let result = state.db.fetch_unspent_output_hash_by_commitment(bogus).unwrap();
            assert!(result.is_none(), "Should return None for unknown commitment");
        }

        #[test]
        fn fetch_outputs_in_block_with_spend_state_tip_unspent() {
            let state = &*SHARED_STATE;
            let tip_exp = state.data.expected.last().unwrap();
            let outputs_with_state = state
                .db
                .fetch_outputs_in_block_with_spend_state(tip_exp.block_hash, Some(tip_exp.block_hash))
                .expect("fetch_outputs_in_block_with_spend_state should succeed");

            assert!(!outputs_with_state.is_empty(), "Tip should have outputs");
            for (output, is_spent) in &outputs_with_state {
                assert!(!is_spent, "Tip output {} should be unspent", output.hash());
            }
        }

        #[test]
        fn fetch_outputs_in_block_with_spend_state_no_header() {
            let state = &*SHARED_STATE;
            let exp = &state.data.expected[3];
            let outputs_with_state = state
                .db
                .fetch_outputs_in_block_with_spend_state(exp.block_hash, None)
                .expect("Should succeed with None spend header");

            assert_eq!(
                outputs_with_state.len(),
                exp.output_hashes.len(),
                "Output count mismatch at height {}",
                exp.height
            );
            for (output, is_spent) in &outputs_with_state {
                assert!(
                    !is_spent,
                    "Output {} should be unspent when no spend header provided",
                    output.hash()
                );
            }
        }
    }

    // === Inputs ===

    mod inputs {
        use super::*;

        #[test]
        fn fetch_inputs_in_block_matches_expected_count() {
            let state = &*SHARED_STATE;
            for exp in &state.data.expected {
                let inputs = state
                    .db
                    .fetch_inputs_in_block(exp.block_hash)
                    .unwrap_or_else(|e| panic!("fetch_inputs_in_block failed at height {}: {}", exp.height, e));

                assert_eq!(
                    inputs.len(),
                    exp.input_count,
                    "Input count mismatch at height {}",
                    exp.height
                );
            }
        }

        #[test]
        fn fetch_inputs_in_block_empty_for_coinbase_only() {
            let state = &*SHARED_STATE;
            let exp = &state.data.expected[1];
            let inputs = state.db.fetch_inputs_in_block(exp.block_hash).unwrap();
            assert_eq!(inputs.len(), exp.input_count);
            assert_eq!(exp.input_count, 0, "Block 1 should be coinbase-only");
        }

        #[test]
        fn fetch_inputs_in_block_empty_for_unknown_hash() {
            let state = &*SHARED_STATE;
            let inputs = state.db.fetch_inputs_in_block(FixedHash::zero()).unwrap();
            assert!(inputs.is_empty());
        }
    }

    // === Kernels ===

    mod kernels {
        use super::*;

        #[test]
        fn fetch_kernels_in_block_matches_expected() {
            let state = &*SHARED_STATE;
            for exp in &state.data.expected {
                let kernels = state
                    .db
                    .fetch_kernels_in_block(exp.block_hash)
                    .unwrap_or_else(|e| panic!("fetch_kernels_in_block failed at height {}: {}", exp.height, e));

                assert_eq!(
                    kernels.len(),
                    exp.kernel_count,
                    "Kernel count mismatch at height {}",
                    exp.height
                );

                for kernel in &kernels {
                    assert!(
                        exp.kernel_excess_sigs.contains(&kernel.excess_sig),
                        "Unexpected kernel excess_sig in block at height {}",
                        exp.height
                    );
                }
            }
        }

        #[test]
        fn fetch_kernel_by_excess_sig_finds_all_canonical() {
            let state = &*SHARED_STATE;
            for exp in state.data.expected.iter().skip(1) {
                for excess_sig in &exp.kernel_excess_sigs {
                    let (found_kernel, found_hash) = state
                        .db
                        .fetch_kernel_by_excess_sig(excess_sig.clone())
                        .unwrap_or_else(|e| panic!("fetch_kernel_by_excess_sig failed at height {}: {}", exp.height, e))
                        .unwrap_or_else(|| {
                            panic!("Kernel with sig {:?} at height {} not found", excess_sig, exp.height)
                        });

                    assert_eq!(found_kernel.excess_sig, *excess_sig, "Excess sig mismatch");
                    assert_eq!(
                        found_hash, exp.block_hash,
                        "Block hash mismatch for kernel at height {}",
                        exp.height
                    );
                }
            }
        }

        #[test]
        fn fetch_kernel_by_excess_sig_returns_none_for_unknown() {
            let state = &*SHARED_STATE;
            let bogus = CompressedSignature::default();
            let result = state.db.fetch_kernel_by_excess_sig(bogus).unwrap();
            assert!(result.is_none());
        }

        #[test]
        fn fetch_kernels_in_block_empty_for_unknown_hash() {
            let state = &*SHARED_STATE;
            let kernels = state.db.fetch_kernels_in_block(FixedHash::zero()).unwrap();
            assert!(kernels.is_empty());
        }
    }

    // === PayRef / MinedInfo ===

    mod payref {
        use super::*;

        #[test]
        fn fetch_mined_info_by_payref_finds_all_canonical_outputs() {
            let state = &*SHARED_STATE;
            for exp in state.data.expected.iter().skip(1) {
                for (i, payref) in exp.payrefs.iter().enumerate() {
                    let mined_info = state.db.fetch_mined_info_by_payref(*payref).unwrap_or_else(|e| {
                        panic!("fetch_mined_info_by_payref failed at height {}: {}", exp.height, e)
                    });

                    let output_info = mined_info.output.as_ref().unwrap_or_else(|| {
                        panic!("MinedInfo.output should be Some for payref at height {}", exp.height)
                    });

                    assert_eq!(
                        output_info.output.hash(),
                        exp.output_hashes[i],
                        "PayRef lookup returned wrong output at height {}",
                        exp.height
                    );
                    assert_eq!(
                        output_info.mined_height, exp.height,
                        "PayRef lookup returned wrong height"
                    );
                }
            }
        }

        #[test]
        fn fetch_mined_info_by_payref_works_for_fork_blocks() {
            let state = &*SHARED_STATE;
            for exp in state.data.expected.iter().skip(6) {
                for payref in &exp.payrefs {
                    let mined_info = state.db.fetch_mined_info_by_payref(*payref).unwrap_or_else(|e| {
                        panic!(
                            "fetch_mined_info_by_payref failed for fork block at height {}: {}",
                            exp.height, e
                        )
                    });

                    assert!(
                        mined_info.output.is_some(),
                        "PayRef lookup should return output for fork block at height {}",
                        exp.height
                    );
                }
            }
        }
    }
}

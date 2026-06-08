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
//! ## Reference LMDB binary fixture
//!
//! The file `tests/fixtures/reference_lmdb/data.mdb.gz` is a gzip-compressed copy of an
//! actual LMDB database containing the first 3 blocks of the test chain.  It is **committed to
//! git** at ~150 KB (the raw `data.mdb` is ~136 MB, but LMDB pre-allocates with zeroes which
//! compress to almost nothing).  Tests that need to open it call `open_reference_db()`, which
//! decompresses the archive on first use.
//!
//! The reference tests prove that the LMDB read path works against a pre-existing on-disk
//! database — not just one that was just written in the same test process.
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
    for block in &data.canonical_blocks[1..=5] {
        db.add_block(Arc::new(block.clone())).unwrap().assert_added();
    }

    // Add original main-chain blocks B6..B10 (these will be reorged out later)
    for block in &data.reorged_blocks {
        db.add_block(Arc::new(block.clone())).unwrap().assert_added();
    }

    // Add fork blocks F6'..F15' (canonical indices 6..=15) - triggers the reorg
    let mut reorg_happened = false;
    for block in &data.canonical_blocks[6..] {
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
#[allow(clippy::too_many_lines)]
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
    for block in &shared_blocks {
        fork_db.add_block(block.clone()).unwrap().assert_added();
    }
    let (fork_blocks, _) = add_chained_blocks(10, &fork_db, &fork_key_manager);

    // Trigger reorg
    let mut reorg_happened = false;
    for fork_block in &fork_blocks {
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
    for block in &canonical_blocks {
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

/// Generates a reference LMDB binary fixture by building the test chain from the JSON fixture,
/// gzip-compressing the resulting `data.mdb`, and writing it to
/// `tests/fixtures/reference_lmdb/data.mdb.gz`.
///
/// LMDB pre-allocates its map as a sparse file.  On disk the raw `data.mdb` for a 3-block chain
/// is ~136 MB, but it compresses to ~150 KB because the unused pages are all zeroes.  We commit
/// the compressed version so that CI can always open a pre-existing on-disk LMDB database without
/// requiring a large binary attachment or git-LFS.
///
/// Run with:
/// ```bash
/// cargo test -p tari_core --lib --features sqlite_bundled \
///   -- chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture --ignored --nocapture
/// ```
///
/// After running, commit the generated file:
/// ```bash
/// git add base_layer/core/src/chain_storage/tests/fixtures/reference_lmdb/data.mdb.gz
/// git commit -S -m "test: regenerate reference LMDB fixture"
/// ```
///
/// **IMPORTANT:** When the JSON fixture is regenerated, this generator must be re-run so the
/// committed `data.mdb.gz` matches the new JSON. The expected values used by the read-side
/// tests are derived from the JSON at runtime, so no separate constants need updating.
#[test]
#[ignore = "Run manually to regenerate the reference LMDB binary fixture"]
fn generate_reference_lmdb_fixture() {
    use flate2::{Compression, write::GzEncoder};
    use tari_storage::lmdb_store::LMDBConfig;
    use tari_utilities::hex::Hex;

    let data = load_test_chain_data();
    // Build the full chain so the fixture matches the JSON: genesis → B1..B5 →
    // reorg (orphans B6..B10, promotes F6'..F15') → tip at height 15.
    // LMDB pre-allocates its map as a sparse file so a 128 MB map still compresses
    // to a few hundred KB.
    let db = {
        use crate::test_helpers::blockchain::create_new_blockchain_with_lmdb_config;
        let db = create_new_blockchain_with_lmdb_config(LMDBConfig::new_from_mb(
            128,
            4,
            2,
            false,
            tari_storage::lmdb_store::DEFAULT_LMDB_COMPACTION_MIN_FREE_BYTES,
        ));
        populate_chain(db, &data)
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
    let src = db_path.join("data.mdb");
    assert!(src.exists(), "data.mdb not found at {}", src.display());
    let raw = fs::read(&src).unwrap_or_else(|e| panic!("Failed to read data.mdb: {e}"));

    // Now drop the database (this deletes the temp dir, but we've already read the bytes)
    drop(db);

    // Gzip-compress the raw bytes.  LMDB pre-allocates its map with zeroes so the
    // compression ratio is typically >99 % (136 MB → ~150 KB).
    let dest = reference_lmdb_dir();
    fs::create_dir_all(&dest).unwrap();
    let dst = dest.join("data.mdb.gz");

    let gz_file = fs::File::create(&dst).unwrap_or_else(|e| panic!("Failed to create {}: {e}", dst.display()));
    let mut enc = GzEncoder::new(gz_file, Compression::best());
    std::io::copy(&mut raw.as_slice(), &mut enc).unwrap();
    enc.finish()
        .unwrap_or_else(|e| panic!("Failed to finalise gzip stream: {e}"));

    println!(
        "Wrote compressed reference LMDB fixture to {} ({} KB)",
        dst.display(),
        fs::metadata(&dst).map(|m| m.len() / 1024).unwrap_or(0)
    );
    println!("Commit it with:");
    println!("  git add {}", dst.display());
    println!("  git commit -S -m 'test: regenerate reference LMDB fixture'");
}

// ---------------------------------------------------------------------------
// Reference LMDB tests: open the committed binary fixture and assert values
// ---------------------------------------------------------------------------

/// Ensure a decompressed copy of the reference LMDB fixture is present in a temporary location.
///
/// The committed fixture is `tests/fixtures/reference_lmdb/data.mdb.gz` (~150 KB).  On first
/// access it is decompressed into the same directory alongside the compressed source.  The
/// `OnceLock` guarantees this is done at most once per test-process execution; subsequent tests
/// that call `open_reference_db` find the decompressed file already in place.
///
/// # Panics
///
/// Panics if the committed `.gz` file is missing — which means the fixture has not been
/// generated yet.  Run `generate_reference_lmdb_fixture` and commit the result.
fn ensure_reference_fixture_exists() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let fixture_dir = reference_lmdb_dir();
        let data_mdb = fixture_dir.join("data.mdb");

        if data_mdb.exists() {
            return; // Already decompressed from a previous test run in this process or on disk
        }

        let gz_path = fixture_dir.join("data.mdb.gz");
        assert!(
            gz_path.exists(),
            "Reference LMDB fixture not found at {}.\nRun the generator and commit the result:\ncargo test -p \
             tari_core --lib --features sqlite_bundled -- \
             chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture --ignored --nocapture",
            gz_path.display()
        );

        // Decompress data.mdb.gz → data.mdb next to the compressed source.
        let gz_file = fs::File::open(&gz_path).unwrap_or_else(|e| panic!("Failed to open {}: {e}", gz_path.display()));
        let mut decoder = flate2::read::GzDecoder::new(gz_file);
        fs::create_dir_all(&fixture_dir).unwrap();
        let mut out_file =
            fs::File::create(&data_mdb).unwrap_or_else(|e| panic!("Failed to create {}: {e}", data_mdb.display()));
        let bytes_written = std::io::copy(&mut decoder, &mut out_file)
            .unwrap_or_else(|e| panic!("Failed to decompress {}: {e}", gz_path.display()));

        println!(
            "[reference_lmdb] Decompressed fixture to {} ({} MB)",
            data_mdb.display(),
            bytes_written / 1024 / 1024
        );
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
    use super::*;

    /// Test 1 — tip header check.
    ///
    /// Opens the committed binary fixture and verifies the tip height and block hash, proving
    /// that the LMDB on-disk serialisation format has not changed since the fixture was generated.
    /// Expected values are derived from the JSON fixture at runtime so the JSON is the single
    /// source of truth: regenerating it automatically propagates here without manual constant
    /// edits.
    #[test]
    fn test_reads_reference_lmdb_tip_header() {
        let data = load_test_chain_data();
        let expected_tip = data.expected.last().expect("JSON fixture has no expected blocks");

        let db = open_reference_db();
        let tip = db.fetch_tip_header().unwrap();
        assert_eq!(
            tip.height(),
            expected_tip.height,
            "Tip height from reference LMDB does not match expected value from JSON"
        );
        assert_eq!(
            *tip.hash(),
            expected_tip.block_hash,
            "Tip block hash from reference LMDB does not match expected value from JSON"
        );
    }

    /// Test 2 — UTXO lookup by commitment.
    ///
    /// Verifies that the UTXO commitment index in the binary fixture is intact and returns the
    /// correct output hash for a known commitment from block 1. Both the commitment to look up
    /// and the expected output hash come from the JSON fixture, so the JSON remains the single
    /// source of truth.
    #[test]
    fn test_reads_reference_lmdb_utxo_by_commitment() {
        let data = load_test_chain_data();
        let block1 = &data.expected[1];
        let commitment = block1
            .output_commitments
            .first()
            .expect("Block 1 in JSON fixture has no output commitments")
            .clone();
        let expected_output_hash = block1
            .output_hashes
            .first()
            .expect("Block 1 in JSON fixture has no output hashes");

        let db = open_reference_db();
        let found_hash = db
            .fetch_unspent_output_hash_by_commitment(commitment)
            .expect("fetch_unspent_output_hash_by_commitment failed on reference LMDB")
            .expect("Block-1 UTXO commitment not found in reference LMDB");

        assert_eq!(
            found_hash, *expected_output_hash,
            "Commitment lookup returned unexpected output hash from reference LMDB"
        );
    }

    /// Test 3 — kernel lookup by excess signature.
    ///
    /// Verifies that the kernel excess-sig index is intact and can locate the block-1 kernel,
    /// which is the core backward-compatibility check for the LMDB serialisation format. The
    /// excess signature used for the lookup is taken straight from the JSON fixture.
    #[test]
    fn test_reads_reference_lmdb_kernel_by_excess_sig() {
        let data = load_test_chain_data();
        let excess_sig = data.expected[1]
            .kernel_excess_sigs
            .first()
            .expect("Block 1 in JSON fixture has no kernel excess sigs")
            .clone();

        let db = open_reference_db();
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

    /// Byte-level comparison of stored data: dump every (key, value) byte pair from every
    /// named LMDB sub-database in a freshly-built chain, dump the same from the committed
    /// reference fixture, and assert they are bit-identical.
    ///
    /// This is the strongest practical regression check on the LMDB on-disk format. Any
    /// change to key encoding, value encoding (serde format, length prefixes, struct
    /// layout), record ordering, or set membership will fail this test.
    ///
    /// # Why a kv-pair dump rather than `data.mdb` byte equality
    ///
    /// LMDB pre-allocates a sparse map and stores internal bookkeeping (free-page list,
    /// transaction id in the meta page) that is not deterministic between independent
    /// fresh builds even when the user-visible data is identical. A naive `data.mdb`
    /// `assert_eq!(fresh_bytes, reference_bytes)` therefore fails by ~4 bytes in the
    /// meta-page region on every run. Dumping kv-pairs strips out the LMDB-internal
    /// bookkeeping while keeping every byte of every stored key and value subject to
    /// the equality check.
    ///
    /// # On failure
    ///
    /// The assertion message names the database that diverged and the index of the
    /// first differing kv-pair. If the change is intentional, regenerate the reference
    /// fixture with the `generate_reference_lmdb_fixture` ignored test.
    #[test]
    fn fresh_lmdb_bytes_match_reference_fixture() {
        use tari_storage::lmdb_store::LMDBConfig;

        use crate::test_helpers::blockchain::create_new_blockchain_with_lmdb_config;

        // Build a fresh chain using the exact config used to produce the committed fixture.
        // See `generate_reference_lmdb_fixture` above — these arguments must stay in sync.
        let lmdb_config = LMDBConfig::new_from_mb(
            128,
            4,
            2,
            false,
            tari_storage::lmdb_store::DEFAULT_LMDB_COMPACTION_MIN_FREE_BYTES,
        );
        let data = load_test_chain_data();
        let fresh_db = create_new_blockchain_with_lmdb_config(lmdb_config.clone());
        let fresh_db = populate_chain(fresh_db, &data);

        // Copy the freshly-built data.mdb to a stable directory before the underlying
        // TempDatabase is dropped (which would delete it). We then re-open it cleanly via
        // `dump_lmdb_kv_pairs` for byte enumeration.
        let fresh_src = fresh_db.db_read_access().unwrap().path().to_path_buf();
        let fresh_dump_path = tari_test_utils::paths::create_temporary_data_path();
        copy_dir_recursive(&fresh_src, &fresh_dump_path);
        drop(fresh_db);

        // Decompress the committed reference fixture into a sibling stable directory.
        let ref_dump_path = tari_test_utils::paths::create_temporary_data_path();
        decompress_reference_to(&ref_dump_path);

        // Dump every kv-pair from every named sub-database in both LMDBs.
        let fresh_dump = dump_lmdb_kv_pairs(&fresh_dump_path, &lmdb_config);
        let reference_dump = dump_lmdb_kv_pairs(&ref_dump_path, &lmdb_config);

        // Compare top-level structure (same set of named databases).
        assert_eq!(
            fresh_dump.len(),
            reference_dump.len(),
            "Fresh and reference LMDBs have a different number of named databases (fresh={}, ref={})",
            fresh_dump.len(),
            reference_dump.len(),
        );

        // Compare each named database by kv-pair set, with informative diagnostics on first
        // divergence.
        for ((fresh_name, fresh_pairs), (ref_name, ref_pairs)) in fresh_dump.iter().zip(reference_dump.iter()) {
            assert_eq!(
                fresh_name, ref_name,
                "Database name order diverged between fresh ({fresh_name}) and reference ({ref_name})",
            );
            assert_eq!(
                fresh_pairs.len(),
                ref_pairs.len(),
                "Database `{}` has {} kv-pairs in fresh build but {} in reference fixture. If this change is \
                 intentional, regenerate the reference fixture with:\ncargo test -p tari_core --lib --features \
                 sqlite_bundled -- chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture --ignored \
                 --nocapture",
                fresh_name,
                fresh_pairs.len(),
                ref_pairs.len(),
            );

            for (idx, ((fk, fv), (rk, rv))) in fresh_pairs.iter().zip(ref_pairs.iter()).enumerate() {
                if fk != rk || fv != rv {
                    panic!(
                        "Database `{}` diverges at kv-pair index {}.\nFresh    key: {:02x?} ({} bytes)\nReference \
                         key: {:02x?} ({} bytes)\nFresh    val: {:02x?} ({} bytes)\nReference val: {:02x?} ({} \
                         bytes)\nIf this change is intentional, regenerate the reference fixture with:\ncargo test -p \
                         tari_core --lib --features sqlite_bundled -- \
                         chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture --ignored --nocapture",
                        fresh_name,
                        idx,
                        fk,
                        fk.len(),
                        rk,
                        rk.len(),
                        fv,
                        fv.len(),
                        rv,
                        rv.len(),
                    );
                }
            }
        }
    }
}

/// Decompress the committed `reference_lmdb/data.mdb.gz` into `dest_dir/data.mdb`.
///
/// Used by tests that need a stable, writable copy of the reference LMDB independent of
/// the shared `ensure_reference_fixture_exists` cache.
fn decompress_reference_to(dest_dir: &Path) {
    let gz_path = reference_lmdb_dir().join("data.mdb.gz");
    assert!(
        gz_path.exists(),
        "Reference fixture not found at {}.\nRun the generator and commit the result:\ncargo test -p tari_core --lib \
         --features sqlite_bundled -- chain_storage::tests::lmdb_unit_tests::generate_reference_lmdb_fixture \
         --ignored --nocapture",
        gz_path.display()
    );
    fs::create_dir_all(dest_dir).unwrap();
    let gz_file = fs::File::open(&gz_path).unwrap_or_else(|e| panic!("Failed to open {}: {e}", gz_path.display()));
    let mut decoder = flate2::read::GzDecoder::new(gz_file);
    let dst = dest_dir.join("data.mdb");
    let mut out = fs::File::create(&dst).unwrap_or_else(|e| panic!("Failed to create {}: {e}", dst.display()));
    std::io::copy(&mut decoder, &mut out).unwrap_or_else(|e| panic!("Failed to decompress {}: {e}", gz_path.display()));
}

/// A single raw key/value pair from an LMDB sub-database.
type LmdbKvPair = (Vec<u8>, Vec<u8>);

/// All kv-pairs from a single named LMDB sub-database, paired with that database's name.
type NamedDbDump = (String, Vec<LmdbKvPair>);

/// Open the LMDB at `path` with the same database list and flags as production, then
/// dump every (key, value) raw byte pair from every named sub-database. Returns a Vec
/// of (db_name, kv_pairs) preserving the iteration order of `get_all_database_names()`.
///
/// Within each sub-database, kv-pairs are returned in LMDB cursor order (lexicographic
/// for byte keys, numeric for INTEGERKEY databases).
fn dump_lmdb_kv_pairs(path: &Path, config: &tari_storage::lmdb_store::LMDBConfig) -> Vec<NamedDbDump> {
    use lmdb_zero::{LmdbResultExt, ReadTransaction};

    use crate::chain_storage::lmdb_db::{build_lmdb_store, get_all_database_names};

    let (lmdb_store, _file_lock) =
        build_lmdb_store(path, config.clone()).unwrap_or_else(|e| panic!("Failed to open LMDB at {:?}: {}", path, e));

    let mut result: Vec<NamedDbDump> = Vec::new();
    for name in get_all_database_names() {
        let handle = lmdb_store
            .get_handle(name)
            .unwrap_or_else(|| panic!("Database `{name}` not present in LMDB at {:?}", path));
        let db_ref = handle.db();
        let env = db_ref.env();
        let txn = ReadTransaction::new(env).unwrap_or_else(|e| panic!("ReadTransaction::new failed for `{name}`: {e}"));
        let access = txn.access();
        let mut cursor = txn
            .cursor(db_ref.clone())
            .unwrap_or_else(|e| panic!("cursor() failed for `{name}`: {e}"));

        let mut pairs: Vec<LmdbKvPair> = Vec::new();
        let mut next: lmdb_zero::Result<(&[u8], &[u8])> = cursor.first(&access);
        loop {
            match next.to_opt() {
                Ok(Some((k, v))) => {
                    pairs.push((k.to_vec(), v.to_vec()));
                    next = cursor.next(&access);
                },
                Ok(None) => break,
                Err(e) => panic!("Cursor iteration failed in `{name}`: {e}"),
            }
        }
        result.push((name.to_string(), pairs));
    }
    result
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

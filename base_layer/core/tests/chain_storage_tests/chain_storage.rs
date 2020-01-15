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

use crate::helpers::{
    block_builders::{append_block, create_genesis_block, create_genesis_block_with_utxos, generate_new_block},
    sample_blockchains::create_new_blockchain,
};
use croaring::Bitmap;
use env_logger;
use std::thread;
use tari_core::{
    blocks::{Block, BlockHeader},
    chain_storage::{
        BlockAddResult,
        BlockchainDatabase,
        ChainStorageError,
        DbKey,
        DbTransaction,
        MemoryDatabase,
        MmrTree,
        Validators,
    },
    helpers::{create_mem_db, create_orphan_block},
    validation::mocks::MockValidator,
};
use tari_mmr::{MerkleChangeTrackerConfig, MutableMmr};
use tari_transactions::{
    helpers::{create_test_kernel, create_utxo, spend_utxos},
    tari_amount::{uT, MicroTari, T},
    tx,
    txn_schema,
    types::{CryptoFactories, HashDigest},
};
use tari_utilities::{hex::Hex, Hashable};

fn init_log() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn fetch_nonexistent_kernel() {
    let store = create_mem_db();
    let h = vec![0u8; 32];
    assert_eq!(
        store.fetch_kernel(h.clone()),
        Err(ChainStorageError::ValueNotFound(DbKey::TransactionKernel(h)))
    );
}

#[test]
fn insert_and_fetch_kernel() {
    let store = create_mem_db();
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone(), true);
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_kernel(hash), Ok(kernel));
}

#[test]
fn fetch_nonexistent_header() {
    let store = create_mem_db();
    assert_eq!(
        store.fetch_header(0),
        Err(ChainStorageError::ValueNotFound(DbKey::BlockHeader(0)))
    );
}

#[test]
fn insert_and_fetch_header() {
    let store = create_mem_db();
    let mut header = BlockHeader::new(0);
    header.height = 42;

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(
        store.fetch_header(0),
        Err(ChainStorageError::ValueNotFound(DbKey::BlockHeader(0)))
    );
    assert_eq!(store.fetch_header(42), Ok(header));
}

#[test]
fn insert_and_fetch_utxo() {
    let factories = CryptoFactories::default();
    let store = create_mem_db();
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
    let hash = utxo.hash();
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), false);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), true);
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), true);
    assert_eq!(store.fetch_utxo(hash), Ok(utxo));
}

#[test]
fn insert_and_fetch_orphan() {
    let store = create_mem_db();
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs);
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash), Ok(orphan));
}

#[test]
fn multiple_threads() {
    let store = create_mem_db();
    // Save a kernel in thread A
    let store_a = store.clone();
    let a = thread::spawn(move || {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel.clone(), true);
        assert!(store_a.commit(txn).is_ok());
        hash
    });
    // Save a kernel in thread B
    let store_b = store.clone();
    let b = thread::spawn(move || {
        let kernel = create_test_kernel(10.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel.clone(), true);
        assert!(store_b.commit(txn).is_ok());
        hash
    });
    let hash_a = a.join().unwrap();
    let hash_b = b.join().unwrap();
    // Get the kernels back
    let kernel_a = store.fetch_kernel(hash_a).unwrap();
    assert_eq!(kernel_a.fee, 5.into());
    let kernel_b = store.fetch_kernel(hash_b).unwrap();
    assert_eq!(kernel_b.fee, 10.into());
}

#[test]
fn utxo_and_rp_merkle_root() {
    let store = create_mem_db();
    let factories = CryptoFactories::default();
    let root = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        &root.to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(10_000), &factories);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();
    // Calculate the Range proof MMR root as a check
    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert_eq!(rp_mmr_check.push(&utxo1.proof.hash()).unwrap(), 1);
    assert_eq!(rp_mmr_check.push(&utxo2.proof.hash()).unwrap(), 2);
    // Store the UTXOs
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_utxo(utxo2, true);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
    let rp_root = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
    assert_eq!(rp_root.to_hex(), rp_mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn kernel_merkle_root() {
    let store = create_mem_db();
    let root = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        &root.to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let kernel3 = create_test_kernel(300.into(), 0);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();
    let hash3 = kernel3.hash();
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1, true);
    txn.insert_kernel(kernel2, true);
    txn.insert_kernel(kernel3, true);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert!(mmr_check.push(&hash3).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn utxo_and_rp_future_merkle_root() {
    let store = create_mem_db();
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let utxo_hash2 = utxo2.hash();
    let rp_hash2 = utxo2.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    assert!(store.commit(txn).is_ok());

    let utxo_future_root = store
        .calculate_mmr_root(MmrTree::Utxo, vec![utxo_hash2], Vec::new())
        .unwrap()
        .to_hex();
    let rp_future_root = store
        .calculate_mmr_root(MmrTree::RangeProof, vec![rp_hash2], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(utxo_future_root, store.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    assert_ne!(
        rp_future_root,
        store.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex()
    );

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2, true);
    assert!(store.commit(txn).is_ok());

    assert_eq!(utxo_future_root, store.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    assert_eq!(
        rp_future_root,
        store.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex()
    );
}

#[test]
fn kernel_future_merkle_root() {
    let store = create_mem_db();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let hash2 = kernel2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1, true);
    assert!(store.commit(txn).is_ok());

    let future_root = store
        .calculate_mmr_root(MmrTree::Kernel, vec![hash2], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(future_root, store.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel2, true);
    assert!(store.commit(txn).is_ok());

    assert_eq!(future_root, store.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());
}

#[test]
fn utxo_and_rp_mmr_proof() {
    let store = create_mem_db();
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(5_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo3, _) = create_utxo(MicroTari(15_000), &factories);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    txn.insert_utxo(utxo3.clone(), true);
    assert!(store.commit(txn).is_ok());

    let root = store.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    let proof1 = store.fetch_mmr_proof(MmrTree::Utxo, 0).unwrap();
    let proof2 = store.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    let proof3 = store.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 0).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&root, &utxo1.hash(), 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&root, &utxo2.hash(), 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&root, &utxo3.hash(), 2).is_ok());
}

#[test]
fn kernel_mmr_proof() {
    let store = create_mem_db();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), true);
    txn.insert_kernel(kernel2.clone(), true);
    txn.insert_kernel(kernel3.clone(), true);
    assert!(store.commit(txn).is_ok());

    let root = store.fetch_mmr_only_root(MmrTree::Kernel).unwrap();
    let proof1 = store.fetch_mmr_proof(MmrTree::Kernel, 0).unwrap();
    let proof2 = store.fetch_mmr_proof(MmrTree::Kernel, 1).unwrap();
    let proof3 = store.fetch_mmr_proof(MmrTree::Kernel, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&root, &kernel1.hash(), 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&root, &kernel2.hash(), 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&root, &kernel3.hash(), 2).is_ok());
}

#[test]
fn store_and_retrieve_block() {
    // Create new database
    let (store, blocks, _) = create_new_blockchain();
    let hash = blocks[0].hash();
    // Check the metadata
    let metadata = store.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(0));
    assert_eq!(metadata.best_block, Some(hash));
    assert_eq!(metadata.horizon_block(metadata.height_of_longest_chain.unwrap()), 0);
    // Fetch the block back
    let block2 = store.fetch_block(0).unwrap();
    assert_eq!(block2.confirmations(), 1);
    // Compare the blocks
    let block2 = Block::from(block2);
    assert_eq!(blocks[0], block2);
}

#[test]
fn add_multiple_blocks() {
    init_log();
    // Create new database
    let store = create_mem_db();
    let metadata = store.get_metadata().unwrap();
    let factories = CryptoFactories::default();
    assert_eq!(metadata.height_of_longest_chain, None);
    assert_eq!(metadata.best_block, None);
    // Add the Genesis block
    let (block0, _) = create_genesis_block(&store, &factories);
    store.add_block(block0.clone()).unwrap();
    // Add another block
    let block1 = append_block(&store, &block0, vec![]).unwrap();
    let metadata = store.get_metadata().unwrap();
    let hash = block1.hash();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    assert_eq!(metadata.best_block.unwrap(), hash);
    // Adding blocks is idempotent
    assert_eq!(store.add_block(block1.clone()), Ok(BlockAddResult::BlockExists));
    // Check the metadata
    let metadata = store.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    assert_eq!(metadata.best_block.unwrap(), hash);
}

#[test]
fn test_checkpoints() {
    let factories = CryptoFactories::default();
    let store = create_mem_db();
    // Add the Genesis block
    let (block0, output) = create_genesis_block(&store, &factories);
    store.add_block(block0.clone()).unwrap();
    let txn = txn_schema!(from: vec![output], to: vec![MicroTari(5_000), MicroTari(6_000)]);
    let (txn, _, _) = spend_utxos(txn);
    let block1 = append_block(&store, &block0, vec![txn]).unwrap();
    // Get the checkpoint
    let block_a = store.fetch_block(0).unwrap();
    assert_eq!(block_a.confirmations(), 2);
    assert_eq!(block0, Block::from(block_a));
    let block_b = store.fetch_block(1).unwrap();
    assert_eq!(block_b.confirmations(), 1);
    let block1 = serde_json::to_string(&block1).unwrap();
    let block_b = serde_json::to_string(&Block::from(block_b)).unwrap();
    assert_eq!(block1, block_b);
}

#[test]
fn rewind_to_height() {
    let factories = CryptoFactories::default();
    let mut db = create_mem_db();
    let (block0, output) = create_genesis_block_with_utxos(&db, &factories, &[10 * T]);
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![output];
    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][1].clone()], to: vec![6 * T, 3 * T])];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema).unwrap();
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema).unwrap();
    // Block 3
    let schema = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T, 500_000 * uT]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![500_000 * uT]),
    ];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema).unwrap();

    assert!(db.rewind_to_height(3).is_ok());
    // Check MMRs are correct
    let mmr_check = blocks[3].header.kernel_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::Kernel).unwrap();
    assert_eq!(mmr, mmr_check);
    let mmr_check = blocks[3].header.range_proof_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    assert_eq!(mmr, mmr_check);
    let mmr_check = blocks[3].header.output_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::Utxo).unwrap();
    assert_eq!(mmr, mmr_check);
    // Invalid rewind
    assert!(db.rewind_to_height(4).is_err());
    assert!(db.rewind_to_height(1).is_ok());
    // Check MMRs are correct
    let mmr_check = blocks[1].header.kernel_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::Kernel).unwrap();
    assert_eq!(mmr, mmr_check);
    let mmr_check = blocks[1].header.range_proof_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    assert_eq!(mmr, mmr_check);
    let mmr_check = blocks[1].header.output_mr.clone();
    let mmr = db.fetch_mmr_root(MmrTree::Utxo).unwrap();
    assert_eq!(mmr, mmr_check);
}

#[test]
#[ignore] // TODO Wait for reorg logic to be refactored
fn handle_reorg() {
    // GB --> A1 --> A2(Main Chain)
    //          \--> B2(?) --> B3 --> B4 (Orphan Chain)
    // Initially, the main chain is GB->A1-A2 with orphaned blocks B3, B4. When B2 arrives late and is added to
    // the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3->B4

    let (mut store, mut blocks, mut outputs) = create_new_blockchain();
    // A parallel store that will "mine" the orphan chain
    let mut orphan_store = create_mem_db();
    orphan_store.add_block(blocks[0].clone()).unwrap();

    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    assert!(generate_new_block(&mut store, &mut blocks, &mut outputs, txs).is_ok());
    orphan_store.add_block(blocks[1].clone()).unwrap();
    let mut orphan_blocks = blocks.clone();
    let mut orphan_outputs = outputs.clone();

    // Fork happens from here.

    // Block A2 - main chain
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    assert!(generate_new_block(&mut store, &mut blocks, &mut outputs, txs).is_ok());

    // Block B2 - forked chain
    let txs = vec![txn_schema!(from: vec![orphan_outputs[1][0].clone()], to: vec![5 * T])];
    assert!(generate_new_block(&mut orphan_store, &mut orphan_blocks, &mut orphan_outputs, txs).is_ok());
    // Block B3
    let txs = vec![
        txn_schema!(from: vec![orphan_outputs[1][3].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan_outputs[2][0].clone()], to: vec![3 * T]),
    ];
    assert!(generate_new_block(&mut orphan_store, &mut orphan_blocks, &mut orphan_outputs, txs).is_ok());
    // Block B3
    let txs = vec![txn_schema!(from: vec![orphan_outputs[3][0].clone()], to: vec![1 * T])];
    assert!(generate_new_block(&mut orphan_store, &mut orphan_blocks, &mut orphan_outputs, txs).is_ok());

    // Now add the fork blocks to the first DB and observe a re-org
    store.add_block(orphan_blocks[3].clone()).unwrap();
    store.add_block(orphan_blocks[4].clone()).unwrap();
    println!("Block 1: {}", blocks[1]);
    println!("Block 2: {}", blocks[2]);
    println!("Orphan block 1: {}", orphan_blocks[2]);
    println!("Orphan block 2: {}", orphan_blocks[3]);
    assert_eq!(
        store.add_block(orphan_blocks[2].clone()),
        Ok(BlockAddResult::ChainReorg)
    );
}

#[test]
fn store_and_retrieve_block_with_mmr_pruning_horizon() {
    let factories = CryptoFactories::default();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 2,
        max_history_len: 3,
    };
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let db = MemoryDatabase::<HashDigest>::new(mct_config);
    let mut store = BlockchainDatabase::new(db).unwrap();
    store.set_validators(validators);

    let (block0, _) = create_genesis_block(&store, &factories);
    store.add_block(block0.clone()).unwrap();
    let block1 = append_block(&store, &block0, vec![]).unwrap();
    let block2 = append_block(&store, &block1, vec![]).unwrap();

    assert_eq!(*store.fetch_block(0).unwrap().block(), block0);
    assert_eq!(*store.fetch_block(1).unwrap().block(), block1);
    assert_eq!(*store.fetch_block(2).unwrap().block(), block2);

    // When block3 is added then maximum history length would have been reached and block0 and block  will be committed
    // to the base MMR.
    let block3 = append_block(&store, &block2, vec![]).unwrap();

    assert!(store.fetch_block(0).is_err());
    assert!(store.fetch_block(1).is_err());
    assert_eq!(*store.fetch_block(2).unwrap().block(), block2);
    assert_eq!(*store.fetch_block(3).unwrap().block(), block3);
}

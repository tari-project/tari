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
    block_builders::{append_block, generate_new_block, generate_new_block_with_achieved_difficulty},
    sample_blockchains::create_new_blockchain,
};
use croaring::Bitmap;
use env_logger;
use std::thread;
use tari_core::{
    blocks::{genesis_block, Block, BlockHash, BlockHeader},
    chain_storage::{
        create_lmdb_database,
        BlockAddResult,
        BlockchainDatabase,
        ChainStorageError,
        DbKey,
        DbTransaction,
        MemoryDatabase,
        MmrTree,
        Validators,
    },
    consensus::{ConsensusManagerBuilder, Network},
    helpers::{create_mem_db, create_orphan_block},
    proof_of_work::Difficulty,
    transactions::{
        helpers::{create_test_kernel, create_utxo, spend_utxos},
        tari_amount::{uT, MicroTari, T},
        types::{CryptoFactories, HashDigest},
    },
    tx,
    txn_schema,
    validation::mocks::MockValidator,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_mmr::{MmrCacheConfig, MutableMmr};
use tari_test_utils::paths::create_temporary_data_path;

fn init_log() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn fetch_nonexistent_kernel() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let h = vec![0u8; 32];
    assert_eq!(
        store.fetch_kernel(h.clone()),
        Err(ChainStorageError::ValueNotFound(DbKey::TransactionKernel(h)))
    );
}

#[test]
fn insert_and_fetch_kernel() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone(), true);
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_kernel(hash), Ok(kernel));
}

#[test]
fn fetch_nonexistent_header() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    assert_eq!(
        store.fetch_header(1),
        Err(ChainStorageError::ValueNotFound(DbKey::BlockHeader(1)))
    );
}

#[test]
fn insert_and_fetch_header() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let mut header = BlockHeader::new(0);
    header.height = 42;

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(store.commit(txn).is_ok());
    assert!(store.fetch_header(0).is_ok());
    assert_eq!(
        store.fetch_header(1),
        Err(ChainStorageError::ValueNotFound(DbKey::BlockHeader(1)))
    );
    assert_eq!(store.fetch_header(42), Ok(header));
}

#[test]
fn insert_and_fetch_utxo() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
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
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus_manager.consensus_constants());
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash), Ok(orphan));
}

#[test]
fn multiple_threads() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
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
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(consensus_manager.clone());
    let factories = CryptoFactories::default();
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let utxo0 = block0.body.outputs()[0].clone();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(10_000), &factories, None);
    let hash0 = utxo0.hash();
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();
    // Calculate the Range proof MMR root as a check
    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert_eq!(rp_mmr_check.push(&utxo0.proof.hash()).unwrap(), 1);
    assert_eq!(rp_mmr_check.push(&utxo1.proof.hash()).unwrap(), 2);
    assert_eq!(rp_mmr_check.push(&utxo2.proof.hash()).unwrap(), 3);
    // Store the UTXOs
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_utxo(utxo2, true);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
    let rp_root = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(mmr_check.push(&hash0).is_ok());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
    assert_eq!(rp_root.to_hex(), rp_mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn kernel_merkle_root() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let kernel3 = create_test_kernel(300.into(), 0);
    let hash0 = block0.body.kernels()[0].hash();
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
    assert!(mmr_check.push(&hash0).is_ok());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert!(mmr_check.push(&hash3).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn utxo_and_rp_future_merkle_root() {
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(consensus_manager.clone());
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
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
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(consensus_manager.clone());

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
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(consensus_manager.clone());
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(5_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(15_000), &factories, None);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    txn.insert_utxo(utxo3.clone(), true);
    assert!(store.commit(txn).is_ok());

    let root = store.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    let proof1 = store.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    let proof2 = store.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    let proof3 = store.fetch_mmr_proof(MmrTree::Utxo, 3).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    store.fetch_mmr_proof(MmrTree::RangeProof, 3).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&root, &utxo1.hash(), 1).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&root, &utxo2.hash(), 2).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&root, &utxo3.hash(), 3).is_ok());
}

#[test]
fn kernel_mmr_proof() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), true);
    txn.insert_kernel(kernel2.clone(), true);
    txn.insert_kernel(kernel3.clone(), true);
    assert!(store.commit(txn).is_ok());

    let root = store.fetch_mmr_only_root(MmrTree::Kernel).unwrap();
    let proof1 = store.fetch_mmr_proof(MmrTree::Kernel, 1).unwrap();
    let proof2 = store.fetch_mmr_proof(MmrTree::Kernel, 2).unwrap();
    let proof3 = store.fetch_mmr_proof(MmrTree::Kernel, 3).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&root, &kernel1.hash(), 1).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&root, &kernel2.hash(), 2).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&root, &kernel3.hash(), 3).is_ok());
}

#[test]
fn store_and_retrieve_block() {
    let (db, blocks, _, _) = create_new_blockchain(Network::LocalNet);
    let hash = blocks[0].hash();
    // Check the metadata
    let metadata = db.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(0));
    assert_eq!(metadata.best_block, Some(hash));
    assert_eq!(metadata.horizon_block(metadata.height_of_longest_chain.unwrap()), 0);
    // Fetch the block back
    let block0 = db.fetch_block(0).unwrap();
    assert_eq!(block0.confirmations(), 1);
    // Compare the blocks
    let block0 = Block::from(block0);
    assert_eq!(blocks[0], block0);
}

#[test]
fn add_multiple_blocks() {
    init_log();
    // Create new database with genesis block
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let metadata = store.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(0));
    let block0 = store.fetch_block(0).unwrap().block().clone();
    assert_eq!(metadata.best_block, Some(block0.hash()));
    // Add another block
    let block1 = append_block(
        &store,
        &block0,
        vec![],
        &consensus_manager.consensus_constants(),
        1.into(),
    )
    .unwrap();
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
    let network = Network::LocalNet;
    let (db, blocks, outputs, consensus_manager) = create_new_blockchain(network);

    let txn = txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![MicroTari(5_000), MicroTari(6_000)]
    );
    let (txn, _, _) = spend_utxos(txn);
    let block1 = append_block(
        &db,
        &blocks[0],
        vec![txn],
        &consensus_manager.consensus_constants(),
        1.into(),
    )
    .unwrap();
    // Get the checkpoint
    let block_a = db.fetch_block(0).unwrap();
    assert_eq!(block_a.confirmations(), 2);
    assert_eq!(blocks[0], Block::from(block_a));
    let block_b = db.fetch_block(1).unwrap();
    assert_eq!(block_b.confirmations(), 1);
    let block1 = serde_json::to_string(&block1).unwrap();
    let block_b = serde_json::to_string(&Block::from(block_b)).unwrap();
    assert_eq!(block1, block_b);
}

#[test]
fn rewind_to_height() {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![6 * T, 3 * T])];
    generate_new_block(
        &mut db,
        &mut blocks,
        &mut outputs,
        schema,
        &consensus_manager.consensus_constants(),
    )
    .unwrap();
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    generate_new_block(
        &mut db,
        &mut blocks,
        &mut outputs,
        schema,
        &consensus_manager.consensus_constants(),
    )
    .unwrap();
    // Block 3
    let schema = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T, 500_000 * uT]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![500_000 * uT]),
    ];
    generate_new_block(
        &mut db,
        &mut blocks,
        &mut outputs,
        schema,
        &consensus_manager.consensus_constants(),
    )
    .unwrap();

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
fn handle_tip_reorg() {
    // GB --> A1 --> A2(Low PoW)      [Main Chain]
    //          \--> B2(Highest PoW)  [Forked Chain]
    // Initially, the main chain is GB->A1->A2. B2 has a higher accumulated PoW and when B2 is added the main chain is
    // reorged to GB->A1->B2

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager.consensus_constants()
    )
    .is_ok());

    // Create Forked Chain
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan_store = create_mem_db(consensus_manager_fork.clone());
    orphan_store.add_block(blocks[1].clone()).unwrap();
    let mut orphan_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan_outputs[1][0].clone()], to: vec![5 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        txs,
        Difficulty::from(7),
        &consensus_manager_fork.consensus_constants()
    )
    .is_ok());

    // Adding B2 to the main chain will produce a reorg to GB->A1->B2.
    if let Ok(BlockAddResult::ChainReorg(_)) = store.add_block(orphan_blocks[2].clone()) {
        assert!(true);
    } else {
        assert!(false);
    }
    assert_eq!(store.fetch_tip_header(), Ok(orphan_blocks[2].header.clone()));

    // Check that B2 was removed from the block orphans and A2 has been orphaned.
    assert!(store.fetch_orphan(orphan_blocks[2].hash()).is_err());
    assert!(store.fetch_orphan(blocks[2].hash()).is_ok());
}

#[test]
fn handle_reorg() {
    // GB --> A1 --> A2 --> A3 -----> A4(Low PoW)     [Main Chain]
    //          \--> B2 --> B3(?) --> B4(Medium PoW)  [Forked Chain 1]
    //                        \-----> C4(Highest PoW) [Forked Chain 2]
    // Initially, the main chain is GB->A1->A2->A3->A4 with orphaned blocks B2, B4, C4. When B3 arrives late and is
    // added to the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3->C4.

    // Create Main Chain
    let network = Network::LocalNet;
    let (mut store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block A1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block A2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(3),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block A3
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block A4
    let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());

    // Create Forked Chain 1
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan1_store = create_mem_db(consensus_manager_fork); // GB
    orphan1_store.add_block(blocks[1].clone()).unwrap(); // A1
    let mut orphan1_blocks = vec![blocks[0].clone(), blocks[1].clone()];
    let mut orphan1_outputs = vec![outputs[0].clone(), outputs[1].clone()];
    // Block B2
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[1][0].clone()], to: vec![5 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block B3
    let txs = vec![
        txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
        txn_schema!(from: vec![orphan1_outputs[2][0].clone()], to: vec![3 * T]),
    ];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(1),
        &consensus_manager.consensus_constants()
    )
    .is_ok());
    // Block B4
    let txs = vec![txn_schema!(from: vec![orphan1_outputs[3][0].clone()], to: vec![1 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan1_store,
        &mut orphan1_blocks,
        &mut orphan1_outputs,
        txs,
        Difficulty::from(5),
        &consensus_manager.consensus_constants()
    )
    .is_ok());

    // Create Forked Chain 2
    let consensus_manager_fork2 = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan2_store = create_mem_db(consensus_manager_fork2); // GB
    orphan2_store.add_block(blocks[1].clone()).unwrap(); // A1
    orphan2_store.add_block(orphan1_blocks[2].clone()).unwrap(); // B2
    orphan2_store.add_block(orphan1_blocks[3].clone()).unwrap(); // B3
    let mut orphan2_blocks = vec![
        blocks[0].clone(),
        blocks[1].clone(),
        orphan1_blocks[2].clone(),
        orphan1_blocks[3].clone(),
    ];
    let mut orphan2_outputs = vec![
        outputs[0].clone(),
        outputs[1].clone(),
        orphan1_outputs[2].clone(),
        orphan1_outputs[3].clone(),
    ];
    // Block C4
    let txs = vec![txn_schema!(from: vec![orphan2_outputs[3][1].clone()], to: vec![1 * T])];
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan2_store,
        &mut orphan2_blocks,
        &mut orphan2_outputs,
        txs,
        Difficulty::from(20),
        &consensus_manager.consensus_constants()
    )
    .is_ok());

    // Now add the fork blocks C4, B2, B4 and B3 (out of order) to the first DB and observe a re-org. Blocks are added
    // out of order to test the forward and reverse chaining.
    store.add_block(orphan2_blocks[4].clone()).unwrap(); // C4
    store.add_block(orphan1_blocks[2].clone()).unwrap(); // B2
    store.add_block(orphan1_blocks[4].clone()).unwrap(); // B4
    store.add_block(orphan1_blocks[3].clone()).unwrap(); // B3
    assert_eq!(store.fetch_tip_header(), Ok(orphan2_blocks[4].header.clone()));

    // Check that B2,B3 and C4 were removed from the block orphans and A2,A3,A4 and B4 has been orphaned.
    assert!(store.fetch_orphan(orphan1_blocks[2].hash()).is_err()); // B2
    assert!(store.fetch_orphan(orphan1_blocks[3].hash()).is_err()); // B3
    assert!(store.fetch_orphan(orphan2_blocks[4].hash()).is_err()); // C4
    assert!(store.fetch_orphan(blocks[2].hash()).is_ok()); // A2
    assert!(store.fetch_orphan(blocks[3].hash()).is_ok()); // A3
    assert!(store.fetch_orphan(blocks[4].hash()).is_ok()); // A4
    assert!(store.fetch_orphan(blocks[4].hash()).is_ok()); // B4
}

#[test]
fn store_and_retrieve_blocks() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 2 };
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    let mut store = BlockchainDatabase::new(db, rules.clone()).unwrap();
    store.set_validators(validators);

    let block0 = store.fetch_block(0).unwrap().block().clone();
    let block1 = append_block(&store, &block0, vec![], &rules.consensus_constants(), 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &rules.consensus_constants(), 1.into()).unwrap();
    assert_eq!(*store.fetch_block(0).unwrap().block(), block0);
    assert_eq!(*store.fetch_block(1).unwrap().block(), block1);
    assert_eq!(*store.fetch_block(2).unwrap().block(), block2);

    let block3 = append_block(&store, &block2, vec![], &rules.consensus_constants(), 1.into()).unwrap();
    assert_eq!(*store.fetch_block(0).unwrap().block(), block0);
    assert_eq!(*store.fetch_block(1).unwrap().block(), block1);
    assert_eq!(*store.fetch_block(2).unwrap().block(), block2);
    assert_eq!(*store.fetch_block(3).unwrap().block(), block3);
}

#[test]
fn store_and_retrieve_chain_and_orphan_blocks_with_hashes() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 2 };
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    let mut store = BlockchainDatabase::new(db, rules.clone()).unwrap();
    store.set_validators(validators);

    let block0 = store.fetch_block(0).unwrap().block().clone();
    let block1 = append_block(&store, &block0, vec![], &rules.consensus_constants(), 1.into()).unwrap();
    let orphan = create_orphan_block(10, vec![], &rules.consensus_constants());
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(store.commit(txn).is_ok());

    let hash0 = block0.hash();
    let hash1 = block1.hash();
    let hash2 = orphan.hash();
    assert_eq!(*store.fetch_block_with_hash(hash0).unwrap().unwrap().block(), block0);
    assert_eq!(*store.fetch_block_with_hash(hash1).unwrap().unwrap().block(), block1);
    assert_eq!(*store.fetch_block_with_hash(hash2).unwrap().unwrap().block(), orphan);
}

#[test]
fn total_kernel_excess() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let kernel3 = create_test_kernel(300.into(), 0);

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), false);
    txn.insert_kernel(kernel2.clone(), false);
    txn.insert_kernel(kernel3.clone(), false);
    assert!(store.commit(txn).is_ok());

    let total_kernel_excess = store.total_kernel_excess().unwrap();
    assert_eq!(
        total_kernel_excess,
        &(&(block0.body.kernels()[0].excess) + &kernel1.excess) + &(&kernel2.excess + &kernel3.excess)
    );
}

#[test]
fn total_kernel_offset() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(consensus_manager.clone());
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let header2 = BlockHeader::from_previous(&block0.header);
    let header3 = BlockHeader::from_previous(&header2);
    let mut txn = DbTransaction::new();
    txn.insert_header(header2.clone());
    txn.insert_header(header3.clone());
    assert!(store.commit(txn).is_ok());

    let total_kernel_offset = store.total_kernel_offset().unwrap();
    assert_eq!(
        total_kernel_offset,
        &(&block0.header.total_kernel_offset + &header2.total_kernel_offset) + &header3.total_kernel_offset
    );
}

#[test]
fn total_utxo_commitment() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(consensus_manager.clone());
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    txn.insert_utxo(utxo3.clone(), true);
    assert!(store.commit(txn).is_ok());

    let total_utxo_commitment = store.total_utxo_commitment().unwrap();
    assert_eq!(
        total_utxo_commitment,
        &(&(block0.body.outputs()[0].commitment) + &utxo1.commitment) + &(&utxo2.commitment + &utxo3.commitment)
    );
}

#[test]
fn restore_metadata() {
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let block_hash: BlockHash;
    let path = create_temporary_data_path();
    {
        let db = create_lmdb_database(&path, MmrCacheConfig::default()).unwrap();
        let mut db = BlockchainDatabase::new(db, rules.clone()).unwrap();
        db.set_validators(validators.clone());

        let block0 = db.fetch_block(0).unwrap().block().clone();
        let block1 = append_block(&db, &block0, vec![], &rules.consensus_constants(), 1.into()).unwrap();
        db.add_block(block1.clone()).unwrap();
        block_hash = block1.hash();
        let metadata = db.get_metadata().unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(1));
        assert_eq!(metadata.best_block, Some(block_hash.clone()));
    }
    // Restore blockchain db
    let db = create_lmdb_database(&path, MmrCacheConfig::default()).unwrap();
    let mut db = BlockchainDatabase::new(db, rules.clone()).unwrap();
    db.set_validators(validators);

    let metadata = db.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    assert_eq!(metadata.best_block, Some(block_hash));
}

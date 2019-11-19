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
//

use crate::{
    blocks::{genesis_block::get_genesis_block, Block, BlockHeader},
    chain_storage::{
        blockchain_database::BlockAddResult,
        db_transaction::DbKey,
        error::ChainStorageError,
        BlockchainDatabase,
        DbTransaction,
        MemoryDatabase,
        MmrTree,
    },
    test_utils::{
        builders::{
            add_block_and_update_header,
            chain_block,
            create_genesis_block,
            create_test_block,
            create_test_kernel,
            create_utxo,
            spend_utxos,
        },
        sample_blockchains::{create_new_blockchain, generate_new_block},
    },
    tx,
    txn_schema,
};
use env_logger;
use std::thread;
use tari_mmr::{MerkleChangeTrackerConfig, MutableMmr};
use tari_transactions::{
    tari_amount::{uT, MicroTari, T},
    types::{CryptoFactories, HashDigest},
};
use tari_utilities::{hex::Hex, Hashable};

fn init_log() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[test]
fn fetch_nonexistent_kernel() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let h = vec![0u8; 32];
    assert_eq!(
        store.fetch_kernel(h.clone()),
        Err(ChainStorageError::ValueNotFound(DbKey::TransactionKernel(h)))
    );
}

#[test]
fn insert_and_fetch_kernel() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_kernel(hash), Ok(kernel));
}

#[test]
fn fetch_nonexistent_header() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    assert_eq!(
        store.fetch_header(0),
        Err(ChainStorageError::ValueNotFound(DbKey::BlockHeader(0)))
    );
}

#[test]
fn insert_and_fetch_header() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
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
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
    let hash = utxo.hash();
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), false);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), true);
    assert_eq!(store.fetch_utxo(hash), Ok(utxo));
}

#[test]
fn insert_and_fetch_orphan() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_test_block(10, None, txs);
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash), Ok(orphan));
}

#[test]
fn multiple_threads() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    // Save a kernel in thread A
    let store_a = store.clone();
    let a = thread::spawn(move || {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel.clone());
        assert!(store_a.commit(txn).is_ok());
        hash
    });
    // Save a kernel in thread B
    let store_b = store.clone();
    let b = thread::spawn(move || {
        let kernel = create_test_kernel(10.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel.clone());
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
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
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
    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert_eq!(rp_mmr_check.push(&utxo1.proof.hash()).unwrap(), 1);
    assert_eq!(rp_mmr_check.push(&utxo2.proof.hash()).unwrap(), 2);
    // Store the UTXOs
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_utxo(utxo2);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
    let rp_root = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
    assert_eq!(rp_root.to_hex(), rp_mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn header_merkle_root() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let root = store.fetch_mmr_root(MmrTree::Header).unwrap();
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        &root.to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    let header1 = BlockHeader::new(0);
    let mut header2 = BlockHeader::new(0);
    header2.height = 1;
    let hash1 = header1.hash();
    let hash2 = header2.hash();
    let mut txn = DbTransaction::new();
    txn.insert_header(header1);
    txn.insert_header(header2);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Header).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn kernel_merkle_root() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
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
    txn.insert_kernel(kernel1);
    txn.insert_kernel(kernel2);
    txn.insert_kernel(kernel3);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert!(mmr_check.push(&hash3).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn utxo_and_rp_mmr_proof() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(5_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo3, _) = create_utxo(MicroTari(15_000), &factories);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    txn.insert_utxo(utxo2.clone());
    txn.insert_utxo(utxo3.clone());
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
fn header_mmr_proof() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();

    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    let mut header3 = BlockHeader::new(0);
    header3.height = 3;
    let mut txn = DbTransaction::new();
    txn.insert_header(header1.clone());
    txn.insert_header(header2.clone());
    txn.insert_header(header3.clone());
    assert!(store.commit(txn).is_ok());

    let root = store.fetch_mmr_only_root(MmrTree::Header).unwrap();
    let proof1 = store.fetch_mmr_proof(MmrTree::Header, 0).unwrap();
    let proof2 = store.fetch_mmr_proof(MmrTree::Header, 1).unwrap();
    let proof3 = store.fetch_mmr_proof(MmrTree::Header, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&root, &header1.hash(), 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&root, &header2.hash(), 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&root, &header3.hash(), 2).is_ok());
}

#[test]
fn kernel_mmr_proof() {
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone());
    txn.insert_kernel(kernel2.clone());
    txn.insert_kernel(kernel3.clone());
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
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let metadata = store.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, None);
    assert_eq!(metadata.best_block, None);
    // Add the Genesis block
    let block_gb = add_block_and_update_header(&store, get_genesis_block());
    println!("{}\nHash={}", block_gb, block_gb.hash().to_hex());
    // Add another block
    let mut block = chain_block(&block_gb, vec![]);
    println!("{}", block);
    let metadata = store.get_metadata().unwrap();
    println!("{}", metadata);
    block = add_block_and_update_header(&store, block);
    let hash = block.hash();
    // Adding blocks is idempotent
    assert_eq!(store.add_block(block.clone()), Ok(BlockAddResult::BlockExists));
    // Check the metadata
    let metadata = store.get_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    assert_eq!(metadata.best_block, Some(hash));
}

#[test]
fn test_checkpoints() {
    let factories = CryptoFactories::default();
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    // Add the Genesis block
    let (mut block0, output) = create_genesis_block(&factories);
    block0 = add_block_and_update_header(&store, block0);
    let txn = txn_schema!(from: vec![output], to: vec![MicroTari(5_000), MicroTari(6_000)]);
    let (txn, _, _) = spend_utxos(txn);
    let mut block1 = chain_block(&block0, vec![txn]);
    block1 = add_block_and_update_header(&store, block1);
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
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let block0 = add_block_and_update_header(&store, create_genesis_block(&factories).0);

    let (tx1, inputs1, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx2, inputs2, _) = tx!(10_000*uT, fee: 20*uT, inputs: 1, outputs: 1);
    let (tx3, inputs3, _) = tx!(10_000*uT, fee: 100*uT, inputs: 1, outputs: 1);
    let (tx4, inputs4, _) = tx!(10_000*uT, fee: 30*uT, inputs: 1, outputs: 1);
    let (tx5, inputs5, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx6, inputs6, _) = tx!(10_000*uT, fee: 75*uT, inputs: 1, outputs: 1);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(inputs1[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs2[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs3[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs4[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs5[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs6[0].as_transaction_output(&factories).unwrap());
    assert!(store.commit(txn).is_ok());

    let mut block1 = chain_block(&block0, vec![tx1.clone(), tx2.clone()]);
    block1 = add_block_and_update_header(&store, block1);
    let mut block2 = chain_block(&block1, vec![tx3.clone()]);
    block2 = add_block_and_update_header(&store, block2);
    let mut block3 = chain_block(&block2, vec![tx4.clone(), tx5.clone(), tx6.clone()]);
    block3 = add_block_and_update_header(&store, block3);

    assert!(store.rewind_to_height(3).is_ok());
    assert!(store.rewind_to_height(4).is_err());

    let tx1_input_hash = tx1.body.inputs()[0].hash();
    let tx2_input_hash = tx2.body.inputs()[0].hash();
    let tx3_input_hash = tx3.body.inputs()[0].hash();
    let tx4_input_hash = tx4.body.inputs()[0].hash();
    let tx5_input_hash = tx5.body.inputs()[0].hash();
    let tx6_input_hash = tx6.body.inputs()[0].hash();
    let tx1_output_hash = tx1.body.outputs()[0].hash();
    let tx2_output_hash = tx2.body.outputs()[0].hash();
    let tx3_output_hash = tx3.body.outputs()[0].hash();
    let tx4_output_hash = tx4.body.outputs()[0].hash();
    let tx5_output_hash = tx5.body.outputs()[0].hash();
    let tx6_output_hash = tx6.body.outputs()[0].hash();
    let tx1_kernel_hash = tx1.body.kernels()[0].hash();
    let tx2_kernel_hash = tx2.body.kernels()[0].hash();
    let tx3_kernel_hash = tx3.body.kernels()[0].hash();
    let tx4_kernel_hash = tx4.body.kernels()[0].hash();
    let tx5_kernel_hash = tx5.body.kernels()[0].hash();
    let tx6_kernel_hash = tx6.body.kernels()[0].hash();
    let block0_header_hash = block0.header.hash();
    let block1_header_hash = block1.header.hash();
    let block2_header_hash = block2.header.hash();
    let block3_header_hash = block3.header.hash();

    assert_eq!(store.fetch_header(0).unwrap().height, 0);
    assert_eq!(store.fetch_header(1).unwrap().height, 1);
    assert_eq!(store.fetch_header(2).unwrap().height, 2);
    assert_eq!(store.fetch_header(3).unwrap().height, 3);
    assert_eq!(store.fetch_header(4).is_ok(), false);

    assert!(store.fetch_kernel(tx1_kernel_hash.clone()).is_ok());
    assert!(store.fetch_kernel(tx2_kernel_hash.clone()).is_ok());
    assert!(store.fetch_kernel(tx3_kernel_hash.clone()).is_ok());
    assert!(store.fetch_kernel(tx4_kernel_hash.clone()).is_ok());
    assert!(store.fetch_kernel(tx5_kernel_hash.clone()).is_ok());
    assert!(store.fetch_kernel(tx6_kernel_hash.clone()).is_ok());

    assert_eq!(store.is_utxo(tx1_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx2_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx3_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx4_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx5_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx6_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx1_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx2_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx3_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx4_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx5_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx6_output_hash.clone()), Ok(true));

    assert!(store.fetch_stxo(tx1_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx2_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx3_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx4_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx5_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx6_input_hash.clone()).is_ok());
    assert!(store.fetch_stxo(tx1_output_hash.clone()).is_err());
    assert!(store.fetch_stxo(tx2_output_hash.clone()).is_err());
    assert!(store.fetch_stxo(tx3_output_hash.clone()).is_err());
    assert!(store.fetch_stxo(tx4_output_hash.clone()).is_err());
    assert!(store.fetch_stxo(tx5_output_hash.clone()).is_err());
    assert!(store.fetch_stxo(tx6_output_hash.clone()).is_err());

    assert!(store.fetch_orphan(block0_header_hash.clone()).is_err());
    assert!(store.fetch_orphan(block1_header_hash.clone()).is_err());
    assert!(store.fetch_orphan(block2_header_hash.clone()).is_err());
    assert!(store.fetch_orphan(block3_header_hash.clone()).is_err());

    assert!(store.rewind_to_height(1).is_ok());

    assert_eq!(store.fetch_header(0).unwrap().height, 0);
    assert_eq!(store.fetch_header(1).unwrap().height, 1);
    assert_eq!(store.fetch_header(2).is_ok(), false);
    assert_eq!(store.fetch_header(3).is_ok(), false);

    assert!(store.fetch_kernel(tx1_kernel_hash).is_ok());
    assert!(store.fetch_kernel(tx2_kernel_hash).is_ok());
    assert!(store.fetch_kernel(tx3_kernel_hash).is_err());
    assert!(store.fetch_kernel(tx4_kernel_hash).is_err());
    assert!(store.fetch_kernel(tx5_kernel_hash).is_err());
    assert!(store.fetch_kernel(tx6_kernel_hash).is_err());

    assert_eq!(store.is_utxo(tx1_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx2_input_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx3_input_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx4_input_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx5_input_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx6_input_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx1_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx2_output_hash.clone()), Ok(true));
    assert_eq!(store.is_utxo(tx3_output_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx4_output_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx5_output_hash.clone()), Ok(false));
    assert_eq!(store.is_utxo(tx6_output_hash.clone()), Ok(false));

    assert!(store.fetch_stxo(tx1_input_hash).is_ok());
    assert!(store.fetch_stxo(tx2_input_hash).is_ok());
    assert!(store.fetch_stxo(tx3_input_hash).is_err());
    assert!(store.fetch_stxo(tx4_input_hash).is_err());
    assert!(store.fetch_stxo(tx5_input_hash).is_err());
    assert!(store.fetch_stxo(tx6_input_hash).is_err());
    assert!(store.fetch_stxo(tx1_output_hash).is_err());
    assert!(store.fetch_stxo(tx2_output_hash).is_err());
    assert!(store.fetch_stxo(tx3_output_hash).is_err());
    assert!(store.fetch_stxo(tx4_output_hash).is_err());
    assert!(store.fetch_stxo(tx5_output_hash).is_err());
    assert!(store.fetch_stxo(tx6_output_hash).is_err());

    assert!(store.fetch_orphan(block0_header_hash).is_err());
    assert!(store.fetch_orphan(block1_header_hash).is_err());
    assert!(store.fetch_orphan(block2_header_hash).is_ok());
    assert!(store.fetch_orphan(block3_header_hash).is_ok());
}

#[test]
fn handle_reorg() {
    // GB --> A1 --> A2(Main Chain)
    //          \--> B2(?) --> B3 --> B4 (Orphan Chain)
    // Initially, the main chain is GB->A1-A2 with orphaned blocks B3, B4. When B2 arrives late and is added to
    // the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3->B4

    let (mut store, mut blocks, mut outputs) = create_new_blockchain();
    // A parallel store that will "mine" the orphan chain
    let mut orphan_store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    println!("Genesis block:\n{}", blocks[0]);
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
    assert_eq!(
        store.add_block(orphan_blocks[2].clone()),
        Ok(BlockAddResult::ChainReorg)
    );
}

#[test]
fn restore_mmr() {
    let factories = CryptoFactories::default();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 2,
        max_history_len: 3,
    };
    let store = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::new(mct_config)).unwrap();

    let block0 = add_block_and_update_header(&store, create_genesis_block(&factories).0);

    let (tx1, inputs1, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx2, inputs2, _) = tx!(10_000*uT, fee: 20*uT, inputs: 1, outputs: 1);
    let (tx3, inputs3, _) = tx!(10_000*uT, fee: 100*uT, inputs: 1, outputs: 1);
    let (tx4, inputs4, _) = tx!(10_000*uT, fee: 30*uT, inputs: 1, outputs: 1);
    let (tx5, inputs5, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
    let (tx6, inputs6, _) = tx!(10_000*uT, fee: 75*uT, inputs: 1, outputs: 1);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(inputs1[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs2[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs3[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs4[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs5[0].as_transaction_output(&factories).unwrap());
    txn.insert_utxo(inputs6[0].as_transaction_output(&factories).unwrap());
    assert!(store.commit(txn).is_ok());

    let mut block1 = chain_block(&block0, vec![tx1.clone(), tx2.clone()]);
    block1 = add_block_and_update_header(&store, block1);
    let mut block2 = chain_block(&block1, vec![tx3.clone()]);
    block2 = add_block_and_update_header(&store, block2);
    let mut block3 = chain_block(&block2, vec![tx4.clone()]);
    block3 = add_block_and_update_header(&store, block3);

    // Genesis block and block 1 has been added to base MMR
    let utxo_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100).unwrap();
    let kernel_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100).unwrap();
    let rp_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100).unwrap();
    let header_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100).unwrap();

    assert_eq!(utxo_mmr_state.total_leaf_count, 9);
    assert_eq!(kernel_mmr_state.total_leaf_count, 3);
    assert_eq!(rp_mmr_state.total_leaf_count, 9);
    assert_eq!(header_mmr_state.total_leaf_count, 2);

    let mut block4 = chain_block(&block3, vec![tx5.clone()]);
    block4 = add_block_and_update_header(&store, block4);
    let block5 = chain_block(&block4, vec![tx6.clone()]);
    store.add_new_block(block5.clone()).unwrap();

    let utxo_mmr_leaf_count = store
        .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100)
        .unwrap()
        .total_leaf_count;
    let kernel_mmr_leaf_count = store
        .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100)
        .unwrap()
        .total_leaf_count;
    let rp_mmr_leaf_count = store
        .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100)
        .unwrap()
        .total_leaf_count;
    let header_mmr_leaf_count = store
        .fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100)
        .unwrap()
        .total_leaf_count;
    assert_eq!(utxo_mmr_leaf_count, 11);
    assert_eq!(kernel_mmr_leaf_count, 5);
    assert_eq!(rp_mmr_leaf_count, 11);
    assert_eq!(header_mmr_leaf_count, 4);

    // Restore previously retrieved MMR state
    assert!(store
        .restore_mmr(MmrTree::Utxo, utxo_mmr_state.leaf_nodes.clone())
        .is_ok());
    assert!(store
        .restore_mmr(MmrTree::Kernel, kernel_mmr_state.leaf_nodes.clone())
        .is_ok());
    assert!(store
        .restore_mmr(MmrTree::RangeProof, rp_mmr_state.leaf_nodes.clone())
        .is_ok());
    assert!(store
        .restore_mmr(MmrTree::Header, header_mmr_state.leaf_nodes.clone())
        .is_ok());

    let restore_utxo_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100).unwrap();
    let restore_kernel_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100).unwrap();
    let restore_rp_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100).unwrap();
    let restore_header_mmr_state = store.fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100).unwrap();
    assert_eq!(restore_utxo_mmr_state, utxo_mmr_state);
    assert_eq!(restore_kernel_mmr_state, kernel_mmr_state);
    assert_eq!(restore_rp_mmr_state, rp_mmr_state);
    assert_eq!(restore_header_mmr_state, header_mmr_state);
}

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
    block_builders::{
        append_block,
        chain_block,
        create_genesis_block,
        find_header_with_achieved_difficulty,
        generate_new_block,
        generate_new_block_with_achieved_difficulty,
        generate_new_block_with_coinbase,
    },
    database::{create_mem_db, create_orphan_block},
    sample_blockchains::{create_new_blockchain, create_new_blockchain_lmdb},
    test_blockchain::TestBlockchain,
};
use croaring::Bitmap;
use rand::{rngs::OsRng, RngCore};
use std::thread;
use tari_core::{
    blocks::{genesis_block, Block, BlockHeader},
    chain_storage::{
        create_lmdb_database,
        BlockAddResult,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainMetadata,
        ChainStorageError,
        DbTransaction,
        InProgressHorizonSyncState,
        MemoryDatabase,
        MmrTree,
        Validators,
    },
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    proof_of_work::Difficulty,
    transactions::{
        helpers::{create_test_kernel, spend_utxos},
        tari_amount::{uT, MicroTari, T},
        types::{BlockHash, CryptoFactories, HashDigest},
        OutputBuilder,
        TransactionOutput,
    },
    tx,
    txn_schema,
    validation::{block_validators::MockStatelessBlockValidator, mocks::MockValidator, ValidationError},
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_mmr::{MmrCacheConfig, MutableMmr};
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::{paths::create_temporary_data_path, unpack_enum};

#[test]
fn write_and_fetch_metadata() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(0));
    assert!(metadata.best_block.is_some());
    assert!(metadata.accumulated_difficulty.is_some());

    let height = 10;
    let accumulated_difficulty = 20;
    let mut metadata = ChainMetadata::default();
    metadata.height_of_longest_chain = Some(height);
    metadata.best_block = None;
    metadata.accumulated_difficulty = Some(accumulated_difficulty);
    store.set_chain_metadata(metadata).unwrap();

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(height));
    assert_eq!(metadata.best_block, None);
    assert_eq!(metadata.accumulated_difficulty, Some(accumulated_difficulty));
    assert_eq!(metadata.effective_pruned_height, 0);

    let state = InProgressHorizonSyncState {
        metadata: metadata.clone(),
        initial_rangeproof_checkpoint_count: 1,
        initial_utxo_checkpoint_count: 2,
        initial_kernel_checkpoint_count: 3,
    };
    store.set_horizon_sync_state(state).unwrap();
    let state = store.get_horizon_sync_state().unwrap().unwrap();
    assert_eq!(state.metadata, metadata);
}

#[test]
fn fetch_nonexistent_kernel() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let h = vec![0u8; 32];
    unpack_enum!(
        ChainStorageError::ValueNotFound { entity, field, value } = store.fetch_kernel(h.clone()).unwrap_err()
    );
    assert_eq!(entity, "Kernel");
    assert_eq!(field, "Hash");
    assert_eq!(value, h.to_hex());
}

#[test]
fn insert_and_fetch_kernel() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let kernel1 = create_test_kernel(5.into(), 0);
    let kernel2 = create_test_kernel(10.into(), 0);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();

    assert!(store
        .horizon_sync_insert_kernels(vec![kernel1.clone(), kernel2.clone()])
        .is_ok());
    assert_eq!(store.fetch_kernel(hash1).unwrap(), kernel1);
    assert_eq!(store.fetch_kernel(hash2).unwrap(), kernel2);
}

#[test]
fn fetch_nonexistent_header() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    unpack_enum!(ChainStorageError::ValueNotFound { entity, field, value } = store.fetch_header(1).unwrap_err());
    assert_eq!(entity, "BlockHeader");
    assert_eq!(field, "Height");
    assert_eq!(value, "1");
}

#[test]
fn insert_and_fetch_header() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let mut header1 = BlockHeader::new(0);
    header1.height = 42;
    let header2 = BlockHeader::from_previous(&header1).unwrap();

    store
        .insert_valid_headers(vec![header1.clone(), header2.clone()])
        .unwrap();
    store.fetch_header(0).unwrap();

    unpack_enum!(ChainStorageError::ValueNotFound { entity, field, value } = store.fetch_header(1).unwrap_err());
    assert_eq!(entity, "BlockHeader");
    assert_eq!(field, "Height");
    assert_eq!(value, "1");
    assert_eq!(store.fetch_header(42).unwrap(), header1);
    assert_eq!(store.fetch_header(43).unwrap(), header2);
}

#[test]
fn insert_and_fetch_utxo() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let utxo = OutputBuilder::new()
        .with_value(10_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let hash = utxo.hash();
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), false);
    assert!(store.insert_utxo(utxo.clone()).is_ok());
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), true);
    assert_eq!(store.fetch_utxo(hash).unwrap(), utxo);
}

#[test]
fn insert_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus_manager);
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash).unwrap(), orphan);
}

#[test]
fn multiple_threads() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    // Save a kernel in thread A
    let store_a = store.clone();
    let a = thread::spawn(move || {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel);
        assert!(store_a.commit(txn).is_ok());
        hash
    });
    // Save a kernel in thread B
    let store_b = store.clone();
    let b = thread::spawn(move || {
        let kernel = create_test_kernel(10.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel);
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
    let store = create_mem_db(&consensus_manager);
    let factories = CryptoFactories::default();
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let utxo0 = block0.body.outputs()[0].clone();
    let utxo1 = OutputBuilder::new()
        .with_value(10_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let utxo2 = OutputBuilder::new()
        .with_value(10_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let hash0 = utxo0.hash();
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();
    // Calculate the Range proof MMR root as a check
    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert_eq!(rp_mmr_check.push(utxo0.proof().hash()).unwrap(), 1);
    assert_eq!(rp_mmr_check.push(utxo1.proof().hash()).unwrap(), 2);
    assert_eq!(rp_mmr_check.push(utxo2.proof().hash()).unwrap(), 3);

    // Store the UTXOs
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_utxo(utxo2);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
    let rp_root = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(mmr_check.push(hash0).is_ok());
    assert!(mmr_check.push(hash1).is_ok());
    assert!(mmr_check.push(hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
    assert_eq!(rp_root.to_hex(), rp_mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn kernel_merkle_root() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let block0 = store.fetch_block(0).unwrap().block().clone();

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let kernel3 = create_test_kernel(300.into(), 0);
    let hash0 = block0.body.kernels()[0].hash();
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();
    let hash3 = kernel3.hash();
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1);
    txn.insert_kernel(kernel2);
    txn.insert_kernel(kernel3);
    assert!(store.commit(txn).is_ok());
    let root = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(mmr_check.push(hash0).is_ok());
    assert!(mmr_check.push(hash1).is_ok());
    assert!(mmr_check.push(hash2).is_ok());
    assert!(mmr_check.push(hash3).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().unwrap().to_hex());
}

#[test]
fn utxo_and_rp_future_merkle_root() {
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(&consensus_manager);
    let factories = CryptoFactories::default();

    let utxo1 = OutputBuilder::new()
        .with_value(10_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let utxo2 = OutputBuilder::new()
        .with_value(15_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let utxo_hash2 = utxo2.hash();
    let rp_hash2 = utxo2.proof().hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
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
    txn.insert_utxo(utxo2);
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
    let store = create_mem_db(&consensus_manager);

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let hash2 = kernel2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1);
    assert!(store.commit(txn).is_ok());

    let future_root = store
        .calculate_mmr_root(MmrTree::Kernel, vec![hash2], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(future_root, store.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel2);
    assert!(store.commit(txn).is_ok());

    assert_eq!(future_root, store.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());
}

#[test]
fn utxo_and_rp_mmr_proof() {
    let network = Network::LocalNet;
    let gen_block = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(gen_block).build();
    let store = create_mem_db(&consensus_manager);
    let factories = CryptoFactories::default();

    let utxo1 = OutputBuilder::new()
        .with_value(5_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let utxo2 = OutputBuilder::new()
        .with_value(10_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let utxo3 = OutputBuilder::new()
        .with_value(15_000)
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    txn.insert_utxo(utxo2.clone());
    txn.insert_utxo(utxo3.clone());
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
    let store = create_mem_db(&consensus_manager);

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone());
    txn.insert_kernel(kernel2.clone());
    txn.insert_kernel(kernel3.clone());
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
    let metadata = db.get_chain_metadata().unwrap();
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
    // Create new database with genesis block
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let store = create_mem_db(&consensus_manager);
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(0));
    let block0 = store.fetch_block(0).unwrap().block().clone();
    assert_eq!(metadata.best_block, Some(block0.hash()));
    // Add another block
    let block1 = append_block(&store, &block0, vec![], &consensus_manager, 1.into()).unwrap();
    let metadata = store.get_chain_metadata().unwrap();
    let hash = block1.hash();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    assert_eq!(metadata.best_block.unwrap(), hash);
    // Adding blocks is idempotent

    assert_eq!(
        store.add_block(block1.clone().into()).unwrap(),
        BlockAddResult::BlockExists
    );
    // Check the metadata
    let metadata = store.get_chain_metadata().unwrap();
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
    let block1 = append_block(&db, &blocks[0], vec![txn], &consensus_manager, 1.into()).unwrap();
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
    assert_eq!(
        generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap(),
        BlockAddResult::Ok
    );
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    assert_eq!(
        generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap(),
        BlockAddResult::Ok
    );
    // Block 3
    let schema = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T, 500_000 * uT]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![500_000 * uT]),
    ];
    assert_eq!(
        generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap(),
        BlockAddResult::Ok
    );

    assert!(db.rewind_to_height(3).is_ok());
    assert_eq!(db.get_height().unwrap(), Some(3));
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
    assert_eq!(db.get_height().unwrap(), Some(3));
    assert!(db.rewind_to_height(1).is_ok());
    assert_eq!(db.get_height().unwrap(), Some(1));
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
fn rewind_past_horizon_height() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 50,
    };
    let store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();

    let block1 = append_block(&store, &block0, vec![], &consensus_manager, 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, 1.into()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, 1.into()).unwrap();
    let _block4 = append_block(&store, &block3, vec![], &consensus_manager, 1.into()).unwrap();

    let metadata = store.get_chain_metadata().unwrap();
    let horizon_height = metadata.horizon_block(metadata.height_of_longest_chain.unwrap_or(0));
    assert!(store.rewind_to_height(horizon_height - 1).is_err());
    assert!(store.rewind_to_height(horizon_height).is_ok());
    assert_eq!(store.get_height().unwrap(), Some(horizon_height));
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
        &consensus_manager
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
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan_store = create_mem_db(&consensus_manager_fork);
    orphan_store.add_block(blocks[1].clone().into()).unwrap();
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
        &consensus_manager_fork
    )
    .is_ok());

    // Adding B2 to the main chain will produce a reorg to GB->A1->B2.
    if let Ok(BlockAddResult::ChainReorg(_, _)) = store.add_block(orphan_blocks[2].clone().into()) {
        assert!(true);
    } else {
        assert!(false);
    }
    assert_eq!(store.fetch_tip_header().unwrap(), orphan_blocks[2].header.clone());

    // Check that B2 was removed from the block orphans and A2 has been orphaned.
    assert!(store.fetch_orphan(orphan_blocks[2].hash()).is_err());
    assert!(store.fetch_orphan(blocks[2].hash()).is_ok());
}

#[test]
fn blockchain_reorgs_to_stronger_chain() {
    let mut blockchain = TestBlockchain::with_genesis("GB");
    let blocks = blockchain.builder();
    blockchain.add_block(blocks.new_block("A1").child_of("GB").difficulty(1));
    blockchain.add_block(blocks.new_block("A2").child_of("A1").difficulty(3));
    blockchain.add_block(blocks.new_block("A3").child_of("A2").difficulty(1));
    blockchain.add_block(blocks.new_block("A4").child_of("A3").difficulty(1));

    assert_eq!(Some(blockchain.tip()), blockchain.get_block("A4"));
    assert!(blockchain.orphan_pool().is_empty());

    blockchain.add_block(blocks.new_block("B2").child_of("A1").difficulty(1));
    assert_eq!(Some(blockchain.tip()), blockchain.get_block("A4"));
    blockchain.add_block(blocks.new_block("B3").child_of("B2").difficulty(1));
    assert_eq!(Some(blockchain.tip()), blockchain.get_block("A4"));
    assert_eq!(blockchain.chain(), ["GB", "A1", "A2", "A3", "A4"]);
    blockchain.add_block(blocks.new_block("B4").child_of("B3").difficulty(5));
    // Should reorg
    assert_eq!(Some(blockchain.tip()), blockchain.get_block("B4"));

    blockchain.add_block(blocks.new_block("C4").child_of("B3").difficulty(20));
    assert_eq!(Some(blockchain.tip()), blockchain.get_block("C4"));

    assert_eq!(blockchain.chain(), ["GB", "A1", "B2", "B3", "C4"]);
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
        &consensus_manager
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
        &consensus_manager
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
        &consensus_manager
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
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain 1
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan1_store = create_mem_db(&consensus_manager_fork); // GB
    orphan1_store.add_block(blocks[1].clone().into()).unwrap(); // A1
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
        &consensus_manager
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
        &consensus_manager
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
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain 2
    let consensus_manager_fork2 = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan2_store = create_mem_db(&consensus_manager_fork2); // GB
    orphan2_store.add_block(blocks[1].clone().into()).unwrap(); // A1
    orphan2_store.add_block(orphan1_blocks[2].clone().into()).unwrap(); // B2
    orphan2_store.add_block(orphan1_blocks[3].clone().into()).unwrap(); // B3
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
        &consensus_manager
    )
    .is_ok());

    // Now add the fork blocks C4, B2, B4 and B3 (out of order) to the first DB and observe a reorg. Blocks are added
    // out of order to test the forward and reverse chaining.
    store.add_block(orphan2_blocks[4].clone().into()).unwrap(); // C4
    store.add_block(orphan1_blocks[2].clone().into()).unwrap(); // B2
    store.add_block(orphan1_blocks[4].clone().into()).unwrap(); // B4
    store.add_block(orphan1_blocks[3].clone().into()).unwrap(); // B3
    assert_eq!(store.fetch_tip_header().unwrap(), orphan2_blocks[4].header.clone());

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
fn handle_reorg_with_no_removed_blocks() {
    // GB --> A1
    //          \--> B2 (?) --> B3)
    // Initially, the main chain is GB->A1 with orphaned blocks B3. When B2 arrives late and is
    // added to the blockchain then a reorg is triggered and the main chain is reorganized to GB->A1->B2->B3.

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
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain 1
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan1_store = create_mem_db(&consensus_manager_fork); // GB
    orphan1_store.add_block(blocks[1].clone().into()).unwrap(); // A1
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
        &consensus_manager
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
        &consensus_manager
    )
    .is_ok());

    // Now add the fork blocks B3 and B2 (out of order) to the first DB and ensure a reorg.
    // see https://github.com/tari-project/tari/issues/2101#issuecomment-679188619
    store.add_block(orphan1_blocks[3].clone().into()).unwrap(); // B3
    let result = store.add_block(orphan1_blocks[2].clone().into()).unwrap(); // B2
    match result {
        BlockAddResult::Ok => panic!("Adding multiple blocks without removing any failed to cause a reorg!"),
        BlockAddResult::ChainReorg(removed, added) => {
            assert_eq!(added.len(), 2);
            assert_eq!(removed.len(), 0);
        },
        _ => assert!(false),
    }

    assert_eq!(store.fetch_tip_header().unwrap(), orphan1_blocks[3].header.clone());
}

#[test]
fn handle_reorg_failure_recovery() {
    // GB --> A1 --> A2 --> A3 -----> A4(Low PoW)     [Main Chain]
    //          \--> B2 --> B3(double spend - rejected by db)  [Forked Chain 1]
    //          \--> B2 --> B3'(validation failed)      [Forked Chain 1]
    // Checks the following cases:
    // 1. recovery from failure to commit reorged blocks, and
    // 2. recovery from failed block validation.

    let temp_path = create_temporary_data_path();
    {
        let block_validator = MockValidator::new(true);
        let validators = Validators::new(block_validator, MockValidator::new(true));
        // Create Main Chain
        let network = Network::LocalNet;
        let (mut store, mut blocks, mut outputs, consensus_manager) =
            create_new_blockchain_lmdb(network, &temp_path, validators, Default::default());
        // Block A1
        let txs = vec![txn_schema!(
            from: vec![outputs[0][0].clone()],
            to: vec![10 * T, 10 * T, 10 * T, 10 * T]
        )];
        generate_new_block_with_achieved_difficulty(
            &mut store,
            &mut blocks,
            &mut outputs,
            txs,
            Difficulty::from(1),
            &consensus_manager,
        )
        .unwrap();
        // Block A2
        let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
        generate_new_block_with_achieved_difficulty(
            &mut store,
            &mut blocks,
            &mut outputs,
            txs,
            Difficulty::from(1),
            &consensus_manager,
        )
        .unwrap();
        // Block A3
        let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
        generate_new_block_with_achieved_difficulty(
            &mut store,
            &mut blocks,
            &mut outputs,
            txs,
            Difficulty::from(2),
            &consensus_manager,
        )
        .unwrap();
        // Block A4
        let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
        generate_new_block_with_achieved_difficulty(
            &mut store,
            &mut blocks,
            &mut outputs,
            txs,
            Difficulty::from(2),
            &consensus_manager,
        )
        .unwrap();

        // Create Forked Chain 1
        let consensus_manager_fork = ConsensusManagerBuilder::new(network)
            .with_block(blocks[0].clone())
            .build();
        let mut orphan1_store = create_mem_db(&consensus_manager_fork); // GB
        orphan1_store.add_block(blocks[1].clone().into()).unwrap(); // A1
        let mut orphan1_blocks = vec![blocks[0].clone(), blocks[1].clone()];
        let mut orphan1_outputs = vec![outputs[0].clone(), outputs[1].clone()];
        // Block B2
        let txs = vec![txn_schema!(from: vec![orphan1_outputs[1][0].clone()], to: vec![5 * T])];
        generate_new_block_with_achieved_difficulty(
            &mut orphan1_store,
            &mut orphan1_blocks,
            &mut orphan1_outputs,
            txs,
            Difficulty::from(1),
            &consensus_manager,
        )
        .unwrap();
        // Block B3 (Incorrect height)
        let double_spend_block = {
            let schemas = vec![
                txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
                // Double spend
                //txn_schema!(from: vec![orphan1_outputs[1][3].clone()], to: vec![3 * T]),
            ];
            let mut txns = Vec::new();
            let mut block_utxos = Vec::new();
            for schema in schemas {
                let (tx, mut utxos, _) = spend_utxos(schema);
                txns.push(tx);
                block_utxos.append(&mut utxos);
            }
            orphan1_outputs.push(block_utxos);

            let template = chain_block(&orphan1_blocks.last().unwrap(), txns, &consensus_manager);
            let mut block = orphan1_store.calculate_mmr_roots(template).unwrap();
            block.header.nonce = OsRng.next_u64();
            block.header.height = block.header.height + 1;
            find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(2));
            block
        };

        // Add an orphaned B2
        let result = store.add_block(orphan1_blocks[2].clone().into()).unwrap(); // B2
        unpack_enum!(BlockAddResult::OrphanBlock = result);

        // Add invalid block B3. Our database should recover
        let err = store.add_block(double_spend_block.clone().into()).unwrap_err(); // B3
        unpack_enum!(ChainStorageError::InvalidBlock = err);
        let tip_header = store.fetch_tip_header().unwrap();
        assert_eq!(tip_header.height, 4);
        assert_eq!(tip_header, blocks[4].header);

        assert!(store.fetch_orphan(blocks[2].hash()).is_err()); // A2
        assert!(store.fetch_orphan(blocks[3].hash()).is_err()); // A3
        assert!(store.fetch_orphan(blocks[4].hash()).is_err()); // A4
    }
    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

#[test]
fn store_and_retrieve_blocks() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 2 };
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let network = Network::LocalNet;
    let rules = ConsensusManagerBuilder::new(network).build();
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    let store = BlockchainDatabase::new(db, &rules, validators, BlockchainDatabaseConfig::default(), false).unwrap();

    let block0 = store.fetch_block(0).unwrap().block().clone();
    let block1 = append_block(&store, &block0, vec![], &rules, 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &rules, 1.into()).unwrap();
    assert_eq!(*store.fetch_block(0).unwrap().block(), block0);
    assert_eq!(*store.fetch_block(1).unwrap().block(), block1);
    assert_eq!(*store.fetch_block(2).unwrap().block(), block2);

    let block3 = append_block(&store, &block2, vec![], &rules, 1.into()).unwrap();
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
    let store = BlockchainDatabase::new(db, &rules, validators, BlockchainDatabaseConfig::default(), false).unwrap();

    let block0 = store.fetch_block(0).unwrap().block().clone();
    let block1 = append_block(&store, &block0, vec![], &rules, 1.into()).unwrap();
    let orphan = create_orphan_block(10, vec![], &rules);
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    assert!(store.commit(txn).is_ok());

    let hash0 = block0.hash();
    let hash1 = block1.hash();
    let hash2 = orphan.hash();
    assert_eq!(*store.fetch_block_with_hash(hash0).unwrap().unwrap().block(), block0);
    assert_eq!(*store.fetch_block_with_hash(hash1).unwrap().unwrap().block(), block1);
    assert_eq!(*store.fetch_block_with_hash(hash2).unwrap().unwrap().block(), orphan);
}

#[test]
fn store_and_retrieve_blocks_from_contents() {
    let network = Network::LocalNet;
    let (mut db, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);

    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![6 * T, 3 * T])];
    assert_eq!(
        generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap(),
        BlockAddResult::Ok
    );
    // Block 2
    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    assert_eq!(
        generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &consensus_manager).unwrap(),
        BlockAddResult::Ok
    );
    let kernel_sig = blocks[1].body.kernels()[0].clone().excess_sig;
    let stxo_commit = blocks[1].body.inputs()[0].clone().commitment().clone();
    let utxo_commit = blocks[1].body.outputs()[0].clone().commitment().clone();
    assert_eq!(
        *db.fetch_block_with_kernel(kernel_sig).unwrap().unwrap().block(),
        blocks[1]
    );
    assert_eq!(
        *db.fetch_block_with_stxo(stxo_commit).unwrap().unwrap().block(),
        blocks[1]
    );
    assert_eq!(
        *db.fetch_block_with_utxo(utxo_commit).unwrap().unwrap().block(),
        blocks[1]
    );
}

#[test]
fn restore_metadata_and_pruning_horizon_update() {
    let path = create_temporary_data_path();

    // Perform test
    {
        let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
        let network = Network::LocalNet;
        let block0 = genesis_block::get_rincewind_genesis_block_raw();
        let rules = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
        let mut config = BlockchainDatabaseConfig::default();
        let block_hash: BlockHash;
        let pruning_horizon1: u64 = 1000;
        let pruning_horizon2: u64 = 900;
        {
            let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
            config.pruning_horizon = pruning_horizon1;
            let db = BlockchainDatabase::new(db, &rules, validators.clone(), config, false).unwrap();

            let block1 = append_block(&db, &block0, vec![], &rules, 1.into()).unwrap();
            db.add_block(block1.clone().into()).unwrap();
            block_hash = block1.hash();
            let metadata = db.get_chain_metadata().unwrap();
            assert_eq!(metadata.height_of_longest_chain, Some(1));
            assert_eq!(metadata.best_block, Some(block_hash.clone()));
            assert_eq!(metadata.pruning_horizon, pruning_horizon1);
        }
        // Restore blockchain db with larger pruning horizon
        {
            config.pruning_horizon = 2000;
            let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
            let db = BlockchainDatabase::new(db, &rules, validators.clone(), config, false).unwrap();

            let metadata = db.get_chain_metadata().unwrap();
            assert_eq!(metadata.height_of_longest_chain, Some(1));
            assert_eq!(metadata.best_block, Some(block_hash.clone()));
            assert_eq!(metadata.pruning_horizon, 2000);
        }
        // Restore blockchain db with smaller pruning horizon update
        {
            config.pruning_horizon = 900;
            let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
            let db = BlockchainDatabase::new(db, &rules, validators, config, false).unwrap();

            let metadata = db.get_chain_metadata().unwrap();
            assert_eq!(metadata.height_of_longest_chain, Some(1));
            assert_eq!(metadata.best_block, Some(block_hash));
            assert_eq!(metadata.pruning_horizon, pruning_horizon2);
        }
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&path).exists() {
        match std::fs::remove_dir_all(&path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}
static EMISSION: [u64; 2] = [10, 10];
#[test]
fn invalid_block() {
    let temp_path = create_temporary_data_path();
    {
        let factories = CryptoFactories::default();
        let network = Network::LocalNet;
        let consensus_constants = ConsensusConstantsBuilder::new(network)
            .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
            .build();
        let (block0, output) = create_genesis_block(&factories, &consensus_constants);
        let consensus_manager = ConsensusManagerBuilder::new(network)
            .with_consensus_constants(consensus_constants)
            .with_block(block0.clone())
            .build();
        let validators = Validators::new(
            MockValidator::new(true),
            MockStatelessBlockValidator::new(consensus_manager.clone(), factories.clone()),
        );
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        let mut store = BlockchainDatabase::new(
            db,
            &consensus_manager,
            validators,
            BlockchainDatabaseConfig::default(),
            false,
        )
        .unwrap();
        let mut blocks = vec![block0];
        let mut outputs = vec![vec![output]];
        let block0_hash = blocks[0].hash();
        let metadata = store.get_chain_metadata().unwrap();
        let utxo_root0 = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
        let kernel_root0 = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
        let rp_root0 = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(0));
        assert_eq!(metadata.best_block, Some(block0_hash.clone()));
        assert_eq!(store.fetch_block(0).unwrap().block().hash(), block0_hash);
        assert!(store.fetch_block(1).is_err());

        // Block 1
        let txs = vec![txn_schema!(
            from: vec![outputs[0][0].clone()],
            to: vec![10 * T, 5 * T, 10 * T, 15 * T]
        )];
        let coinbase_value = consensus_manager.emission_schedule().block_reward(1);
        assert_eq!(
            generate_new_block_with_coinbase(
                &mut store,
                &factories,
                &mut blocks,
                &mut outputs,
                txs,
                coinbase_value,
                &consensus_manager
            )
            .unwrap(),
            BlockAddResult::Ok
        );
        let block1_hash = blocks[1].hash();
        let metadata = store.get_chain_metadata().unwrap();
        let utxo_root1 = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
        let kernel_root1 = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
        let rp_root1 = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(1));
        assert_eq!(metadata.best_block, Some(block1_hash.clone()));
        assert_eq!(store.fetch_block(0).unwrap().block().hash(), block0_hash);
        assert_eq!(store.fetch_block(1).unwrap().block().hash(), block1_hash);
        assert!(store.fetch_block(2).is_err());
        assert_ne!(utxo_root0, utxo_root1);
        assert_ne!(kernel_root0, kernel_root1);
        assert_ne!(rp_root0, rp_root1);

        // Invalid Block 2 - Double spends genesis block output
        let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];
        let coinbase_value = consensus_manager.emission_schedule().block_reward(2);
        unpack_enum!(
            ChainStorageError::UnspendableInput = generate_new_block_with_coinbase(
                &mut store,
                &factories,
                &mut blocks,
                &mut outputs,
                txs,
                coinbase_value,
                &consensus_manager
            )
            .unwrap_err()
        );
        let metadata = store.get_chain_metadata().unwrap();
        let utxo_root2 = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
        let kernel_root2 = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
        let rp_root2 = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(1));
        assert_eq!(metadata.best_block, Some(block1_hash.clone()));
        assert_eq!(store.fetch_block(0).unwrap().block().hash(), block0_hash);
        assert_eq!(store.fetch_block(1).unwrap().block().hash(), block1_hash);
        assert!(store.fetch_block(2).is_err());
        assert_eq!(utxo_root1, utxo_root2);
        assert_eq!(kernel_root1, kernel_root2);
        assert_eq!(rp_root1, rp_root2);

        // Valid Block 2
        let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![4 * T, 4 * T])];
        let coinbase_value = consensus_manager.emission_schedule().block_reward(2);
        assert_eq!(
            generate_new_block_with_coinbase(
                &mut store,
                &factories,
                &mut blocks,
                &mut outputs,
                txs,
                coinbase_value,
                &consensus_manager
            )
            .unwrap(),
            BlockAddResult::Ok
        );
        let block2_hash = blocks[2].hash();
        let metadata = store.get_chain_metadata().unwrap();
        let utxo_root2 = store.fetch_mmr_root(MmrTree::Utxo).unwrap();
        let kernel_root2 = store.fetch_mmr_root(MmrTree::Kernel).unwrap();
        let rp_root2 = store.fetch_mmr_root(MmrTree::RangeProof).unwrap();
        assert_eq!(metadata.height_of_longest_chain, Some(2));
        assert_eq!(metadata.best_block, Some(block2_hash.clone()));
        assert_eq!(store.fetch_block(0).unwrap().block().hash(), block0_hash);
        assert_eq!(store.fetch_block(1).unwrap().block().hash(), block1_hash);
        assert_eq!(store.fetch_block(2).unwrap().block().hash(), block2_hash);
        assert!(store.fetch_block(3).is_err());
        assert_ne!(utxo_root1, utxo_root2);
        assert_ne!(kernel_root1, kernel_root2);
        assert_ne!(rp_root1, rp_root2);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

#[test]
fn orphan_cleanup_on_block_add() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
    };
    let store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();

    let orphan1 = create_orphan_block(500, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(5, vec![], &consensus_manager);
    let orphan3 = create_orphan_block(30, vec![], &consensus_manager);
    let orphan4 = create_orphan_block(700, vec![], &consensus_manager);
    let orphan5 = create_orphan_block(43, vec![], &consensus_manager);
    let orphan6 = create_orphan_block(75, vec![], &consensus_manager);
    let orphan7 = create_orphan_block(150, vec![], &consensus_manager);
    let orphan1_hash = orphan1.hash();
    let orphan2_hash = orphan2.hash();
    let orphan3_hash = orphan3.hash();
    let orphan4_hash = orphan4.hash();
    let orphan5_hash = orphan5.hash();
    let orphan6_hash = orphan6.hash();
    let orphan7_hash = orphan7.hash();
    assert_eq!(
        store.add_block(orphan1.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan2.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan4.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan5.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.add_block(orphan6.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan7.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 3);
    assert_eq!(store.fetch_orphan(orphan1_hash).unwrap(), orphan1);
    assert!(store.fetch_orphan(orphan2_hash).is_err());
    assert!(store.fetch_orphan(orphan3_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan4_hash).unwrap(), orphan4);
    assert!(store.fetch_orphan(orphan5_hash).is_err());
    assert!(store.fetch_orphan(orphan6_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan7_hash).unwrap(), orphan7);
}

#[test]
fn horizon_height_orphan_cleanup() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 50,
    };
    let store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();
    let orphan1 = create_orphan_block(2, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(3, vec![], &consensus_manager);
    let orphan3 = create_orphan_block(1, vec![], &consensus_manager);
    let orphan4 = create_orphan_block(4, vec![], &consensus_manager);
    let orphan1_hash = orphan1.hash();
    let orphan2_hash = orphan2.hash();
    let orphan3_hash = orphan3.hash();
    let orphan4_hash = orphan4.hash();
    assert_eq!(store.add_block(orphan1.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(
        store.add_block(orphan2.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
    assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 3);

    let block1 = append_block(&store, &block0, vec![], &consensus_manager, 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, 1.into()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, 1.into()).unwrap();
    let _block4 = append_block(&store, &block3, vec![], &consensus_manager, 1.into()).unwrap();

    // Adding another orphan block will trigger the orphan cleanup as the storage limit was reached
    assert_eq!(
        store.add_block(orphan4.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 2);
    assert!(store.fetch_orphan(orphan1_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan2_hash).unwrap(), orphan2);
    assert!(store.fetch_orphan(orphan3_hash).is_err());
    assert_eq!(store.fetch_orphan(orphan4_hash).unwrap(), orphan4);
}

#[test]
fn orphan_cleanup_on_reorg() {
    // Create Main Chain
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (block0, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
    };
    let mut store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![output]];

    // Block A1
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(2),
        &consensus_manager
    )
    .is_ok());
    // Block A2
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager
    )
    .is_ok());
    // Block A3
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager
    )
    .is_ok());
    // Block A4
    assert!(generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        vec![],
        Difficulty::from(3),
        &consensus_manager
    )
    .is_ok());

    // Create Forked Chain
    let consensus_manager_fork = ConsensusManagerBuilder::new(network)
        .with_block(blocks[0].clone())
        .build();
    let mut orphan_store = create_mem_db(&consensus_manager_fork);
    let mut orphan_blocks = vec![blocks[0].clone()];
    let mut orphan_outputs = vec![outputs[0].clone()];
    // Block B1
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(2),
        &consensus_manager_fork
    )
    .is_ok());
    // Block B2
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(10),
        &consensus_manager_fork
    )
    .is_ok());
    // Block B3
    assert!(generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut orphan_blocks,
        &mut orphan_outputs,
        vec![],
        Difficulty::from(15),
        &consensus_manager_fork
    )
    .is_ok());

    // Fill orphan block pool

    let orphan1 = create_orphan_block(1, vec![], &consensus_manager);
    let orphan2 = create_orphan_block(1, vec![], &consensus_manager);
    assert_eq!(
        store.add_block(orphan1.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    assert_eq!(
        store.add_block(orphan2.clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );

    // Adding B1 and B2 to the main chain will produce a reorg from GB->A1->A2->A3->A4 to GB->B1->B2->B3.
    assert_eq!(
        store.add_block(orphan_blocks[1].clone().into()).unwrap(),
        BlockAddResult::OrphanBlock
    );
    if let Ok(BlockAddResult::ChainReorg(_, _)) = store.add_block(orphan_blocks[2].clone().into()) {
        assert!(true);
    } else {
        assert!(false);
    }

    // Check that A2, A3 and A4 is in the orphan block pool, A1 and the other orphans were discarded by the orphan
    // cleanup.
    assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 3);
    assert_eq!(store.fetch_orphan(blocks[2].hash()).unwrap(), blocks[2].clone());
    assert_eq!(store.fetch_orphan(blocks[3].hash()).unwrap(), blocks[3].clone());
    assert_eq!(store.fetch_orphan(blocks[4].hash()).unwrap(), blocks[4].clone());
}

#[test]
fn orphan_cleanup_delete_all_orphans() {
    let path = create_temporary_data_path();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 5,
        pruning_horizon: 0,
        pruning_interval: 50,
    };
    // Test cleanup during runtime
    {
        let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        let store = BlockchainDatabase::new(db, &consensus_manager, validators.clone(), config, false).unwrap();

        let orphan1 = create_orphan_block(500, vec![], &consensus_manager);
        let orphan2 = create_orphan_block(5, vec![], &consensus_manager);
        let orphan3 = create_orphan_block(30, vec![], &consensus_manager);
        let orphan4 = create_orphan_block(700, vec![], &consensus_manager);
        let orphan5 = create_orphan_block(43, vec![], &consensus_manager);

        // Add orphans and verify
        assert_eq!(
            store.add_block(orphan1.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan2.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan3.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan4.clone().clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(
            store.add_block(orphan5.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 5);

        // Cleanup orphans and verify
        assert_eq!(store.cleanup_all_orphans().unwrap(), ());
        assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 0);

        // Add orphans again
        assert_eq!(
            store.add_block(orphan1.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(store.add_block(orphan2.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(store.add_block(orphan3.into()).unwrap(), BlockAddResult::OrphanBlock);
        assert_eq!(
            store.add_block(orphan4.clone().into()).unwrap(),
            BlockAddResult::OrphanBlock
        );
        assert_eq!(store.add_block(orphan5.into()).unwrap(), BlockAddResult::OrphanBlock);
    }

    // Test orphans are present on open
    {
        let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        let store = BlockchainDatabase::new(db, &consensus_manager, validators.clone(), config, false).unwrap();
        assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 5);
    }

    // Test orphans cleanup on open
    {
        let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        let store = BlockchainDatabase::new(db, &consensus_manager, validators.clone(), config, true).unwrap();
        assert_eq!(store.db_read_access().unwrap().get_orphan_count().unwrap(), 0);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&path).exists() {
        match std::fs::remove_dir_all(&path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

#[test]
fn fails_validation() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (block0, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validators = Validators::new(MockValidator::new(false), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 0,
        pruning_interval: 50,
    };
    let mut store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![]];

    let schemas = vec![txn_schema!(from: vec![output], to: vec![2 * T, 500_000 * uT])];
    let err = generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        schemas,
        Difficulty::from(3),
        &consensus_manager,
    )
    .unwrap_err();
    unpack_enum!(ChainStorageError::ValidationError { source } = err);
    unpack_enum!(ValidationError::CustomError(_s) = source);

    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain.unwrap(), 0);
}

#[test]
fn pruned_mode_cleanup_and_fetch_block() {
    let network = Network::LocalNet;
    let block0 = genesis_block::get_rincewind_genesis_block_raw();
    let consensus_manager = ConsensusManagerBuilder::new(network).with_block(block0.clone()).build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 2,
    };
    let store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();
    let block1 = append_block(&store, &block0, vec![], &consensus_manager, 1.into()).unwrap();
    let block2 = append_block(&store, &block1, vec![], &consensus_manager, 1.into()).unwrap();
    let block3 = append_block(&store, &block2, vec![], &consensus_manager, 1.into()).unwrap();

    assert!(store.fetch_block(0).is_err()); // Genesis block cant be retrieved in pruned mode
    assert_eq!(store.fetch_block(1).unwrap().block, block1);
    assert_eq!(store.fetch_block(2).unwrap().block, block2);

    let block4 = append_block(&store, &block3, vec![], &consensus_manager, 1.into()).unwrap();

    // Adding block 4 will trigger the pruned mode cleanup, first block after horizon block height is retrievable.
    assert!(store.fetch_block(0).is_err());
    assert!(store.fetch_block(1).is_err());
    assert!(store.fetch_block(2).is_err());
    assert_eq!(store.fetch_block(3).unwrap().block, block3);
    assert_eq!(store.fetch_block(4).unwrap().block, block4);
}

#[test]
fn pruned_mode_is_stxo() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let db = MemoryDatabase::<HashDigest>::default();
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 2,
    };
    let mut store = BlockchainDatabase::new(db, &consensus_manager, validators, config, false).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![output]];
    let txo_hash1 = blocks[0].body.outputs()[0].hash();
    assert!(store.is_utxo(txo_hash1.clone()).unwrap());

    // Block 1
    let txs = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![50 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(1);
    assert_eq!(
        generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap(),
        BlockAddResult::Ok
    );
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(1));
    let txo_hash2 = outputs[1][0].as_transaction_output(&factories).unwrap().hash();
    let txo_hash3 = outputs[1][1].as_transaction_output(&factories).unwrap().hash();
    let txo_hash4 = outputs[1][2].as_transaction_output(&factories).unwrap().hash();
    assert!(store.is_stxo(txo_hash1.clone()).unwrap());
    assert!(store.is_utxo(txo_hash2.clone()).unwrap());
    assert!(store.is_utxo(txo_hash3.clone()).unwrap());
    assert!(store.is_utxo(txo_hash4.clone()).unwrap());

    // Block 2
    let txs = vec![txn_schema!(from: vec![outputs[1][1].clone()], to: vec![40 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(2);
    assert_eq!(
        generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap(),
        BlockAddResult::Ok
    );
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(2));
    let txo_hash5 = outputs[2][0].as_transaction_output(&factories).unwrap().hash();
    let txo_hash6 = outputs[2][1].as_transaction_output(&factories).unwrap().hash();
    let txo_hash7 = outputs[2][2].as_transaction_output(&factories).unwrap().hash();
    assert!(store.is_stxo(txo_hash1.clone()).unwrap());
    assert!(store.is_utxo(txo_hash2.clone()).unwrap());
    assert!(store.is_stxo(txo_hash3.clone()).unwrap());
    assert!(store.is_utxo(txo_hash4.clone()).unwrap());
    assert!(store.is_utxo(txo_hash5.clone()).unwrap());
    assert!(store.is_utxo(txo_hash6.clone()).unwrap());
    assert!(store.is_utxo(txo_hash7.clone()).unwrap());

    // Block 3
    let txs = vec![txn_schema!(from: vec![outputs[2][2].clone()], to: vec![30 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(3);
    assert_eq!(
        generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap(),
        BlockAddResult::Ok
    );
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(3));
    let txo_hash8 = outputs[3][0].as_transaction_output(&factories).unwrap().hash();
    let txo_hash9 = outputs[3][1].as_transaction_output(&factories).unwrap().hash();
    let txo_hash10 = outputs[3][2].as_transaction_output(&factories).unwrap().hash();
    assert!(store.is_stxo(txo_hash1.clone()).unwrap());
    assert!(store.is_utxo(txo_hash2.clone()).unwrap());
    assert!(store.is_stxo(txo_hash3.clone()).unwrap());
    assert!(store.is_utxo(txo_hash4.clone()).unwrap());
    assert!(store.is_utxo(txo_hash5.clone()).unwrap());
    assert!(store.is_utxo(txo_hash6.clone()).unwrap());
    assert!(store.is_stxo(txo_hash7.clone()).unwrap());
    assert!(store.is_utxo(txo_hash8.clone()).unwrap());
    assert!(store.is_utxo(txo_hash9.clone()).unwrap());
    assert!(store.is_utxo(txo_hash10.clone()).unwrap());

    // Block 4
    let txs = vec![txn_schema!(from: vec![outputs[3][1].clone()], to: vec![20 * T])];
    let coinbase_value = consensus_manager.emission_schedule().block_reward(4);
    assert_eq!(
        generate_new_block_with_coinbase(
            &mut store,
            &factories,
            &mut blocks,
            &mut outputs,
            txs,
            coinbase_value,
            &consensus_manager
        )
        .unwrap(),
        BlockAddResult::Ok
    );
    let metadata = store.get_chain_metadata().unwrap();
    assert_eq!(metadata.height_of_longest_chain, Some(4));
    let txo_hash11 = outputs[4][0].as_transaction_output(&factories).unwrap().hash();
    let txo_hash12 = outputs[4][1].as_transaction_output(&factories).unwrap().hash();
    let txo_hash13 = outputs[4][2].as_transaction_output(&factories).unwrap().hash();
    assert!(store.is_stxo(txo_hash1).unwrap());
    assert!(store.is_utxo(txo_hash2).unwrap());
    assert!(store.is_stxo(txo_hash3).unwrap());
    assert!(store.is_utxo(txo_hash4).unwrap());
    assert!(store.is_utxo(txo_hash5).unwrap());
    assert!(store.is_utxo(txo_hash6).unwrap());
    assert!(store.is_stxo(txo_hash7).unwrap());
    assert!(store.is_utxo(txo_hash8).unwrap());
    assert!(store.is_stxo(txo_hash9).unwrap());
    assert!(store.is_utxo(txo_hash10).unwrap());
    assert!(store.is_utxo(txo_hash11).unwrap());
    assert!(store.is_utxo(txo_hash12).unwrap());
    assert!(store.is_utxo(txo_hash13).unwrap());
}

#[test]
fn pruned_mode_fetch_insert_and_commit() {
    // This test demonstrates the basic steps involved in horizon syncing without any of the comms requests.
    let network = Network::LocalNet;
    // Create an archival chain for Alice
    let (mut alice_store, mut blocks, mut outputs, consensus_manager) = create_new_blockchain(network);
    // Block1
    let txs = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![10 * T, 10 * T, 10 * T, 10 * T]
    )];
    assert!(generate_new_block(&mut alice_store, &mut blocks, &mut outputs, txs, &consensus_manager).is_ok());
    // Block2
    let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
    assert!(generate_new_block(&mut alice_store, &mut blocks, &mut outputs, txs, &consensus_manager).is_ok());
    // Block3
    let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block(&mut alice_store, &mut blocks, &mut outputs, txs, &consensus_manager).is_ok());
    // Block4
    let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
    assert!(generate_new_block(&mut alice_store, &mut blocks, &mut outputs, txs, &consensus_manager).is_ok());

    // Perform a manual horizon state sync between Alice and Bob
    let validators = Validators::new(MockValidator::new(true), MockValidator::new(true));
    let config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 2,
        pruning_interval: 2,
    };
    let bob_store = BlockchainDatabase::new(
        MemoryDatabase::<HashDigest>::default(),
        &consensus_manager,
        validators,
        config,
        false,
    )
    .unwrap();
    let network_tip_height = alice_store
        .get_chain_metadata()
        .unwrap()
        .height_of_longest_chain
        .unwrap();
    let bob_metadata = bob_store.get_chain_metadata().unwrap();
    let sync_horizon_height = bob_metadata.horizon_block(network_tip_height) + 1;
    let state = bob_store.horizon_sync_begin().unwrap();
    assert_eq!(state.metadata, bob_metadata);
    assert_eq!(state.initial_kernel_checkpoint_count, 1);
    assert_eq!(state.initial_utxo_checkpoint_count, 1);
    assert_eq!(state.initial_rangeproof_checkpoint_count, 1);

    // Sync headers
    let bob_height = bob_metadata.height_of_longest_chain.unwrap();
    let headers = alice_store.fetch_headers(bob_height + 1, sync_horizon_height).unwrap();
    assert!(bob_store.insert_valid_headers(headers).is_ok());

    // Sync kernels
    let alice_num_kernels = alice_store
        .fetch_mmr_node_count(MmrTree::Kernel, sync_horizon_height)
        .unwrap();
    let bob_num_kernels = bob_store
        .fetch_mmr_node_count(MmrTree::Kernel, sync_horizon_height)
        .unwrap();
    let kernel_hashes = alice_store
        .fetch_mmr_nodes(
            MmrTree::Kernel,
            bob_num_kernels,
            alice_num_kernels - bob_num_kernels,
            Some(sync_horizon_height),
        )
        .unwrap()
        .iter()
        .map(|n| n.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(kernel_hashes.len(), 3);
    let kernels = alice_store.fetch_kernels(kernel_hashes).unwrap();
    assert_eq!(kernels.len(), 3);
    assert!(bob_store.horizon_sync_insert_kernels(kernels).is_ok());
    bob_store.horizon_sync_create_mmr_checkpoint(MmrTree::Kernel).unwrap();

    // Sync Utxos and RangeProofs
    let alice_num_utxos = alice_store
        .fetch_mmr_node_count(MmrTree::Utxo, sync_horizon_height)
        .unwrap();
    let bob_num_utxos = bob_store
        .fetch_mmr_node_count(MmrTree::Utxo, sync_horizon_height)
        .unwrap();
    let alice_num_rps = alice_store
        .fetch_mmr_node_count(MmrTree::RangeProof, sync_horizon_height)
        .unwrap();
    let bob_num_rps = bob_store
        .fetch_mmr_node_count(MmrTree::RangeProof, sync_horizon_height)
        .unwrap();
    assert_eq!(alice_num_utxos, alice_num_rps);
    assert_eq!(bob_num_utxos, bob_num_rps);
    // Check if some of the existing UTXOs need to be marked as deleted.
    let alice_utxo_nodes = alice_store
        .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(sync_horizon_height))
        .unwrap();
    let bob_utxo_nodes = bob_store
        .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(sync_horizon_height))
        .unwrap();
    assert_eq!(alice_utxo_nodes.len(), bob_utxo_nodes.len());
    for index in 0..alice_utxo_nodes.len() {
        let (alice_utxo_hash, alice_utxo_deleted) = alice_utxo_nodes[index].clone();
        let (bob_utxo_hash, bob_utxo_deleted) = bob_utxo_nodes[index].clone();
        assert_eq!(alice_utxo_hash, bob_utxo_hash);
        if alice_utxo_deleted && !bob_utxo_deleted {
            assert!(bob_store.delete_mmr_node(MmrTree::Utxo, &bob_utxo_hash).is_ok());
            assert!(bob_store.spend_utxo(bob_utxo_hash).is_ok());
        }
    }

    // Continue with syncing of missing MMR nodes
    let utxo_mmr_nodes = alice_store
        .fetch_mmr_nodes(
            MmrTree::Utxo,
            bob_num_utxos,
            alice_num_utxos - bob_num_utxos,
            Some(sync_horizon_height),
        )
        .unwrap();
    let rp_hashes = alice_store
        .fetch_mmr_nodes(
            MmrTree::RangeProof,
            bob_num_rps,
            alice_num_rps - bob_num_rps,
            Some(sync_horizon_height),
        )
        .unwrap()
        .iter()
        .map(|n| n.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(utxo_mmr_nodes.len(), 9);
    assert_eq!(rp_hashes.len(), 9);
    for (index, (utxo_hash, is_stxo)) in utxo_mmr_nodes.into_iter().enumerate() {
        if is_stxo {
            assert!(bob_store.insert_mmr_node(MmrTree::Utxo, utxo_hash, is_stxo).is_ok());
            assert!(bob_store
                .insert_mmr_node(MmrTree::RangeProof, rp_hashes[index].clone(), false)
                .is_ok());
        } else {
            let txo = alice_store.fetch_txo(utxo_hash).unwrap().unwrap();
            assert!(bob_store.insert_utxo(txo).is_ok());
        }
    }

    bob_store.horizon_sync_create_mmr_checkpoint(MmrTree::Utxo).unwrap();
    bob_store
        .horizon_sync_create_mmr_checkpoint(MmrTree::RangeProof)
        .unwrap();

    // Finalize horizon state sync
    bob_store.horizon_sync_commit().unwrap();
    assert!(bob_store.get_horizon_sync_state().unwrap().is_none());

    // Check Metadata
    let bob_metadata = bob_store.get_chain_metadata().unwrap();
    let sync_height_header = blocks[sync_horizon_height as usize].header.clone();
    assert_eq!(bob_metadata.height_of_longest_chain, Some(sync_horizon_height));
    assert_eq!(bob_metadata.best_block, Some(sync_height_header.hash()));

    // Check headers
    let alice_headers = alice_store
        .fetch_headers(0, bob_metadata.height_of_longest_chain())
        .unwrap();
    let bob_headers = bob_store
        .fetch_headers(0, bob_metadata.height_of_longest_chain())
        .unwrap();
    assert_eq!(alice_headers, bob_headers);
    // Check Kernel MMR nodes
    let alice_num_kernels = alice_store
        .fetch_mmr_node_count(MmrTree::Kernel, sync_horizon_height)
        .unwrap();
    let bob_num_kernels = bob_store
        .fetch_mmr_node_count(MmrTree::Kernel, sync_horizon_height)
        .unwrap();
    assert_eq!(alice_num_kernels, bob_num_kernels);
    let alice_kernel_nodes = alice_store
        .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(sync_horizon_height))
        .unwrap();
    let bob_kernel_nodes = bob_store
        .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(sync_horizon_height))
        .unwrap();
    assert_eq!(alice_kernel_nodes, bob_kernel_nodes);
    // Check Kernels
    let alice_kernel_hashes = alice_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
    let bob_kernels_hashes = bob_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
    let alice_kernels = alice_store.fetch_kernels(alice_kernel_hashes).unwrap();
    let bob_kernels = bob_store.fetch_kernels(bob_kernels_hashes).unwrap();
    assert_eq!(alice_kernels, bob_kernels);
    // Check UTXO MMR nodes
    let alice_num_utxos = alice_store
        .fetch_mmr_node_count(MmrTree::Utxo, sync_horizon_height)
        .unwrap();
    let bob_num_utxos = bob_store
        .fetch_mmr_node_count(MmrTree::Utxo, sync_horizon_height)
        .unwrap();
    assert_eq!(alice_num_utxos, bob_num_utxos);
    let alice_utxo_nodes = alice_store
        .fetch_mmr_nodes(MmrTree::Utxo, 0, alice_num_utxos, Some(sync_horizon_height))
        .unwrap();
    let bob_utxo_nodes = bob_store
        .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(sync_horizon_height))
        .unwrap();
    assert_eq!(alice_utxo_nodes, bob_utxo_nodes);
    // Check RangeProof MMR nodes
    let alice_num_rps = alice_store
        .fetch_mmr_node_count(MmrTree::RangeProof, sync_horizon_height)
        .unwrap();
    let bob_num_rps = bob_store
        .fetch_mmr_node_count(MmrTree::RangeProof, sync_horizon_height)
        .unwrap();
    assert_eq!(alice_num_rps, bob_num_rps);
    let alice_rps_nodes = alice_store
        .fetch_mmr_nodes(MmrTree::RangeProof, 0, alice_num_rps, Some(sync_horizon_height))
        .unwrap();
    let bob_rps_nodes = bob_store
        .fetch_mmr_nodes(MmrTree::RangeProof, 0, bob_num_rps, Some(sync_horizon_height))
        .unwrap();
    assert_eq!(alice_rps_nodes, bob_rps_nodes);
    // Check UTXOs
    let mut alice_utxos = Vec::<TransactionOutput>::new();
    for (hash, deleted) in alice_utxo_nodes {
        if !deleted {
            alice_utxos.push(alice_store.fetch_txo(hash).unwrap().unwrap());
        }
    }
    let mut bob_utxos = Vec::<TransactionOutput>::new();
    for (hash, deleted) in bob_utxo_nodes {
        if !deleted {
            bob_utxos.push(bob_store.fetch_utxo(hash).unwrap());
        }
    }
    assert_eq!(alice_utxos, bob_utxos);

    // Check if chain can be extending using blocks after horizon state
    let height = sync_horizon_height as usize + 1;
    assert_eq!(
        bob_store.add_block(blocks[height].clone().into()).unwrap(),
        BlockAddResult::Ok
    );
}

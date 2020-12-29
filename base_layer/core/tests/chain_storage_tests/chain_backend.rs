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

use crate::helpers::database::create_orphan_block;
use tari_core::{
    blocks::BlockHeader,
    chain_storage::{
        create_lmdb_database,
        BlockchainBackend,
        ChainStorageError,
        DbKey,
        DbTransaction,
        DbValue,
        MetadataKey,
        MetadataValue,
    },
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    test_helpers::blockchain::create_test_db,
    tx,
};
use tari_crypto::tari_utilities::Hashable;
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;

#[test]
#[ignore = "Required for pruned mode"]
fn lmdb_insert_contains_delete_and_fetch_utxo() {
    let db = create_test_db();
    unimplemented!()
    // let factories = CryptoFactories::default();
    // let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let hash = utxo.hash();
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())).unwrap(), false);
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo.clone());
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())).unwrap(), true);
    // if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash.clone())).unwrap() {
    //     assert_eq!(*retrieved_utxo, utxo);
    // } else {
    //     assert!(false);
    // }
    //
    // let mut txn = DbTransaction::new();
    // txn.delete(DbKey::UnspentOutput(hash.clone()));
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash)).unwrap(), false);;
}

#[test]
#[ignore = "Requires pruned mode"]
fn lmdb_insert_contains_delete_and_fetch_kernel() {
    let db = create_test_db();
    unimplemented!();
    // let kernel = create_test_kernel(5.into(), 0);
    // let hash = kernel.hash();
    // assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())).unwrap(), false);
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel.clone());
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())).unwrap(), true);
    // if let Some(DbValue::TransactionKernel(retrieved_kernel)) =
    //     db.fetch(&DbKey::TransactionKernel(hash.clone())).unwrap()
    // {
    //     assert_eq!(*retrieved_kernel, kernel);
    // } else {
    //     assert!(false);
    // }
    //
    // let mut txn = DbTransaction::new();
    // txn.delete(DbKey::TransactionKernel(hash.clone()));
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::TransactionKernel(hash)).unwrap(), false);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus = ConsensusManagerBuilder::new(network).build();
    let mut db = create_test_db();
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus);
    let hash = orphan.hash();
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    db.write(txn).unwrap();

    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap(), true);
    if let Some(DbValue::OrphanBlock(retrieved_orphan)) = db.fetch(&DbKey::OrphanBlock(hash.clone())).unwrap() {
        assert_eq!(*retrieved_orphan, orphan);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash)).unwrap(), false);
}

fn insert_fetch_metadata<T: BlockchainBackend>(mut db: T) {
    assert!(db.fetch(&DbKey::Metadata(MetadataKey::ChainHeight)).unwrap().is_none());
    assert!(db
        .fetch(&DbKey::Metadata(MetadataKey::AccumulatedWork))
        .unwrap()
        .is_none());
    assert!(db
        .fetch(&DbKey::Metadata(MetadataKey::PruningHorizon))
        .unwrap()
        .is_none());
    assert!(db.fetch(&DbKey::Metadata(MetadataKey::BestBlock)).unwrap().is_none());

    let header = BlockHeader::new(0);
    let hash = header.hash();
    let pruning_horizon = 1u64;
    let chain_height = 2u64;
    let accumulated_work = 3u64;

    let mut txn = DbTransaction::new();
    txn.set_metadata(MetadataKey::ChainHeight, MetadataValue::ChainHeight(chain_height));
    txn.set_metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(accumulated_work.into()),
    );
    txn.set_metadata(
        MetadataKey::PruningHorizon,
        MetadataValue::PruningHorizon(pruning_horizon),
    );
    txn.set_metadata(MetadataKey::BestBlock, MetadataValue::BestBlock(hash.clone()));
    assert!(db.write(txn).is_ok());

    if let Some(DbValue::Metadata(MetadataValue::ChainHeight(retrieved_chain_height))) =
        db.fetch(&DbKey::Metadata(MetadataKey::ChainHeight)).unwrap()
    {
        assert_eq!(retrieved_chain_height, chain_height);
    } else {
        assert!(false);
    }
    if let Some(DbValue::Metadata(MetadataValue::AccumulatedWork(retrieved_accumulated_work))) =
        db.fetch(&DbKey::Metadata(MetadataKey::AccumulatedWork)).unwrap()
    {
        assert_eq!(retrieved_accumulated_work, accumulated_work.into());
    } else {
        assert!(false);
    }
    if let Some(DbValue::Metadata(MetadataValue::PruningHorizon(retrieved_pruning_horizon))) =
        db.fetch(&DbKey::Metadata(MetadataKey::PruningHorizon)).unwrap()
    {
        assert_eq!(retrieved_pruning_horizon, pruning_horizon);
    } else {
        assert!(false);
    }
    if let Some(DbValue::Metadata(MetadataValue::BestBlock(retrieved_hash))) =
        db.fetch(&DbKey::Metadata(MetadataKey::BestBlock)).unwrap()
    {
        assert_eq!(retrieved_hash, hash);
    } else {
        assert!(false);
    }
}

#[test]
fn lmdb_insert_fetch_metadata() {
    let db = create_test_db();
    insert_fetch_metadata(db);
}

#[test]
#[ignore = "Needs to be moved to chain storage"]
fn lmdb_duplicate_utxo() {
    let db = create_test_db();
    unimplemented!("This test should probably be done in chain_storage rather");
    // let factories = CryptoFactories::default();
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let hash1 = utxo1.hash();
    // let block_builder =
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo_with_hash(hash1.clone(), utxo1.clone());
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    // if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
    //     assert_eq!(*retrieved_utxo, utxo1);
    // } else {
    //     assert!(false);
    // }
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo_with_hash(hash1.clone(), utxo2.clone());
    // assert!(db.write(txn).is_err()); // This should fail
    // if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
    //     assert_eq!(*retrieved_utxo, utxo1); // original data should still be there
    // } else {
    //     assert!(false);
    // }
}

#[test]
#[ignore = "To be completed with pruned mode"]
fn lmdb_fetch_utxo_rp_nodes_and_count() {
    let db = create_test_db();
    // let factories = CryptoFactories::default();
    //
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(20_000), &factories, None);
    // let (utxo3, _) = create_utxo(MicroTari(30_000), &factories, None);
    // let (utxo4, _) = create_utxo(MicroTari(40_000), &factories, None);
    // let (utxo5, _) = create_utxo(MicroTari(50_000), &factories, None);
    // let (utxo6, _) = create_utxo(MicroTari(60_000), &factories, None);
    // let utxo_hash1 = utxo1.hash();
    // let utxo_hash2 = utxo2.hash();
    // let utxo_hash3 = utxo3.hash();
    // let utxo_hash4 = utxo4.hash();
    // let utxo_hash5 = utxo5.hash();
    // let utxo_hash6 = utxo6.hash();
    // let utxo_leaf_nodes = vec![
    //     (utxo_hash1.clone(), true),
    //     (utxo_hash2.clone(), false),
    //     (utxo_hash3.clone(), true),
    //     (utxo_hash4.clone(), true),
    //     (utxo_hash5.clone(), false),
    //     (utxo_hash6.clone(), false),
    // ];
    // let rp_leaf_nodes = vec![
    //     (utxo1.proof.hash(), false),
    //     (utxo2.proof.hash(), false),
    //     (utxo3.proof.hash(), false),
    //     (utxo4.proof.hash(), false),
    //     (utxo5.proof.hash(), false),
    //     (utxo6.proof.hash(), false),
    // ];
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo1);
    // txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo2);
    // txn.insert_utxo(utxo3);
    // txn.spend_utxo(utxo_hash1.clone());
    // txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo4);
    // txn.insert_utxo(utxo5);
    // txn.spend_utxo(utxo_hash3.clone());
    // txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo6);
    // txn.spend_utxo(utxo_hash4.clone());
    // txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    // assert!(db.write(txn).is_ok());
    //
    // for i in 0..=3 {
    //     let mmr_node = db.fetch_mmr_node(MmrTree::Utxo, i, None).unwrap();
    //     assert_eq!(mmr_node, utxo_leaf_nodes[i as usize]);
    //     let mmr_node = db.fetch_mmr_node(MmrTree::RangeProof, i, None).unwrap();
    //     assert_eq!(mmr_node, rp_leaf_nodes[i as usize]);
    //
    //     let mmr_node = db.fetch_mmr_nodes(MmrTree::Utxo, i, 3, None).unwrap();
    //     assert_eq!(mmr_node.len(), 3);
    //     assert_eq!(mmr_node[0], utxo_leaf_nodes[i as usize]);
    //     assert_eq!(mmr_node[1], utxo_leaf_nodes[(i + 1) as usize]);
    //     assert_eq!(mmr_node[2], utxo_leaf_nodes[(i + 2) as usize]);
    //     let mmr_node = db.fetch_mmr_nodes(MmrTree::RangeProof, i, 3, None).unwrap();
    //     assert_eq!(mmr_node.len(), 3);
    //     assert_eq!(mmr_node[0], rp_leaf_nodes[i as usize]);
    //     assert_eq!(mmr_node[1], rp_leaf_nodes[(i + 1) as usize]);
    //     assert_eq!(mmr_node[2], rp_leaf_nodes[(i + 2) as usize]);
    // }
    //
    // assert!(db.fetch_mmr_node(MmrTree::Utxo, 7, None).is_err());
    // assert!(db.fetch_mmr_nodes(MmrTree::Utxo, 5, 4, None).is_err());
    // assert!(db.fetch_mmr_node(MmrTree::RangeProof, 7, None).is_err());
    // assert!(db.fetch_mmr_nodes(MmrTree::RangeProof, 5, 4, None).is_err());
    unimplemented!()
}

#[test]
#[ignore = "To be completed with pruned mode"]
fn lmdb_fetch_kernel_nodes_and_count() {
    let db = create_test_db();
    // let kernel1 = create_test_kernel(100.into(), 0);
    // let kernel2 = create_test_kernel(200.into(), 1);
    // let kernel3 = create_test_kernel(300.into(), 1);
    // let kernel4 = create_test_kernel(400.into(), 2);
    // let kernel5 = create_test_kernel(500.into(), 2);
    // let kernel6 = create_test_kernel(600.into(), 3);
    // let leaf_nodes = vec![
    //     (kernel1.hash(), false),
    //     (kernel2.hash(), false),
    //     (kernel3.hash(), false),
    //     (kernel4.hash(), false),
    //     (kernel5.hash(), false),
    //     (kernel6.hash(), false),
    // ];
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel1);
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel2);
    // txn.insert_kernel(kernel3);
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel4);
    // txn.insert_kernel(kernel5);
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    // assert!(db.write(txn).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel6);
    // txn.operations
    //     .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    // assert!(db.write(txn).is_ok());
    //
    // for i in 0..=3 {
    //     let mmr_node = db.fetch_mmr_node(MmrTree::Kernel, i, None).unwrap();
    //     assert_eq!(mmr_node, leaf_nodes[i as usize]);
    //
    //     let mmr_node = db.fetch_mmr_nodes(MmrTree::Kernel, i, 3, None).unwrap();
    //     assert_eq!(mmr_node.len(), 3);
    //     assert_eq!(mmr_node[0], leaf_nodes[i as usize]);
    //     assert_eq!(mmr_node[1], leaf_nodes[(i + 1) as usize]);
    //     assert_eq!(mmr_node[2], leaf_nodes[(i + 2) as usize]);
    // }
    //
    // assert!(db.fetch_mmr_node(MmrTree::Kernel, 7, None).is_err());
    // assert!(db.fetch_mmr_nodes(MmrTree::Kernel, 5, 4, None).is_err());
    unimplemented!()
}

#[test]
fn lmdb_file_lock() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();

        match create_lmdb_database(&temp_path, LMDBConfig::default()) {
            Err(ChainStorageError::CannotAcquireFileLock) => assert!(true),
            _ => assert!(false, "Should not be able to make this db"),
        }

        drop(db);

        let _db2 =
            create_lmdb_database(&temp_path, LMDBConfig::default()).expect("Should be able to make a new lmdb now");
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

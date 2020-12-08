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
use monero::cryptonote::hash::Hashable as MoneroHashable;
use tari_core::{
    blocks::BlockHeader,
    chain_storage::{
        create_lmdb_database,
        BlockchainBackend,
        BlockchainDatabase,
        ChainStorageError,
        DbKey,
        DbTransaction,
        DbValue,
        MetadataKey,
        MetadataValue,
    },
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    proof_of_work::{
        monero_rx::{append_merge_mining_tag, tree_hash, MoneroData},
        Difficulty,
        PowAlgorithm,
    },
    test_helpers::blockchain::create_test_db,
    tx,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, Hashable};
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;

fn insert_contains_delete_and_fetch_header<T: BlockchainBackend>(mut db: T) {
    let mut header = BlockHeader::new(0);
    header.height = 42;
    let hash = header.hash();
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::BlockHash(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::BlockHash(hash.clone())).unwrap(), true);
    if let Some(DbValue::BlockHeader(retrieved_header)) = db.fetch(&DbKey::BlockHeader(header.height)).unwrap() {
        assert_eq!(*retrieved_header, header);
    } else {
        assert!(false);
    }
    if let Some(DbValue::BlockHash(retrieved_header)) = db.fetch(&DbKey::BlockHash(hash.clone())).unwrap() {
        assert_eq!(*retrieved_header, header);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::BlockHash(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::BlockHash(hash)).unwrap(), false);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_header() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        insert_contains_delete_and_fetch_header(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn insert_contains_delete_and_fetch_utxo<T: BlockchainBackend>(mut db: T) {
    unimplemented!();
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
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash)).unwrap(), false);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_utxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        insert_contains_delete_and_fetch_utxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn insert_contains_delete_and_fetch_kernel<T: BlockchainBackend>(mut db: T) {
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
fn lmdb_insert_contains_delete_and_fetch_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        insert_contains_delete_and_fetch_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn insert_contains_delete_and_fetch_orphan<T: BlockchainBackend>(mut db: T, consensus: &ConsensusManager) {
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, consensus);
    let hash = orphan.hash();
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    assert!(db.write(txn).is_ok());

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

#[test]
fn memory_insert_contains_delete_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus = ConsensusManagerBuilder::new(network).build();
    let db = create_test_db();
    insert_contains_delete_and_fetch_orphan(db, &consensus);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_orphan() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let network = Network::LocalNet;
        let consensus = ConsensusManagerBuilder::new(network).build();
        let db = create_test_db();
        insert_contains_delete_and_fetch_orphan(db, &consensus);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn spend_utxo_and_unspend_stxo<T: BlockchainBackend>(mut db: T) {
    unimplemented!();
    // let factories = CryptoFactories::default();
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let hash1 = utxo1.hash();
    // let hash2 = utxo2.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo1.clone());
    // txn.insert_utxo(utxo2.clone());
    // assert!(db.write(txn).is_ok());
    //
    // let mut txn = DbTransaction::new();
    // txn.spend_utxo(hash1.clone());
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), false);
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), true);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())).unwrap(), true);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())).unwrap(), false);
    //
    // let mut txn = DbTransaction::new();
    // txn.spend_utxo(hash2.clone());
    // txn.unspend_stxo(hash1.clone());
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), false);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())).unwrap(), false);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())).unwrap(), true);
    //
    // if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
    //     assert_eq!(*retrieved_utxo, utxo1);
    // } else {
    //     assert!(false);
    // }
    // if let Some(DbValue::SpentOutput(retrieved_utxo)) = db.fetch(&DbKey::SpentOutput(hash2.clone())).unwrap() {
    //     assert_eq!(*retrieved_utxo, utxo2);
    // } else {
    //     assert!(false);
    // }
    //
    // let mut txn = DbTransaction::new();
    // txn.delete(DbKey::SpentOutput(hash2.clone()));
    // assert!(db.write(txn).is_ok());
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    // assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), false);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash1)).unwrap(), false);
    // assert_eq!(db.contains(&DbKey::SpentOutput(hash2)).unwrap(), false);
}

#[test]
fn lmdb_spend_utxo_and_unspend_stxo() {
    spend_utxo_and_unspend_stxo(create_test_db());
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
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        insert_fetch_metadata(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_mmr_root_and_proof_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    unimplemented!()
    // // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
    //     "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    // );
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
    //     "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    // );
    // let factories = CryptoFactories::default();
    //
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    // let utxo_hash1 = utxo1.hash();
    // let utxo_hash2 = utxo2.hash();
    // let utxo_hash3 = utxo3.hash();
    // let rp_hash1 = utxo1.proof.hash();
    // let rp_hash2 = utxo2.proof.hash();
    // let rp_hash3 = utxo3.proof.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo1.clone());
    // txn.insert_utxo(utxo2.clone());
    // txn.insert_utxo(utxo3.clone());
    // assert!(db.write(txn).is_ok());
    //
    // let mut utxo_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    // assert!(utxo_mmr_check.push(utxo_hash1.clone()).is_ok());
    // assert!(utxo_mmr_check.push(utxo_hash2.clone()).is_ok());
    // assert!(utxo_mmr_check.push(utxo_hash3.clone()).is_ok());
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
    //     utxo_mmr_check.get_merkle_root().unwrap().to_hex()
    // );
    //
    // let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    // let proof1 = db.fetch_mmr_proof(MmrTree::Utxo, 0).unwrap();
    // let proof2 = db.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    // let proof3 = db.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    // assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash1, 0).is_ok());
    // assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash2, 1).is_ok());
    // assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash3, 2).is_ok());
    //
    // let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    // assert_eq!(rp_mmr_check.push(rp_hash1.clone()), Ok(1));
    // assert_eq!(rp_mmr_check.push(rp_hash2.clone()), Ok(2));
    // assert_eq!(rp_mmr_check.push(rp_hash3.clone()), Ok(3));
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
    //     rp_mmr_check.get_merkle_root().unwrap().to_hex()
    // );
    //
    // let mmr_only_root = db.fetch_mmr_only_root(MmrTree::RangeProof).unwrap();
    // let proof1 = db.fetch_mmr_proof(MmrTree::RangeProof, 0).unwrap();
    // let proof2 = db.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    // let proof3 = db.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    // assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash1, 0).is_ok());
    // assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash2, 1).is_ok());
    // assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash3, 2).is_ok());
}

#[test]
fn lmdb_fetch_mmr_root_and_proof_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_mmr_root_and_proof_for_utxo_and_rp(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_mmr_root_and_proof_for_kernel<T: BlockchainBackend>(mut db: T) {
    unimplemented!()
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex(),
    //     "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    // );
    //
    // let kernel1 = create_test_kernel(100.into(), 0);
    // let kernel2 = create_test_kernel(200.into(), 1);
    // let kernel3 = create_test_kernel(300.into(), 2);
    // let hash1 = kernel1.hash();
    // let hash2 = kernel2.hash();
    // let hash3 = kernel3.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel1);
    // txn.insert_kernel(kernel2);
    // txn.insert_kernel(kernel3);
    // assert!(db.write(txn).is_ok());
    //
    // let mut kernel_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    // assert!(kernel_mmr_check.push(hash1.clone()).is_ok());
    // assert!(kernel_mmr_check.push(hash2.clone()).is_ok());
    // assert!(kernel_mmr_check.push(hash3.clone()).is_ok());
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex(),
    //     kernel_mmr_check.get_merkle_root().unwrap().to_hex()
    // );
    //
    // let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Kernel).unwrap();
    // let proof1 = db.fetch_mmr_proof(MmrTree::Kernel, 0).unwrap();
    // let proof2 = db.fetch_mmr_proof(MmrTree::Kernel, 1).unwrap();
    // let proof3 = db.fetch_mmr_proof(MmrTree::Kernel, 2).unwrap();
    // assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &hash1, 0).is_ok());
    // assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &hash2, 1).is_ok());
    // assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &hash3, 2).is_ok());
}

#[test]
fn lmdb_fetch_mmr_root_and_proof_for_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_mmr_root_and_proof_for_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_future_mmr_root_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    unimplemented!()
    // let factories = CryptoFactories::default();
    //
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    // let (utxo4, _) = create_utxo(MicroTari(24_000), &factories, None);
    // let utxo_hash1 = utxo1.hash();
    // let utxo_hash3 = utxo3.hash();
    // let utxo_hash4 = utxo4.hash();
    // let rp_hash3 = utxo3.proof.hash();
    // let rp_hash4 = utxo4.proof.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo1);
    // txn.insert_utxo(utxo2);
    // assert!(db.write(txn).is_ok());
    //
    // let utxo_future_root = db
    //     .calculate_mmr_root(MmrTree::Utxo, vec![utxo_hash3, utxo_hash4], vec![utxo_hash1.clone()])
    //     .unwrap()
    //     .to_hex();
    // let rp_future_root = db
    //     .calculate_mmr_root(MmrTree::RangeProof, vec![rp_hash3, rp_hash4], Vec::new())
    //     .unwrap()
    //     .to_hex();
    // assert_ne!(utxo_future_root, db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    // assert_ne!(rp_future_root, db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex());
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo3);
    // txn.insert_utxo(utxo4);
    // txn.spend_utxo(utxo_hash1);
    // assert!(db.write(txn).is_ok());
    //
    // assert_eq!(utxo_future_root, db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    // assert_eq!(rp_future_root, db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex());
}

#[test]
fn lmdb_fetch_future_mmr_root_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_future_mmr_root_for_utxo_and_rp(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_future_mmr_root_for_for_kernel<T: BlockchainBackend>(mut db: T) {
    unimplemented!()
    // let kernel1 = create_test_kernel(100.into(), 0);
    // let kernel2 = create_test_kernel(200.into(), 1);
    // let kernel3 = create_test_kernel(300.into(), 2);
    // let kernel4 = create_test_kernel(400.into(), 3);
    // let hash3 = kernel3.hash();
    // let hash4 = kernel4.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel1);
    // txn.insert_kernel(kernel2);
    // assert!(db.write(txn).is_ok());
    //
    // let future_root = db
    //     .calculate_mmr_root(MmrTree::Kernel, vec![hash3, hash4], Vec::new())
    //     .unwrap()
    //     .to_hex();
    // assert_ne!(future_root, db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_kernel(kernel3);
    // txn.insert_kernel(kernel4);
    // assert!(db.write(txn).is_ok());
    //
    // assert_eq!(future_root, db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());
}

#[test]
fn lmdb_backend_restore() {
    unimplemented!()
    // let factories = CryptoFactories::default();
    // let network = Network::LocalNet;
    //
    // let consensus = ConsensusManagerBuilder::new(network).build();
    //
    // let txs = vec![(tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0];
    // let orphan = create_orphan_block(10, txs, &consensus);
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let kernel = create_test_kernel(100.into(), 0);
    // let mut header = BlockHeader::new(0);
    // header.height = 1;
    // let orphan_hash = orphan.hash();
    // let utxo_hash = utxo1.hash();
    // let stxo_hash = utxo2.hash();
    // let kernel_hash = kernel.hash();
    // let header_hash = header.hash();
    //
    // // Create backend storage
    // let path = create_temporary_data_path();
    // {
    //     {
    //         let mut db = create_lmdb_database(&path, LMDBConfig::default()).unwrap();
    //         let mut txn = DbTransaction::new();
    //         txn.insert_orphan(orphan.clone().into());
    //         txn.insert_utxo(utxo1);
    //         txn.insert_utxo(utxo2);
    //         txn.insert_kernel(kernel);
    //         txn.insert_header(header.clone());
    //         txn.commit_block();
    //         db.write(txn).unwrap();
    //         let mut txn = DbTransaction::new();
    //         txn.spend_utxo(stxo_hash.clone());
    //         db.write(txn).unwrap();
    //
    //         assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), true);
    //         assert_eq!(db.contains(&DbKey::BlockHash(header_hash.clone())).unwrap(), true);
    //         assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash.clone())).unwrap(), true);
    //         assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash.clone())).unwrap(), true);
    //         assert_eq!(
    //             db.contains(&DbKey::TransactionKernel(kernel_hash.clone())).unwrap(),
    //             true
    //         );
    //         assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash.clone())).unwrap(), true);
    //     }
    //     // Restore backend storage
    //     let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
    //     assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), true);
    //     assert_eq!(db.contains(&DbKey::BlockHash(header_hash)).unwrap(), true);
    //     assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash)).unwrap(), true);
    //     assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash)).unwrap(), true);
    //     assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash)).unwrap(), true);
    //     assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash)).unwrap(), true);
    // }
    //
    // // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse
    // files if std::path::Path::new(&path).exists() {
    //     match std::fs::remove_dir_all(&path) {
    //         Err(e) => println!("\n{:?}\n", e),
    //         _ => (),
    //     }
    // }
}

fn duplicate_utxo<T: BlockchainBackend>(mut db: T) {
    unimplemented!()
    // let factories = CryptoFactories::default();
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let hash1 = utxo1.hash();
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
fn lmdb_duplicate_utxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        duplicate_utxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_last_header<T: BlockchainBackend>(mut db: T) {
    let mut header0 = BlockHeader::new(0);
    header0.height = 0;
    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header().unwrap(), header1);

    let mut txn = DbTransaction::new();
    txn.insert_header(header2.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header().unwrap(), header2);
}

#[test]
fn lmdb_fetch_last_header() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_last_header(db);
    }
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
    }
    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_utxo_rp_mmr_nodes_and_count<T: BlockchainBackend>(mut db: T) {
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
fn lmdb_fetch_utxo_rp_nodes_and_count() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_utxo_rp_mmr_nodes_and_count(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn fetch_kernel_mmr_nodes_and_count<T: BlockchainBackend>(mut db: T) {
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
fn lmdb_fetch_kernel_nodes_and_count() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        fetch_kernel_mmr_nodes_and_count(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        match std::fs::remove_dir_all(&temp_path) {
            Err(e) => println!("\n{:?}\n", e),
            _ => (),
        }
    }
}

fn insert_mmr_node_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    // let factories = CryptoFactories::default();
    // let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    // let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    // let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    // let (utxo4, _) = create_utxo(MicroTari(25_000), &factories, None);
    // let utxo_hash1 = utxo1.hash();
    // let utxo_hash2 = utxo2.hash();
    // let utxo_hash3 = utxo3.hash();
    // let utxo_hash4 = utxo4.hash();
    // let rp_hash1 = utxo1.proof.hash();
    // let rp_hash2 = utxo2.proof.hash();
    // let rp_hash3 = utxo3.proof.hash();
    // let rp_hash4 = utxo4.proof.hash();
    //
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo1.clone());
    // assert!(db.write(txn).is_ok());
    // assert!(db.insert_mmr_node(MmrTree::Utxo, utxo_hash2.clone(), true).is_ok());
    // assert!(db.insert_mmr_node(MmrTree::RangeProof, rp_hash2.clone(), false).is_ok());
    // assert!(db.insert_mmr_node(MmrTree::Utxo, utxo_hash3.clone(), false).is_ok());
    // assert!(db.insert_mmr_node(MmrTree::RangeProof, rp_hash3.clone(), false).is_ok());
    // let mut txn = DbTransaction::new();
    // txn.insert_utxo(utxo4.clone());
    // assert!(db.write(txn).is_ok());
    //
    // let mut utxo_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    // assert!(utxo_mmr_check.push(utxo_hash1.clone()).is_ok());
    // assert!(utxo_mmr_check.push(utxo_hash2.clone()).is_ok());
    // assert!(utxo_mmr_check.push(utxo_hash3.clone()).is_ok());
    // assert!(utxo_mmr_check.push(utxo_hash4.clone()).is_ok());
    // let leaf_index = utxo_mmr_check.find_leaf_index(&utxo_hash2).unwrap().unwrap();
    // assert!(utxo_mmr_check.delete(leaf_index));
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
    //     utxo_mmr_check.get_merkle_root().unwrap().to_hex()
    // );
    //
    // let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    // let proof1 = db.fetch_mmr_proof(MmrTree::Utxo, 0).unwrap();
    // let proof2 = db.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    // let proof3 = db.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    // let proof4 = db.fetch_mmr_proof(MmrTree::Utxo, 3).unwrap();
    // assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash1, 0).is_ok());
    // assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash2, 1).is_ok());
    // assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash3, 2).is_ok());
    // assert!(proof4.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash4, 3).is_ok());
    //
    // let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    // assert_eq!(rp_mmr_check.push(rp_hash1.clone()), Ok(1));
    // assert_eq!(rp_mmr_check.push(rp_hash2.clone()), Ok(2));
    // assert_eq!(rp_mmr_check.push(rp_hash3.clone()), Ok(3));
    // assert_eq!(rp_mmr_check.push(rp_hash4.clone()), Ok(4));
    // assert_eq!(
    //     db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
    //     rp_mmr_check.get_merkle_root().unwrap().to_hex()
    // );
    //
    // let mmr_only_root = db.fetch_mmr_only_root(MmrTree::RangeProof).unwrap();
    // let proof1 = db.fetch_mmr_proof(MmrTree::RangeProof, 0).unwrap();
    // let proof2 = db.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    // let proof3 = db.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    // let proof4 = db.fetch_mmr_proof(MmrTree::RangeProof, 3).unwrap();
    // assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash1, 0).is_ok());
    // assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash2, 1).is_ok());
    // assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash3, 2).is_ok());
    // assert!(proof4.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash4, 3).is_ok());
    unimplemented!()
}

#[test]
fn lmdb_insert_mmr_node_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();
        insert_mmr_node_for_utxo_and_rp(db);
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

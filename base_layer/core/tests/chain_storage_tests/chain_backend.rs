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

use croaring::Bitmap;
use tari_core::{
    blocks::BlockHeader,
    chain_storage::{
        create_lmdb_database,
        BlockchainBackend,
        DbKey,
        DbKeyValuePair,
        DbTransaction,
        DbValue,
        MemoryDatabase,
        MetadataKey,
        MetadataValue,
        MmrTree,
        WriteOperation,
    },
    consensus::{ConsensusConstants, Network},
    helpers::create_orphan_block,
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        helpers::{create_test_kernel, create_utxo},
        tari_amount::MicroTari,
        types::{CryptoFactories, HashDigest},
    },
    tx,
};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, Hashable};
use tari_mmr::{MmrCacheConfig, MutableMmr};
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
fn memory_insert_contains_delete_and_fetch_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_header(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_header() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_contains_delete_and_fetch_header(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn insert_contains_delete_and_fetch_utxo<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
    let hash = utxo.hash();
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())).unwrap(), true);
    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::UnspentOutput(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash)).unwrap(), false);
}

#[test]
fn memory_insert_contains_delete_and_fetch_utxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_utxo(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_utxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_contains_delete_and_fetch_utxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn insert_contains_delete_and_fetch_kernel<T: BlockchainBackend>(mut db: T) {
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())).unwrap(), true);
    if let Some(DbValue::TransactionKernel(retrieved_kernel)) =
        db.fetch(&DbKey::TransactionKernel(hash.clone())).unwrap()
    {
        assert_eq!(*retrieved_kernel, kernel);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::TransactionKernel(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash)).unwrap(), false);
}

#[test]
fn memory_insert_contains_delete_and_fetch_kernel() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_kernel(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_contains_delete_and_fetch_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn insert_contains_delete_and_fetch_orphan<T: BlockchainBackend>(mut db: T, consensus_constants: &ConsensusConstants) {
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, consensus_constants);
    let hash = orphan.hash();
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
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
    let consensus_constants = network.create_consensus_constants();
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_orphan(db, &consensus_constants);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_orphan() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let network = Network::LocalNet;
        let consensus_constants = network.create_consensus_constants();
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_contains_delete_and_fetch_orphan(db, &consensus_constants);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn spend_utxo_and_unspend_stxo<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    txn.insert_utxo(utxo2.clone());
    assert!(db.write(txn).is_ok());

    let mut txn = DbTransaction::new();
    txn.spend_utxo(hash1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())).unwrap(), false);

    let mut txn = DbTransaction::new();
    txn.spend_utxo(hash2.clone());
    txn.unspend_stxo(hash1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())).unwrap(), true);

    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo1);
    } else {
        assert!(false);
    }
    if let Some(DbValue::SpentOutput(retrieved_utxo)) = db.fetch(&DbKey::SpentOutput(hash2.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo2);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::SpentOutput(hash2.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2)).unwrap(), false);
}

#[test]
fn memory_spend_utxo_and_unspend_stxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    spend_utxo_and_unspend_stxo(db);
}

#[test]
fn lmdb_spend_utxo_and_unspend_stxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        spend_utxo_and_unspend_stxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
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
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(Some(chain_height)),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::AccumulatedWork,
        MetadataValue::AccumulatedWork(Some(accumulated_work.into())),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::PruningHorizon,
        MetadataValue::PruningHorizon(pruning_horizon),
    ));
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::BestBlock,
        MetadataValue::BestBlock(Some(hash.clone())),
    ));
    assert!(db.write(txn).is_ok());

    if let Some(DbValue::Metadata(MetadataValue::ChainHeight(Some(retrieved_chain_height)))) =
        db.fetch(&DbKey::Metadata(MetadataKey::ChainHeight)).unwrap()
    {
        assert_eq!(retrieved_chain_height, chain_height);
    } else {
        assert!(false);
    }
    if let Some(DbValue::Metadata(MetadataValue::AccumulatedWork(retrieved_accumulated_work))) =
        db.fetch(&DbKey::Metadata(MetadataKey::AccumulatedWork)).unwrap()
    {
        assert_eq!(retrieved_accumulated_work, Some(accumulated_work.into()));
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
    if let Some(DbValue::Metadata(MetadataValue::BestBlock(Some(retrieved_hash)))) =
        db.fetch(&DbKey::Metadata(MetadataKey::BestBlock)).unwrap()
    {
        assert_eq!(retrieved_hash, hash);
    } else {
        assert!(false);
    }
}

#[test]
fn memory_insert_fetch_metadata() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_fetch_metadata(db);
}

#[test]
fn lmdb_insert_fetch_metadata() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_fetch_metadata(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_mmr_root_and_proof_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    assert_eq!(
        db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let utxo_hash1 = utxo1.hash();
    let utxo_hash2 = utxo2.hash();
    let utxo_hash3 = utxo3.hash();
    let rp_hash1 = utxo1.proof.hash();
    let rp_hash2 = utxo2.proof.hash();
    let rp_hash3 = utxo3.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    txn.insert_utxo(utxo2.clone());
    txn.insert_utxo(utxo3.clone());
    assert!(db.write(txn).is_ok());

    let mut utxo_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(utxo_mmr_check.push(&utxo_hash1).is_ok());
    assert!(utxo_mmr_check.push(&utxo_hash2).is_ok());
    assert!(utxo_mmr_check.push(&utxo_hash3).is_ok());
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
        utxo_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::Utxo, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash3, 2).is_ok());

    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert_eq!(rp_mmr_check.push(&rp_hash1), Ok(1));
    assert_eq!(rp_mmr_check.push(&rp_hash2), Ok(2));
    assert_eq!(rp_mmr_check.push(&rp_hash3), Ok(3));
    assert_eq!(
        db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
        rp_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::RangeProof).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::RangeProof, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash3, 2).is_ok());
}

#[test]
fn memory_fetch_mmr_root_and_proof_for_utxo_and_rp() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_mmr_root_and_proof_for_utxo_and_rp(db);
}

#[test]
fn lmdb_fetch_mmr_root_and_proof_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_mmr_root_and_proof_for_utxo_and_rp(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_mmr_root_and_proof_for_kernel<T: BlockchainBackend>(mut db: T) {
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );

    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();
    let hash3 = kernel3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1);
    txn.insert_kernel(kernel2);
    txn.insert_kernel(kernel3);
    assert!(db.write(txn).is_ok());

    let mut kernel_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(kernel_mmr_check.push(&hash1).is_ok());
    assert!(kernel_mmr_check.push(&hash2).is_ok());
    assert!(kernel_mmr_check.push(&hash3).is_ok());
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex(),
        kernel_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Kernel).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::Kernel, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::Kernel, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::Kernel, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &hash3, 2).is_ok());
}

#[test]
fn memory_fetch_mmr_root_and_proof_for_kernel() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_mmr_root_and_proof_for_kernel(db);
}

#[test]
fn lmdb_fetch_mmr_root_and_proof_for_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_mmr_root_and_proof_for_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_future_mmr_root_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let (utxo4, _) = create_utxo(MicroTari(24_000), &factories, None);
    let utxo_hash1 = utxo1.hash();
    let utxo_hash3 = utxo3.hash();
    let utxo_hash4 = utxo4.hash();
    let rp_hash3 = utxo3.proof.hash();
    let rp_hash4 = utxo4.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_utxo(utxo2);
    assert!(db.write(txn).is_ok());

    let utxo_future_root = db
        .calculate_mmr_root(MmrTree::Utxo, vec![utxo_hash3, utxo_hash4], vec![utxo_hash1.clone()])
        .unwrap()
        .to_hex();
    let rp_future_root = db
        .calculate_mmr_root(MmrTree::RangeProof, vec![rp_hash3, rp_hash4], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(utxo_future_root, db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    assert_ne!(rp_future_root, db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo3);
    txn.insert_utxo(utxo4);
    txn.spend_utxo(utxo_hash1);
    assert!(db.write(txn).is_ok());

    assert_eq!(utxo_future_root, db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex());
    assert_eq!(rp_future_root, db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex());
}

#[test]
fn memory_fetch_future_mmr_root_for_utxo_and_rp() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_future_mmr_root_for_utxo_and_rp(db);
}

#[test]
fn lmdb_fetch_future_mmr_root_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_future_mmr_root_for_utxo_and_rp(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_future_mmr_root_for_for_kernel<T: BlockchainBackend>(mut db: T) {
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let kernel4 = create_test_kernel(400.into(), 3);
    let hash3 = kernel3.hash();
    let hash4 = kernel4.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1);
    txn.insert_kernel(kernel2);
    assert!(db.write(txn).is_ok());

    let future_root = db
        .calculate_mmr_root(MmrTree::Kernel, vec![hash3, hash4], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(future_root, db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel3);
    txn.insert_kernel(kernel4);
    assert!(db.write(txn).is_ok());

    assert_eq!(future_root, db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());
}

#[test]
fn memory_fetch_future_mmr_root_for_for_kernel() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_future_mmr_root_for_for_kernel(db);
}

#[test]
fn lmdb_fetch_future_mmr_root_for_for_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_future_mmr_root_for_for_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn commit_block_and_create_fetch_checkpoint_and_rewind_mmr<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let kernel1 = create_test_kernel(100.into(), 0);
    let header1 = BlockHeader::new(0);
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_kernel(kernel1);
    txn.insert_header(header1.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let kernel2 = create_test_kernel(200.into(), 0);
    let header2 = BlockHeader::from_previous(&header1);
    let utxo_hash2 = utxo2.hash();
    let kernel_hash2 = kernel2.hash();
    let rp_hash2 = utxo2.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2);
    txn.spend_utxo(utxo_hash1.clone());
    txn.insert_kernel(kernel2);
    txn.insert_header(header2);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0).unwrap();
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0).unwrap();
    let range_proof_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0).unwrap();
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1).unwrap();
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1).unwrap();
    let range_proof_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1).unwrap();
    assert_eq!(utxo_cp0.nodes_added().len(), 1);
    assert_eq!(utxo_cp0.accumulated_nodes_added_count(), 1);
    assert_eq!(utxo_cp0.nodes_added()[0], utxo_hash1);
    assert_eq!(utxo_cp0.nodes_deleted().to_vec().len(), 0);
    assert_eq!(kernel_cp0.nodes_added()[0], kernel_hash1);
    assert_eq!(range_proof_cp0.nodes_added()[0], rp_hash1);
    assert_eq!(utxo_cp1.accumulated_nodes_added_count(), 2);
    assert_eq!(utxo_cp1.nodes_added()[0], utxo_hash2);
    assert_eq!(utxo_cp1.nodes_deleted().to_vec()[0], 0);
    assert_eq!(kernel_cp1.nodes_added()[0], kernel_hash2);
    assert_eq!(range_proof_cp1.nodes_added()[0], rp_hash2);
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())).unwrap(), false);
    assert_eq!(
        db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())).unwrap(),
        true
    );
    assert_eq!(
        db.contains(&DbKey::TransactionKernel(kernel_hash2.clone())).unwrap(),
        true
    );
    assert_eq!(db.contains(&DbKey::BlockHeader(0)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::BlockHeader(1)).unwrap(), true);

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::BlockHeader(1));
    txn.delete(DbKey::TransactionKernel(kernel_hash2.clone()));
    txn.delete(DbKey::UnspentOutput(utxo_hash2.clone()));
    txn.unspend_stxo(utxo_hash1.clone());
    txn.rewind_kernel_mmr(1);
    txn.rewind_utxo_mmr(1);
    txn.rewind_rangeproof_mmr(1);
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0).unwrap();
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0).unwrap();
    let range_proof_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0).unwrap();
    assert_eq!(utxo_cp0.accumulated_nodes_added_count(), 1);
    assert_eq!(utxo_cp0.nodes_added()[0], utxo_hash1);
    assert_eq!(utxo_cp0.nodes_deleted().to_vec().len(), 0);
    assert_eq!(kernel_cp0.nodes_added()[0], kernel_hash1);
    assert_eq!(range_proof_cp0.nodes_added()[0], rp_hash1);
    assert!(db.fetch_checkpoint_at_height(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_checkpoint_at_height(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1).is_err());

    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash1)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2)).unwrap(), false);
    assert_eq!(db.contains(&DbKey::BlockHeader(0)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::BlockHeader(1)).unwrap(), false);
}

#[test]
fn memory_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    let db = MemoryDatabase::<HashDigest>::default();
    commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
}

#[test]
fn lmdb_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn for_each_orphan<T: BlockchainBackend>(mut db: T, consensus_constants: &ConsensusConstants) {
    let orphan1 = create_orphan_block(
        5,
        vec![(tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0],
        consensus_constants,
    );
    let orphan2 = create_orphan_block(
        10,
        vec![(tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0],
        consensus_constants,
    );
    let orphan3 = create_orphan_block(
        15,
        vec![(tx!(3000.into(), fee: 40.into(), inputs: 1, outputs: 2)).0],
        consensus_constants,
    );
    let hash1 = orphan1.hash();
    let hash2 = orphan2.hash();
    let hash3 = orphan3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan1.clone());
    txn.insert_orphan(orphan2.clone());
    txn.insert_orphan(orphan3.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash3.clone())).unwrap(), true);

    let mut orphan1_found = false;
    let mut orphan2_found = false;
    let mut orphan3_found = false;
    assert!(db
        .for_each_orphan(|pair| {
            let (hash, block) = pair.unwrap();
            if (hash == hash1) && (block == orphan1) {
                orphan1_found = true;
            } else if (hash == hash2) && (block == orphan2) {
                orphan2_found = true;
            } else if (hash == hash3) && (block == orphan3) {
                orphan3_found = true;
            }
        })
        .is_ok());
    assert!(orphan1_found & orphan2_found & orphan3_found);
}

#[test]
fn memory_for_each_orphan() {
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let db = MemoryDatabase::<HashDigest>::default();
    for_each_orphan(db, &consensus_constants);
}

#[test]
fn lmdb_for_each_orphan() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let network = Network::LocalNet;
        let consensus_constants = network.create_consensus_constants();
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        for_each_orphan(db, &consensus_constants);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn for_each_kernel<T: BlockchainBackend>(mut db: T) {
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();
    let hash3 = kernel3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone());
    txn.insert_kernel(kernel2.clone());
    txn.insert_kernel(kernel3.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash3.clone())).unwrap(), true);

    let mut kernel1_found = false;
    let mut kernel2_found = false;
    let mut kernel3_found = false;
    assert!(db
        .for_each_kernel(|pair| {
            let (hash, kernel) = pair.unwrap();
            if (hash == hash1) && (kernel == kernel1) {
                kernel1_found = true;
            } else if (hash == hash2) && (kernel == kernel2) {
                kernel2_found = true;
            } else if (hash == hash3) && (kernel == kernel3) {
                kernel3_found = true;
            }
        })
        .is_ok());
    assert!(kernel1_found & kernel2_found & kernel3_found);
}

#[test]
fn memory_for_each_kernel() {
    let db = MemoryDatabase::<HashDigest>::default();
    for_each_kernel(db);
}

#[test]
fn lmdb_for_each_kernel() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        for_each_kernel(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn for_each_header<T: BlockchainBackend>(mut db: T) {
    let header1 = BlockHeader::new(0);
    let header2 = BlockHeader::from_previous(&header1);
    let header3 = BlockHeader::from_previous(&header2);
    let key1 = header1.height;
    let key2 = header2.height;
    let key3 = header3.height;

    let mut txn = DbTransaction::new();
    txn.insert_header(header1.clone());
    txn.insert_header(header2.clone());
    txn.insert_header(header3.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::BlockHeader(key1)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::BlockHeader(key2)).unwrap(), true);
    assert_eq!(db.contains(&DbKey::BlockHeader(key3)).unwrap(), true);

    let mut header1_found = false;
    let mut header2_found = false;
    let mut header3_found = false;
    assert!(db
        .for_each_header(|pair| {
            let (key, header) = pair.unwrap();
            if (key == key1) && (header == header1) {
                header1_found = true;
            } else if (key == key2) && (header == header2) {
                header2_found = true;
            } else if (key == key3) && (header == header3) {
                header3_found = true;
            }
        })
        .is_ok());
    assert!(header1_found & header2_found & header3_found);
}

#[test]
fn memory_for_each_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    for_each_header(db);
}

#[test]
fn lmdb_for_each_header() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        for_each_header(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn for_each_utxo<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();
    let hash3 = utxo3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    txn.insert_utxo(utxo2.clone());
    txn.insert_utxo(utxo3.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash3.clone())).unwrap(), true);

    let mut utxo1_found = false;
    let mut utxo2_found = false;
    let mut utxo3_found = false;
    assert!(db
        .for_each_utxo(|pair| {
            let (hash, utxo) = pair.unwrap();
            if (hash == hash1) && (utxo == utxo1) {
                utxo1_found = true;
            } else if (hash == hash2) && (utxo == utxo2) {
                utxo2_found = true;
            } else if (hash == hash3) && (utxo == utxo3) {
                utxo3_found = true;
            }
        })
        .is_ok());
    assert!(utxo1_found & utxo2_found & utxo3_found);
}

#[test]
fn memory_for_each_utxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    for_each_utxo(db);
}

#[test]
fn lmdb_for_each_utxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        for_each_utxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

#[test]
fn lmdb_backend_restore() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();

    let txs = vec![(tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0];
    let orphan = create_orphan_block(10, txs, &consensus_constants);
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let kernel = create_test_kernel(100.into(), 0);
    let mut header = BlockHeader::new(0);
    header.height = 1;
    let orphan_hash = orphan.hash();
    let utxo_hash = utxo1.hash();
    let stxo_hash = utxo2.hash();
    let kernel_hash = kernel.hash();
    let header_hash = header.hash();

    // Create backend storage
    let path = create_temporary_data_path();
    {
        {
            let mut db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
            let mut txn = DbTransaction::new();
            txn.insert_orphan(orphan.clone());
            txn.insert_utxo(utxo1);
            txn.insert_utxo(utxo2);
            txn.insert_kernel(kernel);
            txn.insert_header(header.clone());
            txn.commit_block();
            db.write(txn).unwrap();
            let mut txn = DbTransaction::new();
            txn.spend_utxo(stxo_hash.clone());
            db.write(txn).unwrap();

            assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), true);
            assert_eq!(db.contains(&DbKey::BlockHash(header_hash.clone())).unwrap(), true);
            assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash.clone())).unwrap(), true);
            assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash.clone())).unwrap(), true);
            assert_eq!(
                db.contains(&DbKey::TransactionKernel(kernel_hash.clone())).unwrap(),
                true
            );
            assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash.clone())).unwrap(), true);
        }
        // Restore backend storage
        let db = create_lmdb_database(&path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        assert_eq!(db.contains(&DbKey::BlockHeader(header.height)).unwrap(), true);
        assert_eq!(db.contains(&DbKey::BlockHash(header_hash)).unwrap(), true);
        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash)).unwrap(), true);
        assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash)).unwrap(), true);
        assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash)).unwrap(), true);
        assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash)).unwrap(), true);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&path).exists() {
        std::fs::remove_dir_all(&path).unwrap();
    }
}

#[test]
fn lmdb_mmr_reset_and_commit() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let factories = CryptoFactories::default();
        let mut db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();

        let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
        let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
        let kernel1 = create_test_kernel(100.into(), 0);
        let kernel2 = create_test_kernel(200.into(), 0);
        let mut header1 = BlockHeader::new(0);
        header1.height = 1;
        let utxo_hash1 = utxo1.hash();
        let utxo_hash2 = utxo2.hash();
        let kernel_hash1 = kernel1.hash();
        let kernel_hash2 = kernel2.hash();
        let rp_hash1 = utxo1.proof.hash();
        let header_hash1 = header1.hash();

        let mut txn = DbTransaction::new();
        txn.insert_utxo(utxo1);
        txn.insert_kernel(kernel1);
        txn.insert_header(header1);
        txn.commit_block();
        assert!(db.write(txn).is_ok());

        // Reset mmrs as a mmr txn failed without applying storage txns.
        let mut txn = DbTransaction::new();
        txn.spend_utxo(utxo_hash2.clone());
        txn.commit_block();
        assert!(db.write(txn).is_err());

        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())).unwrap(), true);
        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())).unwrap(), false);
        assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), false);
        assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())).unwrap(), false);
        assert_eq!(
            db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())).unwrap(),
            true
        );
        assert_eq!(
            db.contains(&DbKey::TransactionKernel(kernel_hash2.clone())).unwrap(),
            false
        );
        assert_eq!(db.contains(&DbKey::BlockHash(header_hash1.clone())).unwrap(), true);
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
            utxo_hash1
        );
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
            kernel_hash1
        );
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0)
                .unwrap()
                .nodes_added()[0],
            rp_hash1
        );
        assert!(db.fetch_checkpoint_at_height(MmrTree::Utxo, 1).is_err());
        assert!(db.fetch_checkpoint_at_height(MmrTree::Kernel, 1).is_err());
        assert!(db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1).is_err());

        // Reset mmrs as a storage txn failed after the mmr txns were applied, ensure the previous state was preserved.
        let mut txn = DbTransaction::new();
        txn.spend_utxo(utxo_hash1.clone());
        txn.delete(DbKey::TransactionKernel(kernel_hash1.clone()));
        txn.delete(DbKey::TransactionKernel(kernel_hash2.clone()));
        txn.commit_block();
        assert!(db.write(txn).is_err());

        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())).unwrap(), true);
        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())).unwrap(), false);
        assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), false);
        assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2)).unwrap(), false);
        assert_eq!(
            db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())).unwrap(),
            true
        );
        assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2)).unwrap(), false);
        assert_eq!(db.contains(&DbKey::BlockHash(header_hash1.clone())).unwrap(), true);
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
            utxo_hash1
        );
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
            kernel_hash1
        );
        assert_eq!(
            db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0)
                .unwrap()
                .nodes_added()[0],
            rp_hash1
        );
        assert!(db.fetch_checkpoint_at_height(MmrTree::Utxo, 1).is_err());
        assert!(db.fetch_checkpoint_at_height(MmrTree::Kernel, 1).is_err());
        assert!(db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1).is_err());
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_checkpoint<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let kernel1 = create_test_kernel(100.into(), 0);
    let mut header1 = BlockHeader::new(0);
    header1.height = 0;
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_kernel(kernel1);
    txn.insert_header(header1.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let kernel2 = create_test_kernel(200.into(), 0);
    let header2 = BlockHeader::from_previous(&header1);
    let utxo_hash2 = utxo2.hash();
    let kernel_hash2 = kernel2.hash();
    let rp_hash2 = utxo2.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2);
    txn.insert_kernel(kernel2);
    txn.insert_header(header2.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1);
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1);
    let rp_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1);
    assert!(utxo_cp0.unwrap().nodes_added().contains(&utxo_hash1));
    assert!(utxo_cp1.unwrap().nodes_added().contains(&utxo_hash2));
    assert!(kernel_cp0.unwrap().nodes_added().contains(&kernel_hash1));
    assert!(kernel_cp1.unwrap().nodes_added().contains(&kernel_hash2));
    assert!(rp_cp0.unwrap().nodes_added().contains(&rp_hash1));
    assert!(rp_cp1.unwrap().nodes_added().contains(&rp_hash2));

    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let kernel3 = create_test_kernel(300.into(), 0);
    let header3 = BlockHeader::from_previous(&header2);
    let utxo_hash3 = utxo3.hash();
    let kernel_hash3 = kernel3.hash();
    let rp_hash3 = utxo3.proof.hash();

    let (utxo4, _) = create_utxo(MicroTari(20_000), &factories, None);
    let kernel4 = create_test_kernel(300.into(), 0);
    let utxo_hash4 = utxo4.hash();
    let kernel_hash4 = kernel4.hash();
    let rp_hash4 = utxo4.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo3);
    txn.insert_utxo(utxo4);
    txn.insert_kernel(kernel3);
    txn.insert_kernel(kernel4);
    txn.insert_header(header3);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0).unwrap();
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1).unwrap();
    let utxo_cp2 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 2).unwrap();
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0).unwrap();
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1).unwrap();
    let kernel_cp2 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 2).unwrap();
    let rp_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0).unwrap();
    let rp_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1).unwrap();
    let rp_cp2 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 2).unwrap();
    assert!(utxo_cp0.nodes_added().contains(&utxo_hash1));
    assert_eq!(utxo_cp0.accumulated_nodes_added_count(), 1);
    assert!(utxo_cp1.nodes_added().contains(&utxo_hash2));
    assert_eq!(utxo_cp1.accumulated_nodes_added_count(), 2);
    assert!(utxo_cp2.nodes_added().contains(&utxo_hash3));
    assert!(utxo_cp2.nodes_added().contains(&utxo_hash4));
    assert_eq!(utxo_cp2.accumulated_nodes_added_count(), 4);
    assert!(kernel_cp0.nodes_added().contains(&kernel_hash1));
    assert_eq!(kernel_cp0.accumulated_nodes_added_count(), 1);
    assert!(kernel_cp1.nodes_added().contains(&kernel_hash2));
    assert_eq!(kernel_cp1.accumulated_nodes_added_count(), 2);
    assert!(kernel_cp2.nodes_added().contains(&kernel_hash3));
    assert!(kernel_cp2.nodes_added().contains(&kernel_hash4));
    assert_eq!(kernel_cp2.accumulated_nodes_added_count(), 4);
    assert!(rp_cp0.nodes_added().contains(&rp_hash1));
    assert_eq!(rp_cp0.accumulated_nodes_added_count(), 1);
    assert!(rp_cp1.nodes_added().contains(&rp_hash2));
    assert_eq!(rp_cp1.accumulated_nodes_added_count(), 2);
    assert!(rp_cp2.nodes_added().contains(&rp_hash3));
    assert!(rp_cp2.nodes_added().contains(&rp_hash4));
    assert_eq!(rp_cp2.accumulated_nodes_added_count(), 4);
}

#[test]
fn memory_fetch_checkpoint() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    fetch_checkpoint(db);
}

#[test]
fn lmdb_fetch_checkpoint() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), mmr_cache_config).unwrap();
        fetch_checkpoint(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn merging_and_fetch_checkpoints_and_stxo_discard<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 0);
    let kernel3 = create_test_kernel(300.into(), 0);
    let mut header1 = BlockHeader::new(0);
    header1.height = 0;
    let header2 = BlockHeader::from_previous(&header1);
    let header3 = BlockHeader::from_previous(&header2);
    let utxo_hash1 = utxo1.hash();
    let utxo_hash2 = utxo2.hash();
    let utxo_hash3 = utxo3.hash();
    let kernel_hash1 = kernel1.hash();
    let kernel_hash2 = kernel2.hash();
    let kernel_hash3 = kernel3.hash();
    let rp_hash1 = utxo1.proof.hash();
    let rp_hash2 = utxo2.proof.hash();
    let rp_hash3 = utxo3.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.insert_kernel(kernel1);
    txn.insert_header(header1.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2);
    txn.insert_kernel(kernel2);
    txn.insert_header(header2.clone());
    txn.spend_utxo(utxo_hash1.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo3);
    txn.insert_kernel(kernel3);
    txn.insert_header(header3.clone());
    txn.spend_utxo(utxo_hash2.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    // Merge should not be performed as cp_count less than max_cp_count
    let mut txn = DbTransaction::new();
    txn.merge_checkpoints(100);
    assert!(db.write(txn).is_ok());
    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1);
    let utxo_cp2 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 2);
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1);
    let kernel_cp2 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 2);
    let rp_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1);
    let rp_cp2 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 2);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash3.clone())).unwrap(), false);
    assert!(utxo_cp0.unwrap().nodes_added().contains(&utxo_hash1));
    assert!(utxo_cp1.unwrap().nodes_added().contains(&utxo_hash2));
    assert!(utxo_cp2.unwrap().nodes_added().contains(&utxo_hash3));
    assert!(kernel_cp0.unwrap().nodes_added().contains(&kernel_hash1));
    assert!(kernel_cp1.unwrap().nodes_added().contains(&kernel_hash2));
    assert!(kernel_cp2.unwrap().nodes_added().contains(&kernel_hash3));
    assert!(rp_cp0.unwrap().nodes_added().contains(&rp_hash1));
    assert!(rp_cp1.unwrap().nodes_added().contains(&rp_hash2));
    assert!(rp_cp2.unwrap().nodes_added().contains(&rp_hash3));

    let mut txn = DbTransaction::new();
    txn.merge_checkpoints(2);
    assert!(db.write(txn).is_ok());
    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1);
    let utxo_cp2 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 2);
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1);
    let kernel_cp2 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 2);
    let rp_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1);
    let rp_cp2 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 2);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())).unwrap(), true);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash3.clone())).unwrap(), false);
    assert!(utxo_cp0.is_err());
    assert!(utxo_cp1.unwrap().nodes_added().contains(&utxo_hash2));
    assert!(utxo_cp2.unwrap().nodes_added().contains(&utxo_hash3));
    assert!(kernel_cp0.is_err());
    assert!(kernel_cp1.unwrap().nodes_added().contains(&kernel_hash2));
    assert!(kernel_cp2.unwrap().nodes_added().contains(&kernel_hash3));
    assert!(rp_cp0.is_err());
    assert!(rp_cp1.unwrap().nodes_added().contains(&rp_hash2));
    assert!(rp_cp2.unwrap().nodes_added().contains(&rp_hash3));

    let mut txn = DbTransaction::new();
    txn.merge_checkpoints(1);
    assert!(db.write(txn).is_ok());
    let utxo_cp0 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 1);
    let utxo_cp2 = db.fetch_checkpoint_at_height(MmrTree::Utxo, 2);
    let kernel_cp0 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 1);
    let kernel_cp2 = db.fetch_checkpoint_at_height(MmrTree::Kernel, 2);
    let rp_cp0 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 1);
    let rp_cp2 = db.fetch_checkpoint_at_height(MmrTree::RangeProof, 2);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())).unwrap(), false);
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash3.clone())).unwrap(), false);
    assert!(utxo_cp0.is_err());
    assert!(utxo_cp1.is_err());
    assert!(utxo_cp2.unwrap().nodes_added().contains(&utxo_hash3));
    assert!(kernel_cp0.is_err());
    assert!(kernel_cp1.is_err());
    assert!(kernel_cp2.unwrap().nodes_added().contains(&kernel_hash3));
    assert!(rp_cp0.is_err());
    assert!(rp_cp1.is_err());
    assert!(rp_cp2.unwrap().nodes_added().contains(&rp_hash3));
}

#[test]
fn memory_merging_and_fetch_checkpoints_and_stxo_discard() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    merging_and_fetch_checkpoints_and_stxo_discard(db);
}

#[test]
fn lmdb_merging_and_fetch_checkpoints_and_stxo_discard() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), mmr_cache_config).unwrap();
        merging_and_fetch_checkpoints_and_stxo_discard(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn duplicate_utxo<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let hash1 = utxo1.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo_with_hash(hash1.clone(), utxo1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())).unwrap(), true);
    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo1);
    } else {
        assert!(false);
    }
    let mut txn = DbTransaction::new();
    txn.insert_utxo_with_hash(hash1.clone(), utxo2.clone());
    assert!(db.write(txn).is_err()); // This should fail
    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo1); // original data should still be there
    } else {
        assert!(false);
    }
}

#[test]
fn memory_duplicate_utxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    duplicate_utxo(db);
}

#[test]
fn lmdb_duplicate_utxo() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        duplicate_utxo(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_last_header<T: BlockchainBackend>(mut db: T) {
    let mut header0 = BlockHeader::new(0);
    header0.height = 0;
    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    assert_eq!(db.fetch_last_header().unwrap(), None);

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header().unwrap(), Some(header1));

    let mut txn = DbTransaction::new();
    txn.insert_header(header2.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header().unwrap(), Some(header2));
}

#[test]
fn memory_fetch_last_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_last_header(db);
}

#[test]
fn lmdb_fetch_last_header() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_last_header(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_target_difficulties<T: BlockchainBackend>(mut db: T) {
    let mut header0 = BlockHeader::new(0);
    header0.pow.pow_algo = PowAlgorithm::Blake;
    header0.pow.target_difficulty = Difficulty::from(100);
    let mut header1 = BlockHeader::from_previous(&header0);
    header1.pow.pow_algo = PowAlgorithm::Monero;
    header1.pow.target_difficulty = Difficulty::from(1000);
    let mut header2 = BlockHeader::from_previous(&header1);
    header2.pow.pow_algo = PowAlgorithm::Blake;
    header2.pow.target_difficulty = Difficulty::from(2000);
    let mut header3 = BlockHeader::from_previous(&header2);
    header3.pow.pow_algo = PowAlgorithm::Blake;
    header3.pow.target_difficulty = Difficulty::from(3000);
    let mut header4 = BlockHeader::from_previous(&header3);
    header4.pow.pow_algo = PowAlgorithm::Monero;
    header4.pow.target_difficulty = Difficulty::from(200);
    let mut header5 = BlockHeader::from_previous(&header4);
    header5.pow.pow_algo = PowAlgorithm::Blake;
    header5.pow.target_difficulty = Difficulty::from(4000);
    assert!(db.fetch_target_difficulties(PowAlgorithm::Blake, 5, 100).is_err());
    assert!(db.fetch_target_difficulties(PowAlgorithm::Monero, 5, 100).is_err());

    let mut txn = DbTransaction::new();
    txn.insert_header(header0.clone());
    txn.insert_header(header1.clone());
    txn.insert_header(header2.clone());
    txn.insert_header(header3.clone());
    txn.insert_header(header4.clone());
    txn.insert_header(header5.clone());
    txn.insert(DbKeyValuePair::Metadata(
        MetadataKey::ChainHeight,
        MetadataValue::ChainHeight(Some(header5.height)),
    ));
    assert!(db.write(txn).is_ok());

    // Check block window constraint
    let desired_targets: Vec<(EpochTime, Difficulty)> = vec![
        (header2.timestamp, header2.pow.target_difficulty),
        (header3.timestamp, header3.pow.target_difficulty),
    ];
    assert_eq!(
        db.fetch_target_difficulties(PowAlgorithm::Blake, header4.height, 2)
            .unwrap(),
        desired_targets
    );
    let desired_targets: Vec<(EpochTime, Difficulty)> = vec![
        (header1.timestamp, header1.pow.target_difficulty),
        (header4.timestamp, header4.pow.target_difficulty),
    ];
    assert_eq!(
        db.fetch_target_difficulties(PowAlgorithm::Monero, header4.height, 2)
            .unwrap(),
        desired_targets
    );
    // Check search from tip to genesis block
    let desired_targets: Vec<(EpochTime, Difficulty)> = vec![
        (header0.timestamp, header0.pow.target_difficulty),
        (header2.timestamp, header2.pow.target_difficulty),
        (header3.timestamp, header3.pow.target_difficulty),
        (header5.timestamp, header5.pow.target_difficulty),
    ];
    assert_eq!(
        db.fetch_target_difficulties(PowAlgorithm::Blake, header5.height, 100)
            .unwrap(),
        desired_targets
    );
    let desired_targets: Vec<(EpochTime, Difficulty)> = vec![
        (header1.timestamp, header1.pow.target_difficulty),
        (header4.timestamp, header4.pow.target_difficulty),
    ];
    assert_eq!(
        db.fetch_target_difficulties(PowAlgorithm::Monero, header5.height, 100)
            .unwrap(),
        desired_targets
    );
}

#[test]
fn memory_fetch_target_difficulties() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_target_difficulties(db);
}

#[test]
fn lmdb_fetch_target_difficulties() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_target_difficulties(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_utxo_rp_mmr_nodes_and_count<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(20_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(30_000), &factories, None);
    let (utxo4, _) = create_utxo(MicroTari(40_000), &factories, None);
    let (utxo5, _) = create_utxo(MicroTari(50_000), &factories, None);
    let (utxo6, _) = create_utxo(MicroTari(60_000), &factories, None);
    let utxo_hash1 = utxo1.hash();
    let utxo_hash2 = utxo2.hash();
    let utxo_hash3 = utxo3.hash();
    let utxo_hash4 = utxo4.hash();
    let utxo_hash5 = utxo5.hash();
    let utxo_hash6 = utxo6.hash();
    let utxo_leaf_nodes = vec![
        (utxo_hash1.clone(), true),
        (utxo_hash2.clone(), false),
        (utxo_hash3.clone(), true),
        (utxo_hash4.clone(), true),
        (utxo_hash5.clone(), false),
        (utxo_hash6.clone(), false),
    ];
    let rp_leaf_nodes = vec![
        (utxo1.proof.hash(), false),
        (utxo2.proof.hash(), false),
        (utxo3.proof.hash(), false),
        (utxo4.proof.hash(), false),
        (utxo5.proof.hash(), false),
        (utxo6.proof.hash(), false),
    ];

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1);
    txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2);
    txn.insert_utxo(utxo3);
    txn.spend_utxo(utxo_hash1.clone());
    txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo4);
    txn.insert_utxo(utxo5);
    txn.spend_utxo(utxo_hash3.clone());
    txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo6);
    txn.spend_utxo(utxo_hash4.clone());
    txn.operations.push(WriteOperation::CreateMmrCheckpoint(MmrTree::Utxo));
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::RangeProof));
    assert!(db.write(txn).is_ok());

    for i in 0..=3 {
        let mmr_node = db.fetch_mmr_node(MmrTree::Utxo, i, None).unwrap();
        assert_eq!(mmr_node, utxo_leaf_nodes[i as usize]);
        let mmr_node = db.fetch_mmr_node(MmrTree::RangeProof, i, None).unwrap();
        assert_eq!(mmr_node, rp_leaf_nodes[i as usize]);

        let mmr_node = db.fetch_mmr_nodes(MmrTree::Utxo, i, 3, None).unwrap();
        assert_eq!(mmr_node.len(), 3);
        assert_eq!(mmr_node[0], utxo_leaf_nodes[i as usize]);
        assert_eq!(mmr_node[1], utxo_leaf_nodes[(i + 1) as usize]);
        assert_eq!(mmr_node[2], utxo_leaf_nodes[(i + 2) as usize]);
        let mmr_node = db.fetch_mmr_nodes(MmrTree::RangeProof, i, 3, None).unwrap();
        assert_eq!(mmr_node.len(), 3);
        assert_eq!(mmr_node[0], rp_leaf_nodes[i as usize]);
        assert_eq!(mmr_node[1], rp_leaf_nodes[(i + 1) as usize]);
        assert_eq!(mmr_node[2], rp_leaf_nodes[(i + 2) as usize]);
    }

    assert!(db.fetch_mmr_node(MmrTree::Utxo, 7, None).is_err());
    assert!(db.fetch_mmr_nodes(MmrTree::Utxo, 5, 4, None).is_err());
    assert!(db.fetch_mmr_node(MmrTree::RangeProof, 7, None).is_err());
    assert!(db.fetch_mmr_nodes(MmrTree::RangeProof, 5, 4, None).is_err());
}

#[test]
fn memory_fetch_utxo_rp_mmr_nodes_and_count() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_utxo_rp_mmr_nodes_and_count(db);
}

#[test]
fn lmdb_fetch_utxo_rp_nodes_and_count() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_utxo_rp_mmr_nodes_and_count(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn fetch_kernel_mmr_nodes_and_count<T: BlockchainBackend>(mut db: T) {
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 1);
    let kernel4 = create_test_kernel(400.into(), 2);
    let kernel5 = create_test_kernel(500.into(), 2);
    let kernel6 = create_test_kernel(600.into(), 3);
    let leaf_nodes = vec![
        (kernel1.hash(), false),
        (kernel2.hash(), false),
        (kernel3.hash(), false),
        (kernel4.hash(), false),
        (kernel5.hash(), false),
        (kernel6.hash(), false),
    ];

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1);
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel2);
    txn.insert_kernel(kernel3);
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel4);
    txn.insert_kernel(kernel5);
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    assert!(db.write(txn).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel6);
    txn.operations
        .push(WriteOperation::CreateMmrCheckpoint(MmrTree::Kernel));
    assert!(db.write(txn).is_ok());

    for i in 0..=3 {
        let mmr_node = db.fetch_mmr_node(MmrTree::Kernel, i, None).unwrap();
        assert_eq!(mmr_node, leaf_nodes[i as usize]);

        let mmr_node = db.fetch_mmr_nodes(MmrTree::Kernel, i, 3, None).unwrap();
        assert_eq!(mmr_node.len(), 3);
        assert_eq!(mmr_node[0], leaf_nodes[i as usize]);
        assert_eq!(mmr_node[1], leaf_nodes[(i + 1) as usize]);
        assert_eq!(mmr_node[2], leaf_nodes[(i + 2) as usize]);
    }

    assert!(db.fetch_mmr_node(MmrTree::Kernel, 7, None).is_err());
    assert!(db.fetch_mmr_nodes(MmrTree::Kernel, 5, 4, None).is_err());
}

#[test]
fn memory_fetch_kernel_mmr_nodes_and_count() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_kernel_mmr_nodes_and_count(db);
}

#[test]
fn lmdb_fetch_kernel_nodes_and_count() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        fetch_kernel_mmr_nodes_and_count(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

fn insert_mmr_node_for_utxo_and_rp<T: BlockchainBackend>(mut db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let (utxo4, _) = create_utxo(MicroTari(25_000), &factories, None);
    let utxo_hash1 = utxo1.hash();
    let utxo_hash2 = utxo2.hash();
    let utxo_hash3 = utxo3.hash();
    let utxo_hash4 = utxo4.hash();
    let rp_hash1 = utxo1.proof.hash();
    let rp_hash2 = utxo2.proof.hash();
    let rp_hash3 = utxo3.proof.hash();
    let rp_hash4 = utxo4.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone());
    assert!(db.write(txn).is_ok());
    assert!(db.insert_mmr_node(MmrTree::Utxo, utxo_hash2.clone(), true).is_ok());
    assert!(db.insert_mmr_node(MmrTree::RangeProof, rp_hash2.clone(), false).is_ok());
    assert!(db.insert_mmr_node(MmrTree::Utxo, utxo_hash3.clone(), false).is_ok());
    assert!(db.insert_mmr_node(MmrTree::RangeProof, rp_hash3.clone(), false).is_ok());
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo4.clone());
    assert!(db.write(txn).is_ok());

    let mut utxo_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert!(utxo_mmr_check.push(&utxo_hash1).is_ok());
    assert!(utxo_mmr_check.push(&utxo_hash2).is_ok());
    assert!(utxo_mmr_check.push(&utxo_hash3).is_ok());
    assert!(utxo_mmr_check.push(&utxo_hash4).is_ok());
    let leaf_index = utxo_mmr_check.find_leaf_index(&utxo_hash2).unwrap().unwrap();
    assert!(utxo_mmr_check.delete(leaf_index));
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Utxo).unwrap().to_hex(),
        utxo_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Utxo).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::Utxo, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::Utxo, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::Utxo, 2).unwrap();
    let proof4 = db.fetch_mmr_proof(MmrTree::Utxo, 3).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash3, 2).is_ok());
    assert!(proof4.verify_leaf::<HashDigest>(&mmr_only_root, &utxo_hash4, 3).is_ok());

    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new(), Bitmap::create());
    assert_eq!(rp_mmr_check.push(&rp_hash1), Ok(1));
    assert_eq!(rp_mmr_check.push(&rp_hash2), Ok(2));
    assert_eq!(rp_mmr_check.push(&rp_hash3), Ok(3));
    assert_eq!(rp_mmr_check.push(&rp_hash4), Ok(4));
    assert_eq!(
        db.fetch_mmr_root(MmrTree::RangeProof).unwrap().to_hex(),
        rp_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::RangeProof).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::RangeProof, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::RangeProof, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::RangeProof, 2).unwrap();
    let proof4 = db.fetch_mmr_proof(MmrTree::RangeProof, 3).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash3, 2).is_ok());
    assert!(proof4.verify_leaf::<HashDigest>(&mmr_only_root, &rp_hash4, 3).is_ok());
}

#[test]
fn memory_insert_mmr_node_for_utxo_and_rp() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_mmr_node_for_utxo_and_rp(db);
}

#[test]
fn lmdb_insert_mmr_node_for_utxo_and_rp() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), MmrCacheConfig::default()).unwrap();
        insert_mmr_node_for_utxo_and_rp(db);
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).unwrap();
    }
}

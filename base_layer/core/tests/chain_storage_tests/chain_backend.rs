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
    },
    consensus::{ConsensusConstants, Network},
    helpers::create_orphan_block,
    transactions::{
        helpers::{create_test_kernel, create_utxo},
        tari_amount::MicroTari,
        types::{CryptoFactories, HashDigest},
    },
    tx,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_mmr::{MmrCacheConfig, MutableMmr};
use tari_test_utils::paths::create_temporary_data_path;

fn insert_contains_delete_and_fetch_header<T: BlockchainBackend>(db: T) {
    let mut header = BlockHeader::new(0);
    header.height = 42;
    let hash = header.hash();
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHash(hash.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHash(hash.clone())), Ok(true));
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
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHash(hash)), Ok(false));
}

#[test]
fn memory_insert_contains_delete_and_fetch_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_header(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_header() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    insert_contains_delete_and_fetch_header(db);
}

fn insert_contains_delete_and_fetch_utxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories, None);
    let hash = utxo.hash();
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash.clone())), Ok(true));
    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::UnspentOutput(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash)), Ok(false));
}

#[test]
fn memory_insert_contains_delete_and_fetch_utxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_utxo(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_utxo() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    insert_contains_delete_and_fetch_utxo(db);
}

fn insert_contains_delete_and_fetch_kernel<T: BlockchainBackend>(db: T) {
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash.clone())), Ok(true));
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
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash)), Ok(false));
}

#[test]
fn memory_insert_contains_delete_and_fetch_kernel() {
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_kernel(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_kernel() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    insert_contains_delete_and_fetch_kernel(db);
}

fn insert_contains_delete_and_fetch_orphan<T: BlockchainBackend>(db: T, consensus_constants: &ConsensusConstants) {
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, consensus_constants);
    let hash = orphan.hash();
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(db.write(txn).is_ok());

    assert_eq!(db.contains(&DbKey::OrphanBlock(hash.clone())), Ok(true));
    if let Some(DbValue::OrphanBlock(retrieved_orphan)) = db.fetch(&DbKey::OrphanBlock(hash.clone())).unwrap() {
        assert_eq!(*retrieved_orphan, orphan);
    } else {
        assert!(false);
    }

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::OrphanBlock(hash.clone()));
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash)), Ok(false));
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
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    insert_contains_delete_and_fetch_orphan(db, &consensus_constants);
}

fn spend_utxo_and_unspend_stxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    assert!(db.write(txn).is_ok());

    let mut txn = DbTransaction::new();
    txn.spend_utxo(hash1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.spend_utxo(hash2.clone());
    txn.unspend_stxo(hash1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2.clone())), Ok(true));

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
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash1)), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(hash2)), Ok(false));
}

#[test]
fn memory_spend_utxo_and_unspend_stxo() {
    let db = MemoryDatabase::<HashDigest>::default();
    spend_utxo_and_unspend_stxo(db);
}

#[test]
fn lmdb_spend_utxo_and_unspend_stxo() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    spend_utxo_and_unspend_stxo(db);
}

fn insert_fetch_metadata<T: BlockchainBackend>(db: T) {
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
        MetadataValue::AccumulatedWork(accumulated_work),
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
        assert_eq!(retrieved_accumulated_work, accumulated_work);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    insert_fetch_metadata(db);
}

fn fetch_mmr_root_and_proof_for_utxo_and_rp<T: BlockchainBackend>(db: T) {
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
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    txn.insert_utxo(utxo3.clone(), true);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    fetch_mmr_root_and_proof_for_utxo_and_rp(db);
}

fn fetch_mmr_root_and_proof_for_kernel<T: BlockchainBackend>(db: T) {
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
    txn.insert_kernel(kernel1, true);
    txn.insert_kernel(kernel2, true);
    txn.insert_kernel(kernel3, true);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    fetch_mmr_root_and_proof_for_kernel(db);
}

fn fetch_future_mmr_root_for_utxo_and_rp<T: BlockchainBackend>(db: T) {
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
    txn.insert_utxo(utxo1, true);
    txn.insert_utxo(utxo2, true);
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
    txn.insert_utxo(utxo3, true);
    txn.insert_utxo(utxo4, true);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    fetch_future_mmr_root_for_utxo_and_rp(db);
}

fn fetch_future_mmr_root_for_for_kernel<T: BlockchainBackend>(db: T) {
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let kernel4 = create_test_kernel(400.into(), 3);
    let hash3 = kernel3.hash();
    let hash4 = kernel4.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1, true);
    txn.insert_kernel(kernel2, true);
    assert!(db.write(txn).is_ok());

    let future_root = db
        .calculate_mmr_root(MmrTree::Kernel, vec![hash3, hash4], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(future_root, db.fetch_mmr_root(MmrTree::Kernel).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel3, true);
    txn.insert_kernel(kernel4, true);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    fetch_future_mmr_root_for_for_kernel(db);
}

fn commit_block_and_create_fetch_checkpoint_and_rewind_mmr<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let kernel1 = create_test_kernel(100.into(), 0);
    let header1 = BlockHeader::new(0);
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_kernel(kernel1, true);
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
    txn.insert_utxo(utxo2, true);
    txn.spend_utxo(utxo_hash1.clone());
    txn.insert_kernel(kernel2, true);
    txn.insert_header(header2);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint(MmrTree::Utxo, 0).unwrap();
    let kernel_cp0 = db.fetch_checkpoint(MmrTree::Kernel, 0).unwrap();
    let range_proof_cp0 = db.fetch_checkpoint(MmrTree::RangeProof, 0).unwrap();
    let utxo_cp1 = db.fetch_checkpoint(MmrTree::Utxo, 1).unwrap();
    let kernel_cp1 = db.fetch_checkpoint(MmrTree::Kernel, 1).unwrap();
    let range_proof_cp1 = db.fetch_checkpoint(MmrTree::RangeProof, 1).unwrap();
    assert_eq!(utxo_cp0.nodes_added()[0], utxo_hash1);
    assert_eq!(utxo_cp0.nodes_deleted().to_vec().len(), 0);
    assert_eq!(kernel_cp0.nodes_added()[0], kernel_hash1);
    assert_eq!(range_proof_cp0.nodes_added()[0], rp_hash1);
    assert_eq!(utxo_cp1.nodes_added()[0], utxo_hash2);
    assert_eq!(utxo_cp1.nodes_deleted().to_vec()[0], 0);
    assert_eq!(kernel_cp1.nodes_added()[0], kernel_hash2);
    assert_eq!(range_proof_cp1.nodes_added()[0], rp_hash2);
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHeader(0)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHeader(1)), Ok(true));

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::BlockHeader(1));
    txn.delete(DbKey::TransactionKernel(kernel_hash2.clone()));
    txn.delete(DbKey::UnspentOutput(utxo_hash2.clone()));
    txn.unspend_stxo(utxo_hash1.clone());
    txn.rewind_kernel_mmr(1);
    txn.rewind_utxo_mmr(1);
    txn.rewind_rp_mmr(1);
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint(MmrTree::Utxo, 0).unwrap();
    let kernel_cp0 = db.fetch_checkpoint(MmrTree::Kernel, 0).unwrap();
    let range_proof_cp0 = db.fetch_checkpoint(MmrTree::RangeProof, 0).unwrap();
    assert_eq!(utxo_cp0.nodes_added()[0], utxo_hash1);
    assert_eq!(utxo_cp0.nodes_deleted().to_vec().len(), 0);
    assert_eq!(kernel_cp0.nodes_added()[0], kernel_hash1);
    assert_eq!(range_proof_cp0.nodes_added()[0], rp_hash1);
    assert!(db.fetch_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::RangeProof, 1).is_err());

    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1)), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2)), Ok(false));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash1)), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2)), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHeader(0)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHeader(1)), Ok(false));
}

#[test]
fn memory_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    let db = MemoryDatabase::<HashDigest>::default();
    commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
}

#[test]
fn lmdb_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
}

// TODO: Test Needed: fetch_mmr_node

fn for_each_orphan<T: BlockchainBackend>(db: T, consensus_constants: &ConsensusConstants) {
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
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::OrphanBlock(hash3.clone())), Ok(true));

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
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    for_each_orphan(db, &consensus_constants);
}

fn for_each_kernel<T: BlockchainBackend>(db: T) {
    let kernel1 = create_test_kernel(100.into(), 0);
    let kernel2 = create_test_kernel(200.into(), 1);
    let kernel3 = create_test_kernel(300.into(), 2);
    let hash1 = kernel1.hash();
    let hash2 = kernel2.hash();
    let hash3 = kernel3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel1.clone(), false);
    txn.insert_kernel(kernel2.clone(), false);
    txn.insert_kernel(kernel3.clone(), false);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(hash3.clone())), Ok(true));

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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    for_each_kernel(db);
}

fn for_each_header<T: BlockchainBackend>(db: T) {
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
    assert_eq!(db.contains(&DbKey::BlockHeader(key1)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHeader(key2)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHeader(key3)), Ok(true));

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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    for_each_header(db);
}

fn for_each_utxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories, None);
    let hash1 = utxo1.hash();
    let hash2 = utxo2.hash();
    let hash3 = utxo3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1.clone(), true);
    txn.insert_utxo(utxo2.clone(), true);
    txn.insert_utxo(utxo3.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash2.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash3.clone())), Ok(true));

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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    for_each_utxo(db);
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
        let db = create_lmdb_database(&path, MmrCacheConfig::default()).unwrap();
        let mut txn = DbTransaction::new();
        txn.insert_orphan(orphan.clone());
        txn.insert_utxo(utxo1, true);
        txn.insert_utxo(utxo2, true);
        txn.insert_kernel(kernel, true);
        txn.insert_header(header.clone());
        txn.commit_block();
        assert!(db.write(txn).is_ok());
        let mut txn = DbTransaction::new();
        txn.spend_utxo(stxo_hash.clone());
        assert!(db.write(txn).is_ok());

        assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(true));
        assert_eq!(db.contains(&DbKey::BlockHash(header_hash.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash.clone())), Ok(true));
        assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash.clone())), Ok(true));
    }
    // Restore backend storage
    let db = create_lmdb_database(&path, MmrCacheConfig::default()).unwrap();
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(true));
    assert_eq!(db.contains(&DbKey::BlockHash(header_hash)), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash)), Ok(true));
    assert_eq!(db.contains(&DbKey::SpentOutput(stxo_hash)), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash)), Ok(true));
    assert_eq!(db.contains(&DbKey::OrphanBlock(orphan_hash)), Ok(true));
}

#[test]
fn lmdb_mmr_reset_and_commit() {
    let factories = CryptoFactories::default();
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();

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
    txn.insert_utxo(utxo1, true);
    txn.insert_kernel(kernel1, true);
    txn.insert_header(header1);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    // Reset mmrs as a mmr txn failed without applying storage txns.
    let mut txn = DbTransaction::new();
    txn.spend_utxo(utxo_hash2.clone());
    txn.commit_block();
    assert!(db.write(txn).is_err());

    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHash(header_hash1.clone())), Ok(true));
    assert_eq!(
        db.fetch_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert!(db.fetch_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::RangeProof, 1).is_err());

    // Reset mmrs as a storage txn failed after the mmr txns were applied, ensure the previous state was preserved.
    let mut txn = DbTransaction::new();
    txn.spend_utxo(utxo_hash1.clone());
    txn.delete(DbKey::TransactionKernel(kernel_hash1.clone()));
    txn.delete(DbKey::TransactionKernel(kernel_hash2.clone()));
    txn.commit_block();
    assert!(db.write(txn).is_err());

    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::UnspentOutput(utxo_hash2.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash1.clone())), Ok(false));
    assert_eq!(db.contains(&DbKey::SpentOutput(utxo_hash2)), Ok(false));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash1.clone())), Ok(true));
    assert_eq!(db.contains(&DbKey::TransactionKernel(kernel_hash2)), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHash(header_hash1.clone())), Ok(true));
    assert_eq!(
        db.fetch_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert!(db.fetch_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_checkpoint(MmrTree::RangeProof, 1).is_err());
}

fn fetch_checkpoint<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let kernel1 = create_test_kernel(100.into(), 0);
    let mut header1 = BlockHeader::new(0);
    header1.height = 0;
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_kernel(kernel1, true);
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
    txn.insert_utxo(utxo2, true);
    txn.insert_kernel(kernel2, true);
    txn.insert_header(header2.clone());
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint(MmrTree::Utxo, 1);
    let kernel_cp0 = db.fetch_checkpoint(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint(MmrTree::Kernel, 1);
    let rp_cp0 = db.fetch_checkpoint(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint(MmrTree::RangeProof, 1);
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

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo3, true);
    txn.insert_kernel(kernel3, true);
    txn.insert_header(header3);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_checkpoint(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_checkpoint(MmrTree::Utxo, 1);
    let utxo_cp2 = db.fetch_checkpoint(MmrTree::Utxo, 2);
    let kernel_cp0 = db.fetch_checkpoint(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_checkpoint(MmrTree::Kernel, 1);
    let kernel_cp2 = db.fetch_checkpoint(MmrTree::Kernel, 2);
    let rp_cp0 = db.fetch_checkpoint(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_checkpoint(MmrTree::RangeProof, 1);
    let rp_cp2 = db.fetch_checkpoint(MmrTree::RangeProof, 2);
    assert!(utxo_cp0.unwrap().nodes_added().contains(&utxo_hash1));
    assert!(utxo_cp1.unwrap().nodes_added().contains(&utxo_hash2));
    assert!(utxo_cp2.unwrap().nodes_added().contains(&utxo_hash3));
    assert!(kernel_cp0.unwrap().nodes_added().contains(&kernel_hash1));
    assert!(kernel_cp1.unwrap().nodes_added().contains(&kernel_hash2));
    assert!(kernel_cp2.unwrap().nodes_added().contains(&kernel_hash3));
    assert!(rp_cp0.unwrap().nodes_added().contains(&rp_hash1));
    assert!(rp_cp1.unwrap().nodes_added().contains(&rp_hash2));
    assert!(rp_cp2.unwrap().nodes_added().contains(&rp_hash3));
}

#[test]
fn memory_fetch_checkpoint() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
    let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
    fetch_checkpoint(db);
}

#[test]
fn lmdb_fetch_checkpoint() {
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 1 };
    let db = create_lmdb_database(&create_temporary_data_path(), mmr_cache_config).unwrap();
    fetch_checkpoint(db);
}

fn duplicate_utxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories, None);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories, None);
    let hash1 = utxo1.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo_with_hash(hash1.clone(), utxo1.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.contains(&DbKey::UnspentOutput(hash1.clone())), Ok(true));
    if let Some(DbValue::UnspentOutput(retrieved_utxo)) = db.fetch(&DbKey::UnspentOutput(hash1.clone())).unwrap() {
        assert_eq!(*retrieved_utxo, utxo1);
    } else {
        assert!(false);
    }
    let mut txn = DbTransaction::new();
    txn.insert_utxo_with_hash(hash1.clone(), utxo2.clone(), true);
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
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    duplicate_utxo(db);
}

fn fetch_last_header<T: BlockchainBackend>(db: T) {
    let mut header0 = BlockHeader::new(0);
    header0.height = 0;
    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    assert_eq!(db.fetch_last_header(), Ok(None));

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header(), Ok(Some(header1)));

    let mut txn = DbTransaction::new();
    txn.insert_header(header2.clone());
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header(), Ok(Some(header2)));
}

#[test]
fn memory_fetch_last_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_last_header(db);
}

#[test]
fn lmdb_fetch_last_header() {
    let db = create_lmdb_database(&create_temporary_data_path(), MmrCacheConfig::default()).unwrap();
    fetch_last_header(db);
}

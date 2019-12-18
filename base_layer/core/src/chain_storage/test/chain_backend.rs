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
    blocks::BlockHeader,
    chain_storage::{
        blockchain_database::BlockchainBackend,
        db_transaction::{DbKey, DbKeyValuePair, DbValue, MetadataKey, MetadataValue},
        lmdb_db::create_lmdb_database,
        DbTransaction,
        MemoryDatabase,
        MmrTree,
    },
    test_utils::builders::{create_test_block, create_test_kernel, create_utxo},
    tx,
};
use tari_mmr::{Hash, MerkleChangeTrackerConfig, MutableMmr};
use tari_test_utils::paths::create_temporary_data_path;
use tari_transactions::{
    tari_amount::MicroTari,
    types::{CryptoFactories, HashDigest},
};
use tari_utilities::{hex::Hex, Hashable};

fn insert_contains_delete_and_fetch_header<T: BlockchainBackend>(db: T) {
    let mut header = BlockHeader::new(0);
    header.height = 42;
    let hash = header.hash();
    assert_eq!(db.contains(&DbKey::BlockHeader(header.height)), Ok(false));
    assert_eq!(db.contains(&DbKey::BlockHash(hash.clone())), Ok(false));

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone(), true);
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    insert_contains_delete_and_fetch_header(db);
}

fn insert_contains_delete_and_fetch_utxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    insert_contains_delete_and_fetch_kernel(db);
}

fn insert_contains_delete_and_fetch_orphan<T: BlockchainBackend>(db: T) {
    let txs = vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_test_block(10, None, txs);
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
    let db = MemoryDatabase::<HashDigest>::default();
    insert_contains_delete_and_fetch_orphan(db);
}

#[test]
fn lmdb_insert_contains_delete_and_fetch_orphan() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    insert_contains_delete_and_fetch_orphan(db);
}

fn spend_utxo_and_unspend_stxo<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
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

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories);
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

    let mut utxo_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
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

    let mut rp_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
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

    let mut kernel_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_mmr_root_and_proof_for_kernel(db);
}

fn fetch_mmr_root_and_proof_for_header<T: BlockchainBackend>(db: T) {
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Header).unwrap().to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );

    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    let mut header3 = BlockHeader::new(0);
    header3.height = 3;
    let hash1 = header1.hash();
    let hash2 = header2.hash();
    let hash3 = header3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_header(header1, true);
    txn.insert_header(header2, true);
    txn.insert_header(header3, true);
    assert!(db.write(txn).is_ok());

    let mut header_mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(header_mmr_check.push(&hash1).is_ok());
    assert!(header_mmr_check.push(&hash2).is_ok());
    assert!(header_mmr_check.push(&hash3).is_ok());
    assert_eq!(
        db.fetch_mmr_root(MmrTree::Header).unwrap().to_hex(),
        header_mmr_check.get_merkle_root().unwrap().to_hex()
    );

    let mmr_only_root = db.fetch_mmr_only_root(MmrTree::Header).unwrap();
    let proof1 = db.fetch_mmr_proof(MmrTree::Header, 0).unwrap();
    let proof2 = db.fetch_mmr_proof(MmrTree::Header, 1).unwrap();
    let proof3 = db.fetch_mmr_proof(MmrTree::Header, 2).unwrap();
    assert!(proof1.verify_leaf::<HashDigest>(&mmr_only_root, &hash1, 0).is_ok());
    assert!(proof2.verify_leaf::<HashDigest>(&mmr_only_root, &hash2, 1).is_ok());
    assert!(proof3.verify_leaf::<HashDigest>(&mmr_only_root, &hash3, 2).is_ok());
}

#[test]
fn memory_fetch_mmr_root_and_proof_for_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_mmr_root_and_proof_for_header(db);
}

#[test]
fn lmdb_fetch_mmr_root_and_proof_for_header() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_mmr_root_and_proof_for_header(db);
}

fn fetch_future_mmr_root_for_utxo_and_rp<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories);
    let (utxo4, _) = create_utxo(MicroTari(24_000), &factories);
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_future_mmr_root_for_for_kernel(db);
}

fn fetch_future_mmr_root_for_header<T: BlockchainBackend>(db: T) {
    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    let mut header3 = BlockHeader::new(0);
    header3.height = 3;
    let mut header4 = BlockHeader::new(0);
    header4.height = 4;
    let hash3 = header3.hash();
    let hash4 = header4.hash();

    let mut txn = DbTransaction::new();
    txn.insert_header(header1, true);
    txn.insert_header(header2, true);
    assert!(db.write(txn).is_ok());

    let future_root = db
        .calculate_mmr_root(MmrTree::Header, vec![hash3, hash4], Vec::new())
        .unwrap()
        .to_hex();
    assert_ne!(future_root, db.fetch_mmr_root(MmrTree::Header).unwrap().to_hex());

    let mut txn = DbTransaction::new();
    txn.insert_header(header3, true);
    txn.insert_header(header4, true);
    assert!(db.write(txn).is_ok());

    assert_eq!(future_root, db.fetch_mmr_root(MmrTree::Header).unwrap().to_hex());
}

#[test]
fn memory_fetch_future_mmr_root_for_header() {
    let db = MemoryDatabase::<HashDigest>::default();
    fetch_future_mmr_root_for_header(db);
}

#[test]
fn lmdb_fetch_future_mmr_root_for_header() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_future_mmr_root_for_header(db);
}

fn commit_block_and_create_fetch_checkpoint_and_rewind_mmr<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let kernel1 = create_test_kernel(100.into(), 0);
    let mut header1 = BlockHeader::new(0);
    header1.height = 1;
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();
    let header_hash1 = header1.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_kernel(kernel1, true);
    txn.insert_header(header1, true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let kernel2 = create_test_kernel(200.into(), 0);
    let mut header2 = BlockHeader::new(0);
    header2.height = 2;
    let utxo_hash2 = utxo2.hash();
    let kernel_hash2 = kernel2.hash();
    let rp_hash2 = utxo2.proof.hash();
    let header_hash2 = header2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2, true);
    txn.insert_kernel(kernel2, true);
    txn.insert_header(header2, true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Header, 0).unwrap().nodes_added()[0],
        header_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Utxo, 1).unwrap().nodes_added()[0],
        utxo_hash2
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Kernel, 1).unwrap().nodes_added()[0],
        kernel_hash2
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1).unwrap().nodes_added()[0],
        rp_hash2
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Header, 1).unwrap().nodes_added()[0],
        header_hash2
    );

    let mut txn = DbTransaction::new();
    txn.delete(DbKey::BlockHeader(2)); // Remove header to ensure height is calculated correctly
    txn.rewind_header_mmr(1);
    txn.rewind_kernel_mmr(1);
    txn.rewind_utxo_mmr(1);
    txn.rewind_rp_mmr(1);
    assert!(db.write(txn).is_ok());

    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Header, 0).unwrap().nodes_added()[0],
        header_hash1
    );
    assert!(db.fetch_mmr_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Header, 1).is_err());
}

#[test]
fn memory_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    let db = MemoryDatabase::<HashDigest>::default();
    commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
}

#[test]
fn lmdb_commit_block_and_create_fetch_checkpoint_and_rewind_mmr() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    commit_block_and_create_fetch_checkpoint_and_rewind_mmr(db);
}

// TODO: Test Needed: fetch_mmr_node

fn for_each_orphan<T: BlockchainBackend>(db: T) {
    let orphan1 = create_test_block(5, None, vec![
        (tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0,
    ]);
    let orphan2 = create_test_block(10, None, vec![
        (tx!(2000.into(), fee: 30.into(), inputs: 1, outputs: 1)).0,
    ]);
    let orphan3 = create_test_block(15, None, vec![
        (tx!(3000.into(), fee: 40.into(), inputs: 1, outputs: 2)).0,
    ]);
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
    let db = MemoryDatabase::<HashDigest>::default();
    for_each_orphan(db);
}

#[test]
fn lmdb_for_each_orphan() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    for_each_orphan(db);
}

#[test]
fn lmdb_backend_restore() {
    let factories = CryptoFactories::default();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };

    let txs = vec![(tx!(1000.into(), fee: 20.into(), inputs: 2, outputs: 1)).0];
    let orphan = create_test_block(10, None, txs);
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
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
        let db = create_lmdb_database(&path, mct_config).unwrap();
        let mut txn = DbTransaction::new();
        txn.insert_orphan(orphan.clone());
        txn.insert_utxo(utxo1, true);
        txn.insert_utxo(utxo2, true);
        txn.insert_kernel(kernel, true);
        txn.insert_header(header.clone(), true);
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
    let db = create_lmdb_database(&path, mct_config).unwrap();
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();

    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
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
    txn.insert_header(header1, true);
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
        db.fetch_mmr_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Header, 0).unwrap().nodes_added()[0],
        header_hash1
    );
    assert!(db.fetch_mmr_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Header, 1).is_err());

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
        db.fetch_mmr_checkpoint(MmrTree::Utxo, 0).unwrap().nodes_added()[0],
        utxo_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Kernel, 0).unwrap().nodes_added()[0],
        kernel_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0).unwrap().nodes_added()[0],
        rp_hash1
    );
    assert_eq!(
        db.fetch_mmr_checkpoint(MmrTree::Header, 0).unwrap().nodes_added()[0],
        header_hash1
    );
    assert!(db.fetch_mmr_checkpoint(MmrTree::Utxo, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Kernel, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1).is_err());
    assert!(db.fetch_mmr_checkpoint(MmrTree::Header, 1).is_err());
}

fn fetch_mmr_base_leaf_nodes_and_restore<T: BlockchainBackend>(db: T) {
    struct R {
        utxo: Hash,
        kernel: Hash,
        rp: Hash,
        header: Hash,
    }
    let factories = CryptoFactories::default();
    let mut test_hashes = Vec::<R>::new();
    for height in 0..10 {
        let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
        let kernel = create_test_kernel(100.into(), 0);
        let mut header = BlockHeader::new(0);
        header.height = height;
        let mut txn = DbTransaction::new();
        txn.insert_utxo(utxo.clone(), true);
        txn.insert_kernel(kernel.clone(), true);
        txn.insert_header(header.clone(), true);
        txn.commit_block();
        assert!(db.write(txn).is_ok());

        test_hashes.push(R {
            utxo: utxo.hash(),
            kernel: kernel.hash(),
            rp: utxo.proof.hash(),
            header: header.hash(),
        });
    }

    let utxo_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100).unwrap();
    let kernel_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100).unwrap();
    let rp_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100).unwrap();
    let header_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100).unwrap();
    assert_eq!(utxo_mmr_state.total_leaf_count, 4);
    assert_eq!(kernel_mmr_state.total_leaf_count, 4);
    assert_eq!(rp_mmr_state.total_leaf_count, 4);
    assert_eq!(header_mmr_state.total_leaf_count, 4);

    assert_eq!(utxo_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(kernel_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(rp_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(header_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    for i in 0..4 {
        assert_eq!(utxo_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].utxo);
        assert_eq!(kernel_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].kernel);
        assert_eq!(rp_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].rp);
        assert_eq!(header_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].header);
    }

    // Adding one more set of data will make the base MMR grow from 4 to 8
    let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
    let kernel = create_test_kernel(100.into(), 0);
    let mut header = BlockHeader::new(0);
    header.height = 10;
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), true);
    txn.insert_kernel(kernel.clone(), true);
    txn.insert_header(header.clone(), true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_mmr_leaf_count = db
        .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100)
        .unwrap()
        .total_leaf_count;
    let kernel_mmr_leaf_count = db
        .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100)
        .unwrap()
        .total_leaf_count;
    let rp_mmr_leaf_count = db
        .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100)
        .unwrap()
        .total_leaf_count;
    let header_mmr_leaf_count = db
        .fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100)
        .unwrap()
        .total_leaf_count;
    assert_eq!(utxo_mmr_leaf_count, 8);
    assert_eq!(kernel_mmr_leaf_count, 8);
    assert_eq!(rp_mmr_leaf_count, 8);
    assert_eq!(header_mmr_leaf_count, 8);

    // Restore previously retrieved MMR state
    assert!(db.restore_mmr(MmrTree::Utxo, utxo_mmr_state.leaf_nodes).is_ok());
    assert!(db.restore_mmr(MmrTree::Kernel, kernel_mmr_state.leaf_nodes).is_ok());
    assert!(db.restore_mmr(MmrTree::RangeProof, rp_mmr_state.leaf_nodes).is_ok());
    assert!(db.restore_mmr(MmrTree::Header, header_mmr_state.leaf_nodes).is_ok());

    let utxo_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 100).unwrap();
    let kernel_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 100).unwrap();
    let rp_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 100).unwrap();
    let header_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100).unwrap();
    assert_eq!(utxo_mmr_state.total_leaf_count, 4);
    assert_eq!(kernel_mmr_state.total_leaf_count, 4);
    assert_eq!(rp_mmr_state.total_leaf_count, 4);
    assert_eq!(header_mmr_state.total_leaf_count, 4);

    assert_eq!(utxo_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(kernel_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(rp_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    assert_eq!(header_mmr_state.leaf_nodes.leaf_hashes.len(), 4);
    for i in 0..4 {
        assert_eq!(utxo_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].utxo);
        assert_eq!(kernel_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].kernel);
        assert_eq!(rp_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].rp);
        assert_eq!(header_mmr_state.leaf_nodes.leaf_hashes[i], test_hashes[i].header);
    }
}

#[test]
fn memory_fetch_mmr_base_leaf_nodes_and_restore() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 3,
        max_history_len: 6,
    };
    let db = MemoryDatabase::<HashDigest>::new(mct_config);
    fetch_mmr_base_leaf_nodes_and_restore(db);
}

#[test]
fn lmdb_fetch_mmr_base_leaf_nodes_and_restore() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 3,
        max_history_len: 6,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_mmr_base_leaf_nodes_and_restore(db);
}

fn fetch_checkpoint_with_pruning_horizon<T: BlockchainBackend>(db: T) {
    let factories = CryptoFactories::default();
    let (utxo1, _) = create_utxo(MicroTari(10_000), &factories);
    let kernel1 = create_test_kernel(100.into(), 0);
    let mut header1 = BlockHeader::new(0);
    header1.height = 0;
    let utxo_hash1 = utxo1.hash();
    let kernel_hash1 = kernel1.hash();
    let rp_hash1 = utxo1.proof.hash();
    let header_hash1 = header1.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo1, true);
    txn.insert_kernel(kernel1, true);
    txn.insert_header(header1.clone(), true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let (utxo2, _) = create_utxo(MicroTari(15_000), &factories);
    let kernel2 = create_test_kernel(200.into(), 0);
    let header2 = BlockHeader::from_previous(&header1);
    let utxo_hash2 = utxo2.hash();
    let kernel_hash2 = kernel2.hash();
    let rp_hash2 = utxo2.proof.hash();
    let header_hash2 = header2.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo2, true);
    txn.insert_kernel(kernel2, true);
    txn.insert_header(header2.clone(), true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_mmr_checkpoint(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_mmr_checkpoint(MmrTree::Utxo, 1);
    let kernel_cp0 = db.fetch_mmr_checkpoint(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_mmr_checkpoint(MmrTree::Kernel, 1);
    let rp_cp0 = db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1);
    let header_cp0 = db.fetch_mmr_checkpoint(MmrTree::Header, 0);
    let header_cp1 = db.fetch_mmr_checkpoint(MmrTree::Header, 1);
    assert!(utxo_cp0.unwrap().nodes_added().contains(&utxo_hash1));
    assert!(utxo_cp1.unwrap().nodes_added().contains(&utxo_hash2));
    assert!(kernel_cp0.unwrap().nodes_added().contains(&kernel_hash1));
    assert!(kernel_cp1.unwrap().nodes_added().contains(&kernel_hash2));
    assert!(rp_cp0.unwrap().nodes_added().contains(&rp_hash1));
    assert!(rp_cp1.unwrap().nodes_added().contains(&rp_hash2));
    assert!(header_cp0.unwrap().nodes_added().contains(&header_hash1));
    assert!(header_cp1.unwrap().nodes_added().contains(&header_hash2));

    let (utxo3, _) = create_utxo(MicroTari(20_000), &factories);
    let kernel3 = create_test_kernel(300.into(), 0);
    let header3 = BlockHeader::from_previous(&header2);
    let utxo_hash3 = utxo3.hash();
    let kernel_hash3 = kernel3.hash();
    let rp_hash3 = utxo3.proof.hash();
    let header_hash3 = header3.hash();

    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo3, true);
    txn.insert_kernel(kernel3, true);
    txn.insert_header(header3, true);
    txn.commit_block();
    assert!(db.write(txn).is_ok());

    let utxo_cp0 = db.fetch_mmr_checkpoint(MmrTree::Utxo, 0);
    let utxo_cp1 = db.fetch_mmr_checkpoint(MmrTree::Utxo, 1);
    let utxo_cp2 = db.fetch_mmr_checkpoint(MmrTree::Utxo, 2);
    let kernel_cp0 = db.fetch_mmr_checkpoint(MmrTree::Kernel, 0);
    let kernel_cp1 = db.fetch_mmr_checkpoint(MmrTree::Kernel, 1);
    let kernel_cp2 = db.fetch_mmr_checkpoint(MmrTree::Kernel, 2);
    let rp_cp0 = db.fetch_mmr_checkpoint(MmrTree::RangeProof, 0);
    let rp_cp1 = db.fetch_mmr_checkpoint(MmrTree::RangeProof, 1);
    let rp_cp2 = db.fetch_mmr_checkpoint(MmrTree::RangeProof, 2);
    let header_cp0 = db.fetch_mmr_checkpoint(MmrTree::Header, 0);
    let header_cp1 = db.fetch_mmr_checkpoint(MmrTree::Header, 1);
    let header_cp2 = db.fetch_mmr_checkpoint(MmrTree::Header, 2);
    assert!(utxo_cp0.is_err());
    assert!(utxo_cp1.is_err());
    assert!(utxo_cp2.unwrap().nodes_added().contains(&utxo_hash3));
    assert!(kernel_cp0.is_err());
    assert!(kernel_cp1.is_err());
    assert!(kernel_cp2.unwrap().nodes_added().contains(&kernel_hash3));
    assert!(rp_cp0.is_err());
    assert!(rp_cp1.is_err());
    assert!(rp_cp2.unwrap().nodes_added().contains(&rp_hash3));
    assert!(header_cp0.is_err());
    assert!(header_cp1.is_err());
    assert!(header_cp2.unwrap().nodes_added().contains(&header_hash3));
}

#[test]
fn memory_fetch_checkpoint_with_pruning_horizon() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 1,
        max_history_len: 2,
    };
    let db = MemoryDatabase::<HashDigest>::new(mct_config);
    fetch_checkpoint_with_pruning_horizon(db);
}

#[test]
fn lmdb_fetch_checkpoint_with_pruning_horizon() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 1,
        max_history_len: 2,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_checkpoint_with_pruning_horizon(db);
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
    txn.insert_header(header0, true);
    txn.insert_header(header1.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header(), Ok(Some(header1)));

    let mut txn = DbTransaction::new();
    txn.insert_header(header2.clone(), true);
    assert!(db.write(txn).is_ok());
    assert_eq!(db.fetch_last_header(), Ok(Some(header2)));
}

#[test]
fn memory_fetch_last_header() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = MemoryDatabase::<HashDigest>::new(mct_config);
    fetch_last_header(db);
}

#[test]
fn lmdb_fetch_last_header() {
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let db = create_lmdb_database(&create_temporary_data_path(), mct_config).unwrap();
    fetch_last_header(db);
}

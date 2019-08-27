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
    blocks::blockheader::BlockHeader,
    chain_storage::{transaction::DbKey::TransactionKernel, BlockChainDatabase, DbTransaction, MemoryDatabase},
    tari_amount::MicroTari,
    test_utils::builders::{create_test_block, create_test_kernel, create_test_tx, create_utxo},
    types::HashDigest,
};
use std::thread;
use tari_mmr::MutableMmr;
use tari_utilities::{hex::Hex, Hashable};

#[test]
fn fetch_nonexistent_kernel() {
    let store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    assert!(store.fetch_kernel(vec![0u8; 32]).unwrap().is_none());
}

#[test]
fn insert_and_fetch_kernel() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let kernel = create_test_kernel(5.into(), 0);
    let hash = kernel.hash();

    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_kernel(hash).unwrap(), Some(kernel));
}

#[test]
fn fetch_nonexistent_header() {
    let store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    assert!(store.fetch_header(0).unwrap().is_none());
}
#[test]
fn insert_and_fetch_header() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let mut header = BlockHeader::new(0);
    header.height = 42;

    let mut txn = DbTransaction::new();
    txn.insert_header(header.clone());
    assert!(store.commit(txn).is_ok());
    assert!(store.fetch_header(0).unwrap().is_none());
}

#[test]
fn insert_and_fetch_utxo() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let (utxo, _) = create_utxo(MicroTari(10_000));
    let hash = utxo.hash();
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), false);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.is_utxo(hash.clone()).unwrap(), true);
    assert_eq!(store.fetch_utxo(hash).unwrap(), Some(utxo));
}

#[test]
fn insert_and_fetch_orphan() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let txs = vec![
        create_test_tx(1000.into(), 10.into(), 0, 2, 1),
        create_test_tx(2000.into(), 20.into(), 0, 1, 1),
    ];
    let orphan = create_test_block(10, txs);
    let orphan_hash = orphan.hash();
    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone());
    assert!(store.commit(txn).is_ok());
    assert_eq!(store.fetch_orphan(orphan_hash).unwrap(), Some(orphan));
}

#[test]
fn multiple_threads() {
    let store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    // Save a kernel in thread A
    let mut store_a = store.clone();
    let a = thread::spawn(move || {
        let kernel = create_test_kernel(5.into(), 0);
        let hash = kernel.hash();
        let mut txn = DbTransaction::new();
        txn.insert_kernel(kernel.clone());
        assert!(store_a.commit(txn).is_ok());
        hash
    });
    // Save a kernel in thread B
    let mut store_b = store.clone();
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
    let kernel_a = store.fetch_kernel(hash_a).unwrap().unwrap();
    assert_eq!(kernel_a.fee, 5.into());
    let kernel_b = store.fetch_kernel(hash_b).unwrap().unwrap();
    assert_eq!(kernel_b.fee, 10.into());
}

#[test]
fn utxo_and_rp_merkle_root() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let root = store.get_utxo_root().unwrap();
    // This is the zero-length MMR of a mutable MMR with Blake256 as hasher
    assert_eq!(
        &root.to_hex(),
        "26146a5435ef15e8cf7dc3354cb7268137e8be211794e93d04551576c6561565"
    );
    let (utxo1, _) = create_utxo(MicroTari(10_000));
    let (utxo2, _) = create_utxo(MicroTari(10_000));
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
    let root = store.get_utxo_root().unwrap();
    let rp_root = store.get_range_proof_root().unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().to_hex());
    assert_eq!(rp_root.to_hex(), rp_mmr_check.get_merkle_root().to_hex());
}

#[test]
fn header_merkle_root() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let root = store.get_header_root().unwrap();
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
    let root = store.get_header_root().unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().to_hex());
}

#[test]
fn kernel_merkle_root() {
    let mut store = BlockChainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let root = store.get_header_root().unwrap();
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
    let root = store.get_kernel_root().unwrap();
    let mut mmr_check = MutableMmr::<HashDigest, _>::new(Vec::new());
    assert!(mmr_check.push(&hash1).is_ok());
    assert!(mmr_check.push(&hash2).is_ok());
    assert!(mmr_check.push(&hash3).is_ok());
    assert_eq!(root.to_hex(), mmr_check.get_merkle_root().to_hex());
}

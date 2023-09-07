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

use rand::rngs::OsRng;
use tari_common::configuration::Network;
use tari_common_types::types::{
    ComAndPubSignature,
    Commitment,
    FixedHash,
    PrivateKey,
    PublicKey,
    RangeProof,
    Signature,
};
use tari_core::{
    blocks::{BlockAccumulatedData, BlockHeader, BlockHeaderAccumulatedData, ChainHeader, UpdateBlockAccumulatedData},
    chain_storage::{create_lmdb_database, BlockchainBackend, ChainStorageError, DbKey, DbTransaction, DbValue},
    consensus::{ConsensusManager, ConsensusManagerBuilder},
    covenants::Covenant,
    test_helpers::blockchain::create_test_db,
    transactions::transaction_components::{
        EncryptedOpenings,
        KernelFeatures,
        OutputFeatures,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
    tx,
};
use tari_crypto::keys::{PublicKey as PKtrait, SecretKey as SKtrait};
use tari_script::TariScript;
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;

use crate::helpers::database::create_orphan_block;

#[test]
fn test_lmdb_insert_contains_delete_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus = ConsensusManagerBuilder::new(network).build();
    let mut db = create_test_db();
    let txs = vec![
        (tx!(1000.into(), fee: 4.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 6.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus);
    let hash = orphan.hash();
    assert!(!db.contains(&DbKey::OrphanBlock(hash)).unwrap());

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    db.write(txn).unwrap();

    assert!(db.contains(&DbKey::OrphanBlock(hash)).unwrap());
    if let Some(DbValue::OrphanBlock(retrieved_orphan)) = db.fetch(&DbKey::OrphanBlock(hash)).unwrap() {
        assert_eq!(*retrieved_orphan, orphan);
    } else {
        panic!();
    }

    let mut txn = DbTransaction::new();
    txn.delete_orphan(hash);
    assert!(db.write(txn).is_ok());
    assert!(!db.contains(&DbKey::OrphanBlock(hash)).unwrap());
}

#[test]
fn test_kernel_order() {
    let mut db = create_test_db();

    let block_hash = FixedHash::zero();
    let mut kernels = Vec::with_capacity(2000);
    let version = TransactionKernelVersion::V0;
    let features = KernelFeatures::default();
    for _i in 0..2000 {
        let pvt_key = PrivateKey::random(&mut OsRng);
        let pub_key = PublicKey::from_secret_key(&pvt_key);
        let commitment = Commitment::from_public_key(&pub_key);
        let sig = Signature::new(pub_key, pvt_key);
        let kernel = TransactionKernel::new(version, features, 0.into(), 0, commitment, sig, None);
        kernels.push(kernel);
    }
    kernels.sort();

    for (i, kernel) in kernels.iter().enumerate().take(2000) {
        let mut tx = DbTransaction::new();
        tx.insert_kernel(kernel.clone(), block_hash, i as u32);
        db.write(tx).unwrap();
    }

    let read_kernels = db.fetch_kernels_in_block(&block_hash).unwrap();
    assert_eq!(kernels.len(), read_kernels.len());
    for i in 0..2000 {
        assert_eq!(kernels[i], read_kernels[i]);
    }
}

#[test]
fn test_utxo_order() {
    let mut db = create_test_db();

    let block_data = BlockAccumulatedData::default();
    let header = BlockHeader::new(0);
    let block_hash = header.hash();
    let mut utxos = Vec::with_capacity(2000);
    let version = TransactionOutputVersion::V0;
    let features = OutputFeatures::default();
    let script = script!(Nop);
    let proof = RangeProof::default();
    let sig = ComAndPubSignature::default();
    let covenant = Covenant::default();
    let encrypt = EncryptedOpenings::default();
    for _i in 0..2000 {
        let pvt_key = PrivateKey::random(&mut OsRng);
        let pub_key = PublicKey::from_secret_key(&pvt_key);
        let commitment = Commitment::from_public_key(&pub_key);
        let utxo = TransactionOutput::new(
            version,
            features.clone(),
            commitment,
            Some(proof.clone()),
            script.clone(),
            pub_key,
            sig.clone(),
            covenant.clone(),
            encrypt,
            0.into(),
        );
        utxos.push(utxo);
    }
    utxos.sort();

    for (i, utxo) in utxos.iter().enumerate().take(2000) {
        let mut tx = DbTransaction::new();
        tx.insert_utxo(utxo.clone(), block_hash, 0, i as u32, 0);
        db.write(tx).unwrap();
    }

    let mut tx = DbTransaction::new();
    let data = BlockHeaderAccumulatedData {
        hash: header.hash(),
        ..Default::default()
    };
    let chainheader = ChainHeader::try_construct(header, data).unwrap();
    let sum = block_data.kernel_sum().clone();
    let (kernels, utxo_set, deleted) = block_data.dissolve();
    let update_data = UpdateBlockAccumulatedData {
        kernel_hash_set: Some(kernels),
        utxo_hash_set: Some(utxo_set),
        deleted_diff: Some(deleted.into()),
        kernel_sum: Some(sum),
    };
    tx.insert_chain_header(chainheader);
    tx.update_block_accumulated_data(block_hash, update_data);

    db.write(tx).unwrap();

    let read_utxos = db.fetch_utxos_in_block(&block_hash, None).unwrap().0;
    assert_eq!(utxos.len(), read_utxos.len());
    for i in 0..2000 {
        assert_eq!(&utxos[i], read_utxos[i].as_transaction_output().unwrap());
    }
}

#[test]
fn test_lmdb_file_lock() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
        let db = create_lmdb_database(&temp_path, LMDBConfig::default(), consensus_manager.clone()).unwrap();

        match create_lmdb_database(&temp_path, LMDBConfig::default(), consensus_manager.clone()) {
            Err(ChainStorageError::CannotAcquireFileLock) => {},
            _ => panic!("Should not be able to make this db"),
        }

        drop(db);

        let _db2 = create_lmdb_database(&temp_path, LMDBConfig::default(), consensus_manager)
            .expect("Should be able to make a new lmdb now");
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).expect("Could not clear temp storage for db");
    }
}

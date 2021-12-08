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

use tari_common::configuration::Network;
use tari_core::{
    chain_storage::{create_lmdb_database, BlockchainBackend, ChainStorageError, DbKey, DbTransaction, DbValue},
    consensus::ConsensusManagerBuilder,
    test_helpers::blockchain::create_test_db,
    tx,
};
use tari_crypto::tari_utilities::Hashable;
use tari_storage::lmdb_store::LMDBConfig;
use tari_test_utils::paths::create_temporary_data_path;

use crate::helpers::database::create_orphan_block;

#[test]
fn lmdb_insert_contains_delete_and_fetch_orphan() {
    let network = Network::LocalNet;
    let consensus = ConsensusManagerBuilder::new(network).build();
    let mut db = create_test_db();
    let txs = vec![
        (tx!(1000.into(), fee: 4.into(), inputs: 2, outputs: 1)).0,
        (tx!(2000.into(), fee: 6.into(), inputs: 1, outputs: 1)).0,
    ];
    let orphan = create_orphan_block(10, txs, &consensus);
    let hash = orphan.hash();
    assert!(!db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap());

    let mut txn = DbTransaction::new();
    txn.insert_orphan(orphan.clone().into());
    db.write(txn).unwrap();

    assert!(db.contains(&DbKey::OrphanBlock(hash.clone())).unwrap());
    if let Some(DbValue::OrphanBlock(retrieved_orphan)) = db.fetch(&DbKey::OrphanBlock(hash.clone())).unwrap() {
        assert_eq!(*retrieved_orphan, orphan);
    } else {
        panic!();
    }

    let mut txn = DbTransaction::new();
    txn.delete_orphan(hash.clone());
    assert!(db.write(txn).is_ok());
    assert!(!db.contains(&DbKey::OrphanBlock(hash)).unwrap());
}

#[test]
fn lmdb_file_lock() {
    // Create temporary test folder
    let temp_path = create_temporary_data_path();

    // Perform test
    {
        let db = create_lmdb_database(&temp_path, LMDBConfig::default()).unwrap();

        match create_lmdb_database(&temp_path, LMDBConfig::default()) {
            Err(ChainStorageError::CannotAcquireFileLock) => {},
            _ => panic!("Should not be able to make this db"),
        }

        drop(db);

        let _db2 =
            create_lmdb_database(&temp_path, LMDBConfig::default()).expect("Should be able to make a new lmdb now");
    }

    // Cleanup test data - in Windows the LMBD `set_mapsize` sets file size equals to map size; Linux use sparse files
    if std::path::Path::new(&temp_path).exists() {
        std::fs::remove_dir_all(&temp_path).expect("Could not clear temp storage for db");
    }
}

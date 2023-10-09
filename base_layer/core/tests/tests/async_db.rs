//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::ops::Deref;

use tari_common::configuration::Network;
use tari_core::{
    blocks::Block,
    chain_storage::{async_db::AsyncBlockchainDb, BlockAddResult},
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        tari_amount::T,
        test_helpers::{schema_to_transaction, TestKeyManager},
        transaction_components::{TransactionOutput, WalletOutput},
    },
    txn_schema,
};
use tari_test_utils::runtime::test_async;

use crate::helpers::{
    block_builders::chain_block_with_new_coinbase,
    database::create_orphan_block,
    sample_blockchains::{create_blockchain_db_no_cut_through, create_new_blockchain},
};

/// Finds the UTXO in a block corresponding to the wallet output. We have to search for outputs because UTXOs get
/// sorted in blocks, and so the order they were inserted in can change.
async fn find_utxo(output: &WalletOutput, block: &Block, key_manager: &TestKeyManager) -> Option<TransactionOutput> {
    let commitment = key_manager
        .get_commitment(&output.spending_key_id, &output.value.into())
        .await
        .unwrap();
    for utxo in block.body.outputs().iter() {
        if commitment == utxo.commitment {
            return Some(utxo.clone());
        }
    }
    None
}

#[test]
fn fetch_async_headers() {
    test_async(move |rt| {
        rt.spawn(async move {
            let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
            let db = AsyncBlockchainDb::new(db);
            for block in blocks {
                let height = block.height();
                let hash = *block.hash();
                let db = db.clone();
                let header_height = db.fetch_header(height).await.unwrap().unwrap();
                let header_hash = db.fetch_header_by_block_hash(hash).await.unwrap().unwrap();
                assert_eq!(block.header(), &header_height);
                assert_eq!(block.header(), &header_hash);
            }
        });
    });
}

#[test]
fn async_rewind_to_height() {
    test_async(move |rt| {
        rt.spawn(async move {
            let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
            let db = AsyncBlockchainDb::new(db);
            db.rewind_to_height(2).await.unwrap();
            let result = db.fetch_block(3, true).await;
            assert!(result.is_err());
            let block = db.fetch_block(2, true).await.unwrap();
            assert_eq!(block.confirmations(), 1);
            assert_eq!(blocks[2].block(), block.block());
        });
    });
}

#[test]
fn fetch_async_utxo() {
    test_async(move |rt| {
        rt.spawn(async move {
            let (adb, blocks, outputs, _, key_manager) = create_blockchain_db_no_cut_through().await;
            // Retrieve a UTXO and an STXO
            let utxo = find_utxo(&outputs[4][0], blocks[4].block(), &key_manager)
                .await
                .unwrap();
            let stxo = find_utxo(&outputs[1][0], blocks[1].block(), &key_manager)
                .await
                .unwrap();
            let db = AsyncBlockchainDb::new(adb.clone());
            let db2 = AsyncBlockchainDb::new(adb);

            let utxo_check = db.fetch_utxo(utxo.hash()).await.unwrap().unwrap();
            assert_eq!(utxo_check, utxo);
            let stxo_check = db2.fetch_utxo(stxo.hash()).await.unwrap().unwrap();
            assert_eq!(stxo_check, stxo);
        });
    });
}

#[test]
fn fetch_async_block() {
    test_async(move |rt| {
        rt.spawn(async move {
            let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
            let db = AsyncBlockchainDb::new(db);
            for block in blocks {
                let height = block.height();
                let block_check = db.fetch_block(height, true).await.unwrap();
                assert_eq!(block.block(), block_check.block());
            }
        });
    });
}

#[test]
fn async_add_new_block() {
    test_async(|rt| {
        rt.spawn(async move {
            let network = Network::LocalNet;
            let (db, blocks, outputs, consensus_manager, key_manager) = create_new_blockchain(network).await;
            let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];

            let txns = schema_to_transaction(&schema, &key_manager)
                .await
                .0
                .iter()
                .map(|t| t.deref().clone())
                .collect();
            let new_block =
                chain_block_with_new_coinbase(blocks.last().unwrap(), txns, &consensus_manager, None, &key_manager)
                    .await
                    .0;

            let new_block = db.prepare_new_block(new_block).unwrap();
            let db = AsyncBlockchainDb::new(db);
            let result = db.add_block(new_block.clone().into()).await.unwrap();
            let block = db.fetch_block(1, true).await.unwrap();
            match result {
                BlockAddResult::Ok(_) => assert_eq!(Block::from(block).hash(), new_block.hash()),
                _ => panic!("Unexpected result"),
            }
        });
    });
}

#[test]
fn async_add_block_fetch_orphan() {
    test_async(move |rt| {
        rt.spawn(async move {
            let (db, _, _, consensus, key_manager) = create_blockchain_db_no_cut_through().await;

            let orphan = create_orphan_block(7, vec![], &consensus, &key_manager).await;
            let block_hash = orphan.hash();
            let db = AsyncBlockchainDb::new(db);
            db.add_block(orphan.clone().into()).await.unwrap();
            let block = db.fetch_orphan(block_hash).await.unwrap();
            assert_eq!(orphan, block);
        });
    });
}

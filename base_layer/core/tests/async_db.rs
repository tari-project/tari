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

use std::ops::Deref;

use helpers::{
    block_builders::chain_block_with_new_coinbase,
    database::create_orphan_block,
    sample_blockchains::{create_blockchain_db_no_cut_through, create_new_blockchain},
};
use tari_common::configuration::Network;
use tari_common_types::types::CommitmentFactory;
use tari_core::{
    blocks::Block,
    chain_storage::{async_db::AsyncBlockchainDb, BlockAddResult, PrunedOutput},
    transactions::{
        tari_amount::T,
        test_helpers::schema_to_transaction,
        transaction_components::{TransactionOutput, UnblindedOutput},
        CryptoFactories,
    },
    txn_schema,
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, tari_utilities::Hashable};
use tari_test_utils::runtime::test_async;

#[allow(dead_code)]
mod helpers;

/// Finds the UTXO in a block corresponding to the unblinded output. We have to search for outputs because UTXOs get
/// sorted in blocks, and so the order they were inserted in can change.
fn find_utxo(output: &UnblindedOutput, block: &Block, factory: &CommitmentFactory) -> Option<TransactionOutput> {
    for utxo in block.body.outputs().iter() {
        if factory.open_value(&output.spending_key, output.value.into(), &utxo.commitment) {
            return Some(utxo.clone());
        }
    }
    None
}

#[test]
fn fetch_async_headers() {
    let (db, blocks, _, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        let db = AsyncBlockchainDb::new(db);
        for block in blocks.into_iter() {
            let height = block.height();
            let hash = block.hash().clone();
            let db = db.clone();
            rt.spawn(async move {
                let header_height = db.fetch_header(height).await.unwrap().unwrap();
                let header_hash = db.fetch_header_by_block_hash(hash).await.unwrap().unwrap();
                assert_eq!(block.header(), &header_height);
                assert_eq!(block.header(), &header_hash);
            });
        }
    });
}

#[test]
fn async_rewind_to_height() {
    let (db, blocks, _, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        let db = AsyncBlockchainDb::new(db);
        rt.spawn(async move {
            db.rewind_to_height(2).await.unwrap();
            let result = db.fetch_block(3).await;
            assert!(result.is_err());
            let block = db.fetch_block(2).await.unwrap();
            assert_eq!(block.confirmations(), 1);
            assert_eq!(blocks[2].block(), block.block());
        });
    });
}

#[test]
fn fetch_async_utxo() {
    let (adb, blocks, outputs, _) = create_blockchain_db_no_cut_through();
    let factory = CommitmentFactory::default();
    // Retrieve a UTXO and an STXO
    let utxo = find_utxo(&outputs[4][0], blocks[4].block(), &factory).unwrap();
    let stxo = find_utxo(&outputs[1][0], blocks[1].block(), &factory).unwrap();
    test_async(move |rt| {
        let db = AsyncBlockchainDb::new(adb.clone());
        let db2 = AsyncBlockchainDb::new(adb);
        rt.spawn(async move {
            let utxo_check = db.fetch_utxo(utxo.hash()).await.unwrap().unwrap();
            assert_eq!(utxo_check, PrunedOutput::NotPruned { output: utxo });
        });
        rt.spawn(async move {
            let stxo_check = db2.fetch_utxo(stxo.hash()).await.unwrap().unwrap();
            assert_eq!(stxo_check, PrunedOutput::NotPruned { output: stxo });
        });
    });
}

#[test]
fn fetch_async_block() {
    let (db, blocks, _, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        let db = AsyncBlockchainDb::new(db);
        rt.spawn(async move {
            for block in blocks.into_iter() {
                let height = block.height();
                let block_check = db.fetch_block(height).await.unwrap();
                assert_eq!(block.block(), block_check.block());
            }
        });
    });
}

#[test]
fn async_add_new_block() {
    let network = Network::LocalNet;
    let (db, blocks, outputs, consensus_manager) = create_new_blockchain(network);
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];

    let txns = schema_to_transaction(&schema)
        .0
        .iter()
        .map(|t| t.deref().clone())
        .collect();
    let new_block = chain_block_with_new_coinbase(
        blocks.last().unwrap(),
        txns,
        &consensus_manager,
        &CryptoFactories::default(),
    )
    .0;

    let new_block = db.prepare_new_block(new_block).unwrap();

    test_async(|rt| {
        let db = AsyncBlockchainDb::new(db);
        rt.spawn(async move {
            let result = db.add_block(new_block.clone().into()).await.unwrap();
            let block = db.fetch_block(1).await.unwrap();
            match result {
                BlockAddResult::Ok(_) => assert_eq!(Block::from(block).hash(), new_block.hash()),
                _ => panic!("Unexpected result"),
            }
        });
    });
}

#[test]
fn async_add_block_fetch_orphan() {
    let (db, _, _, consensus) = create_blockchain_db_no_cut_through();

    let orphan = create_orphan_block(7, vec![], &consensus);
    let block_hash = orphan.hash();
    test_async(move |rt| {
        let db = AsyncBlockchainDb::new(db);
        rt.spawn(async move {
            db.add_block(orphan.clone().into()).await.unwrap();
            let block = db.fetch_orphan(block_hash).await.unwrap();
            assert_eq!(orphan, block);
        });
    });
}

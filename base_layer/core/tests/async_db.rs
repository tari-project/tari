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

#[allow(dead_code)]
mod helpers;

use helpers::{
    block_builders::chain_block,
    sample_blockchains::{create_blockchain_db_no_cut_through, create_new_blockchain},
};
use std::ops::Deref;
use tari_core::{
    blocks::Block,
    chain_storage::{async_db, BlockAddResult, MmrTree},
    helpers::create_orphan_block,
    transactions::{
        helpers::schema_to_transaction,
        tari_amount::T,
        transaction::{TransactionOutput, UnblindedOutput},
        types::CommitmentFactory,
    },
    txn_schema,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    tari_utilities::{hex::Hex, Hashable},
};
use tari_test_utils::runtime::test_async;

/// Finds the UTXO in a block corresponding to the unblinded output. We have to search for outputs because UTXOs get
/// sorted in blocks, and so the order they were inserted in can change.
fn find_utxo(output: &UnblindedOutput, block: &Block, factory: &CommitmentFactory) -> Option<TransactionOutput> {
    for utxo in block.body.outputs().iter() {
        if factory.open_value(&output.spending_key, output.value.into(), &utxo.commitment) {
            return Some(utxo.clone());
        }
    }
    return None;
}

#[test]
fn fetch_async_kernel() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(|rt| {
        for block in blocks.into_iter() {
            block.body.kernels().into_iter().for_each(|k| {
                let db = db.clone();
                let k = k.clone();
                let hash = k.hash();
                rt.spawn(async move {
                    let kern_db = async_db::fetch_kernel(db, hash).await;
                    assert_eq!(k, kern_db.unwrap());
                });
            });
        }
    });
}

#[test]
fn fetch_async_headers() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        for block in blocks.into_iter() {
            let height = block.header.height;
            let hash = block.hash();
            let db = db.clone();
            rt.spawn(async move {
                let header_height = async_db::fetch_header(db.clone(), height).await.unwrap();
                let header_hash = async_db::fetch_header_with_block_hash(db.clone(), hash).await.unwrap();
                assert_eq!(block.header, header_height);
                assert_eq!(block.header, header_hash);
            });
        }
    });
}

#[test]
fn async_rewind_to_height() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            async_db::rewind_to_height(dbc.clone(), 2).await.unwrap();
            let result = async_db::fetch_block(dbc.clone(), 3).await;
            assert!(result.is_err());
            let block = async_db::fetch_block(dbc.clone(), 2).await.unwrap();
            assert_eq!(block.confirmations(), 1);
            assert_eq!(blocks[2], Block::from(block));
        });
    });
}

#[test]
fn fetch_async_utxo() {
    let (db, blocks, outputs) = create_blockchain_db_no_cut_through();
    let factory = CommitmentFactory::default();
    // Retrieve a UTXO and an STXO
    let utxo = find_utxo(&outputs[4][0], &blocks[4], &factory).unwrap();
    let stxo = find_utxo(&outputs[1][0], &blocks[1], &factory).unwrap();
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        let _blocks2 = blocks.clone();
        rt.spawn(async move {
            let utxo_check = async_db::fetch_utxo(db.clone(), utxo.hash()).await;
            assert_eq!(utxo_check, Ok(utxo));
        });
        rt.spawn(async move {
            let stxo_check = async_db::fetch_stxo(db2.clone(), stxo.hash()).await;
            assert_eq!(stxo_check, Ok(stxo));
        });
    });
}

#[test]
fn async_is_utxo() {
    let (db, blocks, outputs) = create_blockchain_db_no_cut_through();
    let factory = CommitmentFactory::default();
    blocks.iter().for_each(|b| println!("{}", b));
    // Retrieve a UTXO and an STXO
    let utxo = find_utxo(&outputs[4][0], &blocks[4], &factory).unwrap();
    let stxo = find_utxo(&outputs[1][0], &blocks[1], &factory).unwrap();
    // Check using sync functions
    assert_eq!(db.is_utxo(utxo.hash()), Ok(true));
    assert_eq!(db.is_utxo(stxo.hash()), Ok(false));
    test_async(move |rt| {
        let db = db.clone();
        let db2 = db.clone();
        let _blocks2 = blocks.clone();
        rt.spawn(async move {
            let is_utxo = async_db::is_utxo(db.clone(), utxo.hash()).await;
            assert_eq!(is_utxo, Ok(true));
        });
        rt.spawn(async move {
            let is_utxo = async_db::is_utxo(db2.clone(), stxo.hash()).await;
            assert_eq!(is_utxo, Ok(false));
        });
    });
}

#[test]
fn fetch_async_block() {
    let (db, blocks, _) = create_blockchain_db_no_cut_through();
    test_async(move |rt| {
        for block in blocks.into_iter() {
            let height = block.header.height;
            let db = db.clone();
            rt.spawn(async move {
                let block_check = async_db::fetch_block(db.clone(), height).await.unwrap();
                assert_eq!(&block, block_check.block());
            });
        }
    });
}

#[test]
fn async_add_new_block() {
    let (db, blocks, outputs) = create_new_blockchain();
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];
    let txns = schema_to_transaction(&schema)
        .0
        .iter()
        .map(|t| t.deref().clone())
        .collect();
    let new_block = chain_block(&blocks.last().unwrap(), txns);
    let new_block = db.calculate_mmr_roots(new_block).unwrap();
    test_async(|rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            let result = async_db::add_block(dbc.clone(), new_block.clone()).await.unwrap();
            let block = async_db::fetch_block(dbc.clone(), 1).await.unwrap();
            match result {
                BlockAddResult::Ok => assert_eq!(Block::from(block).hash(), new_block.hash()),
                _ => panic!("Unexpected result"),
            }
        });
    });
}

#[test]
fn fetch_async_mmr_roots() {
    let (db, _blocks, _) = create_blockchain_db_no_cut_through();
    let metadata = db.get_metadata().unwrap();
    test_async(move |rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            let root = futures::join!(
                async_db::fetch_mmr_root(dbc.clone(), MmrTree::Utxo),
                async_db::fetch_mmr_root(dbc.clone(), MmrTree::Kernel),
            );
            let block_height = metadata.height_of_longest_chain.unwrap();
            let header = async_db::fetch_header(dbc.clone(), block_height).await.unwrap();
            let utxo_mmr = root.0.unwrap().to_hex();
            let kernel_mmr = root.1.unwrap().to_hex();
            assert_eq!(utxo_mmr, header.output_mr.to_hex());
            assert_eq!(kernel_mmr, header.kernel_mr.to_hex(), "Kernel MMR roots don't match");
        });
    });
}

#[test]
fn async_add_block_fetch_orphan() {
    env_logger::init();
    let (db, _, _) = create_blockchain_db_no_cut_through();
    let orphan = create_orphan_block(7, vec![]);
    let block_hash = orphan.hash();
    test_async(move |rt| {
        let dbc = db.clone();
        rt.spawn(async move {
            async_db::add_block(dbc.clone(), orphan.clone()).await.unwrap();
            let block = async_db::fetch_orphan(dbc.clone(), block_hash).await.unwrap();
            assert_eq!(orphan, block);
        });
    });
}

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

#![allow(clippy::indexing_slicing)]
use std::ops::Deref;

use tari_common::configuration::Network;
use tari_core::chain_storage::{async_db::AsyncBlockchainDb, BlockAddResult};
use tari_node_components::blocks::Block;
use tari_transaction_components::{tari_amount::T, test_helpers::schema_to_transaction, txn_schema};

use crate::helpers::{
    block_builders::chain_block_with_new_coinbase,
    database::create_orphan_block,
    sample_blockchains::{create_blockchain_db_no_cut_through, create_new_blockchain},
};

#[tokio::test]
async fn fetch_async_headers() {
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
}

#[tokio::test]
async fn async_rewind_to_height() {
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
    let db = AsyncBlockchainDb::new(db);
    db.rewind_to_height(2).await.unwrap();
    let result = db.fetch_block(3, true).await;
    assert!(result.is_err());
    let block = db.fetch_block(2, true).await.unwrap();
    assert_eq!(block.confirmations(), 1);
    assert_eq!(blocks[2].block(), block.block());
}

#[tokio::test]
async fn fetch_async_block() {
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
    let db = AsyncBlockchainDb::new(db);
    for block in blocks {
        let height = block.height();
        let block_check = db.fetch_block(height, true).await.unwrap();
        assert_eq!(block.block(), block_check.block());
    }
}

#[tokio::test]
async fn async_add_new_block() {
    let network = Network::LocalNet;
    let (db, blocks, outputs, consensus_manager, mut key_manager) = create_new_blockchain(network).await;
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];

    let txns = schema_to_transaction(&schema, &mut key_manager)
        .await
        .0
        .iter()
        .map(|t| t.deref().clone())
        .collect();
    let new_block =
        chain_block_with_new_coinbase(blocks.last().unwrap(), txns, &consensus_manager, None, &mut key_manager)
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
}

#[tokio::test]
async fn async_add_block_fetch_orphan() {
    let (db, _, _, consensus, mut key_manager) = create_blockchain_db_no_cut_through().await;

    let orphan = create_orphan_block(7, vec![], &consensus, &mut key_manager).await;
    let block_hash = orphan.hash();
    let db = AsyncBlockchainDb::new(db);
    db.add_block(orphan.clone().into()).await.unwrap();
    let block = db.fetch_orphan(block_hash).await.unwrap();
    assert_eq!(orphan, block);
}

#[tokio::test]
async fn generate_kernel_merkle_proof() {
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through().await;
    let db = AsyncBlockchainDb::new(db);
    for block in blocks.into_iter().skip(1) {
        let kernels = block.block().body.kernels();
        for kernel in kernels {
            let kernel_hash = kernel.hash();
            let proof = db
                .generate_kernel_merkle_proof(kernel.excess_sig.clone())
                .await
                .unwrap();
            assert_eq!(proof.block_hash, block.header().hash());
            assert_eq!(proof.kernel_hash, kernel_hash);
            proof.verify(&block.header().kernel_mr).unwrap();
        }
    }
}

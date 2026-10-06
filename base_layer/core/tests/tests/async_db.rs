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
#![allow(clippy::arithmetic_side_effects)]
use std::ops::Deref;

use tari_common::configuration::Network;
use tari_common_types::burn_proof::BurnOutputProof;
use tari_core::{
    chain_storage::{
        BlockAddResult,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        ChainStorageError,
        Validators,
        async_db::AsyncBlockchainDb,
    },
    consensus::BaseNodeConsensusManager,
    test_helpers::blockchain::create_test_db,
    validation::{DifficultyCalculator, mocks::MockValidator},
};
use tari_node_components::blocks::Block;
use tari_transaction_components::{
    consensus::ConsensusConstantsBuilder,
    key_manager::KeyManager,
    tari_amount::{T, uT},
    test_helpers::{TransactionSchema, schema_to_transaction},
    transaction_components::{
        OutputFeatures,
        WalletOutput,
        burn_output_proof::{BurnOutputProofExt, OutputHashPreimageExt},
    },
    txn_schema,
};

use crate::helpers::{
    block_builders::{chain_block_with_new_coinbase, create_genesis_block_with_utxos, generate_new_block},
    database::create_orphan_block,
    sample_blockchains::{create_blockchain_db_no_cut_through, create_new_blockchain},
};

#[tokio::test]
async fn fetch_async_headers() {
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through();
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
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through();
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
    let (db, blocks, _, _, _) = create_blockchain_db_no_cut_through();
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
    let (db, blocks, outputs, consensus_manager, key_manager) = create_new_blockchain(network);
    let schema = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![20 * T, 20 * T])];

    let txns = schema_to_transaction(&schema, &key_manager)
        .0
        .iter()
        .map(|t| t.deref().clone())
        .collect();
    let new_block =
        chain_block_with_new_coinbase(blocks.last().unwrap(), txns, &consensus_manager, None, &key_manager).0;

    let new_block = db.prepare_new_block(new_block).unwrap();
    let db = AsyncBlockchainDb::new(db);
    let result = db.add_block(new_block.clone().into()).await.unwrap().result;
    let block = db.fetch_block(1, true).await.unwrap();
    match result {
        BlockAddResult::Ok(_) => assert_eq!(Block::from(block).hash(), new_block.hash()),
        _ => panic!("Unexpected result"),
    }
}

#[tokio::test]
async fn async_add_block_fetch_orphan() {
    let (db, _, _, consensus, key_manager) = create_blockchain_db_no_cut_through();

    let orphan = create_orphan_block(7, vec![], &consensus, &key_manager);
    let block_hash = orphan.hash();
    let db = AsyncBlockchainDb::new(db);
    db.add_block(orphan.clone().into()).await.unwrap();
    let block = db.fetch_orphan(block_hash).await.unwrap();
    assert_eq!(orphan, block);
}

#[tokio::test]
async fn generate_burn_output_proof() {
    let key_manager = KeyManager::new_random().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (genesis, gen_outputs) = create_genesis_block_with_utxos(&[T, T, T, T], &consensus_constants, &key_manager);
    let rules = BaseNodeConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let mut store = BlockchainDatabase::start_new(
        create_test_db(),
        rules.clone(),
        Validators::new(
            MockValidator::new(true),
            MockValidator::new(true),
            MockValidator::new(true),
        ),
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    let mut blocks = vec![genesis];
    let mut outputs = vec![gen_outputs];

    // Block 1: a single burn beside the coinbase
    let schemas = vec![burn_schema(outputs[0][1].clone(), 700_000)];
    generate_new_block(&mut store, &mut blocks, &mut outputs, schemas, &rules, &key_manager).unwrap();
    // Block 2: two burns among other outputs
    let schemas = vec![
        txn_schema!(from: vec![outputs[0][2].clone()], to: vec![T / 4, T / 4]),
        burn_schema(outputs[0][3].clone(), 800_000),
        burn_schema(outputs[0][4].clone(), 900_000),
    ];
    generate_new_block(&mut store, &mut blocks, &mut outputs, schemas, &rules, &key_manager).unwrap();

    let db = AsyncBlockchainDb::new(store);
    let mut num_burns = 0;
    for block in blocks.iter().skip(1) {
        for burn in block.block().body.outputs().iter().filter(|o| o.is_burned()) {
            num_burns += 1;
            let proof = db.generate_burn_output_proof(burn.commitment.clone()).await.unwrap();
            assert_eq!(proof.block_hash, *block.hash());
            assert_eq!(proof.block_height, block.height());
            assert_eq!(proof.output.hash().unwrap(), burn.hash());
            proof.verify(&block.header().block_output_mr).unwrap();
            // The proof is only valid for the block it was mined in
            proof.verify(&blocks[0].header().block_output_mr).unwrap_err();

            let json = serde_json::to_string(&proof).unwrap();
            let proof = serde_json::from_str::<BurnOutputProof>(&json).unwrap();
            proof.verify(&block.header().block_output_mr).unwrap();
        }
    }
    assert_eq!(num_burns, 3);

    let not_a_burn = blocks[1]
        .block()
        .body
        .outputs()
        .iter()
        .find(|o| !o.is_burned())
        .unwrap();
    let err = db
        .generate_burn_output_proof(not_a_burn.commitment.clone())
        .await
        .unwrap_err();
    assert!(err.is_value_not_found(), "{err}");

    // A pruned node cannot generate proofs for burns at or below its pruned height
    db.write_transaction().set_pruned_height(1).commit().await.unwrap();
    let burn = blocks[1].block().body.outputs().iter().find(|o| o.is_burned()).unwrap();
    let err = db
        .generate_burn_output_proof(burn.commitment.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(err, ChainStorageError::BlockBodyPruned {
            height: 1,
            pruned_height: 1
        }),
        "{err}"
    );
    let burn = blocks[2].block().body.outputs().iter().find(|o| o.is_burned()).unwrap();
    db.generate_burn_output_proof(burn.commitment.clone()).await.unwrap();
}

fn burn_schema(input: WalletOutput, amount: u64) -> TransactionSchema {
    txn_schema!(
        from: vec![input],
        to: vec![amount * uT],
        fee: 5.into(),
        lock: 0,
        features: OutputFeatures::create_burn_output()
    )
}

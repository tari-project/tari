//  Copyright 2021, The Tari Project
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

use std::sync::Arc;

use tari_common::configuration::Network;
use tari_crypto::script;
use tari_test_utils::unpack_enum;

use crate::{
    consensus::{ConsensusConstantsBuilder, ConsensusManager},
    test_helpers::{
        blockchain::{TempDatabase, TestBlockchain},
        BlockSpec,
    },
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::T,
        test_helpers::schema_to_transaction,
        transaction::TransactionError,
        CoinbaseBuilder,
        CryptoFactories,
    },
    txn_schema,
    validation::{block_validators::BlockValidator, ValidationError},
};

fn setup_with_rules(rules: ConsensusManager) -> (TestBlockchain, BlockValidator<TempDatabase>) {
    let blockchain = TestBlockchain::create(rules.clone());
    let validator = BlockValidator::new(
        blockchain.db().clone().into(),
        rules,
        CryptoFactories::default(),
        false,
        2,
    );
    (blockchain, validator)
}

fn setup() -> (TestBlockchain, BlockValidator<TempDatabase>) {
    setup_with_rules(ConsensusManager::builder(Network::LocalNet).build())
}

#[tokio::test]
async fn it_passes_if_block_is_valid() {
    let (blockchain, validator) = setup();

    let (block, _) = blockchain.create_next_tip(BlockSpec::default());
    let out = validator.validate_block_body(block.block().clone()).await.unwrap();
    assert_eq!(out, *block.block());
}

#[tokio::test]
async fn it_checks_the_coinbase_reward() {
    let (blockchain, validator) = setup();

    let (block, _) = blockchain.create_chained_block("GB", BlockSpec::new().with_reward(10_000_000.into()).finish());
    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(
        err,
        ValidationError::TransactionError(TransactionError::InvalidCoinbase)
    ));
}

#[tokio::test]
async fn it_checks_exactly_one_coinbase() {
    let (blockchain, validator) = setup();

    let (mut block, coinbase) = blockchain.create_unmined_block("GB", BlockSpec::new());

    let (_, coinbase_output) = CoinbaseBuilder::new(CryptoFactories::default())
        .with_block_height(1)
        .with_fees(0.into())
        .with_nonce(0.into())
        .with_spend_key(42.into())
        .build_with_reward(blockchain.rules().consensus_constants(1), coinbase.value)
        .unwrap();

    block.body.add_output(
        coinbase_output
            .as_transaction_output(&CryptoFactories::default())
            .unwrap(),
    );
    let block = blockchain.mine_block("GB", block, 1.into());

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    unpack_enum!(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase) = err);

    let (block, _) = blockchain.create_unmined_block("GB", BlockSpec::new().skip_coinbase().finish());
    let block = blockchain.mine_block("GB", block, 1.into());

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    unpack_enum!(ValidationError::TransactionError(TransactionError::NoCoinbase) = err);
}

#[tokio::test]
async fn it_checks_double_spends() {
    let (mut blockchain, validator) = setup();

    let (_, coinbase_a) = blockchain.add_next_tip("A", Default::default());
    let (txs, _) = schema_to_transaction(&[txn_schema!(from: vec![coinbase_a], to: vec![50 * T])]);

    blockchain.add_next_tip(
        "B",
        BlockSpec::new()
            .with_transactions(txs.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(txs.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(err, ValidationError::ContainsTxO));
}

#[tokio::test]
async fn it_checks_input_maturity() {
    let rules = ConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build();
    let (mut blockchain, validator) = setup_with_rules(rules);

    let (_, coinbase_a) = blockchain.add_next_tip("A", Default::default());
    let mut schema = txn_schema!(from: vec![coinbase_a], to: vec![50 * T]);
    schema.from[0].features.maturity = 100;
    let (txs, _) = schema_to_transaction(&[schema]);

    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(txs.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(
        err,
        ValidationError::TransactionError(TransactionError::InputMaturity)
    ));
    unpack_enum!(ValidationError::TransactionError(TransactionError::InputMaturity) = err);
}

#[tokio::test]
async fn it_checks_txo_sort_order() {
    let rules = ConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build();
    let (mut blockchain, validator) = setup_with_rules(rules);

    let (_, coinbase_a) = blockchain.add_next_tip("A", Default::default());

    let schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1]);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block("A", BlockSpec::new().with_transactions(txs).finish());
    let outputs = block.body.outputs().iter().rev().cloned().collect::<Vec<_>>();
    let inputs = block.body.inputs().clone();
    let kernels = block.body.kernels().clone();
    block.body = AggregateBody::new_sorted_unchecked(inputs, outputs, kernels);
    let block = blockchain.mine_block("A", block, 1.into());

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateOutput));
}

#[tokio::test]
async fn it_limits_the_script_byte_size() {
    let rules = ConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .with_max_script_byte_size(0)
                .build(),
        )
        .build();
    let (mut blockchain, validator) = setup_with_rules(rules);

    let (_, coinbase_a) = blockchain.add_next_tip("A", Default::default());

    let mut schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
    schema1.script = script!(Nop Nop Nop);
    let (txs, _) = schema_to_transaction(&[schema1]);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (block, _) = blockchain.create_next_tip(BlockSpec::new().with_transactions(txs).finish());

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(err, ValidationError::TariScriptExceedsMaxSize { .. }));
}

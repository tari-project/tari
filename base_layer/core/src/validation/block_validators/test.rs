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
use tari_script::script;
use tari_test_utils::unpack_enum;

use crate::{
    block_spec,
    consensus::{ConsensusConstantsBuilder, ConsensusManager},
    test_helpers::{
        blockchain::{TempDatabase, TestBlockchain},
        BlockSpec,
    },
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::T,
        test_helpers::{schema_to_transaction},
        transaction_components::{ TransactionError},
        CoinbaseBuilder,
        CryptoFactories,
    },
    txn_schema,
    validation::{
        block_validators::{BlockValidator, BodyOnlyValidator, OrphanBlockValidator},
        traits::PostOrphanBodyValidation,
        BlockSyncBodyValidation,
        OrphanValidation,
        ValidationError,
    },
};

fn setup_with_rules(rules: ConsensusManager) -> (TestBlockchain, BlockValidator<TempDatabase>) {
    let blockchain = TestBlockchain::create(rules.clone());
    let validator = BlockValidator::new(
        blockchain.db().clone().into(),
        rules,
        CryptoFactories::default(),
        false,
        6,
    );
    (blockchain, validator)
}

fn setup() -> (TestBlockchain, BlockValidator<TempDatabase>) {
    let rules = ConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build();
    setup_with_rules(rules)
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

    let (block, _) = blockchain.create_chained_block(block_spec!("A", parent: "GB", reward: 10 * T, ));
    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(
        err,
        ValidationError::TransactionError(TransactionError::InvalidCoinbase)
    ));
}

#[tokio::test]
async fn it_checks_exactly_one_coinbase() {
    let (blockchain, validator) = setup();

    let (mut block, coinbase) = blockchain.create_unmined_block(block_spec!("A1", parent: "GB"));

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

    let (block, _) = blockchain.create_unmined_block(block_spec!("A2", parent: "GB", skip_coinbase: true,));
    let block = blockchain.mine_block("GB", block, 1.into());

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    unpack_enum!(ValidationError::TransactionError(TransactionError::NoCoinbase) = err);
}

#[tokio::test]
async fn it_checks_double_spends() {
    let (mut blockchain, validator) = setup();

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let (txs, _) = schema_to_transaction(&[txn_schema!(from: vec![coinbase_a], to: vec![50 * T])]);

    blockchain
        .add_next_tip(block_spec!("1", transactions: txs.iter().map(|t| (**t).clone()).collect()))
        .unwrap();
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
    let (mut blockchain, validator) = setup();

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
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
    let (mut blockchain, validator) = setup();

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1]);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block(block_spec!("B->A", transactions: txs));
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

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let mut schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
    schema1.script = script!(Nop Nop Nop);
    let (txs, _) = schema_to_transaction(&[schema1]);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(err, ValidationError::TariScriptExceedsMaxSize { .. }));
}

#[tokio::test]
async fn it_rejects_invalid_input_metadata() {
    let rules = ConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build();
    let (mut blockchain, validator) = setup_with_rules(rules);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let mut schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
    schema1.from[0].sender_offset_public_key = Default::default();
    let (txs, _) = schema_to_transaction(&[schema1]);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let err = validator.validate_block_body(block.block().clone()).await.unwrap_err();
    assert!(matches!(err, ValidationError::UnknownInputs(_)));
}

#[tokio::test]
async fn it_rejects_zero_conf_double_spends() {
    let (mut blockchain, validator) = setup();
    let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

    let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
    let (initial_tx, outputs) = schema_to_transaction(&[schema]);

    let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![200 * T]);
    let (first_spend, _) = schema_to_transaction(&[schema]);

    let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![150 * T]);
    let (double_spend, _) = schema_to_transaction(&[schema]);

    let transactions = initial_tx
        .into_iter()
        .chain(first_spend)
        .chain(double_spend)
        .map(|b| Arc::try_unwrap(b).unwrap())
        .collect::<Vec<_>>();

    let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
    let err = validator.validate_body(unmined).await.unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput));
}

mod body_only {
    use super::*;

    #[test]
    fn it_rejects_invalid_input_metadata() {
        let rules = ConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BodyOnlyValidator::new(rules);

        let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

        let mut schema1 = txn_schema!(from: vec![coinbase_a], to: vec![50 * T, 12 * T]);
        schema1.from[0].sender_offset_public_key = Default::default();
        let (txs, _) = schema_to_transaction(&[schema1]);
        let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
        let (block, _) = blockchain.create_next_tip(BlockSpec::new().with_transactions(txs).finish());

        let metadata = blockchain.db().get_chain_metadata().unwrap();

        let db = blockchain.db().db_read_access().unwrap();
        let err = validator
            .validate_body_for_valid_orphan(&*db, &block, &metadata)
            .unwrap_err();
        assert!(matches!(err, ValidationError::UnknownInputs(_)));
    }
}

mod orphan_validator {
    use super::*;
    use crate::txn_schema;

    #[test]
    fn it_rejects_zero_conf_double_spends() {
        let rules = ConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = OrphanBlockValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase], to: vec![201 * T]);
        let (initial_tx, outputs) = schema_to_transaction(&[schema]);

        let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![200 * T]);
        let (first_spend, _) = schema_to_transaction(&[schema]);

        let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![150 * T]);
        let (double_spend, _) = schema_to_transaction(&[schema]);

        let transactions = initial_tx
            .into_iter()
            .chain(first_spend)
            .chain(double_spend)
            .map(|b| Arc::try_unwrap(b).unwrap())
            .collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        let err = validator.validate(&unmined).unwrap_err();
        assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput));
    }
}

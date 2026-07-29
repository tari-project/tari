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

#![allow(clippy::indexing_slicing)]
use std::sync::Arc;

use tari_common::configuration::Network;
use tari_common_types::tari_address::TariAddress;
use tari_node_components::blocks::BlockValidationError;
use tari_script::{push_pubkey_script, script};
use tari_test_utils::unpack_enum;
use tari_transaction_components::{
    CoinbaseBuilder,
    aggregated_body::AggregateBody,
    consensus::ConsensusConstantsBuilder,
    crypto_factories::CryptoFactories,
    key_manager::{TariKeyId, TransactionKeyManagerInterface},
    tari_amount::{T, uT},
    tari_proof_of_work::Difficulty,
    test_helpers::schema_to_transaction,
    transaction_components::{
        EncryptedData,
        MemoField,
        RangeProofType,
        TransactionError,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
    },
    txn_schema,
    validation::AggregatedBodyValidationError,
};
use tokio::time::Instant;

use super::BlockBodyFullValidator;
use crate::{
    block_spec,
    consensus::BaseNodeConsensusManager,
    test_helpers::{BlockSpec, blockchain::TestBlockchain},
    validation::{BlockBodyValidator, ValidationError},
};
fn setup_with_rules(
    rules: BaseNodeConsensusManager,
    check_rangeproof: bool,
) -> (TestBlockchain, BlockBodyFullValidator) {
    let blockchain = TestBlockchain::create(rules.clone());
    let validator = BlockBodyFullValidator::new(rules, check_rangeproof);
    (blockchain, validator)
}

fn setup(check_rangeproof: bool) -> (TestBlockchain, BlockBodyFullValidator) {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .with_max_block_transaction_weight(127_795)
                .build(),
        )
        .build()
        .unwrap();
    setup_with_rules(rules, check_rangeproof)
}

#[tokio::test]
async fn it_passes_if_large_output_block_is_valid() {
    // we use this test to benchmark a block with multiple outputs
    let (mut blockchain, validator) = setup(false);
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let mut outs = Vec::new();
    // create 498 outputs, so we have a block with 500 outputs, 498 + change + coinbase
    for _ in 0..498 {
        outs.push(9000 * uT);
    }

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: outs);
    let (txs, _outputs) = schema_to_transaction(&[schema1], &blockchain.km);

    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (chain_block, _coinbase_b) = blockchain.create_next_tip(block_spec!("B",parent: "A", transactions: txs));
    let (mut block, mmr_roots) = blockchain
        .db()
        .calculate_mmr_roots(chain_block.block().clone())
        .unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.block_output_mr = mmr_roots.block_output_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;

    let txn = blockchain.db().db_read_access().unwrap();
    let start = Instant::now();
    assert!(validator.validate_body(&*txn, &block).is_ok());
    let finished = start.elapsed();
    // this here here for benchmarking purposes.
    // we can extrapolate full block validation by multiplying the time by 4.6, this we get from the max_weight /weight
    // of the block
    println!("finished validating in: {}", finished.as_millis());
}

#[tokio::test]
async fn it_validates_when_a_coinbase_is_spent() {
    // we use this test to benchmark a block with multiple outputs
    let (mut blockchain, validator) = setup(false);
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![9000 * uT]);
    let (txs, _outputs) = schema_to_transaction(&[schema1], &blockchain.km);

    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (chain_block, _coinbase_b) = blockchain.create_next_tip(block_spec!("B",parent: "A", transactions: txs));
    let (mut block, mmr_roots) = blockchain
        .db()
        .calculate_mmr_roots(chain_block.block().clone())
        .unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.block_output_mr = mmr_roots.block_output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;

    let txn = blockchain.db().db_read_access().unwrap();
    assert!(validator.validate_body(&*txn, &block).is_ok());
}

#[tokio::test]
async fn it_passes_if_large_block_is_valid() {
    // we use this test to benchmark a block with multiple inputs and outputs
    let (mut blockchain, validator) = setup(false);
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T, 5 * T]);
    let (txs, outputs) = schema_to_transaction(&[schema1], &blockchain.km);

    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (_block, _coinbase_b) = blockchain
        .append(block_spec!("B", parent: "A", transactions: txs))
        .unwrap();

    let mut schemas = Vec::new();
    for output in outputs {
        let new_schema = txn_schema!(from: vec![output.clone()], to: vec![1 * T, 1 * T, 1 * T, 1 * T]);
        schemas.push(new_schema);
    }
    let (txs, _) = schema_to_transaction(&schemas, &blockchain.km);

    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (chain_block, _coinbase_c) = blockchain.create_next_tip(block_spec!("C",parent: "B", transactions: txs));
    let (mut block, mmr_roots) = blockchain
        .db()
        .calculate_mmr_roots(chain_block.block().clone())
        .unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.block_output_mr = mmr_roots.block_output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;

    let txn = blockchain.db().db_read_access().unwrap();
    let start = Instant::now();
    validator.validate_body(&*txn, &block).unwrap();
    // assert!(validator.validate_body(&*txn, &block).is_ok());
    let finished = start.elapsed();
    // this here here for benchmarking purposes.
    // we can extrapolate full block validation by multiplying the time by 32.9, this we get from the max_weight /weight
    // of the block
    println!("finished validating in: {}", finished.as_millis());
}

#[tokio::test]
async fn it_passes_if_block_is_valid() {
    let (mut blockchain, validator) = setup(true);

    let (chain_block, _) = blockchain.create_next_tip(BlockSpec::default());

    let (mut block, mmr_roots) = blockchain
        .db()
        .calculate_mmr_roots(chain_block.block().clone())
        .unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.block_output_mr = mmr_roots.block_output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;

    let txn = blockchain.db().db_read_access().unwrap();
    assert!(validator.validate_body(&*txn, &block).is_ok());
}

#[tokio::test]
async fn it_checks_the_coinbase_reward() {
    let (mut blockchain, validator) = setup(true);

    let (block, _) = blockchain.create_chained_block(block_spec!("A", parent: "GB", reward: 10 * T, ));
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    println!("err {err:?}");
    assert!(matches!(
        err,
        ValidationError::BlockError(BlockValidationError::TransactionError(
            TransactionError::InvalidCoinbase
        ))
    ));
}

#[tokio::test]
async fn it_allows_multiple_coinbases() {
    let (mut blockchain, validator) = setup(true);

    let (mut block, coinbase) = blockchain.create_unmined_block(block_spec!("A1", parent: "GB"));
    let commitment_mask_key = blockchain.km.get_random_key(None, None).unwrap();
    let wallet_payment_address = TariAddress::default();
    let (_, coinbase_output) = CoinbaseBuilder::new(blockchain.km.clone())
        .with_block_height(1)
        .with_fees(0.into())
        .with_commitment_mask_id(commitment_mask_key.key_id.clone())
        .with_encryption_key_id(TariKeyId::default())
        .with_sender_offset_key_id(TariKeyId::default())
        .with_script_key_id(TariKeyId::default())
        .with_script(push_pubkey_script(wallet_payment_address.public_spend_key()))
        .with_range_proof_type(RangeProofType::RevealedValue)
        .build_with_reward(
            blockchain.rules().consensus_constants(1),
            coinbase.value(),
            MemoField::new_empty(),
        )
        .unwrap();

    block.body.add_output(coinbase_output.to_transaction_output().unwrap());
    block.body.sort();

    let (block, _) = blockchain.create_unmined_block(block_spec!("A2", parent: "GB", skip_coinbase: true,));
    let block = blockchain.mine_block("GB", block, Difficulty::min());
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::BlockError(BlockValidationError::TransactionError(TransactionError::NoCoinbase))
    ));
}

#[tokio::test]
async fn it_checks_duplicate_kernel() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let (txs, _) = schema_to_transaction(
        &[txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T])],
        &blockchain.km,
    );

    blockchain
        .add_next_tip(block_spec!("1", transactions: txs.iter().map(|t| (**t).clone()).collect()))
        .unwrap();
    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(txs.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(err, ValidationError::DuplicateKernelError(_)));
}

#[tokio::test]
async fn it_checks_double_spends() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let (txs, _) = schema_to_transaction(
        &[txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T])],
        &blockchain.km,
    );

    blockchain
        .add_next_tip(block_spec!("1", transactions: txs.iter().map(|t| (**t).clone()).collect()))
        .unwrap();
    // lets create a new transction from the same input
    let (txs2, _) = schema_to_transaction(
        &[txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T])],
        &blockchain.km,
    );
    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(txs2.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(err, ValidationError::ContainsSTxO));
}

#[tokio::test]
async fn it_checks_input_maturity() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let mut schema = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T]);
    let mut features = schema.from[0].features().clone();
    features.maturity = 100;
    schema.from[0].set_features(features);
    let (txs, _) = schema_to_transaction(&[schema], &blockchain.km);

    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(txs.iter().map(|t| (**t).clone()).collect())
            .finish(),
    );
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::TransactionError(TransactionError::InputMaturity)
    ));
    unpack_enum!(ValidationError::TransactionError(TransactionError::InputMaturity) = err);
}

#[tokio::test]
async fn it_checks_txo_sort_order() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block(block_spec!("B->A", transactions: txs));
    let outputs = block.body.outputs().iter().rev().cloned().collect::<Vec<_>>();
    let inputs = block.body.inputs().clone();
    let kernels = block.body.kernels().clone();
    block.body = AggregateBody::new_sorted_unchecked(inputs, outputs, kernels);
    let block = blockchain.mine_block("A", block, Difficulty::min());

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
    ));
}

#[tokio::test]
async fn it_limits_the_script_byte_size() {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .with_max_script_byte_size(2)
                .build(),
        )
        .build()
        .unwrap();
    let (mut blockchain, validator) = setup_with_rules(rules, true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let mut schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    schema1.script = script!(Nop Nop Nop).unwrap();
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::TariScriptExceedsMaxSize { .. })
    ));
}

#[tokio::test]
async fn it_limits_the_encrypted_data_byte_size() {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build()
        .unwrap();
    let (mut blockchain, validator) = setup_with_rules(rules, true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let mut schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    schema1.script = script!(Nop Nop Nop).unwrap();
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let mut txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let mut outputs = txs[0].body.outputs().clone();
    outputs[0].encrypted_data = EncryptedData::from_bytes(&vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL + 250]).unwrap();
    txs[0].body = AggregateBody::new(txs[0].body.inputs().clone(), outputs, txs[0].body.kernels().clone());
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(
            AggregatedBodyValidationError::EncryptedDataExceedsMaxSize { .. }
        )
    ));
}

#[tokio::test]
async fn it_rejects_invalid_input_metadata() {
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build()
        .unwrap();
    let (mut blockchain, validator) = setup_with_rules(rules.clone(), true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let mut schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    schema1.from[0].set_sender_offset_public_key(Default::default());
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block()).unwrap_err();
    assert!(matches!(err, ValidationError::UnknownInputs(_)));
}

#[tokio::test]
async fn it_rejects_zero_conf_double_spends() {
    let (mut blockchain, validator) = setup(true);
    let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

    let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
    let (initial_tx, outputs) = schema_to_transaction(&[schema], &blockchain.km);

    let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![200 * T]);
    let (first_spend, _) = schema_to_transaction(&[schema], &blockchain.km);

    let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![150 * T]);
    let (double_spend, _) = schema_to_transaction(&[schema], &blockchain.km);

    let transactions = initial_tx
        .into_iter()
        .chain(first_spend)
        .chain(double_spend)
        .map(|b| Arc::try_unwrap(b).unwrap())
        .collect::<Vec<_>>();

    let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, &unmined).unwrap_err();
    // `verify_no_duplicated_inputs_outputs` rejects this first, via `AggregateBody::contains_duplicated_inputs`.
    // It previously did not: `contains_duplicated_inputs` compares with `==`, and `TransactionInput::eq` used to
    // include the script signature and input data, so the two competing spends of the same output - which differ in
    // exactly those fields - compared as unequal and slipped past it. The body was still rejected, but only further
    // on by the internal validator's sortedness check, as
    // `AggregatedBodyValidationError::UnsortedOrDuplicateInput`.
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput));
}

mod body_only {
    use super::*;

    #[tokio::test]
    async fn it_rejects_invalid_input_metadata() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyFullValidator::new(rules, true);

        let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

        let mut schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
        schema1.from[0].set_sender_offset_public_key(Default::default());
        let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
        let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
        let (block, _) = blockchain.create_next_tip(BlockSpec::new().with_transactions(txs).finish());

        let metadata = blockchain.db().get_chain_metadata().unwrap();

        let db = blockchain.db().db_read_access().unwrap();
        let err = validator.validate(&*db, block.block(), Some(&metadata)).unwrap_err();
        assert!(matches!(err, ValidationError::UnknownInputs(_)));
    }
}

mod orphan_validator {
    use tari_transaction_components::{transaction_components::OutputType, txn_schema};

    use super::*;
    use crate::validation::block_body::BlockBodyInternalConsistencyValidator;

    #[tokio::test]
    async fn it_rejects_zero_conf_double_spends() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
        let (initial_tx, outputs) = schema_to_transaction(&[schema], &blockchain.km);

        let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![200 * T]);
        let (first_spend, _) = schema_to_transaction(&[schema], &blockchain.km);

        let schema = txn_schema!(from: vec![outputs[0].clone()], to: vec![150 * T]);
        let (double_spend, _) = schema_to_transaction(&[schema], &blockchain.km);

        let transactions = initial_tx
            .into_iter()
            .chain(first_spend)
            .chain(double_spend)
            .map(|b| Arc::try_unwrap(b).unwrap())
            .collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        let err = validator.validate(&unmined).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::UnsortedOrDuplicateInput)
        ));
    }

    #[tokio::test]
    async fn it_rejects_unpermitted_output_types() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_permitted_output_types(vec![OutputType::Coinbase])
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
        let (tx, _) = schema_to_transaction(&[schema], &blockchain.km);

        let transactions = tx.into_iter().map(|b| Arc::try_unwrap(b).unwrap()).collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        let err = validator.validate(&unmined).unwrap_err();
        unpack_enum!(ValidationError::AggregatedBodyValidationError(err) = err);
        unpack_enum!(AggregatedBodyValidationError::OutputTypeNotPermitted { output_type } = err);
        assert_eq!(output_type, OutputType::Standard);
    }

    #[tokio::test]
    async fn it_rejects_unpermitted_range_proof_types() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_permitted_range_proof_types(vec![
                        (OutputType::Standard, vec![RangeProofType::RevealedValue]),
                        (OutputType::Coinbase, vec![RangeProofType::RevealedValue]),
                        (OutputType::Burn, vec![RangeProofType::RevealedValue]),
                        (OutputType::ValidatorNodeRegistration, vec![
                            RangeProofType::RevealedValue,
                        ]),
                        (OutputType::CodeTemplateRegistration, vec![
                            RangeProofType::RevealedValue,
                        ]),
                    ])
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
        let (tx, _) = schema_to_transaction(&[schema], &blockchain.km);

        let transactions = tx.into_iter().map(|b| Arc::try_unwrap(b).unwrap()).collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        let err = validator.validate(&unmined).unwrap_err();
        unpack_enum!(ValidationError::AggregatedBodyValidationError(err) = err);
        unpack_enum!(AggregatedBodyValidationError::RangeProofTypeNotPermitted { range_proof_type } = err);
        assert_eq!(range_proof_type, RangeProofType::BulletProofPlus);
    }

    #[tokio::test]
    async fn it_accepts_permitted_range_proof_types() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_permitted_range_proof_types(vec![
                        (OutputType::Standard, vec![RangeProofType::BulletProofPlus]),
                        (OutputType::Coinbase, vec![RangeProofType::BulletProofPlus]),
                        (OutputType::Burn, vec![RangeProofType::BulletProofPlus]),
                        (OutputType::ValidatorNodeRegistration, vec![
                            RangeProofType::BulletProofPlus,
                        ]),
                        (OutputType::CodeTemplateRegistration, vec![
                            RangeProofType::BulletProofPlus,
                        ]),
                    ])
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
        let (tx, _) = schema_to_transaction(&[schema], &blockchain.km);

        let transactions = tx.into_iter().map(|b| Arc::try_unwrap(b).unwrap()).collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        assert!(validator.validate(&unmined).is_ok());
    }

    #[tokio::test]
    async fn it_rejects_when_output_types_are_not_matched() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
            .add_consensus_constants(
                ConsensusConstantsBuilder::new(Network::LocalNet)
                    .with_permitted_range_proof_types(vec![(OutputType::CodeTemplateRegistration, vec![
                        RangeProofType::BulletProofPlus,
                    ])])
                    .with_coinbase_lockheight(0)
                    .build(),
            )
            .build()
            .unwrap();
        let mut blockchain = TestBlockchain::create(rules.clone());
        let validator = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default());
        let (_, coinbase) = blockchain.append(block_spec!("1", parent: "GB")).unwrap();

        let schema = txn_schema!(from: vec![coinbase.clone()], to: vec![201 * T]);
        let (tx, _) = schema_to_transaction(&[schema], &blockchain.km);

        let transactions = tx.into_iter().map(|b| Arc::try_unwrap(b).unwrap()).collect::<Vec<_>>();

        let (unmined, _) = blockchain.create_unmined_block(block_spec!("2", parent: "1", transactions: transactions));
        let err = validator.validate(&unmined).unwrap_err();
        unpack_enum!(ValidationError::AggregatedBodyValidationError(err) = err);
        unpack_enum!(AggregatedBodyValidationError::OutputTypeNotMatchedToRangeProofType { output_type } = err);
        assert!(output_type == OutputType::Standard || output_type == OutputType::Coinbase);
    }
}

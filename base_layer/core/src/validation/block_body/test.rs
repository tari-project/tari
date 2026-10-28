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
use tari_common_types::{
    payment_reference::generate_payment_reference,
    tari_address::TariAddress,
    types::{CompressedPublicKey, FixedHash, PrivateKey, RangeProof},
};
use tari_node_components::blocks::{Block, BlockValidationError, ChainBlock};
use tari_script::{
    ExecutionStack,
    MAX_SCRIPT_OPCODES,
    MAX_STACK_SIZE,
    Opcode,
    StackItem,
    TariScript,
    inputs,
    push_pubkey_script,
    script,
};
use tari_test_utils::unpack_enum;
use tari_transaction_components::{
    CoinbaseBuilder,
    MicroMinotari,
    TransactionBuilder,
    aggregated_body::AggregateBody,
    consensus::{
        ConsensusConstants,
        ConsensusConstantsBuilder,
        ConsensusManager,
        consensus_constants::MAX_BLOCK_BODY_BYTES,
    },
    crypto_factories::CryptoFactories,
    fee::recipient_output_features_and_scripts_size,
    helpers::borsh::SerializedSize,
    key_manager::{
        KeyManager,
        SecretTransactionKeyManagerInterface,
        TariKeyAndId,
        TariKeyId,
        TransactionKeyManagerInterface,
    },
    tari_amount::{T, uT},
    tari_proof_of_work::Difficulty,
    test_helpers::schema_to_transaction,
    transaction_builder::PendingOutput,
    transaction_components::{
        EncryptedData,
        MemoField,
        OutputFeatures,
        RangeProofType,
        SpentOutput,
        Transaction,
        TransactionError,
        TransactionInput,
        WalletOutput,
        WalletOutputBuilder,
        covenants::Covenant,
        encrypted_data::STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
    },
    txn_schema,
    validation::AggregatedBodyValidationError,
};
use tokio::time::Instant;

use super::{
    BlockBodyFullValidator,
    BlockBodyInternalConsistencyValidator,
    block_body_partial_validator::BlockBodyPartialValidator,
};
use crate::{
    block_spec,
    chain_storage::{
        BlockchainBackend,
        BlockchainDatabaseConfig,
        ChainStorageError,
        DbTransaction,
        MinedInfo,
        Validators,
    },
    consensus::BaseNodeConsensusManager,
    mempool::{Mempool, MempoolConfig, TxStorageResponse},
    test_helpers::{
        BlockSpec,
        blockchain::{TestBlockchain, create_store_with_consensus_and_validators_and_config},
    },
    validation::{
        BlockBodyValidator,
        ValidationError,
        aggregate_body::AggregateBodyChainLinkedValidator,
        helpers::check_input_is_utxo,
        mocks::MockValidator,
        transaction::TransactionFullValidator,
    },
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
    assert!(validator.validate_body(&*txn, block.clone()).is_ok());
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
    assert!(validator.validate_body(&*txn, block.clone()).is_ok());
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
    validator.validate_body(&*txn, block.clone()).unwrap();
    // assert!(validator.validate_body(&*txn, block.clone()).is_ok());
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
    assert!(validator.validate_body(&*txn, block.clone()).is_ok());
}

#[tokio::test]
async fn it_checks_the_coinbase_reward() {
    let (mut blockchain, validator) = setup(true);

    let (block, _) = blockchain.create_chained_block(block_spec!("A", parent: "GB", reward: 10 * T, ));
    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    let sender_offset_key = blockchain.km.get_random_key(None, None).unwrap();
    let wallet_payment_address = TariAddress::default();
    let (_, coinbase_output) = CoinbaseBuilder::new(blockchain.km.clone())
        .with_block_height(1)
        .with_fees(0.into())
        .with_commitment_mask_id(commitment_mask_key.key_id.clone())
        .with_encryption_key_id(blockchain.km.get_random_key(None, None).unwrap().key_id)
        .with_sender_offset_key_id(sender_offset_key.key_id)
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(err, ValidationError::ContainsSTxO));
}

#[tokio::test]
async fn it_checks_double_spends_next_to_a_recreated_output() {
    // A body that re-creates an output with history is still a double spend if it spends a different spent output
    let (mut chain, ..) = duplicate_output_chain();
    let [mined, spent] = ["3", "4"].map(|name| chain.get_block_and_smt_by_name(name).unwrap());
    let spent_input = chain.get_block_and_smt_by_name("2").unwrap().block().body.inputs()[0].clone();
    let recreated_input = spent
        .block()
        .body
        .inputs()
        .iter()
        .find(|input| {
            mined
                .block()
                .body
                .outputs()
                .iter()
                .any(|o| o.hash() == input.output_hash())
        })
        .unwrap()
        .clone();
    let recreated = mined
        .block()
        .body
        .outputs()
        .iter()
        .find(|o| o.hash() == recreated_input.output_hash())
        .unwrap()
        .clone();
    let next_header = chain.create_next_tip(BlockSpec::new()).0.header().clone();
    let validator = AggregateBodyChainLinkedValidator::new(chain.rules().clone());
    let tip = chain.db().fetch_tip_header().unwrap();
    let db = chain.db().db_read_access().unwrap();
    // The same spent input repeated is rejected as a duplicate before any input is looked up
    let body = AggregateBody::new_unsorted(vec![spent_input.clone(); 3], vec![], vec![]);
    let err = validator.validate(&body, &next_header, &*db).unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput), "{err:?}");
    let err = validator
        .validate_transaction_body(&body, tip.header(), &*db)
        .unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput), "{err:?}");
    for inputs in [vec![spent_input.clone()], vec![recreated_input, spent_input.clone()]] {
        let body = AggregateBody::new_unsorted(inputs, vec![recreated.clone()], vec![]);
        let err = validator.validate(&body, &next_header, &*db).unwrap_err();
        assert!(matches!(err, ValidationError::ContainsSTxO), "{err:?}");
        // A transaction reports the input instead, since the mempool may create its output again
        let err = validator
            .validate_transaction_body(&body, tip.header(), &*db)
            .unwrap_err();
        assert!(
            matches!(&err, ValidationError::SpentInputs { unknown, spent } if unknown.is_empty() && *spent == vec![spent_input.output_hash()]),
            "{err:?}"
        );
    }
}

// A consensus-valid chain with output O mined in "2", spent in "3" and mined again in "4". The first returned
// transaction spends O a second time; the second one mines O a third time, once the first is in a block.
pub(crate) fn duplicate_output_chain() -> (TestBlockchain, BlockBodyFullValidator, Transaction, Transaction) {
    let (blockchain, validator, spend_tx, remine_tx, _) = duplicate_output_chain_with_o2_spends();
    (blockchain, validator, spend_tx, remine_tx)
}

// As `duplicate_output_chain`, where output O2 is mined in "3" and spent in "4". Also returns a transaction that mines
// O2 again and spends it in the same body, and one that only spends O2 again.
#[allow(clippy::too_many_lines, clippy::arithmetic_side_effects)]
fn duplicate_output_chain_with_o2_spends() -> (
    TestBlockchain,
    BlockBodyFullValidator,
    Transaction,
    Transaction,
    (Transaction, Transaction),
) {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let (txs, outputs) = schema_to_transaction(
        &[txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 50 * T])],
        &blockchain.km,
    );

    blockchain
        .add_next_tip(block_spec!("1", transactions: txs.iter().map(|t| (**t).clone()).collect()))
        .unwrap();
    // lets create our first tx

    let constants = ConsensusManager::builder(Network::LocalNet)
        .build()
        .consensus_constants(0)
        .clone();
    // Build a `Nop` output around a sender offset key reserved from `tx_builder`. Every transaction reserves its
    // keys in one call, which is what folds its input script keys into the script offset, so the output cannot be
    // built until the reservation has happened.
    fn build_output(
        km: &KeyManager,
        constants: &ConsensusConstants,
        tx_builder: &mut TransactionBuilder<KeyManager>,
        value: MicroMinotari,
    ) -> (WalletOutput, TariKeyAndId) {
        // The output does not exist yet, so it is declared from the shape it will have; `build` checks the
        // declaration against what actually arrives.
        let pending = PendingOutput::measured(
            constants.transaction_weight_params(),
            value,
            &OutputFeatures::default(),
            &script![Nop].unwrap(),
            &Covenant::default(),
            &MemoField::new_empty(),
        )
        .unwrap();
        let sender_offset = tx_builder
            .reserve_sender_offset_keys(&[pending])
            .unwrap()
            .pop()
            .unwrap();
        let commitment_mask = km.get_random_key(None, None).unwrap();
        let script_key_id = TariKeyId::Derived {
            key: (&commitment_mask.key_id).into(),
        };
        let script_public_key = km.get_public_key_at_key_id(&script_key_id).unwrap();
        let output = WalletOutputBuilder::new(value, commitment_mask.key_id)
            .with_script(script![Nop].unwrap())
            .encrypt_data_for_recovery(km, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(inputs!(script_public_key))
            .with_sender_offset_public_key(sender_offset.pub_key.clone())
            .with_script_key(script_key_id)
            .sign_metadata_signature(km, &sender_offset.key_id)
            .unwrap()
            .try_build(km)
            .unwrap();
        (output, sender_offset)
    }

    // Re-publishing an output another transaction already built means publishing a sender offset key this builder
    // never reserved, so what it contributes to the script offset has to be registered by hand. That is legitimate
    // here because the key is one this same wallet made and still holds - it is not a device secret.
    // Ristretto scalar arithmetic, not integer arithmetic: this cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn republish_output(
        km: &KeyManager,
        tx_builder: &mut TransactionBuilder<KeyManager>,
        output: &WalletOutput,
        sender_offset: &TariKeyAndId,
    ) {
        let negated = PrivateKey::default() - km.get_private_key(&sender_offset.key_id).unwrap();
        tx_builder.with_host_derived_partial_script_offset(negated);
        // This one already exists, so it can be declared from the output itself.
        let pending = PendingOutput::from_output(output).unwrap();
        tx_builder
            .reserve_sender_offset_keys(&[PendingOutput::custom_sender_offset(
                pending.value(),
                pending.features_and_scripts_size(),
            )])
            .unwrap();
        tx_builder
            .with_output(output.clone(), sender_offset.key_id.clone(), None)
            .unwrap();
    }

    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(outputs[0].clone()).unwrap();
    tx_builder.with_fee(100.into());
    let (output, output_sender_offset) = build_output(
        &blockchain.km,
        &constants,
        &mut tx_builder,
        outputs[0].value() - MicroMinotari(200),
    );
    tx_builder
        .with_output(output.clone(), output_sender_offset.key_id.clone(), None)
        .unwrap();
    let finalized_tx = tx_builder.build().unwrap();
    blockchain
        .add_next_tip(block_spec!("2", transactions: vec![finalized_tx.transaction]))
        .unwrap();

    // lets create our second tx
    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(output.clone()).unwrap();
    tx_builder.with_fee(100.into());
    let (output_2, output_2_sender_offset) = build_output(
        &blockchain.km,
        &constants,
        &mut tx_builder,
        output.value() - MicroMinotari(200),
    );
    tx_builder
        .with_output(output_2.clone(), output_2_sender_offset.key_id.clone(), None)
        .unwrap();
    let finalized_tx = tx_builder.build().unwrap();
    blockchain
        .add_next_tip(block_spec!("3", transactions: vec![finalized_tx.transaction.clone()]))
        .unwrap();

    // lets create duplicate commitment tx and spend output2
    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(outputs[1].clone()).unwrap();
    tx_builder.with_fee(100.into());
    republish_output(&blockchain.km, &mut tx_builder, &output, &output_sender_offset);
    let finalized_tx1 = tx_builder.build().unwrap();

    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(output_2.clone()).unwrap();
    tx_builder.with_fee(100.into());
    let (output_3, output_3_sender_offset) = build_output(
        &blockchain.km,
        &constants,
        &mut tx_builder,
        output_2.value() - MicroMinotari(200),
    );
    tx_builder
        .with_output(output_3.clone(), output_3_sender_offset.key_id.clone(), None)
        .unwrap();
    let (block, _) = blockchain.create_next_tip(
        BlockSpec::new()
            .with_transactions(vec![finalized_tx1.transaction.clone()])
            .finish(),
    );
    {
        let txn = blockchain.db().db_read_access().unwrap();
        validator.validate_body(&*txn, block.block().clone()).unwrap();
    }
    let finalized_tx2 = tx_builder.build().unwrap();
    let (_, coinbase_4) = blockchain
        .add_next_tip(
            block_spec!("4", transactions: vec![finalized_tx1.transaction.clone(), finalized_tx2.transaction]),
        )
        .unwrap();

    // lets create our 4th tx
    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(output.clone()).unwrap();
    tx_builder.with_input(outputs[2].clone()).unwrap();
    tx_builder.with_fee(100.into());
    republish_output(&blockchain.km, &mut tx_builder, &output_2, &output_2_sender_offset);
    let finalized_tx = tx_builder.build().unwrap();

    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(output_2.clone()).unwrap();
    tx_builder.with_input(outputs[2].clone()).unwrap();
    tx_builder.with_fee(100.into());
    republish_output(&blockchain.km, &mut tx_builder, &output_2, &output_2_sender_offset);
    let recreate_and_spend_tx = tx_builder.build().unwrap();

    let mut tx_builder = TransactionBuilder::new(constants.clone(), blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(output_2.clone()).unwrap();
    tx_builder.with_fee(100.into());
    let (output_4, output_4_sender_offset) = build_output(
        &blockchain.km,
        &constants,
        &mut tx_builder,
        output_2.value() - MicroMinotari(200),
    );
    tx_builder
        .with_output(output_4, output_4_sender_offset.key_id.clone(), None)
        .unwrap();
    let plain_spend_tx = tx_builder.build().unwrap();

    let mut tx_builder = TransactionBuilder::new(constants, blockchain.km.clone(), Network::LocalNet).unwrap();
    tx_builder.with_input(coinbase_4).unwrap();
    tx_builder.with_fee(100.into());
    republish_output(&blockchain.km, &mut tx_builder, &output, &output_sender_offset);
    let remine_tx = tx_builder.build().unwrap();

    (
        blockchain,
        validator,
        finalized_tx.transaction,
        remine_tx.transaction,
        (recreate_and_spend_tx.transaction, plain_spend_tx.transaction),
    )
}

#[tokio::test]
async fn allow_duplicate_outputs() {
    let (mut blockchain, validator, transaction, _) = duplicate_output_chain();
    let (block, _) = blockchain.create_next_tip(block_spec!("5", transactions: vec![transaction.clone()]));
    {
        let txn = blockchain.db().db_read_access().unwrap();
        validator.validate_body(&*txn, block.block().clone()).unwrap();
    }
    blockchain
        .add_next_tip(block_spec!("5", transactions: vec![transaction]))
        .unwrap();
}

fn summarize_lifecycles(infos: Vec<MinedInfo>) -> Vec<(Option<u64>, Option<u64>)> {
    infos
        .into_iter()
        .map(|i| (i.output.map(|o| o.mined_height), i.input.map(|i| i.spent_height)))
        .collect()
}

fn is_spent_in_block(chain: &TestBlockchain, block: &ChainBlock, hash: FixedHash, at: &ChainBlock) -> bool {
    chain
        .db()
        .fetch_outputs_in_block_with_spend_state(*block.hash(), Some(*at.hash()))
        .unwrap()
        .into_iter()
        .find(|(o, _)| o.hash() == hash)
        .unwrap()
        .1
}

#[tokio::test]
async fn duplicate_output_lifecycle_pruning() {
    let (mut chain, validator, transaction, _) = duplicate_output_chain();
    let mined = chain.get_block_and_smt_by_name("2").unwrap();
    let spent = chain.get_block_and_smt_by_name("3").unwrap();
    let remined = chain.get_block_and_smt_by_name("4").unwrap();
    let hash = spent.block().body.inputs()[0].output_hash();
    let m1 = mined.height();
    let s1 = spent.height();
    let m2 = remined.height();
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![(Some(m1), Some(s1)), (Some(m2), None)]
    );
    assert!(!is_spent_in_block(&chain, &remined, hash, &remined));
    BlockBodyPartialValidator::new(chain.rules().clone())
        .validate(&*chain.db().db_read_access().unwrap(), spent.block())
        .unwrap();
    let output = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output;

    let mut txn = DbTransaction::new();
    txn.prune_outputs_spent_at_hash(*spent.hash());
    chain.db().write(txn).unwrap();
    let surviving = chain.db().fetch_outputs(hash).unwrap();
    assert_eq!(surviving.len(), 1);
    assert_eq!(surviving[0].mined_height, m2);
    assert_eq!(
        chain
            .db()
            .fetch_unspent_output_hash_by_commitment(output.commitment.clone())
            .unwrap(),
        Some(hash)
    );
    assert_eq!(
        chain
            .db()
            .fetch_mined_info_by_payref(generate_payment_reference(remined.hash(), &hash))
            .unwrap()
            .output
            .unwrap()
            .mined_height,
        m2
    );
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![(Some(m2), None)]
    );

    let (block, _) = chain.create_next_tip(block_spec!("5", transactions: vec![transaction.clone()]));
    validator
        .validate_body(&*chain.db().db_read_access().unwrap(), block.block().to_compact())
        .unwrap();
    let (spent_again, _) = chain
        .add_next_tip(block_spec!("5", transactions: vec![transaction]))
        .unwrap();
    assert!(is_spent_in_block(&chain, &remined, hash, &spent_again));
}

#[tokio::test]
async fn duplicate_output_prune_reports_missing_row() {
    let (chain, ..) = duplicate_output_chain();
    let mined = chain.get_block_and_smt_by_name("2").unwrap();
    let spent = chain.get_block_and_smt_by_name("3").unwrap();
    let hash = spent.block().body.inputs()[0].output_hash();
    chain
        .db()
        .db_read_access()
        .unwrap()
        .delete_output_row_for_test(mined.hash(), &hash);

    let mut txn = DbTransaction::new();
    txn.prune_outputs_spent_at_hash(*spent.hash());
    let err = chain.db().write(txn).unwrap_err();
    assert!(matches!(err, ChainStorageError::ValueNotFound { .. }), "{err:?}");
}

#[tokio::test]
async fn duplicate_output_partial_validation_checks_spent_instance() {
    let (mut chain, _, spend_tx, _) = duplicate_output_chain();
    chain
        .add_next_tip(block_spec!("5", transactions: vec![spend_tx]))
        .unwrap();
    let [s1, m2, s2] = ["3", "4", "5"].map(|name| chain.get_block_and_smt_by_name(name).unwrap());
    let hash = s1.block().body.inputs()[0].output_hash();
    let validator = BlockBodyPartialValidator::new(chain.rules().clone());

    // The earlier instance alone must not satisfy a spend of the later one.
    chain
        .db()
        .db_read_access()
        .unwrap()
        .delete_output_row_for_test(m2.hash(), &hash);
    validator
        .validate(&*chain.db().db_read_access().unwrap(), s1.block())
        .unwrap();
    let err = validator
        .validate(&*chain.db().db_read_access().unwrap(), s2.block())
        .unwrap_err();
    assert!(matches!(err, ValidationError::MissingOutputError(_)), "{err:?}");
}

#[tokio::test]
async fn duplicate_output_third_lifecycle() {
    let (mut chain, _, spend_tx, remine_tx) = duplicate_output_chain();
    chain
        .add_next_tip(block_spec!("5", transactions: vec![spend_tx]))
        .unwrap();
    chain
        .add_next_tip(block_spec!("6", transactions: vec![remine_tx]))
        .unwrap();
    let [m1, s1, m2, s2, m3] = ["2", "3", "4", "5", "6"].map(|name| chain.get_block_and_smt_by_name(name).unwrap());
    let hash = s1.block().body.inputs()[0].output_hash();
    let output = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output;

    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![
            (Some(m1.height()), Some(s1.height())),
            (Some(m2.height()), Some(s2.height())),
            (Some(m3.height()), None)
        ]
    );
    assert!(!is_spent_in_block(&chain, &m2, hash, &s1));
    assert!(is_spent_in_block(&chain, &m2, hash, &m3));
    assert!(!is_spent_in_block(&chain, &m3, hash, &m3));
    let latest = MinedInfo::latest(chain.db().fetch_mined_info_by_output_hash(hash).unwrap());
    assert_eq!(latest.output.unwrap().mined_height, m3.height());
    assert!(latest.input.is_none());
    let validator = BlockBodyPartialValidator::new(chain.rules().clone());
    for block in [&s1, &s2] {
        validator
            .validate(&*chain.db().db_read_access().unwrap(), block.block())
            .unwrap();
    }

    for block in [&s1, &s2] {
        let mut txn = DbTransaction::new();
        txn.prune_outputs_spent_at_hash(*block.hash());
        chain.db().write(txn).unwrap();
    }
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![(Some(m3.height()), None)]
    );
    assert!(!is_spent_in_block(&chain, &m3, hash, &m3));
    assert_eq!(
        chain
            .db()
            .fetch_unspent_output_hash_by_commitment(output.commitment)
            .unwrap(),
        Some(hash)
    );
}

#[tokio::test]
async fn duplicate_output_rewind_restores_single_lifecycle() {
    let (chain, _, _, _) = duplicate_output_chain();
    let mined = chain.get_block_and_smt_by_name("2").unwrap();
    let spent = chain.get_block_and_smt_by_name("3").unwrap();
    let hash = spent.block().body.inputs()[0].output_hash();
    let output = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output;

    chain.db().rewind_to_height(spent.height()).unwrap();
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![(Some(mined.height()), Some(spent.height()))]
    );
    assert!(is_spent_in_block(&chain, &mined, hash, &spent));
    BlockBodyPartialValidator::new(chain.rules().clone())
        .validate(&*chain.db().db_read_access().unwrap(), spent.block())
        .unwrap();
    assert_eq!(
        chain
            .db()
            .fetch_unspent_output_hash_by_commitment(output.commitment)
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn duplicate_output_pruned_node_accepts_second_spend() {
    let (mut chain, validator, transaction, _) = duplicate_output_chain();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let pruned = create_store_with_consensus_and_validators_and_config(
        chain.rules().clone(),
        validators,
        BlockchainDatabaseConfig {
            pruning_horizon: 1,
            pruning_interval: 1,
            ..Default::default()
        },
    );
    for name in ["A", "1", "2", "3", "4"] {
        let block = chain.get_block_and_smt_by_name(name).unwrap();
        assert!(pruned.add_block(block.to_arc_block()).unwrap().result.is_added());
    }
    let spent = chain.get_block_and_smt_by_name("3").unwrap();
    assert!(pruned.get_chain_metadata().unwrap().pruned_height() >= spent.height());

    let (block, _) = chain.create_next_tip(block_spec!("5", transactions: vec![transaction]));
    validator
        .validate_body(&*pruned.db_read_access().unwrap(), block.block().to_compact())
        .unwrap();
    assert!(pruned.add_block(block.to_arc_block()).unwrap().result.is_added());
}

// The hash of the output that `tx` both creates and spends
fn recreated_and_spent_hash(tx: &Transaction) -> FixedHash {
    tx.body
        .inputs()
        .iter()
        .map(|input| input.output_hash())
        .find(|hash| tx.body.outputs().iter().any(|output| output.hash() == *hash))
        .unwrap()
}

#[tokio::test]
async fn recreated_output_spent_in_the_same_body_is_valid_on_archival_and_pruned_nodes() {
    let (mut chain, validator, _, _, (recreate_and_spend_tx, _)) = duplicate_output_chain_with_o2_spends();
    chain.add_next_tip(block_spec!("5")).unwrap();
    chain.add_next_tip(block_spec!("6")).unwrap();
    let hash = recreated_and_spent_hash(&recreate_and_spend_tx);
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let pruned = create_store_with_consensus_and_validators_and_config(
        chain.rules().clone(),
        validators,
        BlockchainDatabaseConfig {
            pruning_horizon: 1,
            pruning_interval: 1,
            ..Default::default()
        },
    );
    for name in ["A", "1", "2", "3", "4", "5", "6"] {
        let block = chain.get_block_and_smt_by_name(name).unwrap();
        assert!(pruned.add_block(block.to_arc_block()).unwrap().result.is_added());
    }
    // The archival node still holds the spent instance of the output, the pruned node does not
    let spent_in = chain.get_block_and_smt_by_name("4").unwrap();
    assert!(pruned.get_chain_metadata().unwrap().pruned_height() >= spent_in.height());
    assert!(!chain.db().fetch_outputs(hash).unwrap().is_empty());
    assert!(pruned.fetch_outputs(hash).unwrap().is_empty());
    let input = recreate_and_spend_tx
        .body
        .inputs()
        .iter()
        .find(|input| input.output_hash() == hash)
        .unwrap();
    assert!(matches!(
        check_input_is_utxo(&*chain.db().db_read_access().unwrap(), input),
        Err(ValidationError::ContainsSTxO)
    ));
    assert!(matches!(
        check_input_is_utxo(&*pruned.db_read_access().unwrap(), input),
        Err(ValidationError::UnknownInput)
    ));

    let (block, _) = chain.create_next_tip(block_spec!("7", transactions: vec![recreate_and_spend_tx]));
    validator
        .validate_body(&*chain.db().db_read_access().unwrap(), block.block().to_compact())
        .unwrap();
    validator
        .validate_body(&*pruned.db_read_access().unwrap(), block.block().to_compact())
        .unwrap();
    assert!(chain.append_block("7", block.clone()).unwrap().is_added());
    assert!(pruned.add_block(block.to_arc_block()).unwrap().result.is_added());
}

#[tokio::test]
async fn recreated_output_spent_in_the_same_block_is_stored_on_an_archival_node() {
    let (mut chain, validator, _, _, (recreate_and_spend_tx, plain_spend_tx)) = duplicate_output_chain_with_o2_spends();
    let hash = recreated_and_spent_hash(&recreate_and_spend_tx);
    let [m1, s1] = ["3", "4"].map(|name| chain.get_block_and_smt_by_name(name).unwrap());
    let commitment = chain.db().fetch_outputs(hash).unwrap().pop().unwrap().output.commitment;
    let (z, _) = chain.create_next_tip(block_spec!("5", transactions: vec![recreate_and_spend_tx]));
    validator
        .validate_body(&*chain.db().db_read_access().unwrap(), z.block().to_compact())
        .unwrap();
    assert!(chain.append_block("5", z.clone()).unwrap().is_added());

    assert_eq!(chain.db().fetch_outputs(hash).unwrap().len(), 2);
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![
            (Some(m1.height()), Some(s1.height())),
            (Some(z.height()), Some(z.height()))
        ]
    );
    assert_eq!(
        chain
            .db()
            .fetch_unspent_output_hash_by_commitment(commitment.clone())
            .unwrap(),
        None
    );
    assert!(!is_spent_in_block(&chain, &m1, hash, &m1));
    assert!(is_spent_in_block(&chain, &m1, hash, &z));
    assert!(is_spent_in_block(&chain, &z, hash, &z));
    // Spending it once more without re-creating it is a double spend
    let (block, _) = chain.create_next_tip(block_spec!("6", transactions: vec![plain_spend_tx]));
    let err = validator
        .validate_body(&*chain.db().db_read_access().unwrap(), block.block().clone())
        .unwrap_err();
    assert!(matches!(err, ValidationError::ContainsSTxO), "{err:?}");

    // Rewinding Z leaves only the earlier lifecycle
    chain.db().rewind_to_height(s1.height()).unwrap();
    assert_eq!(
        summarize_lifecycles(chain.db().fetch_mined_info_by_output_hash(hash).unwrap()),
        vec![(Some(m1.height()), Some(s1.height()))]
    );
    assert_eq!(
        chain
            .db()
            .fetch_unspent_output_hash_by_commitment(commitment.clone())
            .unwrap(),
        None
    );
    assert!(is_spent_in_block(&chain, &m1, hash, &s1));
    // Re-applying Z and pruning both spends removes both instances
    assert!(chain.db().add_block(z.to_arc_block()).unwrap().result.is_added());
    for block in [&z, &s1] {
        let mut txn = DbTransaction::new();
        txn.prune_outputs_spent_at_hash(*block.hash());
        chain.db().write(txn).unwrap();
    }
    assert!(chain.db().fetch_outputs(hash).unwrap().is_empty());
    assert!(chain.db().fetch_mined_info_by_output_hash(hash).unwrap().is_empty());
    assert_eq!(
        chain.db().fetch_unspent_output_hash_by_commitment(commitment).unwrap(),
        None
    );
}

#[tokio::test]
async fn the_mempool_accepts_a_recreated_output_spent_in_the_same_transaction() {
    let (chain, _, _, _, (recreate_and_spend_tx, plain_spend_tx)) = duplicate_output_chain_with_o2_spends();
    let mempool = Mempool::new(
        MempoolConfig::default(),
        chain.rules().clone(),
        Box::new(TransactionFullValidator::new(
            CryptoFactories::default(),
            true,
            chain.db().clone(),
            chain.rules().clone(),
        )),
    );
    assert_eq!(
        mempool.insert(Arc::new(plain_spend_tx)).await.unwrap(),
        TxStorageResponse::NotStoredAlreadySpent
    );
    assert_eq!(
        mempool.insert(Arc::new(recreate_and_spend_tx)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
}

#[tokio::test]
async fn the_mempool_accepts_a_spend_of_a_recreated_output_in_the_pool_on_archival_and_pruned_nodes() {
    // T1 creates O2 again (it was mined in "3" and spent in "4") and T2 spends it
    let (mut chain, validator, recreate_tx, _, (_, spend_tx)) = duplicate_output_chain_with_o2_spends();
    chain.add_next_tip(block_spec!("5")).unwrap();
    chain.add_next_tip(block_spec!("6")).unwrap();
    let hash = spend_tx.body.inputs()[0].output_hash();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let pruned = create_store_with_consensus_and_validators_and_config(
        chain.rules().clone(),
        validators,
        BlockchainDatabaseConfig {
            pruning_horizon: 1,
            pruning_interval: 1,
            ..Default::default()
        },
    );
    for name in ["A", "1", "2", "3", "4", "5", "6"] {
        let block = chain.get_block_and_smt_by_name(name).unwrap();
        assert!(pruned.add_block(block.to_arc_block()).unwrap().result.is_added());
    }
    assert!(!chain.db().fetch_outputs(hash).unwrap().is_empty());
    assert!(pruned.fetch_outputs(hash).unwrap().is_empty());

    let new_mempool = |db| {
        let validator = TransactionFullValidator::new(CryptoFactories::default(), true, db, chain.rules().clone());
        Mempool::new(MempoolConfig::default(), chain.rules().clone(), Box::new(validator))
    };
    // Without its parent in the pool, the spend is still a double spend on the archival node
    assert_eq!(
        new_mempool(chain.db().clone())
            .insert(Arc::new(spend_tx.clone()))
            .await
            .unwrap(),
        TxStorageResponse::NotStoredAlreadySpent
    );
    for db in [chain.db().clone(), pruned] {
        let mempool = new_mempool(db);
        assert_eq!(
            mempool.insert(Arc::new(recreate_tx.clone())).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            mempool.insert(Arc::new(spend_tx.clone())).await.unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
    }

    // A block with both is valid
    let (block, _) = chain.create_next_tip(block_spec!("7", transactions: vec![recreate_tx, spend_tx]));
    validator
        .validate_body(&*chain.db().db_read_access().unwrap(), block.block().to_compact())
        .unwrap();
    assert!(chain.append_block("7", block).unwrap().is_added());
}

#[tokio::test]
async fn contains_output_hash_agrees_with_fetch_outputs() {
    let (mut chain, _, spend_tx, remine_tx) = duplicate_output_chain();
    chain
        .add_next_tip(block_spec!("5", transactions: vec![spend_tx]))
        .unwrap();
    chain
        .add_next_tip(block_spec!("6", transactions: vec![remine_tx]))
        .unwrap();
    let [spent_once_in, s1] = ["2", "3"].map(|name| chain.get_block_and_smt_by_name(name).unwrap());
    // Mined in "2", "4" and "6", the last one unspent
    let re_mined = s1.block().body.inputs()[0].output_hash();
    // Mined in "1" and spent in "2"
    let spent = spent_once_in.block().body.inputs()[0].output_hash();
    let never_seen = FixedHash::from([7u8; 32]);
    let agrees = |hash: FixedHash, expected: bool| {
        let db = chain.db().db_read_access().unwrap();
        assert_eq!(db.contains_output_hash(&hash).unwrap(), expected);
        assert_eq!(!db.fetch_outputs(&hash).unwrap().is_empty(), expected);
    };
    agrees(never_seen, false);
    agrees(re_mined, true);
    agrees(spent, true);

    // Pruning the only spend of `spent` removes it; pruning one spend of `re_mined` leaves its later instances
    for block in [&spent_once_in, &s1] {
        let mut txn = DbTransaction::new();
        txn.prune_outputs_spent_at_hash(*block.hash());
        chain.db().write(txn).unwrap();
    }
    agrees(spent, false);
    agrees(re_mined, true);
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
    ));
}

/// A commitment is what the UTXO set is keyed on, so a block may never create the same commitment twice. Two outputs
/// sharing a commitment are rejected even when nothing else about them matches, because the outputs are ordered on the
/// commitment alone.
#[tokio::test]
async fn it_rejects_a_block_with_two_outputs_sharing_a_commitment() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block(block_spec!("B->A", transactions: txs));
    let mut outputs = block.body.outputs().clone();
    // Twin a non-coinbase output. Outputs are ordered on the commitment, so which one sorts first is random; twinning
    // whichever lands at index 0 gives the block two coinbases whenever that is the coinbase, and the coinbase check
    // runs before the sort check, so the test would fail on `InvalidCoinbase` instead.
    let idx = outputs.iter().position(|o| !o.is_coinbase()).unwrap();
    // Same commitment, different script, so the two outputs hash differently. The hash-based duplicate check in the
    // chain validator cannot see this pair; only the commitment ordering can.
    let mut twin = outputs[idx].clone();
    twin.script = script!(Nop Nop).unwrap();
    assert_eq!(outputs[idx].commitment, twin.commitment);
    assert_ne!(outputs[idx].hash(), twin.hash());
    // Insert next to its twin so the body is still ordered by commitment; the duplicate is the only thing wrong.
    outputs.insert(idx + 1, twin);
    let inputs = block.body.inputs().clone();
    let kernels = block.body.kernels().clone();
    block.body = AggregateBody::new_sorted_unchecked(inputs, outputs, kernels);
    let block = blockchain.mine_block("A", block, Difficulty::min());

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
    ));
}

/// The same output repeated verbatim is caught earlier, by the hash-based duplicate check in the chain-linked
/// validator, which runs before the internal consistency validator.
#[tokio::test]
async fn it_rejects_a_block_with_a_repeated_output() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block(block_spec!("B->A", transactions: txs));
    let mut outputs = block.body.outputs().clone();
    outputs.insert(1, outputs[0].clone());
    let inputs = block.body.inputs().clone();
    let kernels = block.body.kernels().clone();
    block.body = AggregateBody::new_sorted_unchecked(inputs, outputs, kernels);
    let block = blockchain.mine_block("A", block, Difficulty::min());

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateOutput));
}

/// A block may not spend the same output twice, and swapping the script input data on the second input does not hide
/// it: inputs are deduplicated on the output they spend, not on the input's own hash.
#[tokio::test]
async fn it_rejects_a_block_that_spends_the_same_output_twice() {
    let (mut blockchain, validator) = setup(true);

    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();

    let schema1 = txn_schema!(from: vec![coinbase_a.clone()], to: vec![50 * T, 12 * T]);
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();

    let (mut block, _) = blockchain.create_unmined_block(block_spec!("B->A", transactions: txs));
    let mut inputs = block.body.inputs().clone();
    let mut twin = inputs[0].clone();
    twin.input_data = ExecutionStack::new(vec![StackItem::Number(1)]);
    assert_eq!(inputs[0].output_hash(), twin.output_hash());
    assert_ne!(inputs[0].canonical_hash(), twin.canonical_hash());
    inputs.insert(1, twin);
    let outputs = block.body.outputs().clone();
    let kernels = block.body.kernels().clone();
    block.body = AggregateBody::new_sorted_unchecked(inputs, outputs, kernels);
    let block = blockchain.mine_block("A", block, Difficulty::min());

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(err, ValidationError::UnsortedOrDuplicateInput));
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    txs[0].body = AggregateBody::new_unsorted(txs[0].body.inputs().clone(), outputs, txs[0].body.kernels().clone());
    let (block, _) = blockchain.create_next_tip(block_spec!("B", transactions: txs));

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::AggregatedBodyValidationError(
            AggregatedBodyValidationError::EncryptedDataExceedsMaxSize { .. }
        )
    ));
}

fn rules_with_max_block_body_bytes(max_bytes: usize) -> BaseNodeConsensusManager {
    BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .with_max_block_transaction_weight(127_795)
                .with_max_block_body_bytes(max_bytes)
                .build(),
        )
        .build()
        .unwrap()
}

/// A valid block spending one output, with its MMR roots set so that it passes full validation
fn create_valid_block_with_an_input(blockchain: &mut TestBlockchain) -> Block {
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let schema1 = txn_schema!(from: vec![coinbase_a], to: vec![MicroMinotari::from(9000)]);
    let (txs, _) = schema_to_transaction(&[schema1], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (chain_block, _) = blockchain.create_next_tip(block_spec!("B", parent: "A", transactions: txs));
    with_mmr_roots(blockchain, chain_block.block())
}

/// The block with its MMR roots calculated against the current tip
fn with_mmr_roots(blockchain: &TestBlockchain, block: &Block) -> Block {
    let (mut block, mmr_roots) = blockchain.db().calculate_mmr_roots(block.clone()).unwrap();
    block.header.input_mr = mmr_roots.input_mr;
    block.header.output_mr = mmr_roots.output_mr;
    block.header.block_output_mr = mmr_roots.block_output_mr;
    block.header.output_smt_size = mmr_roots.output_smt_size;
    block.header.kernel_mr = mmr_roots.kernel_mr;
    block.header.kernel_mmr_size = mmr_roots.kernel_mmr_size;
    block.header.validator_node_mr = mmr_roots.validator_node_mr;
    block.header.validator_node_size = mmr_roots.validator_node_size;
    block
}

#[tokio::test]
async fn it_rejects_a_block_whose_revealed_value_coinbase_carries_a_proof() {
    // Range proof verification is bypassed, to show the presence rule still runs
    let (blockchain, validator) = setup(true);
    let mut blockchain = blockchain.with_coinbase_range_proof_type(RangeProofType::RevealedValue);
    let (chain_block, _) = blockchain.create_next_tip(block_spec!("A"));
    let block = with_mmr_roots(&blockchain, chain_block.block());
    let txn = blockchain.db().db_read_access().unwrap();
    validator.validate_body(&*txn, block.clone()).unwrap();

    let (header, inputs, mut outputs, kernels) = block.dissolve();
    let coinbase = outputs.iter_mut().find(|o| o.is_coinbase()).unwrap();
    assert_eq!(coinbase.features.range_proof_type, RangeProofType::RevealedValue);
    assert!(coinbase.proof.is_none());
    coinbase.proof = Some(RangeProof::from(vec![1; 32]));
    let block = Block::new(header, AggregateBody::new_sorted_unchecked(inputs, outputs, kernels));
    let block = with_mmr_roots(&blockchain, &block);
    let err = validator.validate_body(&*txn, block).unwrap_err();
    assert!(
        matches!(
            err,
            ValidationError::AggregatedBodyValidationError(AggregatedBodyValidationError::RangeProofPresenceMismatch {
                range_proof_type: RangeProofType::RevealedValue,
                proof_present: true
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn it_accepts_a_block_body_at_the_byte_limit_and_rejects_one_byte_more() {
    let (mut blockchain, _) = setup(false);
    let block = create_valid_block_with_an_input(&mut blockchain);
    assert!(!block.body.inputs().is_empty());
    let body_bytes = block.body.compact_serialized_size().unwrap();
    // The limit applies to the compact form, which is smaller than the hydrated form being validated here
    assert!(block.body.get_serialized_size().unwrap() > body_bytes);
    // The same block as received during block sync, with compact inputs
    let (header, inputs, outputs, kernels) = block.clone().dissolve();
    let inputs = inputs.iter().map(TransactionInput::to_compact).collect();
    let compact_block = Block::new(header, AggregateBody::new_sorted_unchecked(inputs, outputs, kernels));
    assert_eq!(compact_block.body.compact_serialized_size().unwrap(), body_bytes);

    let txn = blockchain.db().db_read_access().unwrap();

    // Exactly at the limit
    let rules = rules_with_max_block_body_bytes(body_bytes);
    let validator = BlockBodyFullValidator::new(rules.clone(), false);
    validator.validate_body(&*txn, block.clone()).unwrap();
    validator.validate_body(&*txn, compact_block.clone()).unwrap();
    BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default())
        .validate(&block)
        .unwrap();

    // One byte over the limit
    let rules = rules_with_max_block_body_bytes(body_bytes - 1);
    let validator = BlockBodyFullValidator::new(rules.clone(), false);
    for block in [block.clone(), compact_block] {
        let err = validator.validate_body(&*txn, block).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::BlockBodyTooManyBytes { actual_bytes, max_bytes }
                    if actual_bytes == body_bytes && max_bytes == body_bytes - 1
            ),
            "{err:?}"
        );
    }
    let err = BlockBodyInternalConsistencyValidator::new(rules, false, CryptoFactories::default())
        .validate(&block)
        .unwrap_err();
    assert!(matches!(err, ValidationError::BlockBodyTooManyBytes { .. }), "{err:?}");
}

#[tokio::test]
async fn it_rejects_a_block_body_over_the_byte_limit_before_running_scripts() {
    // The real network limit
    let (mut blockchain, validator) = setup(true);
    assert_eq!(
        blockchain.rules().consensus_constants(0).max_block_body_bytes(),
        MAX_BLOCK_BODY_BYTES
    );
    let mut block = create_valid_block_with_an_input(&mut blockchain);
    // Give the input the largest possible input_data, which its script does not accept, and repeat it until the body
    // is over the limit. Inputs are cheap by weight (8 grams each), so this stays well under the maximum block weight.
    let (header, inputs, outputs, kernels) = block.dissolve();
    let mut input = inputs[0].clone();
    input.input_data = ExecutionStack::new(vec![StackItem::Signature(Default::default()); MAX_STACK_SIZE]);
    let num_inputs = MAX_BLOCK_BODY_BYTES / input.to_compact().get_serialized_size().unwrap() + 1;
    block = Block::new(
        header,
        AggregateBody::new_sorted_unchecked(vec![input; num_inputs], outputs, kernels),
    );
    let body_bytes = block.body.compact_serialized_size().unwrap();
    assert!(body_bytes > MAX_BLOCK_BODY_BYTES);

    let txn = blockchain.db().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, block).unwrap_err();
    assert!(
        matches!(
            err,
            ValidationError::BlockBodyTooManyBytes { actual_bytes, max_bytes }
                if actual_bytes == body_bytes && max_bytes == MAX_BLOCK_BODY_BYTES
        ),
        "{err:?}"
    );
}

/// Number of `Drop`s in [max_input_data_script]: one less than the opcode limit, and each one drops one input data item
const DROPPED_ITEMS: usize = MAX_SCRIPT_OPCODES - 1;

/// A script that drops [DROPPED_ITEMS] items and leaves the bottom input data item, the script public key, as its
/// result. It is within both the opcode and the byte limits.
fn max_input_data_script() -> TariScript {
    TariScript::new(vec![Opcode::Drop; DROPPED_ITEMS]).unwrap()
}

/// The input data that [max_input_data_script] accepts: the script public key under [DROPPED_ITEMS] signature-sized
/// (65 byte) items
fn max_input_data(script_public_key: CompressedPublicKey) -> ExecutionStack {
    let mut items = vec![StackItem::PublicKey(script_public_key)];
    items.extend(vec![StackItem::Signature(Default::default()); DROPPED_ITEMS]);
    ExecutionStack::new(items)
}

/// A transaction spending `from` into `count` outputs of `value`, each locked by [max_input_data_script] and carrying
/// [max_input_data] to spend it with, and the change
fn create_max_input_data_outputs(
    key_manager: &KeyManager,
    from: WalletOutput,
    count: usize,
    value: MicroMinotari,
) -> (Transaction, Vec<WalletOutput>, WalletOutput) {
    let constants = ConsensusManager::builder(Network::LocalNet)
        .build()
        .consensus_constants(0)
        .clone();
    let script = max_input_data_script();
    let output_size = recipient_output_features_and_scripts_size(
        constants.transaction_weight_params(),
        &OutputFeatures::default(),
        &script,
        &Covenant::default(),
        &MemoField::new_empty(),
    )
    .unwrap();
    let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();
    builder.with_lock_height(0).with_fee_per_gram(5.into());
    builder.with_input(from).unwrap();
    let pending = (0..count)
        .map(|_| PendingOutput::new(value, output_size))
        .collect::<Vec<_>>();
    let sender_offsets = builder.reserve_sender_offset_keys(&pending).unwrap();
    let mut outputs = Vec::with_capacity(count);
    for sender_offset in sender_offsets {
        let commitment_mask = key_manager.get_random_key(None, None).unwrap();
        let script_key_id = TariKeyId::Derived {
            key: (&commitment_mask.key_id).into(),
        };
        let script_public_key = key_manager.get_public_key_at_key_id(&script_key_id).unwrap();
        let output = WalletOutputBuilder::new(value, commitment_mask.key_id)
            .with_features(OutputFeatures::default())
            .with_script(script.clone())
            .encrypt_data_for_recovery(key_manager, None, MemoField::new_empty())
            .unwrap()
            .with_input_data(max_input_data(script_public_key))
            .with_covenant(Covenant::default())
            .with_sender_offset_public_key(sender_offset.pub_key.clone())
            .with_script_key(script_key_id)
            .sign_metadata_signature(key_manager, &sender_offset.key_id)
            .unwrap()
            .try_build(key_manager)
            .unwrap();
        outputs.push(output.clone());
        builder.with_output(output, sender_offset.key_id, None).unwrap();
    }
    let finalized = builder.build().unwrap();
    (finalized.transaction, outputs, finalized.change.unwrap())
}

/// A valid block at the real block body byte limit, made of inputs carrying as much input data as a valid script can
/// consume, is accepted by full validation (weight, scripts, signatures and range proofs), and can be both synced
/// (one RPC response) and served over messaging (one frame).
#[tokio::test]
async fn it_accepts_a_valid_block_of_max_input_data_inputs_at_the_byte_limit() {
    use prost::Message;
    use tari_comms::protocol::{messaging::MAX_FRAME_LENGTH, rpc::max_response_payload_size};

    use crate::{base_node::comms_interface::NodeCommsResponse, proto::base_node as proto};

    // The real LocalNet constants (block weight and block body byte limit), except that coinbases mature at once
    let rules = BaseNodeConsensusManager::builder(Network::LocalNet)
        .add_consensus_constants(
            ConsensusConstantsBuilder::new(Network::LocalNet)
                .with_coinbase_lockheight(0)
                .build(),
        )
        .build()
        .unwrap();
    let constants = rules.consensus_constants(0).clone();
    assert_eq!(constants.max_block_body_bytes(), MAX_BLOCK_BODY_BYTES);
    let (mut blockchain, validator) = setup_with_rules(rules, false);

    // Room for everything in the block other than the inputs: the coinbase, the spend's outputs and the kernels
    const NON_INPUT_BYTES: usize = 8 * 1024;
    let sample_input = TransactionInput::new_current_version(
        SpentOutput::OutputHash(Default::default()),
        max_input_data(Default::default()),
        Default::default(),
    );
    let input_bytes = sample_input.get_serialized_size().unwrap();
    let num_inputs = (MAX_BLOCK_BODY_BYTES - NON_INPUT_BYTES) / input_bytes;

    // Fund the inputs. A transaction has at most 500 outputs, so this takes a chain of transactions, each spending the
    // change of the one before.
    let (_, coinbase_a) = blockchain.add_next_tip(block_spec!("A")).unwrap();
    let mut funding_txs = Vec::new();
    let mut funded = Vec::with_capacity(num_inputs);
    let mut from = coinbase_a;
    while funded.len() < num_inputs {
        let count = (num_inputs - funded.len()).min(400);
        let (tx, outputs, change) = create_max_input_data_outputs(&blockchain.km, from, count, T);
        funding_txs.push(tx);
        funded.extend(outputs);
        from = change;
    }
    blockchain
        .append(block_spec!("B", parent: "A", transactions: funding_txs))
        .unwrap();

    // Spend them all in one block
    let schema = txn_schema!(from: funded, to: vec![T]);
    let (txs, _) = schema_to_transaction(&[schema], &blockchain.km);
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (chain_block, _) = blockchain.create_next_tip(block_spec!("C", parent: "B", transactions: txs));
    let block = with_mmr_roots(&blockchain, chain_block.block());
    assert_eq!(block.body.inputs().len(), num_inputs);
    assert!(
        block
            .body
            .calculate_weight(constants.transaction_weight_params())
            .unwrap() <=
            constants.max_block_transaction_weight()
    );
    let body_bytes = block.body.compact_serialized_size().unwrap();
    assert!(body_bytes <= MAX_BLOCK_BODY_BYTES);
    assert!(MAX_BLOCK_BODY_BYTES - body_bytes < input_bytes + NON_INPUT_BYTES);

    let txn = blockchain.db().db_read_access().unwrap();
    validator.validate_body(&*txn, block.clone()).unwrap();

    // Block sync: one compact block per BlockBodyResponse, in one RPC response
    let compact = block.to_compact();
    let sync_bytes = proto::BlockBodyResponse::try_from(compact.clone())
        .unwrap()
        .encoded_len();
    assert!(
        sync_bytes <= max_response_payload_size(),
        "{sync_bytes} > {}",
        max_response_payload_size()
    );
    // A full block request (e.g. a compact block that could not be rebuilt): one compact block in one base node
    // service response, sent in one messaging frame. 64 KiB is left for the DHT envelope.
    let messaging_bytes = proto::BaseNodeServiceResponse {
        request_key: u64::MAX,
        response: Some(NodeCommsResponse::Block(Box::new(Some(compact))).try_into().unwrap()),
        is_synced: true,
    }
    .encoded_len();
    println!(
        "{num_inputs} inputs: compact body {body_bytes} bytes, sync response {sync_bytes} bytes, messaging response \
         {messaging_bytes} bytes"
    );
    assert!(
        messaging_bytes + 64 * 1024 <= MAX_FRAME_LENGTH,
        "{messaging_bytes} + 64 KiB > {MAX_FRAME_LENGTH}"
    );
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
    let err = validator.validate_body(&*txn, block.block().clone()).unwrap_err();
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
    let err = validator.validate_body(&*txn, unmined).unwrap_err();
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

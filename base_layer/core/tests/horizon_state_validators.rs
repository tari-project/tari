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

#[allow(dead_code)]
mod helpers;

use crate::helpers::block_builders::{
    create_genesis_block,
    create_genesis_block_with_coinbase_value,
    generate_new_block_with_coinbase,
};
use std::sync::Arc;
use tari_core::{
    blocks::{genesis_block::get_genesis_block, Block, BlockHeader, BlockHeaderValidationError},
    chain_storage::{BlockchainDatabase, ChainStorageError, DbTransaction, MemoryDatabase, Validators},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{DiffAdjManager, Difficulty},
    validation::{
        chain_validators::{ChainTipValidator, GenesisBlockValidator},
        horizon_state_validators::HorizonStateHeaderValidator,
        mocks::MockValidator,
        ValidationError,
    },
};
use tari_transactions::{
    fee::Fee,
    helpers::{create_test_kernel, create_utxo},
    tari_amount::{uT, MicroTari},
    transaction::UnblindedOutput,
    txn_schema,
    types::{CryptoFactories, HashDigest},
};

fn find_header_with_achieved_difficulty(header: &mut BlockHeader, achieved_difficulty: Difficulty) {
    while header.achieved_difficulty() != achieved_difficulty {
        header.nonce += 1;
    }
}

#[test]
fn validate_header_sequence_and_chaining() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let rules = ConsensusManager::default();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        HorizonStateHeaderValidator::new(rules, store.clone()),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    store.set_validators(validators);

    let header0 = BlockHeader::new(0);
    let header1 = BlockHeader::from_previous(&header0);
    let mut header2 = BlockHeader::from_previous(&header1);
    header2.prev_hash = header0.prev_hash.clone(); // Change to incorrect hash chain

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1);
    txn.insert_header(header2);
    assert!(store.commit(txn).is_ok());
    assert!(store.validate_horizon_state().is_err());
}

#[test]
fn validate_median_timestamp() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let consensus = ConsensusConstants::current();
    let rules = ConsensusManager::default();
    rules
        .set_diff_manager(DiffAdjManager::new(store.clone()).unwrap())
        .unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        HorizonStateHeaderValidator::new(rules, store.clone()),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    store.set_validators(validators);

    let mut header0 = BlockHeader::new(0);
    find_header_with_achieved_difficulty(&mut header0, Difficulty::from(1));
    let mut header1 = BlockHeader::from_previous(&header0);
    header1.timestamp = header0.timestamp.increase(consensus.get_diff_target_block_interval());
    find_header_with_achieved_difficulty(&mut header1, Difficulty::from(1));
    let mut header2 = BlockHeader::from_previous(&header1);
    header2.timestamp = header1.timestamp.increase(consensus.get_diff_target_block_interval());
    find_header_with_achieved_difficulty(&mut header2, Difficulty::from(1));
    let mut header3 = BlockHeader::from_previous(&header2);
    header3.timestamp = header0.timestamp;
    find_header_with_achieved_difficulty(&mut header3, Difficulty::from(1));

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1);
    txn.insert_header(header2);
    assert!(store.commit(txn).is_ok());
    assert!(store.validate_horizon_state().is_ok());

    let mut txn = DbTransaction::new();
    txn.insert_header(header3);
    assert!(store.commit(txn).is_ok());
    assert!(store.validate_horizon_state().is_err());
}

#[test]
fn validate_achieved_difficulty() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let consensus = ConsensusConstants::current();
    let rules = ConsensusManager::default();
    rules
        .set_diff_manager(DiffAdjManager::new(store.clone()).unwrap())
        .unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        HorizonStateHeaderValidator::new(rules, store.clone()),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    store.set_validators(validators);

    let mut header0 = BlockHeader::new(0);
    find_header_with_achieved_difficulty(&mut header0, Difficulty::from(1));
    let mut header1 = BlockHeader::from_previous(&header0);
    header1.timestamp = header0.timestamp.increase(consensus.get_diff_target_block_interval());
    find_header_with_achieved_difficulty(&mut header1, Difficulty::from(1));
    let mut header2 = BlockHeader::from_previous(&header1);
    header2.timestamp = header1.timestamp.increase(consensus.get_diff_target_block_interval());
    find_header_with_achieved_difficulty(&mut header2, Difficulty::from(4));
    let mut header3 = BlockHeader::from_previous(&header2);
    header3.timestamp = header3.timestamp.increase(consensus.get_diff_target_block_interval());
    find_header_with_achieved_difficulty(&mut header3, Difficulty::from(2));

    let mut txn = DbTransaction::new();
    txn.insert_header(header0);
    txn.insert_header(header1);
    txn.insert_header(header2);
    assert!(store.commit(txn).is_ok());
    assert!(store.validate_horizon_state().is_ok());

    let mut txn = DbTransaction::new();
    txn.insert_header(header3);
    assert!(store.commit(txn).is_ok());
    assert!(store.validate_horizon_state().is_err());
}

#[test]
fn validate_chain_genesis_block() {
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        GenesisBlockValidator::new(),
        MockValidator::new(true),
    );
    let factories = Arc::new(CryptoFactories::default());

    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    store.set_validators(validators.clone());
    let (block0, _) = create_genesis_block(&store, &factories);
    store.add_block(block0).unwrap();
    assert_eq!(
        store.validate_horizon_state(),
        Err(ChainStorageError::ValidationError(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::IncorrectGenesisBlockHeader
        )))
    );

    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    store.set_validators(validators);
    store.add_block(get_genesis_block()).unwrap();
    assert!(store.validate_horizon_state().is_ok());
}

fn create_test_blockchain_with_emission(
    rules: &ConsensusManager<MemoryDatabase<HashDigest>>,
    factories: &CryptoFactories,
    store: &mut BlockchainDatabase<MemoryDatabase<HashDigest>>,
) -> (Vec<Block>, Vec<Vec<UnblindedOutput>>)
{
    // Block 0
    let block_reward0 = rules.emission_schedule().block_reward(0);
    let (block0, output) = create_genesis_block_with_coinbase_value(&store, &factories, block_reward0);
    store.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![vec![output]];

    // Block 1
    let fee_per_gram = 25 * uT;
    let schema = vec![txn_schema!(
        from: vec![outputs[0][0].clone()],
        to: vec![1000 * uT],
        fee: fee_per_gram
    )];
    let block_reward1 = rules.emission_schedule().block_reward(1);
    generate_new_block_with_coinbase(
        store,
        &factories,
        &mut blocks,
        &mut outputs,
        schema,
        block_reward1 + Fee::calculate(fee_per_gram, 1, 2),
    )
    .unwrap();
    (blocks, outputs)
}

#[test]
fn validate_accounting_balance() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let factories = Arc::new(CryptoFactories::default());
    let rules = ConsensusManager::default();
    rules
        .set_diff_manager(DiffAdjManager::new(store.clone()).unwrap())
        .unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        ChainTipValidator::new(rules.clone(), factories.clone(), store.clone()),
    );
    store.set_validators(validators);

    let (mut blocks, mut outputs) = create_test_blockchain_with_emission(&rules, &factories, &mut store);
    assert!(store.validate_horizon_state().is_ok());

    let fee_per_gram = 25 * uT;
    let schema = vec![txn_schema!(
        from: vec![outputs[1][0].clone()],
        to: vec![2 * uT],
        fee: fee_per_gram
    )];
    let total_fee = Fee::calculate(fee_per_gram, 1, 1);
    let block_reward2 = rules.emission_schedule().block_reward(2);
    generate_new_block_with_coinbase(
        &mut store,
        &factories,
        &mut blocks,
        &mut outputs,
        schema,
        block_reward2 + total_fee,
    )
    .unwrap();
    assert_eq!(
        store.validate_horizon_state(),
        Err(ChainStorageError::ValidationError(
            ValidationError::InvalidAccountingBalance
        ))
    );
}

#[test]
fn validate_kernel_mmr_roots() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let factories = Arc::new(CryptoFactories::default());
    let rules = ConsensusManager::default();
    rules
        .set_diff_manager(DiffAdjManager::new(store.clone()).unwrap())
        .unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        ChainTipValidator::new(rules.clone(), factories.clone(), store.clone()),
    );
    store.set_validators(validators);

    create_test_blockchain_with_emission(&rules, &factories, &mut store);
    assert!(store.validate_horizon_state().is_ok());

    let kernel = create_test_kernel(100.into(), 0);
    let mut txn = DbTransaction::new();
    txn.insert_kernel(kernel.clone(), true);
    assert!(store.commit(txn).is_ok());
    assert_eq!(
        store.validate_horizon_state(),
        Err(ChainStorageError::ValidationError(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::MismatchedMmrRoots
        )))
    );
}

#[test]
fn validate_output_and_rp_mmr_roots() {
    let db = MemoryDatabase::<HashDigest>::default();
    let mut store = BlockchainDatabase::new(db).unwrap();
    let factories = Arc::new(CryptoFactories::default());
    let rules = ConsensusManager::default();
    rules
        .set_diff_manager(DiffAdjManager::new(store.clone()).unwrap())
        .unwrap();
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        ChainTipValidator::new(rules.clone(), factories.clone(), store.clone()),
    );
    store.set_validators(validators);

    create_test_blockchain_with_emission(&rules, &factories, &mut store);
    assert!(store.validate_horizon_state().is_ok());

    let (utxo, _) = create_utxo(MicroTari(10_000), &factories);
    let mut txn = DbTransaction::new();
    txn.insert_utxo(utxo.clone(), true);
    assert!(store.commit(txn).is_ok());
    assert_eq!(
        store.validate_horizon_state(),
        Err(ChainStorageError::ValidationError(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::MismatchedMmrRoots
        )))
    );
}

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

mod helpers;
use crate::helpers::block_builders::generate_new_block_with_coinbase;
use helpers::block_builders::{
    calculate_new_block,
    create_genesis_block_with_utxos,
    find_header_with_achieved_difficulty,
    generate_new_block,
};
use std::sync::Arc;
use tari_core::{
    blocks::genesis_block::get_genesis_block,
    chain_storage::{BlockAddResult, BlockchainDatabase, MemoryDatabase, Validators},
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{DiffAdjManager, Difficulty},
    transactions::{
        tari_amount::{uT, MicroTari, T},
        types::{CryptoFactories, HashDigest},
    },
    txn_schema,
    validation::{
        block_validators::{FullConsensusValidator, StatelessValidator},
        mocks::MockValidator,
    },
};
use tari_utilities::epoch_time::EpochTime;

#[test]
fn test_genesis_block() {
    let factories = CryptoFactories::default();
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    db.set_validators(validators);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();
    let block = get_genesis_block();
    let result = db.add_block(block);
    assert!(result.is_ok());
}

#[test]
fn test_valid_chain() {
    let factories = CryptoFactories::default();
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators_true = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let validators_false = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    db.set_validators(validators_false);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();

    let (mut block0, output) = create_genesis_block_with_utxos(&db, &factories, &[10 * T]);
    block0.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(5_000)).unwrap();
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![output];
    db.set_validators(validators_true);
    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][1].clone()], to: vec![6 * T, 3 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(1);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(4_000)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    assert_eq!(db.add_block(block.clone()), Ok(BlockAddResult::Ok));
    blocks.push(block);
    // Block 2

    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(2);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(3_000)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(100));

    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(4_000)).unwrap();
    assert_eq!(db.add_block(block.clone()), Ok(BlockAddResult::Ok));
    blocks.push(block);
    // Block 3

    let schema = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T, 500_000 * uT]),
        txn_schema!(from: vec![outputs[1][1].clone()], to: vec![500_000 * uT]),
    ];
    let coinbase_amount = 326 * uT + 226 * uT + rules.emission_schedule().block_reward(3);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();

    dbg!(&block.body.kernels());

    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(2_000)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(200));
    assert_eq!(db.add_block(block.clone()), Ok(BlockAddResult::Ok));
}

#[test]
fn test_invalid_coinbase() {
    let factories = CryptoFactories::default();
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators_true = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let validators_false = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    db.set_validators(validators_false);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();

    let (block0, output) = create_genesis_block_with_utxos(&db, &factories, &[10 * T]);
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![output];
    db.set_validators(validators_true);
    let schema = vec![txn_schema!(from: vec![outputs[0][1].clone()], to: vec![6 * T, 3 * T])];
    // We have no coinbase, so this should fail
    assert!(generate_new_block(&mut db, &mut blocks, &mut outputs, schema).is_err());
}

#[test]
fn test_invalid_pow() {
    let factories = CryptoFactories::default();
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators_true = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let validators_false = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    db.set_validators(validators_false);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();

    let (mut block0, output) = create_genesis_block_with_utxos(&db, &factories, &[10 * T]);
    block0.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(5_000)).unwrap();
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![output];
    db.set_validators(validators_true);
    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][1].clone()], to: vec![6 * T, 3 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(1);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(4_999)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    assert_eq!(db.add_block(block.clone()), Ok(BlockAddResult::Ok));
    blocks.push(block);

    // Block 2

    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(2);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(4_998)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    assert!(db.add_block(block.clone()).is_err());
}

#[test]
fn test_invalid_time() {
    let factories = CryptoFactories::default();
    let rules = ConsensusManager::default();
    let backend = MemoryDatabase::<HashDigest>::default();
    let mut db = BlockchainDatabase::new(backend).unwrap();
    let validators_true = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone(), db.clone()),
        StatelessValidator::new(),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let validators_false = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    db.set_validators(validators_false);
    let diff_adj_manager = DiffAdjManager::new(db.clone()).unwrap();
    rules.set_diff_manager(diff_adj_manager).unwrap();

    let (mut block0, output) = create_genesis_block_with_utxos(&db, &factories, &[10 * T]);
    block0.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(5_000)).unwrap();
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![output];
    db.set_validators(validators_true);
    // Block 1
    let schema = vec![txn_schema!(from: vec![outputs[0][1].clone()], to: vec![6 * T, 3 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(1);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().checked_sub(EpochTime::from(4_000)).unwrap();
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(1));
    assert_eq!(db.add_block(block.clone()), Ok(BlockAddResult::Ok));
    blocks.push(block);
    // Block 2

    let schema = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![3 * T, 1 * T])];
    let coinbase_amount = 326 * uT + rules.emission_schedule().block_reward(2);
    let mut block =
        calculate_new_block(&mut db, &factories, &mut blocks, &mut outputs, schema, coinbase_amount).unwrap();
    block.header.timestamp = EpochTime::now().increase(ConsensusConstants::default().ftl().as_u64() + 100);
    find_header_with_achieved_difficulty(&mut block.header, Difficulty::from(100));
    assert!(db.add_block(block.clone()).is_err());
}

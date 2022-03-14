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

use std::sync::Arc;

use monero::blockdata::block::Block as MoneroBlock;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_core::{
    blocks::{Block, BlockHeaderAccumulatedData, BlockHeaderValidationError, BlockValidationError, ChainBlock},
    chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, ChainStorageError, Validators},
    consensus::{consensus_constants::PowAlgorithmConstants, ConsensusConstantsBuilder, ConsensusManager},
    proof_of_work::{
        monero_rx,
        monero_rx::{FixedByteArray, MoneroPowData},
        randomx_factory::RandomXFactory,
        PowAlgorithm,
    },
    test_helpers::blockchain::{create_store_with_consensus_and_validators, create_test_db},
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::{uT, T},
        test_helpers::{create_unblinded_output, schema_to_transaction, spend_utxos, TestParams, UtxoTestParams},
        transaction_components::OutputFeatures,
        CryptoFactories,
    },
    txn_schema,
    validation::{
        block_validators::{BlockValidator, BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        mocks::MockValidator,
        BlockSyncBodyValidation,
        DifficultyCalculator,
        HeaderValidation,
        OrphanValidation,
        PostOrphanBodyValidation,
        ValidationError,
    },
};
use tari_crypto::{inputs, script};
use tari_test_utils::unpack_enum;
use tari_utilities::{hex::Hex, Hashable};

use crate::helpers::{
    block_builders::{
        chain_block_with_coinbase,
        chain_block_with_new_coinbase,
        create_coinbase,
        create_genesis_block_with_utxos,
        find_header_with_achieved_difficulty,
    },
    test_blockchain::TestBlockchain,
};

mod helpers;

#[test]
fn test_genesis_block() {
    let factories = CryptoFactories::default();
    let network = Network::Dibbler;
    let rules = ConsensusManager::builder(network).build();
    let backend = create_test_db();
    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        HeaderValidator::new(rules.clone()),
        OrphanBlockValidator::new(rules.clone(), false, factories),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    let block = rules.get_genesis_block();
    match db.add_block(block.to_arc_block()).unwrap_err() {
        ChainStorageError::ValidationError { source } => match source {
            ValidationError::ValidatingGenesis => (),
            _ => panic!("Failed because incorrect validation error was received"),
        },
        _ => panic!("Failed because incorrect ChainStorageError was received"),
    }
}

#[test]
fn test_monero_blocks() {
    // Create temporary test folder
    let seed1 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97";
    let seed2 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad98";

    let factories = CryptoFactories::default();
    let network = Network::Dibbler;
    let cc = ConsensusConstantsBuilder::new(network)
        .with_max_randomx_seed_height(1)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3, PowAlgorithmConstants {
            max_target_time: 1800,
            min_difficulty: 1.into(),
            max_difficulty: 1.into(),
            target_time: 300,
        })
        .add_proof_of_work(PowAlgorithm::Monero, PowAlgorithmConstants {
            max_target_time: 1200,
            min_difficulty: 1.into(),
            max_difficulty: 1.into(),
            target_time: 200,
        })
        .build();
    let cm = ConsensusManager::builder(network).add_consensus_constants(cc).build();
    let header_validator = HeaderValidator::new(cm.clone());
    let db = create_store_with_consensus_and_validators(
        cm.clone(),
        Validators::new(MockValidator::new(true), header_validator, MockValidator::new(true)),
    );
    let block_0 = db.fetch_block(0).unwrap().try_into_chain_block().unwrap();
    let (block_1_t, _) = chain_block_with_new_coinbase(&block_0, vec![], &cm, &factories);
    let mut block_1 = db.prepare_new_block(block_1_t).unwrap();

    // Now we have block 1, lets add monero data to it
    add_monero_data(&mut block_1, seed1);
    let cb_1 = db.add_block(Arc::new(block_1)).unwrap().assert_added();
    // Now lets add a second faulty block using the same seed hash
    let (block_2_t, _) = chain_block_with_new_coinbase(&cb_1, vec![], &cm, &factories);
    let mut block_2 = db.prepare_new_block(block_2_t).unwrap();

    add_monero_data(&mut block_2, seed1);
    let cb_2 = db.add_block(Arc::new(block_2)).unwrap().assert_added();
    // Now lets add a third faulty block using the same seed hash. This should fail.
    let (block_3_t, _) = chain_block_with_new_coinbase(&cb_2, vec![], &cm, &factories);
    let mut block_3 = db.prepare_new_block(block_3_t).unwrap();
    let mut block_3_broken = block_3.clone();
    add_monero_data(&mut block_3_broken, seed1);
    match db.add_block(Arc::new(block_3_broken)) {
        Err(ChainStorageError::ValidationError {
            source: ValidationError::BlockHeaderError(BlockHeaderValidationError::OldSeedHash),
        }) => (),
        Err(e) => {
            panic!("Failed due to other error:{:?}", e);
        },
        Ok(res) => {
            panic!("Block add unexpectedly succeeded with result: {:?}", res);
        },
    };

    // now lets fix the seed, and try again
    add_monero_data(&mut block_3, seed2);
    db.add_block(Arc::new(block_3)).unwrap().assert_added();
}

fn add_monero_data(tblock: &mut Block, seed_key: &str) {
    let blocktemplate_blob =
"0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000"
.to_string();
    let bytes = hex::decode(blocktemplate_blob).unwrap();
    let mut mblock = monero_rx::deserialize::<MoneroBlock>(&bytes[..]).unwrap();
    let hash = tblock.header.merged_mining_hash();
    monero_rx::append_merge_mining_tag(&mut mblock, hash).unwrap();
    let hashes = monero_rx::create_ordered_transaction_hashes_from_block(&mblock);
    let merkle_root = monero_rx::tree_hash(&hashes).unwrap();
    let coinbase_merkle_proof = monero_rx::create_merkle_proof(&hashes, &hashes[0]).unwrap();
    let monero_data = MoneroPowData {
        header: mblock.header,
        randomx_key: FixedByteArray::from_hex(seed_key).unwrap(),
        transaction_count: hashes.len() as u16,
        merkle_root,
        coinbase_merkle_proof,
        coinbase_tx: mblock.miner_tx,
    };
    let serialized = monero_rx::serialize(&monero_data);
    tblock.header.pow.pow_algo = PowAlgorithm::Monero;
    tblock.header.pow.pow_data = serialized;
}

#[tokio::test]
async fn inputs_are_not_malleable() {
    let _ = env_logger::try_init();
    let mut blockchain = TestBlockchain::with_genesis("GB");
    let blocks = blockchain.builder();

    let (_, output) = blockchain.add_block(blocks.new_block("A1").child_of("GB").difficulty(1));

    let (txs, _) = schema_to_transaction(&[txn_schema!(from: vec![output.clone()], to: vec![50 * T])]);
    let txs = txs.into_iter().map(|tx| Clone::clone(&*tx)).collect();
    blockchain.add_block(
        blocks
            .new_block("A2")
            .child_of("A1")
            .difficulty(1)
            .with_transactions(txs),
    );
    let spent_output = output;
    let mut block = blockchain.get_block("A2").cloned().unwrap().block.block().clone();
    blockchain.store().rewind_to_height(block.header.height - 1).unwrap();

    let mut malicious_test_params = TestParams::new();

    // New key which used to manipulate the input
    let (malicious_script_private_key, malicious_script_public_key) = malicious_test_params.get_script_keypair();

    // Oh noes - they've managed to get hold of the private script and spend keys
    malicious_test_params.spend_key = spent_output.spending_key;

    block.header.total_script_offset =
        block.header.total_script_offset - &spent_output.script_private_key + &malicious_script_private_key;

    let (malicious_input, _) = malicious_test_params.create_input(UtxoTestParams {
        value: spent_output.value,
        script: spent_output.script.clone(),
        input_data: Some(inputs![malicious_script_public_key]),
        features: spent_output.features,
        ..Default::default()
    });

    let input_mut = block.body.inputs_mut().get_mut(0).unwrap();
    // Put the crafted input into the block
    input_mut.input_data = malicious_input.input_data;
    input_mut.script_signature = malicious_input.script_signature;

    let validator = BlockValidator::new(
        blockchain.store().clone().into(),
        blockchain.consensus_manager().clone(),
        CryptoFactories::default(),
        true,
        10,
    );
    let err = validator.validate_body(block).await.unwrap_err();

    // All validations pass, except the Input MMR.
    unpack_enum!(ValidationError::BlockError(err) = err);
    unpack_enum!(BlockValidationError::MismatchedMmrRoots { kind } = err);
    assert_eq!(kind, "Input");
}

#[test]
fn test_orphan_validator() {
    let factories = CryptoFactories::default();
    let network = Network::Weatherwax;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_max_block_transaction_weight(80)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&factories, &[T, T, T], &consensus_constants);
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build();
    let backend = create_test_db();
    let orphan_validator = OrphanBlockValidator::new(rules.clone(), false, factories.clone());
    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        HeaderValidator::new(rules.clone()),
        orphan_validator.clone(),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (tx03, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (tx04, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![50_000 * uT], fee: 20*uT, lock: 2, features:
OutputFeatures::default()),
    );
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    // this block should be okay
    assert!(orphan_validator.validate(&new_block).is_ok());

    // lets break the block weight
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone(), tx03], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());

    // lets break the sorting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let outputs = vec![template.body.outputs()[1].clone(), template.body.outputs()[2].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());

    // lets break spend rules
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx04.clone()], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());

    // let break coinbase value
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        &factories,
        10000000.into(),
        1 + rules.consensus_constants(0).coinbase_lock_height(),
    );
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());

    // let break coinbase lock height
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        &factories,
        rules.get_block_reward_at(1) + tx01.body.get_total_fee() + tx02.body.get_total_fee(),
        1,
    );
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());

    // lets break accounting
    let (mut template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, &factories);
    let outputs = vec![template.body.outputs()[1].clone(), tx04.body.outputs()[1].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate(&new_block).is_err());
}

#[test]
fn test_orphan_body_validation() {
    let factories = CryptoFactories::default();
    let network = Network::Weatherwax;
    // we dont want localnet's 1 difficulty or the full mined difficulty of weather wax but we want some.
    let sha3_constants = PowAlgorithmConstants {
        max_target_time: 1800,
        min_difficulty: 10.into(),
        max_difficulty: u64::MAX.into(),
        target_time: 300,
    };
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3, sha3_constants)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&factories, &[T, T, T], &consensus_constants);
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build();
    let backend = create_test_db();
    let body_only_validator = BodyOnlyValidator::new(rules.clone());
    let header_validator = HeaderValidator::new(rules.clone());
    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        HeaderValidator::new(rules.clone()),
        OrphanBlockValidator::new(rules.clone(), false, factories.clone()),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, &factories);
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(new_block.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(new_block.header.total_kernel_offset.clone())
        .build()
        .unwrap();

    let chain_block = ChainBlock::try_construct(Arc::new(new_block), accumulated_data).unwrap();
    let metadata = db.get_chain_metadata().unwrap();
    // this block should be okay
    assert!(body_only_validator
        .validate_body_for_valid_orphan(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_ok());

    // lets break the chain sequence
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    new_block.header.height = 3;
    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(new_block.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(new_block.header.total_kernel_offset.clone())
        .build()
        .unwrap();

    let chain_block = ChainBlock::try_construct(Arc::new(new_block), accumulated_data).unwrap();
    let metadata = db.get_chain_metadata().unwrap();
    assert!(body_only_validator
        .validate_body_for_valid_orphan(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());

    // lets have unknown inputs;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    let test_params1 = TestParams::new();
    let test_params2 = TestParams::new();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let unblinded_utxo =
        create_unblinded_output(script!(Nop), OutputFeatures::default(), test_params1, outputs[1].value);
    let unblinded_utxo2 =
        create_unblinded_output(script!(Nop), OutputFeatures::default(), test_params2, outputs[2].value);
    let inputs = vec![
        unblinded_utxo.as_transaction_input(&factories.commitment).unwrap(),
        unblinded_utxo2.as_transaction_input(&factories.commitment).unwrap(),
    ];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(new_block.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(new_block.header.total_kernel_offset.clone())
        .build()
        .unwrap();

    let chain_block = ChainBlock::try_construct(Arc::new(new_block), accumulated_data).unwrap();
    let metadata = db.get_chain_metadata().unwrap();
    assert!(body_only_validator
        .validate_body_for_valid_orphan(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());

    // lets check duplicate txos
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let inputs = vec![new_block.body.inputs()[0].clone(), new_block.body.inputs()[0].clone()];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(new_block.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(new_block.header.total_kernel_offset.clone())
        .build()
        .unwrap();

    let chain_block = ChainBlock::try_construct(Arc::new(new_block), accumulated_data).unwrap();
    let metadata = db.get_chain_metadata().unwrap();
    assert!(body_only_validator
        .validate_body_for_valid_orphan(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());

    // check mmr roots
    let mut new_block = db.prepare_new_block(template).unwrap();
    new_block.header.output_mr = Vec::new();
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let difficulty_calculator = DifficultyCalculator::new(rules, RandomXFactory::default());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .unwrap();
    let accumulated_data = BlockHeaderAccumulatedData::builder(genesis.accumulated_data())
        .with_hash(new_block.hash())
        .with_achieved_target_difficulty(achieved_target_diff)
        .with_total_kernel_offset(new_block.header.total_kernel_offset.clone())
        .build()
        .unwrap();

    let chain_block = ChainBlock::try_construct(Arc::new(new_block), accumulated_data).unwrap();
    let metadata = db.get_chain_metadata().unwrap();
    assert!(body_only_validator
        .validate_body_for_valid_orphan(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());
}

#[test]
fn test_header_validation() {
    let factories = CryptoFactories::default();
    let network = Network::Weatherwax;
    // we dont want localnet's 1 difficulty or the full mined difficulty of weather wax but we want some.
    let sha3_constants = PowAlgorithmConstants {
        max_target_time: 1800,
        min_difficulty: 20.into(),
        max_difficulty: u64::MAX.into(),
        target_time: 300,
    };
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3, sha3_constants)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&factories, &[T, T, T], &consensus_constants);
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build();
    let backend = create_test_db();
    let header_validator = HeaderValidator::new(rules.clone());
    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        HeaderValidator::new(rules.clone()),
        OrphanBlockValidator::new(rules.clone(), false, factories.clone()),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
    );
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, &factories);
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, 20.into());
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .is_ok());

    // Lets break ftl rules
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    // we take the max ftl time and give 10 seconds for mining then check it, it should still be more than the ftl
    new_block.header.timestamp = rules.consensus_constants(0).ftl().increase(10);
    find_header_with_achieved_difficulty(&mut new_block.header, 20.into());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .is_err());

    // lets break the median rules
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    // we take the max ftl time and give 10 seconds for mining then check it, it should still be more than the ftl
    new_block.header.timestamp = genesis.header().timestamp.checked_sub(100.into()).unwrap();
    find_header_with_achieved_difficulty(&mut new_block.header, 20.into());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .is_err());

    // lets break difficulty
    let mut new_block = db.prepare_new_block(template).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
    let mut result = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &difficulty_calculator,
        )
        .is_err();
    new_block.header.nonce = OsRng.next_u64();
    let mut counter = 0;
    while counter < 10 && !result {
        counter += 1;
        new_block.header.nonce = OsRng.next_u64();
        find_header_with_achieved_difficulty(&mut new_block.header, 10.into());
        result = header_validator
            .validate(
                &*db.db_read_access().unwrap(),
                &new_block.header,
                &difficulty_calculator,
            )
            .is_err();
    }
    assert!(result);
}

#[tokio::test]
async fn test_block_sync_body_validator() {
    let factories = CryptoFactories::default();
    let network = Network::Weatherwax;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_max_block_transaction_weight(80)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&factories, &[T, T, T], &consensus_constants);
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build();
    let backend = create_test_db();

    let validators = Validators::new(
        BodyOnlyValidator::new(rules.clone()),
        HeaderValidator::new(rules.clone()),
        OrphanBlockValidator::new(rules.clone(), false, factories.clone()),
    );

    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    let validator = BlockValidator::new(db.clone().into(), rules.clone(), factories.clone(), false, 2);

    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features: OutputFeatures::default()),
    );
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),
    );
    let (tx03, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),
    );
    let (tx04, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![50_000 * uT], fee: 20*uT, lock: 2, features: OutputFeatures::default()),
    );
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    // this block should be okay
    validator.validate_body(new_block).await.unwrap();

    // lets break the block weight
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone(), tx03], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap_err();

    // lets break spend rules
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx04.clone()], &rules, &factories);
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap_err();

    // lets break the sorting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let output = vec![template.body.outputs()[1].clone(), template.body.outputs()[2].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), output, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap_err();

    // lets have unknown inputs;
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    let test_params1 = TestParams::new();
    let test_params2 = TestParams::new();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let unblinded_utxo =
        create_unblinded_output(script!(Nop), OutputFeatures::default(), test_params1, outputs[1].value);
    let unblinded_utxo2 =
        create_unblinded_output(script!(Nop), OutputFeatures::default(), test_params2, outputs[2].value);
    let inputs = vec![
        unblinded_utxo.as_transaction_input(&factories.commitment).unwrap(),
        unblinded_utxo2.as_transaction_input(&factories.commitment).unwrap(),
    ];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    validator.validate_body(new_block).await.unwrap_err();

    // lets check duplicate txos
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let inputs = vec![new_block.body.inputs()[0].clone(), new_block.body.inputs()[0].clone()];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    validator.validate_body(new_block).await.unwrap_err();

    // let break coinbase value
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        &factories,
        10000000.into(),
        1 + rules.consensus_constants(0).coinbase_lock_height(),
    );
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap_err();

    // let break coinbase lock height
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        &factories,
        rules.get_block_reward_at(1) + tx01.body.get_total_fee() + tx02.body.get_total_fee(),
        1,
    );
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap();

    // lets break accounting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, &factories);
    let outputs = vec![template.body.outputs()[1].clone(), tx04.body.outputs()[1].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    validator.validate_body(new_block).await.unwrap_err();

    // lets the mmr root
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, &factories);
    let mut new_block = db.prepare_new_block(template).unwrap();
    new_block.header.output_mr = Vec::new();
    validator.validate_body(new_block).await.unwrap_err();
}

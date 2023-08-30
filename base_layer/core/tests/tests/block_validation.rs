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

use std::{iter, sync::Arc};

use borsh::BorshSerialize;
use monero::blockdata::block::Block as MoneroBlock;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::types::FixedHash;
use tari_core::{
    blocks::{Block, BlockHeaderAccumulatedData, BlockHeaderValidationError, BlockValidationError, ChainBlock},
    chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, ChainStorageError, Validators},
    consensus::{consensus_constants::PowAlgorithmConstants, ConsensusConstantsBuilder, ConsensusManager},
    proof_of_work::{
        monero_rx,
        monero_rx::{verify_header, FixedByteArray, MoneroPowData},
        randomx_factory::RandomXFactory,
        Difficulty,
        PowAlgorithm,
    },
    test_helpers::blockchain::{create_store_with_consensus_and_validators, create_test_db},
    transactions::{
        aggregated_body::AggregateBody,
        key_manager::TransactionKeyManagerInterface,
        tari_amount::{uT, T},
        test_helpers::{
            create_test_core_key_manager_with_memory_db,
            create_wallet_output_with_data,
            schema_to_transaction,
            spend_utxos,
            TestParams,
            UtxoTestParams,
        },
        transaction_components::{OutputFeatures, TransactionError},
        CryptoFactories,
    },
    txn_schema,
    validation::{
        block_body::{BlockBodyFullValidator, BlockBodyInternalConsistencyValidator},
        header::HeaderFullValidator,
        mocks::MockValidator,
        BlockBodyValidator,
        CandidateBlockValidator,
        DifficultyCalculator,
        HeaderChainLinkedValidator,
        InternalConsistencyValidator,
        ValidationError,
    },
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script};
use tari_test_utils::unpack_enum;
use tari_utilities::hex::Hex;
use tokio::time::Instant;

use crate::{
    helpers::{
        block_builders::{
            chain_block_with_coinbase,
            chain_block_with_new_coinbase,
            create_coinbase,
            create_genesis_block_with_utxos,
            find_header_with_achieved_difficulty,
        },
        test_blockchain::TestBlockchain,
    },
    tests::assert_block_add_result_added,
};

#[tokio::test]
async fn test_monero_blocks() {
    // Create temporary test folder
    let seed1 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97";
    let seed2 = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad98";

    let key_manager = create_test_core_key_manager_with_memory_db();
    let network = Network::Esmeralda;
    let cc = ConsensusConstantsBuilder::new(network)
        .with_max_randomx_seed_height(1)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3x, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 300,
        })
        .add_proof_of_work(PowAlgorithm::RandomX, PowAlgorithmConstants {
            min_difficulty: Difficulty::min(),
            max_difficulty: Difficulty::min(),
            target_time: 200,
        })
        .with_blockchain_version(0)
        .build();
    let cm = ConsensusManager::builder(network)
        .add_consensus_constants(cc)
        .build()
        .unwrap();
    let difficulty_calculator = DifficultyCalculator::new(cm.clone(), RandomXFactory::default());
    let header_validator = HeaderFullValidator::new(cm.clone(), difficulty_calculator);
    let db = create_store_with_consensus_and_validators(
        cm.clone(),
        Validators::new(MockValidator::new(true), header_validator, MockValidator::new(true)),
    );
    let block_0 = db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap();
    let (block_1_t, _) = chain_block_with_new_coinbase(&block_0, vec![], &cm, None, &key_manager).await;
    let mut block_1 = db.prepare_new_block(block_1_t).unwrap();

    // Now we have block 1, lets add monero data to it
    add_monero_data(&mut block_1, seed1);
    let cb_1 = assert_block_add_result_added(&db.add_block(Arc::new(block_1)).unwrap());
    // Now lets add a second faulty block using the same seed hash
    let (block_2_t, _) = chain_block_with_new_coinbase(&cb_1, vec![], &cm, None, &key_manager).await;
    let mut block_2 = db.prepare_new_block(block_2_t).unwrap();

    add_monero_data(&mut block_2, seed1);
    let cb_2 = assert_block_add_result_added(&db.add_block(Arc::new(block_2)).unwrap());
    // Now lets add a third faulty block using the same seed hash. This should fail.
    let (block_3_t, _) = chain_block_with_new_coinbase(&cb_2, vec![], &cm, None, &key_manager).await;
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

    // lets try add some bad data to the block
    let mut extra_bytes_block_3 = block_3.clone();
    add_bad_monero_data(&mut extra_bytes_block_3, seed2);
    match db.add_block(Arc::new(extra_bytes_block_3)) {
        Err(ChainStorageError::ValidationError {
            source: ValidationError::CustomError(_),
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
    // lets break the nonce count
    let hash1 = block_3.hash();
    block_3.header.nonce = 1;
    let hash2 = block_3.hash();
    assert_ne!(hash1, hash2);
    assert!(verify_header(&block_3.header).is_ok());
    match db.add_block(Arc::new(block_3.clone())) {
        Err(ChainStorageError::ValidationError {
            source: ValidationError::BlockHeaderError(BlockHeaderValidationError::InvalidNonce),
        }) => (),
        Err(e) => {
            panic!("Failed due to other error:{:?}", e);
        },
        Ok(res) => {
            panic!("Block add unexpectedly succeeded with result: {:?}", res);
        },
    };
    // lets fix block3
    block_3.header.nonce = 0;
    assert_block_add_result_added(&db.add_block(Arc::new(block_3.clone())).unwrap());
}

fn add_monero_data(tblock: &mut Block, seed_key: &str) {
    let blocktemplate_blob =
"0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000"
.to_string();
    let bytes = hex::decode(blocktemplate_blob).unwrap();
    let mut mblock = monero_rx::deserialize::<MoneroBlock>(&bytes[..]).unwrap();
    let hash = tblock.header.merge_mining_hash();
    monero_rx::append_merge_mining_tag(&mut mblock, hash).unwrap();
    let hashes = monero_rx::create_ordered_transaction_hashes_from_block(&mblock);
    let merkle_root = monero_rx::tree_hash(&hashes).unwrap();
    let coinbase_merkle_proof = monero_rx::create_merkle_proof(&hashes).unwrap();
    #[allow(clippy::cast_possible_truncation)]
    let monero_data = MoneroPowData {
        header: mblock.header,
        randomx_key: FixedByteArray::from_hex(seed_key).unwrap(),
        transaction_count: hashes.len() as u16,
        merkle_root,
        coinbase_merkle_proof,
        coinbase_tx: mblock.miner_tx,
    };
    let mut serialized = Vec::new();
    BorshSerialize::serialize(&monero_data, &mut serialized).unwrap();
    tblock.header.pow.pow_algo = PowAlgorithm::RandomX;
    tblock.header.pow.pow_data = serialized;
}

fn add_bad_monero_data(tblock: &mut Block, seed_key: &str) {
    add_monero_data(tblock, seed_key);
    // Add some "garbage" bytes to the end of the pow_data
    tblock.header.pow.pow_data.extend([1u8; 100]);
}

#[tokio::test]
async fn inputs_are_not_malleable() {
    let _ = env_logger::try_init();
    let mut blockchain = TestBlockchain::with_genesis("GB").await;
    let blocks = blockchain.builder();

    let (_, output) = blockchain
        .add_block(blocks.new_block("A1").child_of("GB").difficulty(1))
        .await;

    let (txs, _) = schema_to_transaction(
        &[txn_schema!(from: vec![output.clone()], to: vec![50 * T])],
        &blockchain.key_manager,
    )
    .await;
    let txs = txs.into_iter().map(|tx| Clone::clone(&*tx)).collect();
    blockchain
        .add_block(
            blocks
                .new_block("A2")
                .child_of("A1")
                .difficulty(1)
                .with_transactions(txs),
        )
        .await;
    let spent_output = output;
    let mut block = blockchain.get_block("A2").cloned().unwrap().block.block().clone();
    blockchain.store().rewind_to_height(block.header.height - 1).unwrap();

    let mut malicious_test_params = TestParams::new(&blockchain.key_manager).await;

    // Oh noes - they've managed to get hold of the private script and spend keys
    malicious_test_params.spend_key_id = spent_output.spending_key_id;
    let modified_so = blockchain
        .key_manager
        .get_script_offset(&vec![spent_output.script_key_id.clone()], &vec![malicious_test_params
            .script_key_id
            .clone()])
        .await
        .unwrap();
    // so is calculated as ks-ko
    // we want to modify the so with -ks + ks
    block.header.total_script_offset = block.header.total_script_offset - &modified_so;
    let malicious_script_public_key = blockchain
        .key_manager
        .get_public_key_at_key_id(&malicious_test_params.script_key_id)
        .await
        .unwrap();
    let malicious_wallet_output = malicious_test_params
        .create_input(
            UtxoTestParams {
                value: spent_output.value,
                script: spent_output.script.clone(),
                input_data: Some(inputs![malicious_script_public_key]),
                features: spent_output.features,
                ..Default::default()
            },
            &blockchain.key_manager,
        )
        .await;

    let malicious_input = malicious_wallet_output
        .to_transaction_input(&blockchain.key_manager)
        .await
        .unwrap();

    let mut inputs = block.body.inputs().clone();
    // Put the crafted input into the block
    inputs[0].input_data = malicious_input.input_data;
    inputs[0].script_signature = malicious_input.script_signature;

    block.body = AggregateBody::new(inputs, block.body.outputs().clone(), block.body.kernels().clone());

    let validator = BlockBodyFullValidator::new(blockchain.consensus_manager().clone(), true);
    let txn = blockchain.store().db_read_access().unwrap();
    let err = validator.validate_body(&*txn, &block).unwrap_err();

    // All validations pass, except the Input MMR.
    unpack_enum!(ValidationError::BlockError(err) = err);
    unpack_enum!(BlockValidationError::MismatchedMmrRoots { kind } = err);
    assert_eq!(kind, "Input");
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_orphan_validator() {
    let factories = CryptoFactories::default();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let network = Network::Igor;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_max_block_transaction_weight(325)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&[T, T, T], &consensus_constants, &key_manager).await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let orphan_validator = BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone());
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());

    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone()),
        orphan_validator.clone(),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        difficulty_calculator,
    )
    .unwrap();
    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features:
        OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
        OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx03, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
        OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx04, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![50_000 * uT], fee: 20*uT, lock: 2, features:
        OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    // this block should be okay
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_ok());

    // lets break the block weight
    let (template, _) = chain_block_with_new_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone(), tx03],
        &rules,
        None,
        &key_manager,
    )
    .await;
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());

    // lets break the sorting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let outputs = vec![template.body.outputs()[1].clone(), template.body.outputs()[2].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());

    // lets break spend rules
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx04.clone()], &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());

    // let break coinbase value
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        10000000.into(),
        1 + rules.consensus_constants(0).coinbase_min_maturity(),
        None,
        &key_manager,
    )
    .await;
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());

    // let break coinbase lock height
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        rules.get_block_reward_at(1) + tx01.body.get_total_fee() + tx02.body.get_total_fee(),
        1,
        None,
        &key_manager,
    )
    .await;
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());

    // lets break accounting
    let (mut template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, None, &key_manager).await;
    let outputs = vec![template.body.outputs()[1].clone(), tx04.body.outputs()[1].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    assert!(orphan_validator.validate_internal_consistency(&new_block).is_err());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_orphan_body_validation() {
    let factories = CryptoFactories::default();
    let network = Network::Igor;
    // we dont want localnet's 1 difficulty or the full mined difficulty of weather wax but we want some.
    let sha3x_constants = PowAlgorithmConstants {
        min_difficulty: Difficulty::from_u64(10).expect("valid difficulty"),
        max_difficulty: Difficulty::max(),
        target_time: 300,
    };
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3x, sha3x_constants)
        .build();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (genesis, outputs) = create_genesis_block_with_utxos(&[T, T, T], &consensus_constants, &key_manager).await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());
    let body_only_validator = BlockBodyFullValidator::new(rules.clone(), true);
    let header_validator = HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone());
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone()),
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
        &key_manager,
    )
    .await;
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, None, &key_manager).await;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();

    let timestamps = db.fetch_block_timestamps(new_block.header.prev_hash).unwrap();
    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &timestamps,
            None,
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
        .validate_body_with_metadata(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_ok());

    // lets break the chain sequence
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    new_block.header.height = 3;
    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &[],
            None,
        )
        .is_err());

    // lets have unknown inputs;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    let prev_header = db.fetch_header(new_block.header.height - 1).unwrap().unwrap();
    let test_params1 = TestParams::new(&key_manager).await;
    let test_params2 = TestParams::new(&key_manager).await;
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let key_manager_utxo = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &test_params1,
        outputs[1].value,
        &key_manager,
    )
    .await
    .unwrap();
    let key_manager_utxo2 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &test_params2,
        outputs[2].value,
        &key_manager,
    )
    .await
    .unwrap();
    let inputs = vec![
        key_manager_utxo.to_transaction_input(&key_manager).await.unwrap(),
        key_manager_utxo2.to_transaction_input(&key_manager).await.unwrap(),
    ];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    new_block.header.nonce = OsRng.next_u64();
    let timestamps = db.fetch_block_timestamps(new_block.header.prev_hash).unwrap();
    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &prev_header,
            &timestamps,
            None,
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
        .validate_body_with_metadata(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());

    // lets check duplicate txos
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let inputs = vec![new_block.body.inputs()[0].clone(), new_block.body.inputs()[0].clone()];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    let timestamps = db.fetch_block_timestamps(new_block.header.prev_hash).unwrap();
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &timestamps,
            None,
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
        .validate_body_with_metadata(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());

    // check mmr roots
    let mut new_block = db.prepare_new_block(template).unwrap();
    let prev_header = db.fetch_header(new_block.header.height - 1).unwrap().unwrap();
    new_block.header.output_mr = FixedHash::zero();
    new_block.header.nonce = OsRng.next_u64();

    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    let timestamps = db.fetch_block_timestamps(new_block.header.prev_hash).unwrap();
    let achieved_target_diff = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            &prev_header,
            &timestamps,
            None,
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
        .validate_body_with_metadata(&*db.db_read_access().unwrap(), &chain_block, &metadata)
        .is_err());
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_header_validation() {
    let factories = CryptoFactories::default();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let network = Network::Igor;
    // we dont want localnet's 1 difficulty or the full mined difficulty of weather wax but we want some.
    let sha3x_constants = PowAlgorithmConstants {
        min_difficulty: Difficulty::from_u64(20).expect("valid difficulty"),
        max_difficulty: Difficulty::max(),
        target_time: 300,
    };
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .clear_proof_of_work()
        .add_proof_of_work(PowAlgorithm::Sha3x, sha3x_constants)
        .build();
    let (genesis, outputs) = create_genesis_block_with_utxos(&[T, T, T], &consensus_constants, &key_manager).await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());
    let header_validator = HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone());
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone()),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone()),
    );
    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        difficulty_calculator,
    )
    .unwrap();
    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features:
OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features:
OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, None, &key_manager).await;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    let timestamps = db.fetch_block_timestamps(new_block.header.prev_hash).unwrap();

    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(20).unwrap());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &timestamps,
            None
        )
        .is_ok());

    // Lets break ftl rules
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    // we take the max ftl time and give 10 seconds for mining then check it, it should still be more than the ftl
    new_block.header.timestamp = rules.consensus_constants(0).ftl().increase(10);
    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(20).unwrap());
    assert!(header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &[],
            None
        )
        .is_err());

    // lets break difficulty
    let mut new_block = db.prepare_new_block(template).unwrap();
    new_block.header.nonce = OsRng.next_u64();
    find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
    let mut result = header_validator
        .validate(
            &*db.db_read_access().unwrap(),
            &new_block.header,
            genesis.header(),
            &[],
            None,
        )
        .is_err();
    new_block.header.nonce = OsRng.next_u64();
    let mut counter = 0;
    while counter < 10 && !result {
        counter += 1;
        new_block.header.nonce = OsRng.next_u64();
        find_header_with_achieved_difficulty(&mut new_block.header, Difficulty::from_u64(10).unwrap());
        result = header_validator
            .validate(
                &*db.db_read_access().unwrap(),
                &new_block.header,
                genesis.header(),
                &[],
                None,
            )
            .is_err();
    }
    assert!(result);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_block_sync_body_validator() {
    let factories = CryptoFactories::default();
    let network = Network::Igor;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_max_block_transaction_weight(400)
        .build();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (genesis, outputs) = create_genesis_block_with_utxos(&[T, T, T], &consensus_constants, &key_manager).await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants.clone())
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone()),
    );

    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    let validator = BlockBodyFullValidator::new(rules.clone(), true);

    // we have created the blockchain, lets create a second valid block

    let (tx01, _) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx02, _) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx03, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx04, _) = spend_utxos(
        txn_schema!(from: vec![outputs[3].clone()], to: vec![50_000 * uT], fee: 20*uT, lock: 2, features: OutputFeatures::default()),&key_manager
    ).await;

    // Coinbase extra field is too large
    let extra = iter::repeat(1u8).take(65).collect();
    let (template, _) = chain_block_with_new_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        &rules,
        Some(extra),
        &key_manager,
    )
    .await;
    let new_block = db.prepare_new_block(template).unwrap();
    let max_len = rules.consensus_constants(0).coinbase_output_features_extra_max_length();
    let err = {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err()
    };
    assert!(
        matches!(
            err,
            ValidationError::TransactionError(TransactionError::InvalidOutputFeaturesCoinbaseExtraSize{len, max }) if
            len == 65 && max == max_len
        ),
        "{}",
        err
    );

    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    // this block should be okay
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap();
    }

    // lets break the block weight
    let (template, _) = chain_block_with_new_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone(), tx03],
        &rules,
        None,
        &key_manager,
    )
    .await;
    let new_block = db.prepare_new_block(template).unwrap();

    assert!(
        new_block
            .body
            .calculate_weight(consensus_constants.transaction_weight_params())
            .expect("Failed to calculate weight") >
            400,
        "If this is not more than 400, then the next line should fail"
    );
    let err = {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err()
    };
    assert!(
        matches!(
            err,
            ValidationError::BlockTooLarge { actual_weight, max_weight } if
            actual_weight == 455 && max_weight == 400
        ),
        "{}",
        err
    );

    // lets break spend rules
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx04.clone()], &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // lets break the sorting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let output = vec![template.body.outputs()[1].clone(), template.body.outputs()[2].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), output, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // lets have unknown inputs;
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    let test_params1 = TestParams::new(&key_manager).await;
    let test_params2 = TestParams::new(&key_manager).await;
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let unblinded_utxo = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &test_params1,
        outputs[1].value,
        &key_manager,
    )
    .await
    .unwrap();
    let unblinded_utxo2 = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &test_params2,
        outputs[2].value,
        &key_manager,
    )
    .await
    .unwrap();
    let inputs = vec![
        unblinded_utxo.to_transaction_input(&key_manager).await.unwrap(),
        unblinded_utxo2.to_transaction_input(&key_manager).await.unwrap(),
    ];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // lets check duplicate txos
    let (template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let mut new_block = db.prepare_new_block(template.clone()).unwrap();
    // We dont need proper utxo's with signatures as the post_orphan validator does not check accounting balance +
    // signatures.
    let inputs = vec![new_block.body.inputs()[0].clone(), new_block.body.inputs()[0].clone()];
    new_block.body = AggregateBody::new(inputs, template.body.outputs().clone(), template.body.kernels().clone());
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // let break coinbase value
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        10000000.into(),
        1 + rules.consensus_constants(0).coinbase_min_maturity(),
        None,
        &key_manager,
    )
    .await;
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // let break coinbase lock height
    let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
        rules.get_block_reward_at(1) + tx01.body.get_total_fee() + tx02.body.get_total_fee(),
        1 + rules.consensus_constants(1).coinbase_min_maturity(),
        None,
        &key_manager,
    )
    .await;
    let template = chain_block_with_coinbase(
        &genesis,
        vec![tx01.clone(), tx02.clone()],
        coinbase_utxo,
        coinbase_kernel,
        &rules,
    );
    let new_block = db.prepare_new_block(template).unwrap();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap();
    }

    // lets break accounting
    let (mut template, _) =
        chain_block_with_new_coinbase(&genesis, vec![tx01.clone(), tx02.clone()], &rules, None, &key_manager).await;
    let outputs = vec![template.body.outputs()[1].clone(), tx04.body.outputs()[1].clone()];
    template.body = AggregateBody::new(template.body.inputs().clone(), outputs, template.body.kernels().clone());
    let new_block = db.prepare_new_block(template).unwrap();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }

    // lets the mmr root
    let (template, _) = chain_block_with_new_coinbase(&genesis, vec![tx01, tx02], &rules, None, &key_manager).await;
    let mut new_block = db.prepare_new_block(template).unwrap();
    new_block.header.output_mr = FixedHash::zero();
    {
        // `MutexGuard` cannot be held across an `await` point
        let txn = db.db_read_access().unwrap();
        validator.validate_body(&*txn, &new_block).unwrap_err();
    }
}

#[tokio::test]
async fn add_block_with_large_block() {
    // we use this test to benchmark a block with multiple inputs and outputs
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (genesis, outputs) = create_genesis_block_with_utxos(
        &[
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
            5 * T,
        ],
        &consensus_constants,
        &key_manager,
    )
    .await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants.clone())
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), false),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone()),
    );

    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    // lets make our big block (1 -> 5) * 12
    let mut schemas = Vec::new();
    for output in outputs.into_iter().skip(1) {
        let new_schema = txn_schema!(from: vec![output], to: vec![1 * T, 1 * T, 1 * T, 1 * T]);
        schemas.push(new_schema);
    }

    let (txs, _outputs) = schema_to_transaction(&schemas, &key_manager).await;
    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (template, _) = chain_block_with_new_coinbase(&genesis, txs, &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    println!(
        "Total block weight is : {}",
        new_block
            .body
            .calculate_weight(rules.consensus_constants(0).transaction_weight_params())
            .unwrap()
    );
    let start = Instant::now();
    let _unused = db.add_block(Arc::new(new_block)).unwrap();
    let finished = start.elapsed();
    // this here here for benchmarking purposes.
    // we can extrapolate full block validation by 35.7, this we get from the 127_795/block_weight
    // of the block
    println!("finished validating in: {}", finished.as_millis());
}

#[tokio::test]
async fn add_block_with_large_many_output_block() {
    // we use this test to benchmark a block with multiple outputs
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_max_block_transaction_weight(127_795)
        .build();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (genesis, outputs) = create_genesis_block_with_utxos(&[501 * T], &consensus_constants, &key_manager).await;
    let network = Network::LocalNet;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants.clone())
        .with_block(genesis.clone())
        .build()
        .unwrap();
    let backend = create_test_db();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), Default::default());
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), false),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator),
        BlockBodyInternalConsistencyValidator::new(rules.clone(), false, factories.clone()),
    );

    let db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();
    // lets make our big block (1 -> 5) * 12
    let mut outs = Vec::new();
    // create 498 outputs, so we have a block with 500 outputs, 498 + change + coinbase
    for _ in 0..498 {
        outs.push(1 * T);
    }

    let schema = txn_schema!(from: vec![outputs[1].clone()], to: outs);
    let (txs, _outputs) = schema_to_transaction(&[schema], &key_manager).await;

    let txs = txs.into_iter().map(|t| Arc::try_unwrap(t).unwrap()).collect::<Vec<_>>();
    let (template, _) = chain_block_with_new_coinbase(&genesis, txs, &rules, None, &key_manager).await;
    let new_block = db.prepare_new_block(template).unwrap();
    println!(
        "Total block weight is : {}",
        new_block
            .body
            .calculate_weight(rules.consensus_constants(0).transaction_weight_params())
            .unwrap()
    );
    let start = Instant::now();
    let _unused = db.add_block(Arc::new(new_block)).unwrap();
    let finished = start.elapsed();
    // this here here for benchmarking purposes.
    // we can extrapolate full block validation by 4.59, this we get from the 127_795/block_weight
    // of the block
    println!("finished validating in: {}", finished.as_millis());
}

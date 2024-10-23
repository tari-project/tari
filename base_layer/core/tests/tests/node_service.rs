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

use std::{sync::Arc, time::Duration};

use randomx_rs::RandomXFlag;
use tari_common::configuration::Network;
use tari_comms::{connectivity::ConnectivityEvent, protocol::messaging::MessagingEvent};
use tari_core::{
    base_node::{
        comms_interface::BlockEvent,
        state_machine_service::states::{ListeningInfo, StateInfo, StatusInfo},
    },
    blocks::{ChainBlock, NewBlock},
    chain_storage::BlockchainDatabaseConfig,
    consensus::{ConsensusManager, ConsensusManagerBuilder, NetworkConsensus},
    mempool::TxStorageResponse,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty, PowAlgorithm},
    transactions::{
        key_manager::create_memory_db_key_manager,
        tari_amount::{uT, T},
        test_helpers::{schema_to_transaction, spend_utxos},
        transaction_components::OutputFeatures,
        CryptoFactories,
    },
    txn_schema,
    validation::{
        block_body::{BlockBodyFullValidator, BlockBodyInternalConsistencyValidator},
        header::HeaderFullValidator,
        mocks::MockValidator,
        DifficultyCalculator,
    },
};
use tari_test_utils::unpack_enum;
use tempfile::tempdir;

use crate::{
    helpers::{
        block_builders::{
            append_block,
            chain_block,
            construct_chained_blocks,
            create_coinbase,
            create_genesis_block,
            create_genesis_block_with_utxos,
        },
        event_stream::event_stream_next,
        nodes::{random_node_identity, wait_until_online, BaseNodeBuilder},
    },
    tests::assert_block_add_result_added,
};

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn propagate_and_forward_many_valid_blocks() {
    let temp_dir = tempdir().unwrap();
    let key_manager = create_memory_db_key_manager().unwrap();
    // Alice will propagate a number of block hashes to bob, bob will receive it, request the full block, verify and
    // then propagate the hash to carol and dan. Dan and Carol will also try to propagate the block hashes to each
    // other, but the block should not be re-requested. These duplicate blocks will be discarded and wont be
    // propagated again.
    //              /-> carol <-\
    //             /             |
    // alice -> bob              |
    //             \             |
    //              \->  dan  <-/
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let dan_node_identity = random_node_identity();
    let network = Network::LocalNet;
    let consensus_constants = crate::helpers::sample_blockchains::consensus_constants(network).build();
    let (block0, outputs) = create_genesis_block_with_utxos(&[T, T], &consensus_constants, &key_manager).await;

    let (tx01, _tx01_out) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;

    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build()
        .unwrap();
    let (mut alice_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity.clone())
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("alice").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut bob_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("bob").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut carol_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("carol").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut dan_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(dan_node_identity)
        .with_peers(vec![carol_node_identity, bob_node_identity])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("dan").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;

    wait_until_online(&[&alice_node, &bob_node, &carol_node, &dan_node]).await;
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    dan_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });

    let mut bob_block_event_stream = bob_node.local_nci.get_block_event_stream();
    let mut carol_block_event_stream = carol_node.local_nci.get_block_event_stream();
    let mut dan_block_event_stream = dan_node.local_nci.get_block_event_stream();

    let mut blocks = Vec::with_capacity(6);
    blocks.push(
        append_block(
            &alice_node.blockchain_db,
            &block0,
            vec![tx01],
            &rules,
            Difficulty::min(),
            &key_manager,
        )
        .await
        .unwrap()
        .0,
    );
    blocks
        .extend(construct_chained_blocks(&alice_node.blockchain_db, blocks[0].clone(), &rules, 5, &key_manager).await);

    for block in &blocks {
        alice_node
            .outbound_nci
            .propagate_block(NewBlock::from(block.block()), vec![])
            .await
            .unwrap();

        let bob_block_event_fut = event_stream_next(&mut bob_block_event_stream, Duration::from_millis(20000));
        let carol_block_event_fut = event_stream_next(&mut carol_block_event_stream, Duration::from_millis(20000));
        let dan_block_event_fut = event_stream_next(&mut dan_block_event_stream, Duration::from_millis(20000));
        let (bob_block_event, carol_block_event, dan_block_event) =
            tokio::join!(bob_block_event_fut, carol_block_event_fut, dan_block_event_fut);
        let block_hash = block.hash();

        if let BlockEvent::ValidBlockAdded(received_block, _) = &*bob_block_event.unwrap() {
            assert_eq!(&received_block.hash(), block_hash);
        } else {
            panic!("Bob's node did not receive and validate the expected block");
        }
        if let BlockEvent::ValidBlockAdded(received_block, _block_add_result) = &*carol_block_event.unwrap() {
            assert_eq!(&received_block.hash(), block_hash);
        } else {
            panic!("Carol's node did not receive and validate the expected block");
        }
        if let BlockEvent::ValidBlockAdded(received_block, _block_add_result) = &*dan_block_event.unwrap() {
            assert_eq!(&received_block.hash(), block_hash);
        } else {
            panic!("Dan's node did not receive and validate the expected block");
        }
    }

    alice_node.shutdown().await;
    bob_node.shutdown().await;
    carol_node.shutdown().await;
    dan_node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(clippy::too_many_lines)]
async fn propagate_and_forward_invalid_block_hash() {
    // Alice will propagate a "made up" block hash to Bob, Bob will request the block from Alice. Alice will not be able
    // to provide the block and so Bob will not propagate the hash further to Carol.
    // alice -> bob -> carol

    let temp_dir = tempdir().unwrap();

    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let consensus_constants = crate::helpers::sample_blockchains::consensus_constants(network).build();
    let (block0, genesis_coinbase) = create_genesis_block(&consensus_constants, &key_manager).await;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build()
        .unwrap();
    let (mut alice_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity.clone())
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("alice").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut bob_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("bob").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut carol_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(carol_node_identity)
        .with_peers(vec![bob_node_identity])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("carol").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;

    wait_until_online(&[&alice_node, &bob_node, &carol_node]).await;
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });

    // Add a transaction that Bob does not have to force a request
    let (txs, _) = schema_to_transaction(
        &[txn_schema!(from: vec![genesis_coinbase.clone()], to: vec![5 * T], fee: 5.into())],
        &key_manager,
    )
    .await;
    let txs = txs.into_iter().map(|tx| (*tx).clone()).collect();
    let (block1, _) = append_block(
        &alice_node.blockchain_db,
        &block0,
        txs,
        &rules,
        Difficulty::min(),
        &key_manager,
    )
    .await
    .unwrap();
    let block1 = {
        // Create unknown block hash
        let mut block = block1.block().clone();
        block.header.version += 1;
        let mut accum_data = block1.accumulated_data().clone();
        accum_data.hash = block.hash();
        ChainBlock::try_construct(block.into(), accum_data).unwrap()
    };

    let mut bob_message_events = bob_node.messaging_events.subscribe();
    let mut carol_message_events = carol_node.messaging_events.subscribe();

    alice_node
        .outbound_nci
        .propagate_block(NewBlock::from(block1.block()), vec![])
        .await
        .unwrap();

    // Alice propagated to Bob
    // Bob received the invalid hash
    let msg_event = event_stream_next(&mut bob_message_events, Duration::from_secs(10))
        .await
        .unwrap();
    unpack_enum!(MessagingEvent::MessageReceived(_a, _b) = &msg_event);

    // Bob asks Alice for missing transaction
    let msg_event = event_stream_next(&mut bob_message_events, Duration::from_secs(10))
        .await
        .unwrap();
    unpack_enum!(MessagingEvent::MessageReceived(node_id, _a) = &msg_event);
    assert_eq!(node_id, alice_node.node_identity.node_id());

    // Checking a negative: Bob should not have propagated this hash to Carol. If Bob does, this assertion will be
    // flaky.
    let msg_event = event_stream_next(&mut carol_message_events, Duration::from_secs(1)).await;
    assert!(msg_event.is_none());

    alice_node.shutdown().await;
    bob_node.shutdown().await;
    carol_node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[allow(clippy::too_many_lines)]
async fn propagate_and_forward_invalid_block() {
    let temp_dir = tempdir().unwrap();
    let factories = CryptoFactories::default();
    // Alice will propagate an invalid block to Carol and Bob, they will check the received block and not propagate the
    // block to dan.
    //       /->  bob  -\
    //      /            \
    // alice              -> dan
    //      \            /
    //       \-> carol -/
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let dan_node_identity = random_node_identity();
    let key_manager = create_memory_db_key_manager().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = crate::helpers::sample_blockchains::consensus_constants(network).build();
    let (block0, _) = create_genesis_block(&consensus_constants, &key_manager).await;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build()
        .unwrap();
    let stateless_block_validator = BlockBodyInternalConsistencyValidator::new(rules.clone(), true, factories);

    let mock_validator = MockValidator::new(false);
    let (mut dan_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(dan_node_identity.clone())
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("dan").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut carol_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![dan_node_identity.clone()])
        .with_consensus_manager(rules)
        .with_validators(
            mock_validator.clone(),
            mock_validator.clone(),
            stateless_block_validator.clone(),
        )
        .start(
            temp_dir.path().join("carol").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut bob_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![dan_node_identity])
        .with_consensus_manager(rules)
        .with_validators(mock_validator.clone(), mock_validator, stateless_block_validator)
        .start(
            temp_dir.path().join("bob").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;
    let (mut alice_node, rules) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .with_consensus_manager(rules)
        .start(
            temp_dir.path().join("alice").to_str().unwrap(),
            BlockchainDatabaseConfig::default(),
        )
        .await;

    alice_node
        .comms
        .connectivity()
        .dial_peer(bob_node.node_identity.node_id().clone(), false)
        .await
        .unwrap();
    wait_until_online(&[&alice_node, &bob_node, &carol_node, &dan_node]).await;

    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });
    dan_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true, 0, 0)),
        randomx_vm_cnt: 0,
        randomx_vm_flags: RandomXFlag::FLAG_DEFAULT,
    });

    // This is a valid block, however Bob, Carol and Dan's block validator is set to always reject the block
    // after fetching it.
    let (block1, _) = append_block(
        &alice_node.blockchain_db,
        &block0,
        vec![],
        &rules,
        Difficulty::min(),
        &key_manager,
    )
    .await
    .unwrap();
    let block1_hash = block1.hash();

    let mut bob_connectivity_events = bob_node.comms.connectivity().get_event_subscription();
    assert!(alice_node
        .outbound_nci
        .propagate_block(NewBlock::from(block1.block()), vec![])
        .await
        .is_ok());

    #[allow(unused_assignments)]
    let mut has_banned = false;
    loop {
        let event = event_stream_next(&mut bob_connectivity_events, Duration::from_secs(10))
            .await
            .unwrap();
        #[allow(clippy::single_match)]
        match event {
            ConnectivityEvent::PeerBanned(node_id) => {
                assert_eq!(node_id, *alice_node.node_identity.node_id());
                has_banned = true;
                break;
            },
            _ => {},
        }
    }
    assert!(has_banned);

    assert!(!bob_node
        .blockchain_db
        .chain_block_or_orphan_block_exists(*block1_hash)
        .unwrap());
    assert!(!carol_node
        .blockchain_db
        .chain_block_or_orphan_block_exists(*block1_hash)
        .unwrap());
    assert!(!dan_node
        .blockchain_db
        .chain_block_or_orphan_block_exists(*block1_hash)
        .unwrap());

    alice_node.shutdown().await;
    bob_node.shutdown().await;
    carol_node.shutdown().await;
    dan_node.shutdown().await;
}

#[tokio::test]
async fn local_get_metadata() {
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let (mut node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
        .await;
    let db = &node.blockchain_db;
    let block0 = db.fetch_block(0, true).unwrap().try_into_chain_block().unwrap();
    let (block1, _) = append_block(db, &block0, vec![], &consensus_manager, Difficulty::min(), &key_manager)
        .await
        .unwrap();
    let (block2, _) = append_block(db, &block1, vec![], &consensus_manager, Difficulty::min(), &key_manager)
        .await
        .unwrap();

    let metadata = node.local_nci.get_metadata().await.unwrap();
    assert_eq!(metadata.best_block_height(), 2);
    assert_eq!(metadata.best_block_hash(), block2.hash());

    node.shutdown().await;
}

#[tokio::test]
async fn local_get_new_block_template_and_get_new_block() {
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let consensus_constants = NetworkConsensus::from(network).create_consensus_constants();
    let (block0, outputs) = create_genesis_block_with_utxos(&[T, T], &consensus_constants[0], &key_manager).await;
    let rules = ConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants[0].clone())
        .with_block(block0)
        .build()
        .unwrap();
    let (mut node, _rules) = BaseNodeBuilder::new(network.into())
        .with_consensus_manager(rules)
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
        .await;

    let schema = [
        txn_schema!(from: vec![outputs[1].clone()], to: vec![10_000 * uT, 20_000 * uT]),
        txn_schema!(from: vec![outputs[2].clone()], to: vec![30_000 * uT, 40_000 * uT]),
    ];
    let (txs, _) = schema_to_transaction(&schema, &key_manager).await;
    node.mempool.insert(txs[0].clone()).await.unwrap();
    node.mempool.insert(txs[1].clone()).await.unwrap();

    let block_template = node
        .local_nci
        .get_new_block_template(PowAlgorithm::Sha3x, 0)
        .await
        .unwrap();
    assert_eq!(block_template.header.height, 1);
    assert_eq!(block_template.body.kernels().len(), 2);

    let block = node.local_nci.get_new_block(block_template.clone()).await.unwrap();
    assert_eq!(block.header.height, 1);
    assert_eq!(block.body, block_template.body);

    node.blockchain_db.add_block(block.clone().into()).unwrap();

    node.shutdown().await;
}

#[tokio::test]
async fn local_get_new_block_with_zero_conf() {
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let consensus_constants = NetworkConsensus::from(network).create_consensus_constants();
    let (block0, outputs) = create_genesis_block_with_utxos(&[T, T], &consensus_constants[0], &key_manager).await;
    let rules = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants[0].clone())
        .with_block(block0)
        .build()
        .unwrap();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    let (mut node, rules) = BaseNodeBuilder::new(network.into())
        .with_consensus_manager(rules.clone())
        .with_validators(
            BlockBodyFullValidator::new(rules.clone(), true),
            HeaderFullValidator::new(rules.clone(), difficulty_calculator),
            BlockBodyInternalConsistencyValidator::new(rules, true, factories.clone()),
        )
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
        .await;

    let (tx01, tx01_out) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx02, tx02_out) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    assert_eq!(
        node.mempool.insert(Arc::new(tx01)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        node.mempool.insert(Arc::new(tx02)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    let (tx11, _) = spend_utxos(
        txn_schema!(from: tx01_out, to: vec![10_000 * uT], fee: 50*uT, lock: 0, features: OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx12, _) = spend_utxos(
        txn_schema!(from: tx02_out, to: vec![20_000 * uT], fee: 60*uT, lock: 0, features: OutputFeatures::default()),
        &key_manager,
    )
    .await;
    assert_eq!(
        node.mempool.insert(Arc::new(tx11)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        node.mempool.insert(Arc::new(tx12)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    let mut block_template = node
        .local_nci
        .get_new_block_template(PowAlgorithm::Sha3x, 0)
        .await
        .unwrap();
    assert_eq!(block_template.header.height, 1);
    assert_eq!(block_template.body.kernels().len(), 4);
    let coinbase_value = rules.get_block_reward_at(1) + block_template.body.get_total_fee().unwrap();
    let (output, kernel, _) = create_coinbase(
        coinbase_value,
        rules.consensus_constants(1).coinbase_min_maturity() + 1,
        None,
        &key_manager,
    )
    .await;
    block_template.body.add_kernel(kernel);
    block_template.body.add_output(output);
    block_template.body.sort();
    let block = node.local_nci.get_new_block(block_template.clone()).await.unwrap();
    assert_eq!(block.header.height, 1);
    assert_eq!(block.body, block_template.body);
    assert_eq!(block_template.body.kernels().len(), 5);

    node.blockchain_db.add_block(block.clone().into()).unwrap();

    node.shutdown().await;
}

#[tokio::test]
async fn local_get_new_block_with_combined_transaction() {
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let consensus_constants = NetworkConsensus::from(network).create_consensus_constants();
    let (block0, outputs) = create_genesis_block_with_utxos(&[T, T], &consensus_constants[0], &key_manager).await;
    let rules = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants[0].clone())
        .with_block(block0)
        .build()
        .unwrap();
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), RandomXFactory::default());
    let (mut node, rules) = BaseNodeBuilder::new(network.into())
        .with_consensus_manager(rules.clone())
        .with_validators(
            BlockBodyFullValidator::new(rules.clone(), true),
            HeaderFullValidator::new(rules.clone(), difficulty_calculator),
            BlockBodyInternalConsistencyValidator::new(rules, true, factories.clone()),
        )
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
        .await;

    let (tx01, tx01_out) = spend_utxos(
        txn_schema!(from: vec![outputs[1].clone()], to: vec![20_000 * uT], fee: 10*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx02, tx02_out) = spend_utxos(
        txn_schema!(from: vec![outputs[2].clone()], to: vec![40_000 * uT], fee: 20*uT, lock: 0, features: OutputFeatures::default()),&key_manager
    ).await;
    let (tx11, _) = spend_utxos(
        txn_schema!(from: tx01_out, to: vec![10_000 * uT], fee: 50*uT, lock: 0, features: OutputFeatures::default()),
        &key_manager,
    )
    .await;
    let (tx12, _) = spend_utxos(
        txn_schema!(from: tx02_out, to: vec![20_000 * uT], fee: 60*uT, lock: 0, features: OutputFeatures::default()),
        &key_manager,
    )
    .await;

    // lets create combined transactions
    let tx1 = tx01 + tx11;
    let tx2 = tx02 + tx12;
    assert_eq!(
        node.mempool.insert(Arc::new(tx1)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );
    assert_eq!(
        node.mempool.insert(Arc::new(tx2)).await.unwrap(),
        TxStorageResponse::UnconfirmedPool
    );

    let mut block_template = node
        .local_nci
        .get_new_block_template(PowAlgorithm::Sha3x, 0)
        .await
        .unwrap();
    assert_eq!(block_template.header.height, 1);
    assert_eq!(block_template.body.kernels().len(), 4);
    let coinbase_value = rules.get_block_reward_at(1) + block_template.body.get_total_fee().unwrap();
    let (output, kernel, _) = create_coinbase(
        coinbase_value,
        rules.consensus_constants(1).coinbase_min_maturity() + 1,
        None,
        &key_manager,
    )
    .await;
    block_template.body.add_kernel(kernel);
    block_template.body.add_output(output);
    block_template.body.sort();
    let block = node.local_nci.get_new_block(block_template.clone()).await.unwrap();
    assert_eq!(block.header.height, 1);
    assert_eq!(block.body, block_template.body);
    assert_eq!(block_template.body.kernels().len(), 5);

    node.blockchain_db.add_block(block.clone().into()).unwrap();

    node.shutdown().await;
}

#[tokio::test]
async fn local_submit_block() {
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let key_manager = create_memory_db_key_manager().unwrap();
    let (mut node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
        .await;

    let db = &node.blockchain_db;
    let mut event_stream = node.local_nci.get_block_event_stream();
    let block0 = db.fetch_block(0, true).unwrap().block().clone();
    let mut block1 = db
        .prepare_new_block(chain_block(&block0, vec![], &consensus_manager, &key_manager).await)
        .unwrap();
    block1.header.kernel_mmr_size += 1;
    block1.header.output_smt_size += 1;
    node.local_nci.submit_block(block1.clone()).await.unwrap();

    let event = event_stream_next(&mut event_stream, Duration::from_millis(20000)).await;
    if let BlockEvent::ValidBlockAdded(received_block, result) = &*event.unwrap() {
        assert_eq!(received_block.hash(), block1.hash());
        assert_block_add_result_added(result);
    } else {
        panic!("Block validation failed");
    }

    node.shutdown().await;
}

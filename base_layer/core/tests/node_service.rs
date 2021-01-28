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
use crate::helpers::block_builders::construct_chained_blocks;
use futures::join;
use helpers::{
    block_builders::{
        append_block,
        chain_block,
        create_genesis_block,
        create_genesis_block_with_utxos,
        generate_block,
    },
    event_stream::event_stream_next,
    nodes::{
        create_network_with_2_base_nodes,
        create_network_with_2_base_nodes_with_config,
        create_network_with_3_base_nodes,
        create_network_with_3_base_nodes_with_config,
        random_node_identity,
        wait_until_online,
        BaseNodeBuilder,
    },
};
use std::time::Duration;
use tari_comms::protocol::messaging::MessagingEvent;
use tari_core::{
    base_node::{
        comms_interface::{BlockEvent, Broadcast, CommsInterfaceError},
        service::BaseNodeServiceConfig,
        state_machine_service::states::{ListeningInfo, StateInfo, StatusInfo},
    },
    blocks::{BlockHeader, NewBlock},
    chain_storage::{BlockAddResult, BlockchainDatabaseConfig, DbTransaction},
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    mempool::MempoolServiceConfig,
    proof_of_work::{Difficulty, PowAlgorithm},
    transactions::{
        helpers::schema_to_transaction,
        tari_amount::{uT, T},
        types::CryptoFactories,
    },
    txn_schema,
    validation::{block_validators::OrphanBlockValidator, mocks::MockValidator},
};
use tari_crypto::tari_utilities::hash::Hashable;
use tari_p2p::services::liveness::LivenessConfig;
use tari_test_utils::unpack_enum;
use tempfile::tempdir;
use tokio::runtime::Runtime;

#[test]
fn request_response_get_metadata() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (mut alice_node, bob_node, carol_node, _consensus_manager) = create_network_with_3_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );

    runtime.block_on(async {
        let received_metadata = alice_node.outbound_nci.get_metadata().await.unwrap();
        assert_eq!(received_metadata.height_of_longest_chain(), 0);

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
    });
}

#[test]
fn request_and_response_fetch_blocks() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (mut alice_node, mut bob_node, carol_node, _) = create_network_with_3_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager.clone(),
        temp_dir.path().to_str().unwrap(),
    );

    let mut blocks = vec![block0];
    let db = &mut bob_node.blockchain_db;
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();

    carol_node
        .blockchain_db
        .add_block(blocks[1].block.clone().into())
        .unwrap()
        .assert_added();
    carol_node
        .blockchain_db
        .add_block(blocks[2].block.clone().into())
        .unwrap()
        .assert_added();

    runtime.block_on(async {
        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0]).await.unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0].block(), &blocks[0].block);

        let received_blocks = alice_node.outbound_nci.fetch_blocks(vec![0, 1]).await.unwrap();
        assert_eq!(received_blocks.len(), 2);
        assert_ne!(*received_blocks[0].block(), *received_blocks[1].block());
        assert!((received_blocks[0].block() == &blocks[0].block) || (received_blocks[1].block() == &blocks[0].block));
        assert!((received_blocks[0].block() == &blocks[1].block) || (received_blocks[1].block() == &blocks[1].block));

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
    });
}

#[test]
fn request_and_response_fetch_blocks_with_hashes() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (mut alice_node, mut bob_node, carol_node, _) = create_network_with_3_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager.clone(),
        temp_dir.path().to_str().unwrap(),
    );

    let mut blocks = vec![block0];
    let db = &mut bob_node.blockchain_db;
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();
    generate_block(db, &mut blocks, vec![], &consensus_manager).unwrap();
    let block0_hash = blocks[0].hash();
    let block1_hash = blocks[1].hash();

    carol_node
        .blockchain_db
        .add_block(blocks[1].block.clone().into())
        .unwrap()
        .assert_added();
    carol_node
        .blockchain_db
        .add_block(blocks[2].block.clone().into())
        .unwrap()
        .assert_added();

    runtime.block_on(async {
        let received_blocks = alice_node
            .outbound_nci
            .fetch_blocks_with_hashes(vec![block0_hash.clone()])
            .await
            .unwrap();
        assert_eq!(received_blocks.len(), 1);
        assert_eq!(received_blocks[0].block(), &blocks[0].block);

        let received_blocks = alice_node
            .outbound_nci
            .fetch_blocks_with_hashes(vec![block0_hash.clone(), block1_hash.clone()])
            .await
            .unwrap();
        assert_eq!(received_blocks.len(), 2);
        assert_ne!(received_blocks[0], received_blocks[1]);
        assert!((*received_blocks[0].block() == blocks[0].block) || (*received_blocks[1].block() == blocks[0].block));
        assert!((*received_blocks[0].block() == blocks[1].block) || (*received_blocks[1].block() == blocks[1].block));

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
    });
}

#[test]
fn propagate_and_forward_many_valid_blocks() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let factories = CryptoFactories::default();
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
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let rules = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (mut alice_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("alice").to_str().unwrap());
    let (mut bob_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("bob").to_str().unwrap());
    let (mut carol_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("carol").to_str().unwrap());
    let (mut dan_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(dan_node_identity)
        .with_peers(vec![carol_node_identity, bob_node_identity])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("dan").to_str().unwrap());

    wait_until_online(&mut runtime, &[&alice_node, &bob_node, &carol_node, &dan_node]);
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    dan_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });

    let mut bob_block_event_stream = bob_node.local_nci.get_block_event_stream();
    let mut carol_block_event_stream = carol_node.local_nci.get_block_event_stream();
    let mut dan_block_event_stream = dan_node.local_nci.get_block_event_stream();

    let blocks = construct_chained_blocks(&alice_node.blockchain_db, block0, &rules, 5);

    runtime.block_on(async {
        for block in &blocks {
            alice_node
                .outbound_nci
                .propagate_block(NewBlock::from(&block.block), vec![])
                .await
                .unwrap();

            let bob_block_event_fut = event_stream_next(&mut bob_block_event_stream, Duration::from_millis(20000));
            let carol_block_event_fut = event_stream_next(&mut carol_block_event_stream, Duration::from_millis(20000));
            let dan_block_event_fut = event_stream_next(&mut dan_block_event_stream, Duration::from_millis(20000));
            let (bob_block_event, carol_block_event, dan_block_event) =
                join!(bob_block_event_fut, carol_block_event_fut, dan_block_event_fut);
            let block_hash = block.hash();

            if let BlockEvent::ValidBlockAdded(received_block, _, _) = &*bob_block_event.unwrap().unwrap() {
                assert_eq!(&received_block.hash(), block_hash);
            } else {
                panic!("Bob's node did not receive and validate the expected block");
            }
            if let BlockEvent::ValidBlockAdded(received_block, _block_add_result, _) =
                &*carol_block_event.unwrap().unwrap()
            {
                assert_eq!(&received_block.hash(), block_hash);
            } else {
                panic!("Carol's node did not receive and validate the expected block");
            }
            if let BlockEvent::ValidBlockAdded(received_block, _block_add_result, _) =
                &*dan_block_event.unwrap().unwrap()
            {
                assert_eq!(&received_block.hash(), block_hash);
            } else {
                panic!("Dan's node did not receive and validate the expected block");
            }
        }

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
        dan_node.shutdown().await;
    });
}
static EMISSION: [u64; 2] = [10, 10];
#[test]
fn propagate_and_forward_invalid_block_hash() {
    // Alice will propagate a "made up" block hash to Bob, Bob will request the block from Alice. Alice will not be able
    // to provide the block and so Bob will not propagate the hash further to Carol.
    // alice -> bob -> carol

    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let factories = CryptoFactories::default();

    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let rules = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (mut alice_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("alice").to_str().unwrap());
    let (mut bob_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("bob").to_str().unwrap());
    let (mut carol_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("carol").to_str().unwrap());

    wait_until_online(&mut runtime, &[&alice_node, &bob_node, &carol_node]);
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });

    let mut block1 = append_block(&alice_node.blockchain_db, &block0, vec![], &rules, 1.into()).unwrap();
    // Create unknown block hash
    block1.block.header.height = 0;

    let mut bob_message_events = bob_node.messaging_events.subscribe();
    let mut carol_message_events = carol_node.messaging_events.subscribe();

    runtime.block_on(async {
        alice_node
            .outbound_nci
            .propagate_block(NewBlock::from(&block1.block), vec![])
            .await
            .unwrap();

        // Alice propagated to Bob
        // Bob received the invalid hash
        let msg_event = event_stream_next(&mut bob_message_events, Duration::from_secs(10))
            .await
            .unwrap()
            .unwrap();
        unpack_enum!(MessagingEvent::MessageReceived(_a, _b) = &*msg_event);
        // Sent the request for the block to Alice
        // Bob received a response from Alice
        let msg_event = event_stream_next(&mut bob_message_events, Duration::from_secs(10))
            .await
            .unwrap()
            .unwrap();
        unpack_enum!(MessagingEvent::MessageReceived(node_id, _a) = &*msg_event);
        assert_eq!(&*node_id, alice_node.node_identity.node_id());
        // Checking a negative: Bob should not have propagated this hash to Carol. If Bob does, this assertion will be
        // flaky.
        let msg_event = event_stream_next(&mut carol_message_events, Duration::from_millis(500)).await;
        assert!(msg_event.is_none());

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
    });
}

#[test]
fn propagate_and_forward_invalid_block() {
    let mut runtime = Runtime::new().unwrap();
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
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (block0, _) = create_genesis_block(&factories, &consensus_constants);
    let rules = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let stateless_block_validator = OrphanBlockValidator::new(rules.clone(), factories.clone());

    let mock_validator = MockValidator::new(false);
    let (mut dan_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(dan_node_identity.clone())
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("dan").to_str().unwrap());
    let (mut carol_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![dan_node_identity.clone()])
        .with_consensus_manager(rules)
        .with_validators(
            mock_validator.clone(),
            mock_validator.clone(),
            stateless_block_validator.clone(),
        )
        .start(&mut runtime, temp_dir.path().join("carol").to_str().unwrap());
    let (mut bob_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![dan_node_identity.clone()])
        .with_consensus_manager(rules)
        .with_validators(
            mock_validator.clone(),
            mock_validator.clone(),
            stateless_block_validator.clone(),
        )
        .start(&mut runtime, temp_dir.path().join("bob").to_str().unwrap());
    let (mut alice_node, rules) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity)
        .with_peers(vec![bob_node_identity.clone(), carol_node_identity.clone()])
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().join("alice").to_str().unwrap());

    wait_until_online(&mut runtime, &[&alice_node, &bob_node, &carol_node, &dan_node]);

    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    bob_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    carol_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    dan_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });

    // This is a valid block, however Bob, Carol and Dan's block validator is set to always reject the block
    // after fetching it.
    let block1 = append_block(&alice_node.blockchain_db, &block0, vec![], &rules, 1.into()).unwrap();
    let block1_hash = block1.hash();

    runtime.block_on(async {
        let mut bob_block_event_stream = bob_node.local_nci.get_block_event_stream();
        let mut carol_block_event_stream = carol_node.local_nci.get_block_event_stream();
        let mut dan_block_event_stream = dan_node.local_nci.get_block_event_stream();

        assert!(alice_node
            .outbound_nci
            .propagate_block(NewBlock::from(&block1.block), vec![])
            .await
            .is_ok());

        let bob_block_event_fut = event_stream_next(&mut bob_block_event_stream, Duration::from_millis(20000));
        let carol_block_event_fut = event_stream_next(&mut carol_block_event_stream, Duration::from_millis(20000));
        let dan_block_event_fut = event_stream_next(&mut dan_block_event_stream, Duration::from_millis(5000));
        let (bob_block_event, carol_block_event, dan_block_event) =
            join!(bob_block_event_fut, carol_block_event_fut, dan_block_event_fut);

        if let BlockEvent::AddBlockFailed(received_block, _) = &*bob_block_event.unwrap().unwrap() {
            assert_eq!(&received_block.hash(), block1_hash);
        } else {
            panic!("Bob's node should have detected an invalid block");
        }
        if let BlockEvent::AddBlockFailed(received_block, _) = &*carol_block_event.unwrap().unwrap() {
            assert_eq!(&received_block.hash(), block1_hash);
        } else {
            panic!("Carol's node should have detected an invalid block");
        }
        assert!(dan_block_event.is_none());

        alice_node.shutdown().await;
        bob_node.shutdown().await;
        carol_node.shutdown().await;
        dan_node.shutdown().await;
    });
}

#[test]
fn service_request_timeout() {
    let mut runtime = Runtime::new().unwrap();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let base_node_service_config = BaseNodeServiceConfig {
        service_request_timeout: Duration::from_millis(1),
        fetch_blocks_timeout: Default::default(),
        fetch_utxos_timeout: Default::default(),
        desired_response_fraction: Default::default(),
    };
    let temp_dir = tempdir().unwrap();
    let (mut alice_node, bob_node, _consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        base_node_service_config,
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );

    runtime.block_on(async {
        // Bob should not be reachable
        bob_node.shutdown().await;
        unpack_enum!(CommsInterfaceError::RequestTimedOut = alice_node.outbound_nci.get_metadata().await.unwrap_err());
        alice_node.shutdown().await;
    });
}

#[test]
fn local_get_metadata() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let (mut node, consensus_manager) =
        BaseNodeBuilder::new(network).start(&mut runtime, temp_dir.path().to_str().unwrap());
    let db = &node.blockchain_db;
    let block0 = db.fetch_block(0).unwrap().try_into_chain_block().unwrap();
    let block1 = append_block(db, &block0, vec![], &consensus_manager, 1.into()).unwrap();
    let block2 = append_block(db, &block1, vec![], &consensus_manager, 1.into()).unwrap();

    runtime.block_on(async {
        let metadata = node.local_nci.get_metadata().await.unwrap();
        assert_eq!(metadata.height_of_longest_chain(), 2);
        assert_eq!(metadata.best_block(), block2.hash());

        node.shutdown().await;
    });
}

#[test]
fn local_get_new_block_template_and_get_new_block() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let (block0, outputs) = create_genesis_block_with_utxos(&factories, &[T, T], &consensus_constants[0]);
    let rules = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants[0].clone())
        .with_block(block0)
        .build();
    let (mut node, _rules) = BaseNodeBuilder::new(network)
        .with_consensus_manager(rules)
        .start(&mut runtime, temp_dir.path().to_str().unwrap());

    let schema = [
        txn_schema!(from: vec![outputs[1].clone()], to: vec![10_000 * uT, 20_000 * uT]),
        txn_schema!(from: vec![outputs[2].clone()], to: vec![30_000 * uT, 40_000 * uT]),
    ];
    let (txs, _) = schema_to_transaction(&schema);
    assert!(node.mempool.insert(txs[0].clone()).is_ok());
    assert!(node.mempool.insert(txs[1].clone()).is_ok());

    runtime.block_on(async {
        let block_template = node.local_nci.get_new_block_template(PowAlgorithm::Sha3).await.unwrap();
        assert_eq!(block_template.header.height, 1);
        assert_eq!(block_template.body.kernels().len(), 2);

        let mut block = node.local_nci.get_new_block(block_template.clone()).await.unwrap();
        assert_eq!(block.header.height, 1);
        assert_eq!(block.body, block_template.body);

        assert!(node.blockchain_db.add_block(block.clone().into()).is_ok());

        node.shutdown().await;
    });
}

#[test]
fn local_submit_block() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let (mut node, consensus_manager) =
        BaseNodeBuilder::new(network).start(&mut runtime, temp_dir.path().to_str().unwrap());

    let db = &node.blockchain_db;
    let mut event_stream = node.local_nci.get_block_event_stream();
    let block0 = db.fetch_block(0).unwrap().block().clone();
    let mut block1 = db
        .prepare_block_merkle_roots(chain_block(&block0, vec![], &consensus_manager))
        .unwrap();
    block1.header.kernel_mmr_size += 1;
    block1.header.output_mmr_size += 1;
    runtime.block_on(async {
        node.local_nci
            .submit_block(block1.clone(), Broadcast::from(true))
            .await
            .unwrap();

        let event = event_stream_next(&mut event_stream, Duration::from_millis(20000)).await;
        if let BlockEvent::ValidBlockAdded(received_block, result, _) = &*event.unwrap().unwrap() {
            assert_eq!(received_block.hash(), block1.hash());
            result.assert_added();
        } else {
            panic!("Block validation failed");
        }

        node.shutdown().await;
    });
}

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

use futures::StreamExt;
use helpers::{
    block_builders::{
        append_block,
        chain_block,
        chain_block_with_coinbase,
        create_coinbase,
        create_genesis_block,
        find_header_with_achieved_difficulty,
    },
    chain_metadata::{random_peer_metadata, MockChainMetadata},
    nodes::{
        create_network_with_2_base_nodes_with_config,
        create_network_with_3_base_nodes_with_config,
        random_node_identity,
        wait_until_online,
        BaseNodeBuilder,
    },
};
use rand::{rngs::OsRng, RngCore};
use std::{thread, time::Duration};
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        comms_interface::Broadcast,
        service::BaseNodeServiceConfig,
        state_machine_service::{
            states::{
                BestChainMetadataBlockSync,
                BlockSyncConfig,
                HorizonSyncConfig,
                Listening,
                StateEvent,
                StatusInfo,
                SyncPeer,
                SyncPeerConfig,
                SyncStatus,
                SyncStatus::Lagging,
            },
            BaseNodeStateMachine,
            BaseNodeStateMachineConfig,
        },
        SyncValidators,
    },
    chain_storage::BlockchainDatabaseConfig,
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    mempool::MempoolServiceConfig,
    proof_of_work::Difficulty,
    transactions::{helpers::spend_utxos, types::CryptoFactories},
    txn_schema,
    validation::{block_validators::MockStatelessBlockValidator, mocks::MockValidator},
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_shutdown::Shutdown;
use tari_test_utils::collect_stream;
use tempfile::tempdir;
use tokio::{
    runtime::Runtime,
    sync::{broadcast, watch},
    time,
};

#[test]
fn test_listening_lagging() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig {
            auto_ping_interval: Some(Duration::from_millis(100)),
            ..Default::default()
        },
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );
    wait_until_online(&mut runtime, &[&alice_node, &bob_node]);

    let await_event_task =
        runtime.spawn(async move { Listening { is_synced: true }.next_event(&mut alice_state_machine).await });

    runtime.block_on(async move {
        let bob_db = bob_node.blockchain_db;
        let mut bob_local_nci = bob_node.local_nci;

        // Bob Block 1 - no block event
        let prev_block = append_block(&bob_db, &prev_block, vec![], &consensus_manager, 3.into()).unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db
            .calculate_mmr_roots(chain_block(&prev_block, vec![], &consensus_manager))
            .unwrap();
        bob_local_nci
            .submit_block(prev_block, Broadcast::from(true))
            .await
            .unwrap();
        assert_eq!(bob_db.get_height().unwrap(), Some(2));

        let next_event = time::timeout(Duration::from_secs(10), await_event_task)
            .await
            .expect("Alice did not emit `StateEvent::FallenBehind` within 10 seconds")
            .unwrap();

        match next_event {
            StateEvent::FallenBehind(Lagging(_, _)) => assert!(true),
            _ => assert!(false),
        }
    });
}

#[test]
fn test_event_channel() {
    let temp_dir = tempdir().unwrap();
    let mut runtime = Runtime::new().unwrap();
    let (node, consensus_manager) =
        BaseNodeBuilder::new(Network::Rincewind).start(&mut runtime, temp_dir.path().to_str().unwrap());
    // let shutdown = Shutdown::new();
    let db = create_mem_db(&consensus_manager);
    let shutdown = Shutdown::new();
    let mut mock = MockChainMetadata::new();
    let (state_change_event_publisher, mut state_change_event_subscriber) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = tokio::sync::watch::channel(StatusInfo::new());
    let state_machine = BaseNodeStateMachine::new(
        &db,
        &node.local_nci,
        &node.outbound_nci,
        node.comms.connectivity(),
        node.comms.peer_manager(),
        mock.subscription(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );

    runtime.spawn(state_machine.run());

    let PeerChainMetadata {
        node_id,
        chain_metadata,
    } = random_peer_metadata(10, 5_000.into());
    runtime
        .block_on(mock.publish_chain_metadata(&node_id, &chain_metadata))
        .expect("Could not publish metadata");
    thread::sleep(Duration::from_millis(50));
    runtime.block_on(async {
        let event = state_change_event_subscriber.next().await;
        assert_eq!(*event.unwrap().unwrap(), StateEvent::Initialized);
        let event = state_change_event_subscriber.next().await;
        match *event.unwrap().unwrap() {
            StateEvent::FallenBehind(SyncStatus::Lagging(ref data, ref peers)) => {
                assert_eq!(data.height_of_longest_chain, Some(10));
                assert_eq!(data.accumulated_difficulty, Some(5_000.into()));
                assert_eq!(peers[0].node_id, node_id);
            },
            _ => assert!(false),
        }
    });
}

#[test]
fn test_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig {
            max_metadata_request_retry_attempts: 3,
            max_header_request_retry_attempts: 20,
            max_block_request_retry_attempts: 20,
            max_add_block_retry_attempts: 3,
            header_request_size: 5,
            block_request_size: 1,
            ..Default::default()
        },
        horizon_sync_config: HorizonSyncConfig::default(),
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, mut status_event_receiver) = watch::channel(StatusInfo::new());
    let mut mock = MockChainMetadata::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        mock.subscription(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 1..6 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        }

        // Sync Blocks from genesis block to tip
        let is_bootstrapped = status_event_receiver.recv().await.unwrap().bootstrapped;
        assert!(!is_bootstrapped);

        let mut network_tip = bob_db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = BestChainMetadataBlockSync {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        assert_eq!(alice_db.get_height().unwrap(), bob_db.get_height().unwrap());

        for height in 1..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block(height).unwrap(),
                bob_db.fetch_block(height).unwrap()
            );
        }

        // Bootstrap status changed to true after initial sync was achieved
        let is_bootstrapped = status_event_receiver.recv().await.unwrap().bootstrapped;
        assert!(is_bootstrapped);

        // Publish on the metadata that will indicate we are synced in the Listening state
        let _ = mock
            .publish_chain_metadata(bob_node.node_identity.node_id(), &network_tip)
            .await;
        // Publish a second event saying that we are lagging again so that we will leave the Listening state
        let higher_difficulty = Difficulty::from(u64::MAX);
        network_tip.accumulated_difficulty = Some(higher_difficulty);

        let _ = mock
            .publish_chain_metadata(bob_node.node_identity.node_id(), &network_tip)
            .await;

        // Run the listening State
        let _state_event = Listening { is_synced: true }.next_event(&mut alice_state_machine).await;

        // Bootstrap status remains unchanged
        let is_bootstrapped = status_event_receiver.recv().await.unwrap().bootstrapped;
        assert!(is_bootstrapped);
    });
}

#[test]
fn test_lagging_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig {
            max_metadata_request_retry_attempts: 3,
            max_header_request_retry_attempts: 20,
            max_block_request_retry_attempts: 20,
            max_add_block_retry_attempts: 3,
            header_request_size: 5,
            block_request_size: 1,
            ..Default::default()
        },
        horizon_sync_config: HorizonSyncConfig::default(),
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 0..4 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
            alice_db.add_block(prev_block.clone().into()).unwrap();
        }
        for _ in 0..4 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        }
        assert_eq!(alice_db.get_height().unwrap(), Some(4));
        assert_eq!(bob_db.get_height().unwrap(), Some(8));

        // Lagging state beyond horizon, sync remaining Blocks to tip
        let network_tip = bob_db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = BestChainMetadataBlockSync {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(alice_db.get_height().unwrap(), bob_db.get_height().unwrap());

        for height in 0..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_node.blockchain_db.fetch_block(height).unwrap(),
                bob_node.blockchain_db.fetch_block(height).unwrap()
            );
        }
    });
}

#[test]
fn test_block_sync_recovery() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, carol_node, _) = create_network_with_3_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager.clone(),
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig {
            max_metadata_request_retry_attempts: 3,
            max_header_request_retry_attempts: 20,
            max_block_request_retry_attempts: 20,
            max_add_block_retry_attempts: 3,
            header_request_size: 5,
            block_request_size: 1,
            ..Default::default()
        },
        horizon_sync_config: HorizonSyncConfig::default(),
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let carol_db = &carol_node.blockchain_db;
        // Bob and Carol is ahead of Alice and Bob is ahead of Carol
        prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        carol_db.add_block(prev_block.clone().into()).unwrap();
        for _ in 0..2 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        }

        // Sync Blocks from genesis block to tip. Alice will notice that the chain tip is equivalent to Bobs tip and
        // start to request blocks from her random peers. When Alice requests these blocks from Carol, Carol
        // won't always have these blocks and Alice will have to request these blocks again until her maximum attempts
        // have been reached.
        let network_tip = bob_db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = BestChainMetadataBlockSync
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        for height in 1..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block(height).unwrap().block(),
                bob_db.fetch_block(height).unwrap().block()
            );
        }
    });
}

#[test]
fn test_forked_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig {
            max_metadata_request_retry_attempts: 3,
            max_header_request_retry_attempts: 20,
            max_block_request_retry_attempts: 20,
            max_add_block_retry_attempts: 3,
            header_request_size: 5,
            block_request_size: 1,
            ..Default::default()
        },
        horizon_sync_config: HorizonSyncConfig::default(),
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );

    runtime.block_on(async {
        // Shared chain
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 0..2 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager, 1.into()).unwrap();
            alice_db.add_block(prev_block.clone().into()).unwrap();
        }

        assert_eq!(alice_db.get_height().unwrap(), Some(2));
        assert_eq!(bob_db.get_height().unwrap(), Some(2));

        let mut alice_prev_block = prev_block.clone();
        let mut bob_prev_block = prev_block;
        // Alice fork
        for _ in 0..2 {
            alice_prev_block = append_block(alice_db, &alice_prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        }
        // Bob fork
        for _ in 0..7 {
            bob_prev_block = append_block(bob_db, &bob_prev_block, vec![], &consensus_manager, 1.into()).unwrap();
        }
        assert_eq!(alice_db.get_height().unwrap(), Some(4));
        assert_eq!(bob_db.get_height().unwrap(), Some(9));

        let network_tip = bob_db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = BestChainMetadataBlockSync {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(alice_db.get_height().unwrap(), bob_db.get_height().unwrap());

        for height in 0..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block(height).unwrap(),
                bob_db.fetch_block(height).unwrap()
            );
        }
    });
}

#[test]
fn test_sync_peer_banning() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let stateless_block_validator = MockStatelessBlockValidator::new(consensus_manager.clone(), factories.clone());
    let mock_validator = MockValidator::new(true);
    // Create base nodes
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let base_node_service_config = BaseNodeServiceConfig::default();
    let mmr_cache_config = MmrCacheConfig::default();
    let mempool_service_config = MempoolServiceConfig::default();
    let liveness_service_config = LivenessConfig::default();
    let data_path = temp_dir.path().to_str().unwrap();
    let network = Network::LocalNet;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .with_validators(mock_validator, stateless_block_validator)
        .start(&mut runtime, data_path);
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity)
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(&mut runtime, data_path);

    wait_until_online(&mut runtime, &[&alice_node, &bob_node]);

    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig {
            max_metadata_request_retry_attempts: 3,
            max_header_request_retry_attempts: 20,
            max_block_request_retry_attempts: 20,
            max_add_block_retry_attempts: 3,
            header_request_size: 5,
            block_request_size: 1,
            ..Default::default()
        },
        horizon_sync_config: HorizonSyncConfig::default(),
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
        status_event_sender,
        state_change_event_publisher,
    );
    let mut last_utxo = None;

    runtime.block_on(async {
        // Shared chain
        let alice_db = &alice_node.blockchain_db;
        let alice_peer_manager = &alice_node.comms.peer_manager();
        let bob_db = &bob_node.blockchain_db;
        let bob_public_key = &bob_node.node_identity.public_key();
        for height in 1..=2 {
            let coinbase_value = consensus_manager.emission_schedule(0).block_reward(height);
            let (mut coinbase_utxo, coinbase_kernel, mut coinbase) = create_coinbase(
                &factories,
                coinbase_value,
                height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
            );
            // we want these to fail later
            coinbase_utxo.features.maturity = 1000;
            coinbase.features.maturity = 1000;
            last_utxo = Some(coinbase);
            let template =
                chain_block_with_coinbase(&prev_block, vec![], coinbase_utxo, coinbase_kernel, &consensus_manager);
            prev_block = bob_db.calculate_mmr_roots(template).unwrap();
            prev_block.header.nonce = OsRng.next_u64();
            find_header_with_achieved_difficulty(&mut prev_block.header, 1.into());

            alice_db.add_block(prev_block.clone().into()).unwrap();
            bob_db.add_block(prev_block.clone().into()).unwrap();
        }
        assert_eq!(alice_db.get_height().unwrap(), Some(2));
        assert_eq!(bob_db.get_height().unwrap(), Some(2));
        let peer = alice_peer_manager.find_by_public_key(bob_public_key).await.unwrap();
        assert_eq!(peer.is_banned(), false);

        // Alice fork
        let mut alice_prev_block = prev_block.clone();
        for height in 3..=4 {
            let coinbase_value = consensus_manager.emission_schedule(0).block_reward(height);
            let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
                &factories,
                coinbase_value,
                height + consensus_manager.consensus_constants(0).coinbase_lock_height(),
            );
            let template = chain_block_with_coinbase(
                &alice_prev_block,
                vec![],
                coinbase_utxo,
                coinbase_kernel,
                &consensus_manager,
            );
            alice_prev_block = alice_db.calculate_mmr_roots(template).unwrap();
            alice_prev_block.header.nonce = OsRng.next_u64();
            find_header_with_achieved_difficulty(&mut alice_prev_block.header, 1.into());

            alice_db.add_block(alice_prev_block.clone().into()).unwrap();
        }
        // Bob fork with invalid maturity
        let mut bob_prev_block = prev_block;
        for _ in 3..=6 {
            let utxo = last_utxo.unwrap();
            let txn = txn_schema!(from: vec![utxo.clone()]);
            let (tx, outputs, _) = spend_utxos(txn);

            // tx.body.outputs[0].features.maturity = 1000;

            last_utxo = Some(outputs[0].clone());
            bob_prev_block = append_block(bob_db, &bob_prev_block, vec![tx], &consensus_manager, 3.into()).unwrap();
        }

        assert_eq!(alice_db.get_height().unwrap(), Some(4));
        assert_eq!(bob_db.get_height().unwrap(), Some(6));

        let mut connectivity_events = alice_node.comms.connectivity().get_event_subscription();
        let network_tip = bob_db.get_chain_metadata().unwrap();
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = BestChainMetadataBlockSync {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlockSyncFailure);

        assert_eq!(alice_db.get_height().unwrap(), Some(4));

        let _events = collect_stream!(connectivity_events, take = 1, timeout = Duration::from_secs(10));

        let peer = alice_peer_manager.find_by_public_key(bob_public_key).await.unwrap();
        assert_eq!(peer.is_banned(), true);
    });
}

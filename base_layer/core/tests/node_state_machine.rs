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

use crate::helpers::block_builders::{append_block_with_coinbase, generate_new_block};
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
        states::{
            BestChainMetadataBlockSyncInfo,
            BlockSyncConfig,
            HorizonSyncConfig,
            ListeningData,
            StateEvent,
            SyncPeerConfig,
            SyncStatus,
            SyncStatus::Lagging,
        },
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
    },
    chain_storage::{BlockchainDatabaseConfig, MmrTree},
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    helpers::create_mem_db,
    mempool::MempoolServiceConfig,
    transactions::{tari_amount::T, transaction::TransactionOutput, types::CryptoFactories},
    txn_schema,
    validation::{
        accum_difficulty_validators::MockAccumDifficultyValidator,
        block_validators::MockStatelessBlockValidator,
        mocks::MockValidator,
    },
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_stream, random::string};
use tempdir::TempDir;
use tokio::{runtime::Runtime, time};

#[test]
fn test_listening_lagging() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
        shutdown.to_signal(),
    );
    wait_until_online(&mut runtime, &[&alice_node, &bob_node]);

    let await_event_task = runtime.spawn(async move { ListeningData.next_event(&mut alice_state_machine).await });

    runtime.block_on(async move {
        let bob_db = bob_node.blockchain_db;
        let mut bob_local_nci = bob_node.local_nci;

        // Bob Block 1 - no block event
        let prev_block = append_block(
            &bob_db,
            &prev_block,
            vec![],
            &consensus_manager.consensus_constants(),
            3.into(),
        )
        .unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db
            .calculate_mmr_roots(chain_block(
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
            ))
            .unwrap();
        bob_local_nci
            .submit_block(prev_block, Broadcast::from(true))
            .await
            .unwrap();
        assert_eq!(bob_db.get_height(), Ok(Some(2)));

        let next_event = time::timeout(Duration::from_secs(10), await_event_task)
            .await
            .expect("Alice did not emit `StateEvent::FallenBehind` within 10 seconds")
            .unwrap();

        match next_event {
            StateEvent::FallenBehind(Lagging(_, _)) => assert!(true),
            _ => assert!(false),
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_event_channel() {
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mut runtime = Runtime::new().unwrap();
    let (node, consensus_manager) =
        BaseNodeBuilder::new(Network::Rincewind).start(&mut runtime, temp_dir.path().to_str().unwrap());
    // let shutdown = Shutdown::new();
    let db = create_mem_db(&consensus_manager);
    let mut shutdown = Shutdown::new();
    let mut mock = MockChainMetadata::new();
    let state_machine = BaseNodeStateMachine::new(
        &db,
        &node.local_nci,
        &node.outbound_nci,
        node.comms.peer_manager(),
        node.comms.connectivity(),
        mock.subscriber(),
        BaseNodeStateMachineConfig::default(),
        shutdown.to_signal(),
    );
    let rx = state_machine.get_state_change_event_stream();

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
        let mut fused = rx.fuse();
        let event = fused.next().await;
        assert_eq!(*event.unwrap(), StateEvent::Initialized);
        let event = fused.next().await;
        match *event.unwrap() {
            StateEvent::FallenBehind(SyncStatus::Lagging(ref data, ref id)) => {
                assert_eq!(data.height_of_longest_chain, Some(10));
                assert_eq!(data.accumulated_difficulty, Some(5_000.into()));
                assert_eq!(id[0], node_id);
            },
            _ => assert!(false),
        }
        node.comms.shutdown().await;
    });
    let _ = shutdown.trigger();
}

#[test]
fn test_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 1..6 {
            prev_block = append_block(
                bob_db,
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
        }

        // Sync Blocks from genesis block to tip
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        assert_eq!(alice_db.get_height(), bob_db.get_height());

        for height in 1..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block_with_height(height),
                bob_db.fetch_block_with_height(height)
            );
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_lagging_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 0..4 {
            prev_block = append_block(
                bob_db,
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
            alice_db.add_block(prev_block.clone()).unwrap();
        }
        for _ in 0..4 {
            prev_block = append_block(
                bob_db,
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
        }
        assert_eq!(alice_db.get_height(), Ok(Some(4)));
        assert_eq!(bob_db.get_height(), Ok(Some(8)));

        // Lagging state beyond horizon, sync remaining Blocks to tip
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(alice_db.get_height(), bob_db.get_height());

        for height in 0..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_node.blockchain_db.fetch_block_with_height(height),
                bob_node.blockchain_db.fetch_block_with_height(height)
            );
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_block_sync_recovery() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let carol_db = &carol_node.blockchain_db;
        // Bob and Carol is ahead of Alice and Bob is ahead of Carol
        prev_block = append_block(
            bob_db,
            &prev_block,
            vec![],
            &consensus_manager.consensus_constants(),
            1.into(),
        )
        .unwrap();
        carol_db.add_block(prev_block.clone()).unwrap();
        for _ in 0..2 {
            prev_block = append_block(
                bob_db,
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
        }

        // Sync Blocks from genesis block to tip. Alice will notice that the chain tip is equivalent to Bobs tip and
        // start to request blocks from her random peers. When Alice requests these blocks from Carol, Carol
        // won't always have these blocks and Alice will have to request these blocks again until her maximum attempts
        // have been reached.
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        for height in 1..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block_with_height(height).unwrap().block(),
                bob_db.fetch_block_with_height(height).unwrap().block()
            );
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
        carol_node.comms.shutdown().await;
    });
}

#[test]
fn test_forked_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        // Shared chain
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 0..2 {
            prev_block = append_block(
                bob_db,
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
            alice_db.add_block(prev_block.clone()).unwrap();
        }

        assert_eq!(alice_db.get_height(), Ok(Some(2)));
        assert_eq!(bob_db.get_height(), Ok(Some(2)));

        let mut alice_prev_block = prev_block.clone();
        let mut bob_prev_block = prev_block;
        // Alice fork
        for _ in 0..2 {
            alice_prev_block = append_block(
                alice_db,
                &alice_prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
        }
        // Bob fork
        for _ in 0..7 {
            bob_prev_block = append_block(
                bob_db,
                &bob_prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                1.into(),
            )
            .unwrap();
        }
        assert_eq!(alice_db.get_height(), Ok(Some(4)));
        assert_eq!(bob_db.get_height(), Ok(Some(9)));

        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(alice_db.get_height(), bob_db.get_height());

        for height in 0..=network_tip.height_of_longest_chain.unwrap() {
            assert_eq!(
                alice_db.fetch_block_with_height(height),
                bob_db.fetch_block_with_height(height)
            );
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_sync_peer_banning() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
        .with_validators(
            mock_validator,
            stateless_block_validator,
            MockAccumDifficultyValidator {},
        )
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
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        // Shared chain
        let alice_db = &alice_node.blockchain_db;
        let alice_peer_manager = &alice_node.comms.peer_manager();
        let bob_db = &bob_node.blockchain_db;
        let bob_public_key = &bob_node.node_identity.public_key();
        for height in 1..=2 {
            let coinbase_value = consensus_manager.emission_schedule().block_reward(height);
            let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
                &factories,
                coinbase_value,
                height + consensus_manager.consensus_constants().coinbase_lock_height(),
            );
            let template = chain_block_with_coinbase(
                &prev_block,
                vec![],
                coinbase_utxo,
                coinbase_kernel,
                &consensus_manager.consensus_constants(),
            );
            prev_block = bob_db.calculate_mmr_roots(template).unwrap();
            prev_block.header.nonce = OsRng.next_u64();
            find_header_with_achieved_difficulty(&mut prev_block.header, 1.into());

            alice_db.add_block(prev_block.clone()).unwrap();
            bob_db.add_block(prev_block.clone()).unwrap();
        }
        assert_eq!(alice_db.get_height(), Ok(Some(2)));
        assert_eq!(bob_db.get_height(), Ok(Some(2)));
        let peer = alice_peer_manager.find_by_public_key(bob_public_key).await.unwrap();
        assert_eq!(peer.is_banned(), false);

        // Alice fork
        let mut alice_prev_block = prev_block.clone();
        for height in 3..=4 {
            let coinbase_value = consensus_manager.emission_schedule().block_reward(height);
            let (coinbase_utxo, coinbase_kernel, _) = create_coinbase(
                &factories,
                coinbase_value,
                height + consensus_manager.consensus_constants().coinbase_lock_height(),
            );
            let template = chain_block_with_coinbase(
                &alice_prev_block,
                vec![],
                coinbase_utxo,
                coinbase_kernel,
                &consensus_manager.consensus_constants(),
            );
            alice_prev_block = alice_db.calculate_mmr_roots(template).unwrap();
            alice_prev_block.header.nonce = OsRng.next_u64();
            find_header_with_achieved_difficulty(&mut alice_prev_block.header, 1.into());

            alice_db.add_block(alice_prev_block.clone()).unwrap();
        }
        // Bob fork with invalid coinbases
        let mut bob_prev_block = prev_block;
        for _ in 3..=6 {
            bob_prev_block = append_block(
                bob_db,
                &bob_prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
                3.into(),
            )
            .unwrap();
        }

        assert_eq!(alice_db.get_height(), Ok(Some(4)));
        assert_eq!(bob_db.get_height(), Ok(Some(6)));

        let mut connectivity_events = alice_node.comms.connectivity().subscribe_event_stream();
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlockSyncFailure);

        assert_eq!(alice_db.get_height(), Ok(Some(4)));

        let _events = collect_stream!(connectivity_events, take = 1, timeout = Duration::from_secs(10));

        let peer = alice_peer_manager.find_by_public_key(bob_public_key).await.unwrap();
        assert_eq!(peer.is_banned(), true);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_pruned_mode_sync_with_future_horizon_sync_height() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (mut prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 4,
        pruned_mode_cleanup_interval: 2,
    };
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = 2;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        for _ in 1..10 {
            // Need coinbases for kernels and utxos
            prev_block =
                append_block_with_coinbase(&factories, bob_db, &prev_block, vec![], &consensus_manager, 1.into())
                    .unwrap();
        }

        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        let alice_metadata = alice_db.get_metadata().unwrap();
        assert_eq!(alice_metadata, network_tip);

        // Check headers
        let network_tip_height = network_tip.height_of_longest_chain.unwrap_or(0);
        let block_nums = (0..=network_tip_height).collect::<Vec<u64>>();
        let alice_headers = alice_db.fetch_headers(block_nums.clone()).unwrap();
        let bob_headers = bob_db.fetch_headers(block_nums).unwrap();
        assert_eq!(alice_headers, bob_headers);
        // Check Kernel MMR nodes
        let alice_num_kernels = alice_db
            .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
            .unwrap();
        let bob_num_kernels = bob_db
            .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
            .unwrap();
        assert_eq!(alice_num_kernels, bob_num_kernels);
        let alice_kernel_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(network_tip_height))
            .unwrap();
        let bob_kernel_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_kernel_nodes, bob_kernel_nodes);
        // Check Kernels
        let alice_kernel_hashes = alice_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
        let bob_kernels_hashes = bob_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
        let alice_kernels = alice_db.fetch_kernels(alice_kernel_hashes).unwrap();
        let bob_kernels = bob_db.fetch_kernels(bob_kernels_hashes).unwrap();
        assert_eq!(alice_kernels, bob_kernels);
        // Check UTXO MMR nodes
        let alice_num_utxos = alice_db
            .fetch_mmr_node_count(MmrTree::Utxo, network_tip_height)
            .unwrap();
        let bob_num_utxos = bob_db.fetch_mmr_node_count(MmrTree::Utxo, network_tip_height).unwrap();
        assert_eq!(alice_num_utxos, bob_num_utxos);
        let alice_utxo_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Utxo, 0, alice_num_utxos, Some(network_tip_height))
            .unwrap();
        let bob_utxo_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_utxo_nodes, bob_utxo_nodes);
        // Check RangeProof MMR nodes
        let alice_num_rps = alice_db
            .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
            .unwrap();
        let bob_num_rps = bob_db
            .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
            .unwrap();
        assert_eq!(alice_num_rps, bob_num_rps);
        let alice_rps_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::RangeProof, 0, alice_num_rps, Some(network_tip_height))
            .unwrap();
        let bob_rps_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::RangeProof, 0, bob_num_rps, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_rps_nodes, bob_rps_nodes);
        // Check UTXOs
        let mut alice_utxos = Vec::<TransactionOutput>::new();
        for (hash, deleted) in alice_utxo_nodes {
            if !deleted {
                alice_utxos.push(alice_db.fetch_utxo(hash).unwrap());
            }
        }
        let mut bob_utxos = Vec::<TransactionOutput>::new();
        for (hash, deleted) in bob_utxo_nodes {
            if !deleted {
                bob_utxos.push(bob_db.fetch_utxo(hash).unwrap());
            }
        }
        assert_eq!(alice_utxos, bob_utxos);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_pruned_mode_sync_with_spent_utxos() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (block0, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 4,
        pruned_mode_cleanup_interval: 2,
    };
    let (alice_node, mut bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = 0;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let mut blocks = vec![block0];
        let mut outputs = vec![vec![output]];
        // Block1
        let txs = vec![txn_schema!(
            from: vec![outputs[0][0].clone()],
            to: vec![10 * T, 10 * T, 10 * T, 10 * T]
        )];
        assert!(generate_new_block(
            &mut bob_node.blockchain_db,
            &mut blocks,
            &mut outputs,
            txs,
            &consensus_manager.consensus_constants()
        )
        .is_ok());
        // Block2
        let txs = vec![txn_schema!(from: vec![outputs[1][3].clone()], to: vec![6 * T])];
        assert!(generate_new_block(
            &mut bob_node.blockchain_db,
            &mut blocks,
            &mut outputs,
            txs,
            &consensus_manager.consensus_constants()
        )
        .is_ok());
        // Block3
        let txs = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T])];
        assert!(generate_new_block(
            &mut bob_node.blockchain_db,
            &mut blocks,
            &mut outputs,
            txs,
            &consensus_manager.consensus_constants()
        )
        .is_ok());
        // Block4
        let txs = vec![txn_schema!(from: vec![outputs[1][0].clone()], to: vec![2 * T])];
        assert!(generate_new_block(
            &mut bob_node.blockchain_db,
            &mut blocks,
            &mut outputs,
            txs,
            &consensus_manager.consensus_constants()
        )
        .is_ok());

        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let network_tip = bob_db.get_metadata().unwrap();
        let mut sync_peers = vec![bob_node.node_identity.node_id().clone()];
        let state_event = BestChainMetadataBlockSyncInfo {}
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        let alice_metadata = alice_db.get_metadata().unwrap();
        assert_eq!(alice_metadata, network_tip);

        // Check headers
        let network_tip_height = network_tip.height_of_longest_chain.unwrap_or(0);
        let block_nums = (0..=network_tip_height).collect::<Vec<u64>>();
        let alice_headers = alice_db.fetch_headers(block_nums.clone()).unwrap();
        let bob_headers = bob_db.fetch_headers(block_nums).unwrap();
        assert_eq!(alice_headers, bob_headers);
        // Check Kernel MMR nodes
        let alice_num_kernels = alice_db
            .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
            .unwrap();
        let bob_num_kernels = bob_db
            .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
            .unwrap();
        assert_eq!(alice_num_kernels, bob_num_kernels);
        let alice_kernel_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(network_tip_height))
            .unwrap();
        let bob_kernel_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_kernel_nodes, bob_kernel_nodes);
        // Check Kernels
        let alice_kernel_hashes = alice_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
        let bob_kernels_hashes = bob_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
        let alice_kernels = alice_db.fetch_kernels(alice_kernel_hashes).unwrap();
        let bob_kernels = bob_db.fetch_kernels(bob_kernels_hashes).unwrap();
        assert_eq!(alice_kernels, bob_kernels);
        // Check UTXO MMR nodes
        let alice_num_utxos = alice_db
            .fetch_mmr_node_count(MmrTree::Utxo, network_tip_height)
            .unwrap();
        let bob_num_utxos = bob_db.fetch_mmr_node_count(MmrTree::Utxo, network_tip_height).unwrap();
        assert_eq!(alice_num_utxos, bob_num_utxos);
        let alice_utxo_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Utxo, 0, alice_num_utxos, Some(network_tip_height))
            .unwrap();
        let bob_utxo_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_utxo_nodes, bob_utxo_nodes);
        // Check RangeProof MMR nodes
        let alice_num_rps = alice_db
            .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
            .unwrap();
        let bob_num_rps = bob_db
            .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
            .unwrap();
        assert_eq!(alice_num_rps, bob_num_rps);
        let alice_rps_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::RangeProof, 0, alice_num_rps, Some(network_tip_height))
            .unwrap();
        let bob_rps_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::RangeProof, 0, bob_num_rps, Some(network_tip_height))
            .unwrap();
        assert_eq!(alice_rps_nodes, bob_rps_nodes);
        // Check UTXOs
        let mut alice_utxos = Vec::<TransactionOutput>::new();
        for (hash, deleted) in alice_utxo_nodes {
            if !deleted {
                alice_utxos.push(alice_db.fetch_utxo(hash).unwrap());
            }
        }
        let mut bob_utxos = Vec::<TransactionOutput>::new();
        for (hash, deleted) in bob_utxo_nodes {
            if !deleted {
                bob_utxos.push(bob_db.fetch_utxo(hash).unwrap());
            }
        }
        assert_eq!(alice_utxos, bob_utxos);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

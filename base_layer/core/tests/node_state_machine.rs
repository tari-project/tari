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
    block_builders::{append_block, chain_block, create_genesis_block},
    nodes::{
        create_network_with_2_base_nodes,
        create_network_with_2_base_nodes_with_config,
        create_network_with_3_base_nodes_with_config,
    },
};
use std::time::Duration;
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{
            BaseNodeState,
            BlockSyncConfig,
            BlockSyncInfo,
            ListeningConfig,
            ListeningInfo,
            StateEvent,
            SyncStatus::Lagging,
        },
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
    },
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    mempool::MempoolServiceConfig,
    transactions::types::CryptoFactories,
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_test_utils::random::string;
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[ignore]
fn test_listening_lagging() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig {
            enable_auto_join: false,
            enable_auto_stored_message_request: false,
            auto_ping_interval: Some(Duration::from_millis(100)),
            refresh_neighbours_interval: Duration::from_secs(60),
        },
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
    );

    runtime.block_on(async move {
        let bob_db = bob_node.blockchain_db;
        let mut bob_local_nci = bob_node.local_nci;

        // Bob Block 1 - no block event
        prev_block = append_block(&bob_db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db
            .calculate_mmr_roots(chain_block(
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
            ))
            .unwrap();
        bob_local_nci.submit_block(prev_block.clone()).await.unwrap();
        assert_eq!(bob_db.get_height(), Ok(Some(2)));

        let state_event = ListeningInfo.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::FallenBehind(Lagging(2)));

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_event_channel() {
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig {
            enable_auto_join: false,
            enable_auto_stored_message_request: false,
            auto_ping_interval: Some(Duration::from_millis(100)),
            refresh_neighbours_interval: Duration::from_secs(60),
        },
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
    );
    let rx = alice_state_machine.get_state_change_event_stream();

    runtime.spawn(async move {
        alice_state_machine.run().await;
    });

    runtime.block_on(async {
        let bob_db = bob_node.blockchain_db;
        let mut bob_local_nci = bob_node.local_nci;

        // Bob Block 1 - no block event
        prev_block = append_block(&bob_db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db
            .calculate_mmr_roots(chain_block(
                &prev_block,
                vec![],
                &consensus_manager.consensus_constants(),
            ))
            .unwrap();
        bob_local_nci.submit_block(prev_block.clone()).await.unwrap();
        assert_eq!(bob_db.get_height(), Ok(Some(2)));
        let state = rx.fuse().select_next_some().await;
        if let BaseNodeState::InitialSync(_) = *state {
            assert!(true);
        } else {
            assert!(false);
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}
#[test]
fn test_listening_network_silence() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (alice_node, bob_node, _consensus_manager) =
        create_network_with_2_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        listening_config: ListeningConfig {
            listening_silence_timeout: Duration::from_millis(100),
        },
    };
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
    );

    runtime.block_on(async {
        let state_event = ListeningInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::NetworkSilence);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
    );

    runtime.block_on(async {
        let adb = &alice_node.blockchain_db;
        let db = &bob_node.blockchain_db;
        for _ in 1..6 {
            prev_block = append_block(db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        }

        // Sync Blocks from genesis block to tip
        let state_event = BlockSyncInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        assert_eq!(adb.get_height(), db.get_height());

        let bob_tip_height = db.get_height().unwrap().unwrap();
        for height in 1..=bob_tip_height {
            assert_eq!(adb.fetch_block(height), db.fetch_block(height));
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
    );

    let db = &bob_node.blockchain_db;
    for _ in 0..4 {
        prev_block = append_block(db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        alice_node.blockchain_db.add_block(prev_block.clone()).unwrap();
    }
    for _ in 0..4 {
        prev_block = append_block(db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
    }
    assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(4)));
    assert_eq!(bob_node.blockchain_db.get_height(), Ok(Some(8)));

    runtime.block_on(async {
        // Lagging state beyond horizon, sync remaining Blocks to tip
        let state_event = BlockSyncInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(
            alice_node.blockchain_db.get_height(),
            bob_node.blockchain_db.get_height()
        );

        let bob_tip_height = bob_node.blockchain_db.get_height().unwrap().unwrap();
        for height in 0..=bob_tip_height {
            assert_eq!(
                alice_node.blockchain_db.fetch_block(height),
                bob_node.blockchain_db.fetch_block(height)
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager.clone(),
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
    );

    runtime.block_on(async {
        // Connect Alice to Bob and Carol
        alice_node
            .comms
            .connection_manager()
            .dial_peer(bob_node.node_identity.node_id().clone())
            .await
            .unwrap();
        alice_node
            .comms
            .connection_manager()
            .dial_peer(carol_node.node_identity.node_id().clone())
            .await
            .unwrap();

        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let carol_db = &carol_node.blockchain_db;
        // Bob and Carol is ahead of Alice and Bob is ahead of Carol
        prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        carol_db.add_block(prev_block.clone()).unwrap();
        for _ in 0..2 {
            prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        }

        // Sync Blocks from genesis block to tip. Alice will notice that the chain tip is equivalent to Bobs tip and
        // start to request blocks from her random peers. When Alice requests these blocks from Carol, Carol
        // won't always have these blocks and Alice will have to request these blocks again until her maximum attempts
        // have been reached.
        let state_event = BlockSyncInfo.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        let bob_tip_height = bob_db.get_height().unwrap().unwrap();
        for height in 1..=bob_tip_height {
            assert_eq!(
                alice_db.fetch_block(height).unwrap().block(),
                bob_db.fetch_block(height).unwrap().block()
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
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
    );
    // Shared chain
    let alice_db = &alice_node.blockchain_db;
    let bob_db = &bob_node.blockchain_db;
    for _ in 0..2 {
        prev_block = append_block(bob_db, &prev_block, vec![], &consensus_manager.consensus_constants()).unwrap();
        alice_db.add_block(prev_block.clone()).unwrap();
    }

    assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(2)));
    assert_eq!(bob_node.blockchain_db.get_height(), Ok(Some(2)));

    let mut alice_prev_block = prev_block.clone();
    let mut bob_prev_block = prev_block;
    // Alice fork
    for _ in 0..2 {
        alice_prev_block = append_block(
            alice_db,
            &alice_prev_block,
            vec![],
            &consensus_manager.consensus_constants(),
        )
        .unwrap();
    }
    // Bob fork
    for _ in 0..7 {
        println!(" ");
        println!("add block");
        bob_prev_block = append_block(
            bob_db,
            &bob_prev_block,
            vec![],
            &consensus_manager.consensus_constants(),
        )
        .unwrap();
    }
    assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(4)));
    assert_eq!(bob_node.blockchain_db.get_height(), Ok(Some(9)));

    runtime.block_on(async {
        let state_event = BlockSyncInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        assert_eq!(
            alice_node.blockchain_db.get_height(),
            bob_node.blockchain_db.get_height()
        );

        let bob_tip_height = bob_node.blockchain_db.get_height().unwrap().unwrap();
        for height in 0..=bob_tip_height {
            assert_eq!(
                alice_node.blockchain_db.fetch_block(height),
                bob_node.blockchain_db.fetch_block(height)
            );
        }

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

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

use std::time::Duration;

use tari_common::configuration::Network;
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        state_machine_service::{
            states::{HeaderSyncState, StateEvent, StatusInfo},
            BaseNodeStateMachine,
            BaseNodeStateMachineConfig,
        },
        sync::{HeaderSyncStatus, SyncPeer},
        SyncValidators,
    },
    blocks::ChainBlock,
    consensus::{ConsensusConstantsBuilder, ConsensusManager, ConsensusManagerBuilder},
    mempool::MempoolServiceConfig,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::blockchain::TempDatabase,
    transactions::test_helpers::{create_test_core_key_manager_with_memory_db, TestKeyManager},
    validation::mocks::MockValidator,
};
use tari_p2p::{services::liveness::config::LivenessConfig, P2pConfig};
use tari_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::sync::{broadcast, watch};

use crate::helpers::{
    block_builders::{append_block, create_genesis_block},
    nodes::{create_network_with_2_base_nodes_with_config, NodeInterfaces},
};

static EMISSION: [u64; 2] = [10, 10];

async fn sync_headers(
    alice_state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    alice_node: &NodeInterfaces,
    bob_node: &NodeInterfaces,
) -> StateEvent {
    let mut header_sync = HeaderSyncState::new(
        vec![SyncPeer::from(PeerChainMetadata::new(
            bob_node.node_identity.node_id().clone(),
            bob_node.blockchain_db.get_chain_metadata().unwrap(),
            None,
        ))],
        alice_node.blockchain_db.get_chain_metadata().unwrap(),
    );
    header_sync.next_event(alice_state_machine).await
}

async fn create_network_with_alice_and_bob_nodes() -> (
    BaseNodeStateMachine<TempDatabase>,
    NodeInterfaces,
    NodeInterfaces,
    ChainBlock,
    ConsensusManager,
    TestKeyManager,
) {
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (initial_block, _) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(initial_block.clone())
        .build()
        .unwrap();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        MempoolServiceConfig::default(),
        LivenessConfig {
            auto_ping_interval: Some(Duration::from_millis(100)),
            ..Default::default()
        },
        P2pConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    )
    .await;
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());

    // Alice needs a state machine for header sync
    let alice_state_machine = BaseNodeStateMachine::new(
        alice_node.blockchain_db.clone().into(),
        alice_node.local_nci.clone(),
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        status_event_sender,
        state_change_event_publisher,
        RandomXFactory::default(),
        consensus_manager.clone(),
        shutdown.to_signal(),
    );

    (
        alice_state_machine,
        alice_node,
        bob_node,
        initial_block,
        consensus_manager,
        key_manager,
    )
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync() {
    // Create the network with Alice node and Bob node
    let (mut alice_state_machine, alice_node, bob_node, initial_block, consensus_manager, key_manager) =
        create_network_with_alice_and_bob_nodes().await;

    // Add 1 block to Bob's chain
    let block_1_bob = append_block(
        &bob_node.blockchain_db,
        &initial_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(block_1_bob.height(), 1);
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 1);

    // Alice attempts header sync, still on the genesys block, headers will be lagging
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    // "Lagging"
    match event.clone() {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.peer_best_block_height, 1);
            assert_eq!(sync_result.peer_accumulated_difficulty, 4);
            assert_eq!(sync_result.headers_returned, 1);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.remote_tip_height, 1);
                assert_eq!(val.best_block_header.height(), 0);
                assert_eq!(val.reorg_steps_back, 0);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected HeadersSynchronized event"),
    }

    // Alice attempts header sync again, still on the genesys block, headers will be in sync
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    // "InSyncOrAhead"
    match event.clone() {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.peer_best_block_height, 1);
            assert_eq!(sync_result.peer_accumulated_difficulty, 4);
            assert_eq!(sync_result.headers_returned, 0);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::InSyncOrAhead = sync_result.header_sync_status {
                // Good, headers were in sync
            } else {
                panic!("Should be 'InSyncOrAhead'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }

    // Bob adds another block
    let block_2_bob = append_block(
        &bob_node.blockchain_db,
        &block_1_bob,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(block_2_bob.height(), 2);

    // Alice attempts header sync, still on the genesys block, headers will be lagging
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    // "Lagging"
    match event.clone() {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.peer_best_block_height, 2);
            assert_eq!(sync_result.peer_accumulated_difficulty, 7);
            assert_eq!(sync_result.headers_returned, 1);
            assert_eq!(sync_result.peer_fork_hash_index, 0);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.remote_tip_height, 2);
                assert_eq!(val.best_block_header.height(), 0);
                assert_eq!(val.reorg_steps_back, 0);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }

    // Alice adds 3 (different) blocks, with POW on par with Bob's chain, but with greater height
    let block_1_alice = append_block(
        &alice_node.blockchain_db,
        &initial_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    let block_2_alice = append_block(
        &alice_node.blockchain_db,
        &block_1_alice,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(2).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    // Alice adds another block, with POW on par with Bob's chain, but with greater height
    let block_3_alice = append_block(
        &alice_node.blockchain_db,
        &block_2_alice,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(1).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(block_3_alice.height(), 3);
    assert_eq!(block_3_alice.accumulated_data().total_accumulated_difficulty, 7);
    assert_eq!(
        block_3_alice.accumulated_data().total_accumulated_difficulty,
        block_2_bob.accumulated_data().total_accumulated_difficulty
    );

    // Alice attempts header sync, but POW is on par
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    match event.clone() {
        StateEvent::Continue => {
            // Good - Header sync not attempted, sync peer does not have better POW
        },
        _ => panic!("Expected StateEvent::Continue event"),
    }

    // Bob adds more blocks and draws ahead of Alice
    let block_3_bob = append_block(
        &bob_node.blockchain_db,
        &block_2_bob,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    let block_4_bob = append_block(
        &bob_node.blockchain_db,
        &block_3_bob,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(block_4_bob.height(), 4);

    // Alice attempts header sync, on a higher chain with less POW, headers will be lagging with reorg steps
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    // "Lagging"
    match event {
        StateEvent::HeadersSynchronized(_val, sync_result) => {
            assert_eq!(sync_result.peer_best_block_height, 4);
            assert_eq!(sync_result.peer_accumulated_difficulty, 13);
            assert_eq!(sync_result.headers_returned, 4);
            assert_eq!(sync_result.peer_fork_hash_index, 3);
            if let HeaderSyncStatus::Lagging(val) = sync_result.header_sync_status {
                assert_eq!(val.remote_tip_height, 4);
                assert_eq!(val.best_block_header.height(), 3);
                assert_eq!(val.reorg_steps_back, 3);
            } else {
                panic!("Should be 'Lagging'");
            }
        },
        _ => panic!("Expected StateEvent::HeadersSynchronized event"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_header_sync_different_chains() {
    // Create the network with Alice node and Bob node
    let (_alice_state_machine, _alice_node, bob_node, initial_block, consensus_manager, key_manager) =
        create_network_with_alice_and_bob_nodes().await;

    // Add a block to Bob's chain
    let _block_1_bob = append_block(
        &bob_node.blockchain_db,
        &initial_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 1);

    // Create a different chain for Alice
    let (mut alice_state_machine, alice_node, _bob_node, initial_block, consensus_manager, key_manager) =
        create_network_with_alice_and_bob_nodes().await;

    let _block_1_alice = append_block(
        &alice_node.blockchain_db,
        &initial_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(2).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 1);
    assert!(
        alice_node
            .blockchain_db
            .get_chain_metadata()
            .unwrap()
            .accumulated_difficulty() <
            bob_node
                .blockchain_db
                .get_chain_metadata()
                .unwrap()
                .accumulated_difficulty()
    );

    // Alice attempts header sync, still on the genesys block, headers will be lagging
    let event = sync_headers(&mut alice_state_machine, &alice_node, &bob_node).await;
    println!("\nevent -> bob_node_1: {:?}", event);
    // "Lagging"
    match event {
        StateEvent::HeaderSyncFailed(err) => {
            assert_eq!(&err, "No more sync peers available: Header sync failed");
        },
        _ => panic!("Expected HeaderSyncFailed event"),
    }
}

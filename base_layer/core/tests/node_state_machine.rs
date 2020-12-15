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
    chain_metadata::{random_peer_metadata, MockChainMetadata},
    nodes::{create_network_with_2_base_nodes_with_config, wait_until_online, BaseNodeBuilder},
};
use std::{thread, time::Duration};
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        comms_interface::Broadcast,
        service::BaseNodeServiceConfig,
        state_machine_service::{
            states::{Listening, StateEvent, StatusInfo, SyncStatus, SyncStatus::Lagging},
            BaseNodeStateMachine,
            BaseNodeStateMachineConfig,
        },
        SyncValidators,
    },
    chain_storage::BlockchainDatabaseConfig,
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    mempool::MempoolServiceConfig,
    test_helpers::blockchain::create_test_blockchain_db,
    transactions::types::CryptoFactories,
    validation::mocks::MockValidator,
};
use tari_p2p::services::liveness::LivenessConfig;
use tari_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::{
    runtime::Runtime,
    sync::{broadcast, watch},
    time,
};

static EMISSION: [u64; 2] = [10, 10];
#[test]
fn test_listening_lagging() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
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
        alice_node.blockchain_db.clone().into(),
        alice_node.local_nci.clone(),
        alice_node.outbound_nci.clone(),
        alice_node.comms.connectivity(),
        alice_node.comms.peer_manager(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        status_event_sender,
        state_change_event_publisher,
        consensus_manager.clone(),
        shutdown.to_signal(),
    );
    wait_until_online(&mut runtime, &[&alice_node, &bob_node]);

    let await_event_task = runtime.spawn(async move { Listening::new().next_event(&mut alice_state_machine).await });

    runtime.block_on(async move {
        let bob_db = bob_node.blockchain_db;
        let mut bob_local_nci = bob_node.local_nci;

        // Bob Block 1 - no block event
        let prev_block = append_block(&bob_db, &prev_block, vec![], &consensus_manager, 3.into()).unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db
            .prepare_block_merkle_roots(chain_block(&prev_block, vec![], &consensus_manager))
            .unwrap();
        bob_local_nci
            .submit_block(prev_block, Broadcast::from(true))
            .await
            .unwrap();
        assert_eq!(bob_db.get_height().unwrap(), 2);

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
    let db = create_test_blockchain_db();
    let shutdown = Shutdown::new();
    let mut mock = MockChainMetadata::new();
    let (state_change_event_publisher, mut state_change_event_subscriber) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = tokio::sync::watch::channel(StatusInfo::new());
    let state_machine = BaseNodeStateMachine::new(
        db.into(),
        node.local_nci.clone(),
        node.outbound_nci.clone(),
        node.comms.connectivity(),
        node.comms.peer_manager(),
        mock.subscription(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        status_event_sender,
        state_change_event_publisher,
        consensus_manager.clone(),
        shutdown.to_signal(),
    );

    runtime.spawn(state_machine.run());

    let PeerChainMetadata {
        node_id,
        chain_metadata,
    } = random_peer_metadata(10, 5_000);
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
                assert_eq!(data.height_of_longest_chain(), 10);
                assert_eq!(data.accumulated_difficulty(), 5_000);
                assert_eq!(peers[0].node_id, node_id);
            },
            _ => assert!(false),
        }
    });
}

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

use blake2::Digest;
use tari_common::configuration::Network;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        state_machine_service::{
            states::{Listening, StateEvent, StatusInfo},
            BaseNodeStateMachine,
            BaseNodeStateMachineConfig,
        },
        SyncValidators,
    },
    mempool::MempoolServiceConfig,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::blockchain::create_test_blockchain_db,
    transactions::test_helpers::create_test_core_key_manager_with_memory_db,
    validation::mocks::MockValidator,
};
use tari_crypto::hash::blake2::Blake256;
use tari_p2p::services::liveness::config::LivenessConfig;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tari_utilities::ByteArray;
use tempfile::tempdir;
use tokio::{
    sync::{broadcast, watch},
    task,
    time,
};

use crate::helpers::{
    block_builders::{append_block, chain_block},
    chain_metadata::MockChainMetadata,
    nodes::{create_network_with_2_base_nodes_with_config, random_node_identity, wait_until_online, BaseNodeBuilder},
};
use crate::helpers::block_builders::create_blockchain_with_genesis_block_only;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_listening_lagging() {
    let temp_dir = tempdir().unwrap();
    let key_manager = create_test_core_key_manager_with_memory_db();
    let (initial_block, consensus_manager, blockchain_db) =
        create_blockchain_with_genesis_block_only(Network::LocalNet, &None).await;
    let (alice_node, mut bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        MempoolServiceConfig::default(),
        LivenessConfig {
            auto_ping_interval: Some(Duration::from_millis(100)),
            ..Default::default()
        },
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
        Some(blockchain_db),
    )
    .await;
    let shutdown = Shutdown::new();
    let (state_change_event_publisher, _) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
    let mut alice_state_machine = BaseNodeStateMachine::new(
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
    wait_until_online(&[&alice_node, &bob_node]).await;

    let await_event_task = task::spawn(async move { Listening::new().next_event(&mut alice_state_machine).await });

    // TODO: It seems the event is actually sent
    // Bob Block 1 - no block event
    let block_1 = append_block(
        &bob_node.blockchain_db,
        &initial_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    assert_eq!(alice_node.blockchain_db.fetch_tip_header().unwrap().height(), bob_node.blockchain_db.fetch_tip_header().unwrap().height());

    // Bob Block 2 - with block event and liveness service metadata update
    let block_2 = bob_node.blockchain_db
        .prepare_new_block(chain_block(block_1.block(), vec![], &consensus_manager, &key_manager).await)
        .unwrap();
    bob_node.local_nci.submit_block(block_2).await.unwrap();
    assert_eq!(alice_node.blockchain_db.fetch_tip_header().unwrap().height(), bob_node.blockchain_db.fetch_tip_header().unwrap().height());
    assert_eq!(bob_node.blockchain_db.get_height().unwrap(), 2);

    match time::timeout(Duration::from_secs(5), await_event_task).await {
        Ok(event) => {
            if let Ok(state_event) = event {
                println!("Event: {:?}", state_event);
                assert!(matches!(state_event, StateEvent::FallenBehind(_)));
            } else {
                panic!("Unexpected event");
            }
        },
        Err(e) => {
            println!("TODO: It seems the event is actually sent, so Alice cannot fall behind");
            panic!("Timeout waiting for event ({})", e);
        },
    }
}

#[tokio::test]
async fn test_event_channel() {
    let temp_dir = tempdir().unwrap();
    let (node, consensus_manager) = BaseNodeBuilder::new(Network::Esmeralda.into())
        .start(temp_dir.path().to_str().unwrap(), None)
        .await;
    // let shutdown = Shutdown::new();
    let db = create_test_blockchain_db().unwrap();
    let shutdown = Shutdown::new();
    let mut mock = MockChainMetadata::new();
    let (state_change_event_publisher, mut state_change_event_subscriber) = broadcast::channel(10);
    let (status_event_sender, _status_event_receiver) = tokio::sync::watch::channel(StatusInfo::new());
    let state_machine = BaseNodeStateMachine::new(
        db.into(),
        node.local_nci.clone(),
        node.comms.connectivity(),
        node.comms.peer_manager(),
        mock.subscription(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        status_event_sender,
        state_change_event_publisher,
        RandomXFactory::default(),
        consensus_manager,
        shutdown.to_signal(),
    );

    task::spawn(state_machine.run());

    let node_identity = random_node_identity();
    let block_hash = Blake256::digest(node_identity.node_id().as_bytes()).into();
    let metadata = ChainMetadata::new(10, block_hash, 2800, 0, 5000, 0);

    node.comms
        .peer_manager()
        .add_peer(node_identity.to_peer())
        .await
        .unwrap();

    let peer_chain_metadata = PeerChainMetadata::new(node_identity.node_id().clone(), metadata, None);
    mock.publish_chain_metadata(
        peer_chain_metadata.node_id(),
        peer_chain_metadata.claimed_chain_metadata(),
    )
    .await
    .expect("Could not publish metadata");
    let event = state_change_event_subscriber.recv().await;
    assert_eq!(*event.unwrap(), StateEvent::Initialized);
    let event = state_change_event_subscriber.recv().await;
    let event = event.unwrap();
    unpack_enum!(StateEvent::FallenBehind(_) = &*event);
}

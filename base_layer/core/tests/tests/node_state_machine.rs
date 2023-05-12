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
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder},
    mempool::MempoolServiceConfig,
    proof_of_work::randomx_factory::RandomXFactory,
    test_helpers::blockchain::create_test_blockchain_db,
    transactions::CryptoFactories,
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
    block_builders::{append_block, chain_block, create_genesis_block},
    chain_metadata::MockChainMetadata,
    nodes::{create_network_with_2_base_nodes_with_config, random_node_identity, wait_until_online, BaseNodeBuilder},
};

static EMISSION: [u64; 2] = [10, 10];
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_listening_lagging() {
    let factories = CryptoFactories::default();
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (prev_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build();
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        MempoolServiceConfig::default(),
        LivenessConfig {
            auto_ping_interval: Some(Duration::from_millis(100)),
            ..Default::default()
        },
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
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

    let bob_db = bob_node.blockchain_db;
    let mut bob_local_nci = bob_node.local_nci;

    // Bob Block 1 - no block event
    let prev_block = append_block(&bob_db, &prev_block, vec![], &consensus_manager, 3.into()).unwrap();
    // Bob Block 2 - with block event and liveness service metadata update
    let mut prev_block = bob_db
        .prepare_new_block(chain_block(prev_block.block(), vec![], &consensus_manager))
        .unwrap();
    prev_block.header.output_mmr_size += 1;
    prev_block.header.kernel_mmr_size += 1;
    bob_local_nci.submit_block(prev_block).await.unwrap();
    assert_eq!(bob_db.get_height().unwrap(), 2);

    let next_event = time::timeout(Duration::from_secs(10), await_event_task)
        .await
        .expect("Alice did not emit `StateEvent::FallenBehind` within 10 seconds")
        .unwrap();

    assert!(matches!(next_event, StateEvent::FallenBehind(_)));
}

#[tokio::test]
async fn test_event_channel() {
    let temp_dir = tempdir().unwrap();
    let (node, consensus_manager) = BaseNodeBuilder::new(Network::Esmeralda.into())
        .start(temp_dir.path().to_str().unwrap())
        .await;
    // let shutdown = Shutdown::new();
    let db = create_test_blockchain_db();
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

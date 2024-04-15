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

use blake2::{Blake2b, Digest};
use digest::consts::U32;
use tari_common::configuration::Network;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        state_machine_service::{
            states::{Listening, StateEvent, StatusInfo, SyncStatus::Lagging},
            BaseNodeStateMachine,
            BaseNodeStateMachineConfig,
        },
        SyncValidators,
    },
    chain_storage::BlockchainDatabaseConfig,
    consensus::ConsensusManagerBuilder,
    mempool::MempoolServiceConfig,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::blockchain::create_test_blockchain_db,
    transactions::key_manager::create_memory_db_key_manager,
    validation::mocks::MockValidator,
};
use tari_p2p::{services::liveness::config::LivenessConfig, P2pConfig};
use tari_shutdown::Shutdown;
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
    nodes::{
        create_network_with_multiple_base_nodes_with_config,
        random_node_identity,
        wait_until_online,
        BaseNodeBuilder,
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_listening_lagging() {
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let key_manager = create_memory_db_key_manager();
    let consensus_constants = crate::helpers::sample_blockchains::consensus_constants(network).build();
    let (prev_block, _) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(prev_block.clone())
        .build()
        .unwrap();

    let (mut node_interfaces, consensus_manager) = create_network_with_multiple_base_nodes_with_config(
        vec![MempoolServiceConfig::default(); 2],
        vec![
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_millis(100)),
                ..Default::default()
            };
            2
        ],
        vec![BlockchainDatabaseConfig::default(); 2],
        vec![P2pConfig::default(); 2],
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
        network,
    )
    .await;
    let alice_node = node_interfaces.remove(0);
    let bob_node = node_interfaces.remove(0);

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
    let (prev_block, _) = append_block(
        &bob_db,
        &prev_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    // Bob Block 2 - with block event and liveness service metadata update
    let mut prev_block = bob_db
        .prepare_new_block(chain_block(prev_block.block(), vec![], &consensus_manager, &key_manager).await)
        .unwrap();
    prev_block.header.output_smt_size += 1;
    prev_block.header.kernel_mmr_size += 1;
    bob_local_nci.submit_block(prev_block).await.unwrap();
    assert_eq!(bob_db.get_height().unwrap(), 2);
    let next_event = time::timeout(Duration::from_secs(10), await_event_task)
        .await
        .expect("Alice did not emit `StateEvent::FallenBehind` within 10 seconds")
        .unwrap();

    assert!(matches!(next_event, StateEvent::FallenBehind(_)));
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_listening_initial_fallen_behind() {
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let key_manager = create_memory_db_key_manager();
    let consensus_constants = crate::helpers::sample_blockchains::consensus_constants(network).build();
    let (gen_block, _) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(gen_block.clone())
        .build()
        .unwrap();

    let (mut node_interfaces, consensus_manager) = create_network_with_multiple_base_nodes_with_config(
        vec![MempoolServiceConfig::default(); 3],
        vec![
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_millis(100)),
                ..Default::default()
            };
            3
        ],
        vec![BlockchainDatabaseConfig::default(); 3],
        vec![P2pConfig::default(); 3],
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
        network,
    )
    .await;
    let alice_node = node_interfaces.remove(0);
    let bob_node = node_interfaces.remove(0);
    let charlie_node = node_interfaces.remove(0);

    let shutdown = Shutdown::new();

    let bob_db = bob_node.blockchain_db;
    let mut bob_local_nci = bob_node.local_nci;

    // Bob Block 1 - no block event
    let (prev_block, _) = append_block(
        &bob_db,
        &gen_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    // Bob Block 2 - with block event and liveness service metadata update
    let mut prev_block = bob_db
        .prepare_new_block(chain_block(prev_block.block(), vec![], &consensus_manager, &key_manager).await)
        .unwrap();
    prev_block.header.output_smt_size += 1;
    prev_block.header.kernel_mmr_size += 1;
    bob_local_nci.submit_block(prev_block).await.unwrap();
    assert_eq!(bob_db.get_height().unwrap(), 2);

    let charlie_db = charlie_node.blockchain_db;
    let mut charlie_local_nci = charlie_node.local_nci;

    // charlie Block 1 - no block event
    let (prev_block, _) = append_block(
        &charlie_db,
        &gen_block,
        vec![],
        &consensus_manager,
        Difficulty::from_u64(3).unwrap(),
        &key_manager,
    )
    .await
    .unwrap();
    // charlie Block 2 - with block event and liveness service metadata update
    let mut prev_block = charlie_db
        .prepare_new_block(chain_block(prev_block.block(), vec![], &consensus_manager, &key_manager).await)
        .unwrap();
    prev_block.header.output_smt_size += 1;
    prev_block.header.kernel_mmr_size += 1;
    charlie_local_nci.submit_block(prev_block).await.unwrap();
    assert_eq!(charlie_db.get_height().unwrap(), 2);

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

    assert_eq!(alice_node.blockchain_db.get_height().unwrap(), 0);
    let await_event_task = task::spawn(async move { Listening::new().next_event(&mut alice_state_machine).await });

    let next_event = time::timeout(Duration::from_secs(10), await_event_task)
        .await
        .expect("Alice did not emit `StateEvent::FallenBehind` within 10 seconds")
        .unwrap();

    assert!(matches!(next_event, StateEvent::FallenBehind(_)));
    if let StateEvent::FallenBehind(Lagging {
        local: _,
        network: _,
        sync_peers,
    }) = next_event
    {
        assert_eq!(sync_peers.len(), 2);
    } else {
        panic!("should have gotten a StateEvent::FallenBehind with 2 peers")
    }
}

#[tokio::test]
async fn test_event_channel() {
    let temp_dir = tempdir().unwrap();
    let (node, consensus_manager) = BaseNodeBuilder::new(Network::Esmeralda.into())
        .start(temp_dir.path().to_str().unwrap(), BlockchainDatabaseConfig::default())
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
    let block_hash = Blake2b::<U32>::digest(node_identity.node_id().as_bytes()).into();
    let metadata = ChainMetadata::new(10, block_hash, 2800, 0, 5000.into(), 0).unwrap();

    node.comms
        .peer_manager()
        .add_peer(node_identity.to_peer())
        .await
        .unwrap();

    let peer_chain_metadata = PeerChainMetadata::new(node_identity.node_id().clone(), metadata, None);
    for _ in 0..5 {
        mock.publish_chain_metadata(
            peer_chain_metadata.node_id(),
            peer_chain_metadata.claimed_chain_metadata(),
        )
        .await
        .expect("Could not publish metadata");
    }
    let event = state_change_event_subscriber.recv().await;
    assert_eq!(*event.unwrap(), StateEvent::Initialized);
    let event = state_change_event_subscriber.recv().await;
    let event = event.unwrap();
    assert!(matches!(&*event, StateEvent::FallenBehind(_)));
}

// Copyright 2020. The Tari Project
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

use std::time::Duration;

use tari_common::configuration::Network;
use tari_common_types::types::HashOutput;
use tari_comms::peer_manager::NodeId;
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        state_machine_service::states::{BlockSync, HeaderSyncState, StateEvent, StatusInfo},
        sync::SyncPeer,
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
        SyncValidators,
    },
    blocks::ChainBlock,
    chain_storage::DbTransaction,
    consensus::{ConsensusConstantsBuilder, ConsensusManager, ConsensusManagerBuilder},
    mempool::MempoolServiceConfig,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::blockchain::TempDatabase,
    transactions::key_manager::{create_memory_db_key_manager, MemoryDbKeyManager},
    validation::mocks::MockValidator,
};
use tari_p2p::{services::liveness::LivenessConfig, P2pConfig};
use tari_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::sync::{broadcast, watch};

use crate::helpers::{
    block_builders::{append_block, create_genesis_block},
    nodes::{create_network_with_2_base_nodes_with_config, NodeInterfaces},
};

static EMISSION: [u64; 2] = [10, 10];

pub fn initialize_sync_headers_with_ping_pong_data(
    local_node_interfaces: &NodeInterfaces,
    peer_node_interfaces: &NodeInterfaces,
) -> HeaderSyncState {
    HeaderSyncState::new(
        vec![SyncPeer::from(PeerChainMetadata::new(
            peer_node_interfaces.node_identity.node_id().clone(),
            peer_node_interfaces.blockchain_db.get_chain_metadata().unwrap(),
            None,
        ))],
        local_node_interfaces.blockchain_db.get_chain_metadata().unwrap(),
    )
}

pub async fn sync_headers_execute(
    state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    header_sync: &mut HeaderSyncState,
) -> StateEvent {
    header_sync.next_event(state_machine).await
}

pub fn initialize_sync_blocks(peer_node_interfaces: &NodeInterfaces) -> BlockSync {
    BlockSync::from(vec![SyncPeer::from(PeerChainMetadata::new(
        peer_node_interfaces.node_identity.node_id().clone(),
        peer_node_interfaces.blockchain_db.get_chain_metadata().unwrap(),
        None,
    ))])
}

pub async fn sync_blocks_execute(
    state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    block_sync: &mut BlockSync,
) -> StateEvent {
    block_sync.next_event(state_machine).await
}

pub async fn create_network_with_local_and_peer_nodes() -> (
    BaseNodeStateMachine<TempDatabase>,
    NodeInterfaces,
    NodeInterfaces,
    ChainBlock,
    ConsensusManager,
    MemoryDbKeyManager,
) {
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let key_manager = create_memory_db_key_manager();
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), &EMISSION, 100.into())
        .build();
    let (initial_block, _) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(initial_block.clone())
        .build()
        .unwrap();
    let (local_node, peer_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
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
    let local_state_machine = BaseNodeStateMachine::new(
        local_node.blockchain_db.clone().into(),
        local_node.local_nci.clone(),
        local_node.comms.connectivity(),
        local_node.comms.peer_manager(),
        local_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        status_event_sender,
        state_change_event_publisher,
        RandomXFactory::default(),
        consensus_manager.clone(),
        shutdown.to_signal(),
    );

    (
        local_state_machine,
        local_node,
        peer_node,
        initial_block,
        consensus_manager,
        key_manager,
    )
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum WhatToDelete {
    BlocksAndHeaders,
    Blocks,
    Headers,
}

// Private helper function to setup a delete a block transaction.
// Note: This private function will panic if the index is out of bounds - caller function's responsibility.
fn delete_block(txn: &mut DbTransaction, node: &NodeInterfaces, blocks: &[ChainBlock], index: usize) {
    txn.delete_tip_block(*blocks[index].hash());
    txn.delete_orphan(*blocks[index].hash());
    txn.set_best_block(
        blocks[index + 1].height(),
        blocks[index + 1].accumulated_data().hash,
        blocks[index + 1].accumulated_data().total_accumulated_difficulty,
        *node.blockchain_db.get_chain_metadata().unwrap().best_block(),
        blocks[index + 1].to_chain_header().timestamp(),
    );
}

// Delete blocks and headers in reverse order; the first block in the slice wil not be deleted
pub fn delete_some_blocks_and_headers(
    blocks_with_anchor: &[ChainBlock],
    instruction: WhatToDelete,
    node: &NodeInterfaces,
) {
    if blocks_with_anchor.is_empty() || blocks_with_anchor.len() < 2 {
        panic!("blocks must have at least 2 elements");
    }
    let mut blocks: Vec<_> = blocks_with_anchor.to_vec();
    blocks.reverse();
    for i in 0..blocks.len() - 1 {
        let mut txn = DbTransaction::new();
        match instruction {
            WhatToDelete::BlocksAndHeaders => {
                delete_block(&mut txn, node, &blocks, i);
                txn.delete_header(blocks[i].height());
            },
            WhatToDelete::Blocks => {
                delete_block(&mut txn, node, &blocks, i);
            },
            WhatToDelete::Headers => {
                txn.delete_header(blocks[i].height());
            },
        }
        node.blockchain_db.write(txn).unwrap();
        match instruction {
            WhatToDelete::BlocksAndHeaders => {
                assert!(!node
                    .blockchain_db
                    .chain_block_or_orphan_block_exists(*blocks[i].hash())
                    .unwrap());
                assert!(node
                    .blockchain_db
                    .fetch_header_by_block_hash(*blocks[i].hash())
                    .unwrap()
                    .is_none());
            },
            WhatToDelete::Blocks => {
                assert!(!node
                    .blockchain_db
                    .chain_block_or_orphan_block_exists(*blocks[i].hash())
                    .unwrap());
            },
            WhatToDelete::Headers => {
                assert!(node
                    .blockchain_db
                    .fetch_header_by_block_hash(*blocks[i].hash())
                    .unwrap()
                    .is_none());
            },
        }
    }
}

#[allow(dead_code)]
pub fn set_best_block(block: &ChainBlock, previous_block_hash: &HashOutput, node: &NodeInterfaces) {
    let mut txn = DbTransaction::new();
    txn.set_best_block(
        block.height(),
        block.accumulated_data().hash,
        block.accumulated_data().total_accumulated_difficulty,
        *previous_block_hash,
        block.to_chain_header().timestamp(),
    );
    node.blockchain_db.write(txn).unwrap();
}

pub fn add_some_existing_blocks(blocks: &[ChainBlock], node: &NodeInterfaces) {
    for block in blocks {
        let _res = node.blockchain_db.add_block(block.block().clone().into()).unwrap();
    }
}

// Return blocks added, including the start block
pub async fn create_and_add_some_blocks(
    node: &NodeInterfaces,
    start_block: &ChainBlock,
    number_of_blocks: usize,
    consensus_manager: &ConsensusManager,
    key_manager: &MemoryDbKeyManager,
    difficulties: &[u64],
) -> Vec<ChainBlock> {
    if number_of_blocks != difficulties.len() {
        panic!(
            "Number of blocks ({}) and difficulties length ({}) must be equal",
            number_of_blocks,
            difficulties.len()
        );
    }
    let mut blocks = vec![start_block.clone()];
    let mut prev_block = start_block.clone();
    for item in difficulties.iter().take(number_of_blocks) {
        prev_block = append_block(
            &node.blockchain_db,
            &prev_block,
            vec![],
            consensus_manager,
            Difficulty::from_u64(*item).unwrap(),
            key_manager,
        )
        .await
        .unwrap();
        blocks.push(prev_block.clone());
    }
    blocks
}

// We give some time for the peer to be banned as it is an async process
pub async fn wait_for_is_peer_banned(this_node: &NodeInterfaces, peer_node_id: &NodeId, seconds: u64) -> bool {
    let interval_ms = 100;
    let intervals = seconds * 1000 / interval_ms;
    for _ in 0..intervals {
        tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        if this_node
            .comms
            .peer_manager()
            .is_peer_banned(peer_node_id)
            .await
            .unwrap()
        {
            return true;
        }
    }
    false
}

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

use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use tari_common::configuration::Network;
use tari_common_types::types::HashOutput;
use tari_comms::peer_manager::NodeId;
use tari_core::{
    base_node::{
        chain_metadata_service::PeerChainMetadata,
        state_machine_service::states::{
            BlockSync,
            DecideNextSync,
            HeaderSyncState,
            HorizonStateSync,
            StateEvent,
            StatusInfo,
        },
        sync::SyncPeer,
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
        SyncValidators,
    },
    blocks::ChainBlock,
    chain_storage::{BlockchainDatabaseConfig, DbTransaction},
    consensus::{ConsensusManager, ConsensusManagerBuilder},
    mempool::MempoolServiceConfig,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    test_helpers::blockchain::TempDatabase,
    transactions::{
        key_manager::{create_memory_db_key_manager, MemoryDbKeyManager},
        tari_amount::T,
        test_helpers::schema_to_transaction,
        transaction_components::{Transaction, WalletOutput},
    },
    txn_schema,
    validation::mocks::MockValidator,
    OutputSmt,
};
use tari_p2p::{services::liveness::LivenessConfig, P2pConfig};
use tari_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::sync::{broadcast, watch};

use crate::helpers::{
    block_builders::{append_block, create_genesis_block},
    nodes::{create_network_with_multiple_base_nodes_with_config, NodeInterfaces},
    sample_blockchains,
};

/// Helper function to initialize header sync with a single peer
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

/// Helper function to initialize header sync with a single peer
pub async fn sync_headers_execute(
    state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    header_sync: &mut HeaderSyncState,
) -> StateEvent {
    header_sync.next_event(state_machine).await
}

/// Helper function to initialize block sync with a single peer
pub fn initialize_sync_blocks(peer_node_interfaces: &NodeInterfaces) -> BlockSync {
    BlockSync::from(vec![SyncPeer::from(PeerChainMetadata::new(
        peer_node_interfaces.node_identity.node_id().clone(),
        peer_node_interfaces.blockchain_db.get_chain_metadata().unwrap(),
        None,
    ))])
}

/// Helper function to initialize block sync with a single peer
pub async fn sync_blocks_execute(
    state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    block_sync: &mut BlockSync,
) -> StateEvent {
    block_sync.next_event(state_machine).await
}

/// Helper function to decide what to do next
pub async fn decide_horizon_sync(
    local_state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    local_header_sync: HeaderSyncState,
) -> StateEvent {
    let mut next_sync = DecideNextSync::from(local_header_sync.clone());
    next_sync.next_event(local_state_machine).await
}

/// Helper function to initialize horizon state sync with a single peer
pub fn initialize_horizon_sync_without_header_sync(peer_node_interfaces: &NodeInterfaces) -> HorizonStateSync {
    HorizonStateSync::from(vec![SyncPeer::from(PeerChainMetadata::new(
        peer_node_interfaces.node_identity.node_id().clone(),
        peer_node_interfaces.blockchain_db.get_chain_metadata().unwrap(),
        None,
    ))])
}

/// Helper function to initialize horizon state sync with a single peer
pub async fn horizon_sync_execute(
    state_machine: &mut BaseNodeStateMachine<TempDatabase>,
    horizon_sync: &mut HorizonStateSync,
) -> StateEvent {
    horizon_sync.next_event(state_machine).await
}

/// Helper function to create a network with multiple nodes
pub async fn create_network_with_multiple_nodes(
    blockchain_db_configs: Vec<BlockchainDatabaseConfig>,
) -> (
    Vec<BaseNodeStateMachine<TempDatabase>>,
    Vec<NodeInterfaces>,
    ChainBlock,
    ConsensusManager,
    MemoryDbKeyManager,
    WalletOutput,
) {
    let num_nodes = blockchain_db_configs.len();
    if num_nodes < 2 {
        panic!("Must have at least 2 nodes");
    }
    let network = Network::LocalNet;
    let temp_dir = tempdir().unwrap();
    let key_manager = create_memory_db_key_manager();
    let consensus_constants = sample_blockchains::consensus_constants(network).build();
    let (initial_block, coinbase_wallet_output) = create_genesis_block(&consensus_constants, &key_manager).await;
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .add_consensus_constants(consensus_constants)
        .with_block(initial_block.clone())
        .build()
        .unwrap();
    let (node_interfaces, consensus_manager) = create_network_with_multiple_base_nodes_with_config(
        vec![MempoolServiceConfig::default(); num_nodes],
        vec![
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_millis(100)),
                ..Default::default()
            };
            num_nodes
        ],
        blockchain_db_configs,
        vec![P2pConfig::default(); num_nodes],
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
        network,
    )
    .await;
    let shutdown = Shutdown::new();

    let mut state_machines = Vec::with_capacity(num_nodes);
    for node_interface in node_interfaces.iter().take(num_nodes) {
        let (state_change_event_publisher, _) = broadcast::channel(10);
        let (status_event_sender, _status_event_receiver) = watch::channel(StatusInfo::new());
        state_machines.push(BaseNodeStateMachine::new(
            node_interface.blockchain_db.clone().into(),
            node_interface.local_nci.clone(),
            node_interface.comms.connectivity(),
            node_interface.comms.peer_manager(),
            node_interface.chain_metadata_handle.get_event_stream(),
            BaseNodeStateMachineConfig::default(),
            SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
            status_event_sender,
            state_change_event_publisher,
            RandomXFactory::default(),
            consensus_manager.clone(),
            shutdown.to_signal(),
        ));
    }

    (
        state_machines,
        node_interfaces,
        initial_block,
        consensus_manager,
        key_manager,
        coinbase_wallet_output,
    )
}

/// Helper enum to specify what to delete
#[allow(dead_code)]
#[derive(Debug)]
pub enum WhatToDelete {
    BlocksAndHeaders,
    Blocks,
    Headers,
}

// Private helper function to setup a delete a block transaction.
// Note: This private function will panic if the index is out of bounds - caller function's responsibility.
fn delete_block(
    txn: &mut DbTransaction,
    node: &NodeInterfaces,
    blocks: &[ChainBlock],
    index: usize,
    smt: Arc<RwLock<OutputSmt>>,
) {
    txn.delete_tip_block(*blocks[index].hash(), smt);
    txn.delete_orphan(*blocks[index].hash());
    txn.set_best_block(
        blocks[index + 1].height(),
        blocks[index + 1].accumulated_data().hash,
        blocks[index + 1].accumulated_data().total_accumulated_difficulty,
        *node.blockchain_db.get_chain_metadata().unwrap().best_block_hash(),
        blocks[index + 1].to_chain_header().timestamp(),
    );
}

/// Delete blocks and headers in reverse order; the first block in the slice wil not be deleted
pub fn delete_some_blocks_and_headers(
    blocks_with_anchor: &[ChainBlock],
    instruction: WhatToDelete,
    node: &NodeInterfaces,
) {
    let smt = node.blockchain_db.smt().clone();
    if blocks_with_anchor.is_empty() || blocks_with_anchor.len() < 2 {
        panic!("blocks must have at least 2 elements");
    }
    let mut blocks: Vec<_> = blocks_with_anchor.to_vec();
    blocks.reverse();
    for i in 0..blocks.len() - 1 {
        let mut txn = DbTransaction::new();
        match instruction {
            WhatToDelete::BlocksAndHeaders => {
                delete_block(&mut txn, node, &blocks, i, smt.clone());
                txn.delete_header(blocks[i].height());
            },
            WhatToDelete::Blocks => {
                delete_block(&mut txn, node, &blocks, i, smt.clone());
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

/// Set the best block in the blockchain_db
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

/// Add some existing blocks to the blockchain_db
pub fn add_some_existing_blocks(blocks: &[ChainBlock], node: &NodeInterfaces) {
    for block in blocks {
        let _res = node.blockchain_db.add_block(block.block().clone().into()).unwrap();
    }
}

/// Return blocks and coinbases added, including the start block and coinbase
pub async fn create_and_add_some_blocks(
    node: &NodeInterfaces,
    start_block: &ChainBlock,
    start_coinbase: &WalletOutput,
    number_of_blocks: usize,
    consensus_manager: &ConsensusManager,
    key_manager: &MemoryDbKeyManager,
    difficulties: &[u64],
    transactions: &Option<Vec<Vec<Transaction>>>,
) -> (Vec<ChainBlock>, Vec<WalletOutput>) {
    let transactions = if let Some(val) = transactions {
        val.clone()
    } else {
        vec![vec![]; number_of_blocks]
    };
    if number_of_blocks != difficulties.len() || number_of_blocks != transactions.len() {
        panic!(
            "Number of blocks ({}), transactions length ({}) and difficulties length ({}) must be equal",
            number_of_blocks,
            transactions.len(),
            difficulties.len()
        );
    }
    let mut blocks = vec![start_block.clone()];
    let mut coinbases = vec![start_coinbase.clone()];
    let mut prev_block = start_block.clone();
    for (item, txns) in difficulties.iter().zip(transactions.iter()) {
        let (new_block, coinbase) = append_block(
            &node.blockchain_db,
            &prev_block,
            txns.clone(),
            consensus_manager,
            Difficulty::from_u64(*item).unwrap(),
            key_manager,
        )
        .await
        .unwrap();
        prev_block = new_block.clone();
        blocks.push(new_block.clone());
        coinbases.push(coinbase.clone());
    }
    (blocks, coinbases)
}

/// We give some time for the peer to be banned as it is an async process
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

/// Condensed format of the state machine state for display
pub fn state_event(event: &StateEvent) -> String {
    match event {
        StateEvent::Initialized => "Initialized".to_string(),
        StateEvent::HeadersSynchronized(_, _) => "HeadersSynchronized".to_string(),
        StateEvent::HeaderSyncFailed(_) => "HeaderSyncFailed".to_string(),
        StateEvent::ProceedToHorizonSync(_) => "ProceedToHorizonSync".to_string(),
        StateEvent::ProceedToBlockSync(_) => "ProceedToBlockSync".to_string(),
        StateEvent::HorizonStateSynchronized => "HorizonStateSynchronized".to_string(),
        StateEvent::HorizonStateSyncFailure => "HorizonStateSyncFailure".to_string(),
        StateEvent::BlocksSynchronized => "BlocksSynchronized".to_string(),
        StateEvent::BlockSyncFailed => "BlockSyncFailed".to_string(),
        StateEvent::FallenBehind(_) => "FallenBehind".to_string(),
        StateEvent::NetworkSilence => "NetworkSilence".to_string(),
        StateEvent::FatalError(_) => "FatalError".to_string(),
        StateEvent::Continue => "Continue".to_string(),
        StateEvent::UserQuit => "UserQuit".to_string(),
    }
}

/// Return blocks and coinbases added, including the start block and coinbase
pub async fn create_block_chain_with_transactions(
    node: &NodeInterfaces,
    initial_block: &ChainBlock,
    initial_coinbase: &WalletOutput,
    consensus_manager: &ConsensusManager,
    key_manager: &MemoryDbKeyManager,
    intermediate_height: u64,
    number_of_blocks: usize,
    spend_genesis_coinbase_in_block: usize,
    follow_up_transaction_in_block: usize,
    follow_up_coinbases_to_spend: usize,
) -> (Vec<ChainBlock>, Vec<WalletOutput>) {
    assert!(spend_genesis_coinbase_in_block > 1);
    assert!((spend_genesis_coinbase_in_block as u64) < intermediate_height);
    assert!(follow_up_transaction_in_block > spend_genesis_coinbase_in_block + 1);
    assert!((follow_up_transaction_in_block as u64) > intermediate_height);
    assert!(number_of_blocks as u64 > follow_up_transaction_in_block as u64 + intermediate_height + 1);
    let add_blocks_a = spend_genesis_coinbase_in_block - 1;
    let add_blocks_b = follow_up_transaction_in_block - 1 - add_blocks_a;
    let add_blocks_c = number_of_blocks - add_blocks_a - add_blocks_b;
    assert!(follow_up_coinbases_to_spend > add_blocks_a);
    assert!(follow_up_coinbases_to_spend < follow_up_transaction_in_block);

    // Create a blockchain with some blocks to enable spending the genesys coinbase early on
    let (blocks_a, coinbases_a) = create_and_add_some_blocks(
        node,
        initial_block,
        initial_coinbase,
        add_blocks_a,
        consensus_manager,
        key_manager,
        &vec![3; add_blocks_a],
        &None,
    )
    .await;
    assert_eq!(node.blockchain_db.get_height().unwrap(), add_blocks_a as u64);
    assert_eq!(
        node.blockchain_db.fetch_last_header().unwrap().height,
        add_blocks_a as u64
    );
    // Add a transaction to spend the genesys coinbase
    let schema = txn_schema!(
        from: vec![initial_coinbase.clone()],
        to: vec![1 * T; 10]
    );
    let (txns_genesis_coinbase, _outputs) = schema_to_transaction(&[schema], key_manager).await;
    let mut txns_all = vec![vec![]; add_blocks_b];
    txns_all[0] = txns_genesis_coinbase
        .into_iter()
        .map(|t| Arc::try_unwrap(t).unwrap())
        .collect::<Vec<_>>();
    // Expand the blockchain with the genesys coinbase spend transaction
    let (blocks_b, coinbases_b) = create_and_add_some_blocks(
        node,
        &blocks_a[blocks_a.len() - 1],
        &coinbases_a[coinbases_a.len() - 1],
        add_blocks_b,
        consensus_manager,
        key_manager,
        &vec![3; add_blocks_b],
        &Some(txns_all),
    )
    .await;
    assert_eq!(
        node.blockchain_db.get_height().unwrap(),
        (add_blocks_a + add_blocks_b) as u64
    );
    assert_eq!(
        node.blockchain_db.fetch_last_header().unwrap().height,
        (add_blocks_a + add_blocks_b) as u64
    );
    // Add a transaction to spend some more coinbase outputs
    let mut coinbases_to_spend = Vec::with_capacity(follow_up_coinbases_to_spend);
    for coinbase in coinbases_a.iter().skip(1)
    // Skip the genesys coinbase
    {
        coinbases_to_spend.push(coinbase.clone());
    }
    for coinbase in coinbases_b
        .iter()
        .skip(1) // Skip the last coinbase of the previously added blocks
        .take(follow_up_coinbases_to_spend - coinbases_to_spend.len())
    {
        coinbases_to_spend.push(coinbase.clone());
    }
    assert_eq!(coinbases_to_spend.len(), follow_up_coinbases_to_spend);
    let schema = txn_schema!(
        from: coinbases_to_spend,
        to: vec![1 * T; 20]
    );
    let (txns_additional_coinbases, _outputs) = schema_to_transaction(&[schema], key_manager).await;
    let mut txns_all = vec![vec![]; add_blocks_c];
    txns_all[0] = txns_additional_coinbases
        .into_iter()
        .map(|t| Arc::try_unwrap(t).unwrap())
        .collect::<Vec<_>>();
    // Expand the blockchain with the spend transaction
    let (blocks_c, coinbases_c) = create_and_add_some_blocks(
        node,
        &blocks_b[blocks_b.len() - 1],
        &coinbases_b[coinbases_b.len() - 1],
        add_blocks_c,
        consensus_manager,
        key_manager,
        &vec![3; add_blocks_c],
        &Some(txns_all),
    )
    .await;
    assert_eq!(node.blockchain_db.get_height().unwrap(), number_of_blocks as u64);
    assert_eq!(
        node.blockchain_db.fetch_last_header().unwrap().height,
        number_of_blocks as u64
    );
    let blocks = [&blocks_a[..], &blocks_b[1..], &blocks_c[1..]].concat();
    let coinbases = [&coinbases_a[..], &coinbases_b[1..], &coinbases_c[1..]].concat();

    (blocks, coinbases)
}

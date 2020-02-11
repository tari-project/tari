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

use helpers::{
    block_builders::{append_block, chain_block, create_genesis_block},
    nodes::{create_network_with_2_base_nodes, create_network_with_2_base_nodes_with_config},
};
use std::time::Duration;
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{BlockSyncConfig, BlockSyncInfo, ListeningConfig, ListeningInfo, StateEvent, SyncStatus::Lagging},
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
    },
    mempool::MempoolServiceConfig,
    transactions::types::CryptoFactories,
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_test_utils::random::string;
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn test_listening_lagging() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
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
        temp_dir.path().to_str().unwrap(),
    );
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        runtime.handle().clone(),
        alice_node.chain_metadata_handle.get_event_stream(),
        BaseNodeStateMachineConfig::default(),
    );

    let alice_db = &alice_node.blockchain_db;
    let bob_db = bob_node.blockchain_db;
    let mut bob_local_nci = bob_node.local_nci;
    let (mut prev_block, _) = create_genesis_block(alice_db, &factories);
    alice_db.add_block(prev_block.clone()).unwrap();
    bob_db.add_block(prev_block.clone()).unwrap();

    runtime.block_on(async {
        // Bob Block 1 - no block event
        prev_block = append_block(&bob_db, &prev_block, vec![]).unwrap();
        // Bob Block 2 - with block event and liveness service metadata update
        let prev_block = bob_db.calculate_mmr_roots(chain_block(&prev_block, vec![])).unwrap();
        bob_local_nci.submit_block(prev_block.clone()).await.unwrap();
        assert_eq!(bob_db.get_height(), Ok(Some(2)));

        let state_event = ListeningInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::FallenBehind(Lagging(2)));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_listening_network_silence() {
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (alice_node, bob_node) = create_network_with_2_base_nodes(&mut runtime, temp_dir.path().to_str().unwrap());
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
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    // let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 2 };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        // mmr_cache_config,
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
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

    let adb = &alice_node.blockchain_db;
    let (mut prev_block, _) = create_genesis_block(&adb, &factories);
    adb.add_block(prev_block.clone()).unwrap();

    let db = &bob_node.blockchain_db;
    db.add_block(prev_block.clone()).unwrap();
    for _ in 1..6 {
        prev_block = append_block(db, &prev_block, vec![]).unwrap();
    }

    runtime.block_on(async {
        // Sync Blocks from genesis block to tip
        let state_event = BlockSyncInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        assert_eq!(adb.get_height(), db.get_height());

        let bob_tip_height = db.get_height().unwrap().unwrap();
        for height in 1..=bob_tip_height {
            assert_eq!(adb.fetch_block(height), db.fetch_block(height));
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_lagging_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
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
    let (mut prev_block, _) = create_genesis_block(db, &factories);
    db.add_block(prev_block.clone()).unwrap();

    alice_node.blockchain_db.add_block(prev_block.clone()).unwrap();
    for _ in 0..4 {
        prev_block = append_block(db, &prev_block, vec![]).unwrap();
        alice_node.blockchain_db.add_block(prev_block.clone()).unwrap();
    }
    for _ in 0..2 {
        prev_block = append_block(db, &prev_block, vec![]).unwrap();
    }
    assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(4)));
    assert_eq!(bob_node.blockchain_db.get_height(), Ok(Some(6)));

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
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

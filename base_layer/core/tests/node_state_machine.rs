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
    block_builders::{append_block, create_genesis_block, create_genesis_block_with_utxos, generate_new_block},
    nodes::create_network_with_2_base_nodes_with_config,
};
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{BlockSyncConfig, BlockSyncInfo, HorizonInfo, HorizonSyncConfig, StateEvent},
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
    },
    chain_storage::MmrTree,
    mempool::MempoolServiceConfig,
    transactions::{
        tari_amount::{uT, T},
        types::CryptoFactories,
    },
    txn_schema,
};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_test_utils::random::string;
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn test_horizon_state_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 1,
        max_history_len: 3,
    };
    let (alice_node, mut bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        MempoolServiceConfig::default(),
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig {
        horizon_sync_config: HorizonSyncConfig {
            leaf_nodes_sync_chunk_size: 5,
            headers_sync_chunk_size: 5,
            kernels_sync_chunk_size: 5,
            utxos_sync_chunk_size: 5,
        },
        block_sync_config: BlockSyncConfig::default(),
    };
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        state_machine_config,
    );

    let db = &mut bob_node.blockchain_db;
    let (block0, utxo0) = create_genesis_block_with_utxos(db, &factories, &[10 * T]);
    db.add_block(block0.clone()).unwrap();
    let mut blocks = vec![block0];
    let mut outputs = vec![utxo0];
    for i in 1..13 {
        let schema = vec![txn_schema!(
            from: vec![outputs[i - 1][1].clone()],
            to: vec![50_000 * uT]
        )];
        generate_new_block(db, &mut blocks, &mut outputs, schema).unwrap();
    }

    let bob_utxo_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000).unwrap();
    let bob_kernel_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000).unwrap();
    let bob_rp_mmr_state = db.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000).unwrap();

    runtime.block_on(async {
        let horizon_block = db.fetch_horizon_block_height().unwrap();
        let mut horizon_info = HorizonInfo::new(horizon_block);
        let state_event = horizon_info.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::HorizonStateFetched);

        let adb = &alice_node.blockchain_db;
        let alice_utxo_mmr_state = adb.fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000).unwrap();
        let alice_kernel_mmr_state = adb.fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000).unwrap();
        let alice_rp_mmr_state = adb.fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000).unwrap();

        assert_eq!(alice_utxo_mmr_state, bob_utxo_mmr_state);
        assert_eq!(alice_kernel_mmr_state, bob_kernel_mmr_state);
        assert_eq!(alice_rp_mmr_state, bob_rp_mmr_state);

        for height in 0..=horizon_block {
            assert_eq!(adb.fetch_header(height), db.fetch_header(height));
        }

        for hash in bob_kernel_mmr_state.leaf_nodes.leaf_hashes {
            assert_eq!(adb.fetch_kernel(hash.clone()), db.fetch_kernel(hash));
        }

        for hash in bob_utxo_mmr_state.leaf_nodes.leaf_hashes {
            if let Ok(utxo) = db.fetch_utxo(hash.clone()) {
                assert_eq!(adb.fetch_utxo(hash).unwrap(), utxo);
            }
        }

        assert_eq!(adb.get_height(), Ok(Some(horizon_block)));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_block_sync() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 2,
        max_history_len: 4,
    };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        MempoolServiceConfig::default(),
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
        state_machine_config,
    );

    let db = &bob_node.blockchain_db;
    let (mut prev_block, _) = create_genesis_block(db, &factories);
    db.add_block(prev_block.clone()).unwrap();
    for _ in 0..6 {
        prev_block = append_block(db, &prev_block, vec![]).unwrap();
    }

    runtime.block_on(async {
        // Sync horizon state
        let horizon_block = bob_node.blockchain_db.fetch_horizon_block_height().unwrap();
        let mut horizon_info = HorizonInfo::new(horizon_block);
        let state_event = horizon_info.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::HorizonStateFetched);
        let adb = &alice_node.blockchain_db;
        assert_eq!(adb.get_height(), Ok(Some(horizon_block)));

        // Sync Blocks from horizon state to tip
        let state_event = BlockSyncInfo {}.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        assert_eq!(adb.get_height(), db.get_height());

        let alice_utxo_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000)
            .unwrap();
        let alice_kernel_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000)
            .unwrap();
        let alice_rp_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000)
            .unwrap();
        let bob_utxo_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000)
            .unwrap();
        let bob_kernel_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000)
            .unwrap();
        let bob_rp_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000)
            .unwrap();
        assert_eq!(alice_utxo_mmr_state, bob_utxo_mmr_state);
        assert_eq!(alice_kernel_mmr_state, bob_kernel_mmr_state);
        assert_eq!(alice_rp_mmr_state, bob_rp_mmr_state);

        let bob_tip_height = db.get_height().unwrap().unwrap();
        for height in horizon_block + 1..=bob_tip_height {
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
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        MempoolServiceConfig::default(),
        temp_dir.path().to_str().unwrap(),
    );
    let state_machine_config = BaseNodeStateMachineConfig::default();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.outbound_nci,
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

        let alice_utxo_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000)
            .unwrap();
        let alice_kernel_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000)
            .unwrap();
        let alice_rp_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000)
            .unwrap();
        let bob_utxo_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000)
            .unwrap();
        let bob_kernel_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000)
            .unwrap();
        let bob_rp_mmr_state = alice_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000)
            .unwrap();
        assert_eq!(alice_utxo_mmr_state, bob_utxo_mmr_state);
        assert_eq!(alice_kernel_mmr_state, bob_kernel_mmr_state);
        assert_eq!(alice_rp_mmr_state, bob_rp_mmr_state);

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

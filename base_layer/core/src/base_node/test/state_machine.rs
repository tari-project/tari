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

use crate::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{BlockSyncConfig, BlockSyncInfo, HorizonInfo, HorizonSyncConfig, StateEvent},
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
    },
    blocks::genesis_block::get_genesis_block,
    chain_storage::{DbTransaction, MmrTree},
    mempool::MempoolServiceConfig,
    test_utils::{
        builders::{add_block_and_update_header, chain_block},
        node::create_network_with_2_base_nodes_with_config,
    },
    tx,
};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_test_utils::random::string;
use tari_transactions::{tari_amount::uT, types::CryptoFactories};
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn test_horizon_state_sync() {
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 2,
        max_history_len: 4,
    };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &runtime,
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

    let mut prev_block = add_block_and_update_header(&bob_node.blockchain_db, get_genesis_block());
    for _ in 0..12 {
        let (tx, inputs, _) = tx!(10_000*uT, fee: 50*uT, inputs: 1, outputs: 1);
        let mut txn = DbTransaction::new();
        txn.insert_utxo(inputs[0].as_transaction_output(&factories).unwrap(), true);
        assert!(bob_node.blockchain_db.commit(txn).is_ok());

        let next_block = chain_block(&prev_block, vec![tx.clone()]);
        prev_block = add_block_and_update_header(&bob_node.blockchain_db, next_block);
    }

    let bob_utxo_mmr_state = bob_node
        .blockchain_db
        .fetch_mmr_base_leaf_nodes(MmrTree::Utxo, 0, 1000)
        .unwrap();
    let bob_kernel_mmr_state = bob_node
        .blockchain_db
        .fetch_mmr_base_leaf_nodes(MmrTree::Kernel, 0, 1000)
        .unwrap();
    let bob_rp_mmr_state = bob_node
        .blockchain_db
        .fetch_mmr_base_leaf_nodes(MmrTree::RangeProof, 0, 1000)
        .unwrap();

    runtime.block_on(async {
        // TODO: This is a temporary fix. Currently, there is a disconnect between the Metadata Horizon block height and
        // the MMR changetracker horizon height.
        // let metadata=bob_node.blockchain_db.get_metadata().unwrap();
        // let horizon_block: u64 = metadata.horizon_block(metadata.height_of_longest_chain.unwrap());
        let horizon_block = bob_node
            .blockchain_db
            .fetch_mmr_base_leaf_node_count(MmrTree::Header)
            .unwrap() as u64;

        let mut horizon_info = HorizonInfo::new(horizon_block);
        let state_event = horizon_info.next_event(&mut alice_state_machine).await;
        assert_eq!(state_event, StateEvent::HorizonStateFetched);

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
        assert_eq!(alice_utxo_mmr_state, bob_utxo_mmr_state);
        assert_eq!(alice_kernel_mmr_state, bob_kernel_mmr_state);
        assert_eq!(alice_rp_mmr_state, bob_rp_mmr_state);

        for height in 0..horizon_block {
            assert_eq!(
                alice_node.blockchain_db.fetch_header(height),
                bob_node.blockchain_db.fetch_header(height)
            );
        }

        for hash in bob_kernel_mmr_state.leaf_nodes.leaf_hashes {
            assert_eq!(
                alice_node.blockchain_db.fetch_kernel(hash.clone()),
                bob_node.blockchain_db.fetch_kernel(hash)
            );
        }

        for hash in bob_utxo_mmr_state.leaf_nodes.leaf_hashes {
            if let Ok(utxo) = bob_node.blockchain_db.fetch_utxo(hash.clone()) {
                assert_eq!(alice_node.blockchain_db.fetch_utxo(hash).unwrap(), utxo);
            }
        }

        assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(8)));
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_block_sync_from_horizon() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 2,
        max_history_len: 4,
    };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &runtime,
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

    let mut prev_block = add_block_and_update_header(&bob_node.blockchain_db, get_genesis_block());
    for _ in 0..6 {
        let next_block = chain_block(&prev_block, vec![]);
        prev_block = add_block_and_update_header(&bob_node.blockchain_db, next_block);
    }

    runtime.block_on(async {
        // Sync to horizon state
        let horizon_block = bob_node
            .blockchain_db
            .fetch_mmr_base_leaf_nodes(MmrTree::Header, 0, 100)
            .unwrap()
            .total_leaf_count as u64;
        let mut horizon_info = HorizonInfo::new(horizon_block);
        horizon_info.next_event(&mut alice_state_machine).await;
        assert_eq!(alice_node.blockchain_db.get_height(), Ok(Some(2)));

        // Sync Blocks from horizon state to tip
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

        // Block 0,1 and 2 will be beyond the pruning horizon, the rest are fetchable
        let bob_tip_height = bob_node.blockchain_db.get_height().unwrap().unwrap();
        for height in 0..bob_tip_height {
            assert_eq!(
                alice_node.blockchain_db.fetch_block(height),
                bob_node.blockchain_db.fetch_block(height)
            );
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

#[test]
fn test_lagging_block_sync() {
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    let (alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &runtime,
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

    let mut prev_block = add_block_and_update_header(&bob_node.blockchain_db, get_genesis_block());
    alice_node.blockchain_db.add_block(prev_block.clone()).unwrap();
    for _ in 0..4 {
        let next_block = chain_block(&prev_block, vec![]);
        prev_block = add_block_and_update_header(&bob_node.blockchain_db, next_block);
        alice_node.blockchain_db.add_block(prev_block.clone()).unwrap();
    }
    for _ in 0..2 {
        let next_block = chain_block(&prev_block, vec![]);
        prev_block = add_block_and_update_header(&bob_node.blockchain_db, next_block);
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
        for height in 0..bob_tip_height {
            assert_eq!(
                alice_node.blockchain_db.fetch_block(height),
                bob_node.blockchain_db.fetch_block(height)
            );
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

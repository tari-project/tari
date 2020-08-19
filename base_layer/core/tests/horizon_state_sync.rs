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

use crate::helpers::block_builders::append_block_with_coinbase;
use helpers::{block_builders::create_genesis_block, nodes::create_network_with_2_base_nodes_with_config};
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{
            BestChainMetadataBlockSyncInfo,
            BlockSyncConfig,
            HeaderSync,
            HorizonStateSync,
            HorizonSyncConfig,
            StateEvent,
            SyncPeer,
            SyncPeerConfig,
        },
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
        SyncValidators,
    },
    chain_storage::{BlockchainBackend, BlockchainDatabase, BlockchainDatabaseConfig, MmrTree},
    consensus::{ConsensusConstantsBuilder, ConsensusManagerBuilder, Network},
    crypto::script::TariScript,
    mempool::MempoolServiceConfig,
    transactions::{
        fee::Fee,
        helpers::{create_utxo, spend_utxos},
        tari_amount::uT,
        transaction::UnblindedOutput,
        types::CryptoFactories,
    },
    txn_schema,
    validation::mocks::MockValidator,
};
use tari_crypto::tari_utilities::Hashable;
use tari_mmr::MmrCacheConfig;
use tari_p2p::services::liveness::LivenessConfig;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tempfile::tempdir;
use tokio::runtime::Runtime;

#[test]
fn test_pruned_mode_sync_with_future_horizon_sync_height() {
    // Number of blocks to create in addition to the genesis
    const NUM_BLOCKS: u64 = 10;
    const SYNC_OFFSET: u64 = 0;
    const PRUNING_HORIZON: u64 = 4;
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (genesis_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(genesis_block.clone())
        .build();
    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: PRUNING_HORIZON,
        pruning_interval: 5,
    };
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = SYNC_OFFSET;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let mut prev_block = genesis_block.clone();
        for _ in 0..NUM_BLOCKS {
            // Need coinbases for kernels and utxos
            let (block, _) =
                append_block_with_coinbase(&factories, bob_db, &prev_block, vec![], &consensus_manager, 1.into())
                    .unwrap();
            prev_block = block;
        }

        let node_count = bob_db.fetch_mmr_node_count(MmrTree::Kernel, 6).unwrap();
        assert_eq!(node_count, 7);
        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let network_tip = bob_db.get_chain_metadata().unwrap();
        assert_eq!(network_tip.effective_pruned_height, 6);
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];

        // Synchronize headers
        let state_event = HeaderSync::new(network_tip.clone(), sync_peers.clone())
            .next_event(&mut alice_state_machine)
            .await;
        unpack_enum!(StateEvent::HeadersSynchronized(local_metadata, sync_height) = state_event);

        // Synchronize Kernels and UTXOs
        assert_eq!(sync_height, NUM_BLOCKS - PRUNING_HORIZON + SYNC_OFFSET);
        let state_event = HorizonStateSync::new(local_metadata, network_tip.clone(), sync_peers.clone(), sync_height)
            .next_event(&mut alice_state_machine)
            .await;
        assert_eq!(state_event, StateEvent::HorizonStateSynchronized);
        let alice_metadata = alice_db.get_chain_metadata().unwrap();
        // Local height should now be at the horizon sync height
        assert_eq!(alice_metadata.height_of_longest_chain(), sync_height);
        assert_eq!(alice_metadata.effective_pruned_height, sync_height);

        // Check Kernel MMR nodes after horizon sync
        let alice_num_kernels = alice_db.fetch_mmr_node_count(MmrTree::Kernel, sync_height).unwrap();
        let bob_num_kernels = bob_db.fetch_mmr_node_count(MmrTree::Kernel, sync_height).unwrap();
        assert_eq!(alice_num_kernels, bob_num_kernels);
        let alice_kernel_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(sync_height))
            .unwrap();
        let bob_kernel_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(sync_height))
            .unwrap();
        assert_eq!(alice_kernel_nodes, bob_kernel_nodes);

        // Synchronize full blocks
        let state_event = BestChainMetadataBlockSyncInfo
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        let alice_metadata = alice_db.get_chain_metadata().unwrap();
        // Local height should now be at the horizon sync height
        assert_eq!(
            alice_metadata.effective_pruned_height,
            network_tip.height_of_longest_chain() - network_tip.pruning_horizon
        );

        check_final_state(&alice_db, &bob_db);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_pruned_mode_sync_with_spent_utxos() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (genesis_block, output) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(genesis_block.clone())
        .build();
    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 4,
        pruning_interval: 5,
    };
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = 0;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(
            MockValidator::new(true),
            // TODO: Need a test helper which adds the correct reward to a coinbase UTXO as per consensus to use the
            //       ChainBalanceValidator
            MockValidator::new(true),
        ),
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let mut outputs = vec![output];
        let mut prev_block = genesis_block;
        for _ in 0..4 {
            // Need coinbases for kernels and utxos
            let (block, coinbase) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
            prev_block = block;
            outputs.push(coinbase);
        }

        // Spend coinbases before horizon height
        {
            let supply = consensus_manager.emission_schedule().supply_at_block(4);
            let fee = Fee::calculate(25 * uT, 5, 5, 2);
            let schema = txn_schema!(from: outputs, to: vec![supply - fee], fee: 25 * uT);
            let (tx, _, _) = spend_utxos(schema);

            let (block, _) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![tx],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
            prev_block = block;
        }

        let mut outputs = vec![];
        for _ in 0..6 {
            // Need coinbases for kernels and utxos
            let (block, coinbase) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
            prev_block = block;
            outputs.push(coinbase);
        }

        // Spend the other coinbases (why not?)
        {
            let supply = consensus_manager.emission_schedule().supply_at_block(4);
            let fee = Fee::calculate(25 * uT, 5, 5, 2);
            let schema = txn_schema!(from: outputs, to: vec![supply - fee], fee: 25 * uT);
            let (tx, _, _) = spend_utxos(schema);

            let (_, _) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![tx],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
        }

        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let network_tip = bob_db.get_chain_metadata().unwrap();
        // effective_pruned_height is 6 because the interval is 5 - we have 12 blocks but the last time the node was
        // pruned was at 10 (10 - 4 = 6)
        assert_eq!(network_tip.effective_pruned_height, 6);
        assert_eq!(network_tip.height_of_longest_chain(), 12);
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = HeaderSync::new(network_tip.clone(), sync_peers.clone())
            .next_event(&mut alice_state_machine)
            .await;
        unpack_enum!(StateEvent::HeadersSynchronized(local_metadata, sync_height) = state_event);
        // network tip - pruning horizon + offset
        assert_eq!(sync_height, 12 - 4 + 0);
        let state_event = HorizonStateSync::new(local_metadata, network_tip.clone(), sync_peers.clone(), sync_height)
            .next_event(&mut alice_state_machine)
            .await;
        assert_eq!(state_event, StateEvent::HorizonStateSynchronized);
        let state_event = BestChainMetadataBlockSyncInfo
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        check_final_state(&alice_db, &bob_db);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

#[test]
fn test_pruned_mode_sync_with_spent_faucet_utxo_before_horizon() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();

    let consensus_manager = ConsensusManagerBuilder::new(Network::Rincewind).build();
    let mut genesis_block = consensus_manager.get_genesis_block();
    let faucet_value = 5000 * uT;
    let faucet_utxo = create_utxo(faucet_value, &factories, None, None).unwrap(); let faucet_key = faucet_utxo.blinding_factor().clone(); let faucet_utxo = faucet_utxo.as_transaction_output(&factories).unwrap();
    genesis_block.body.add_output(faucet_utxo);
    // Create a LocalNet consensus manager that uses rincewind consensus constants and has a custom rincewind genesis
    // block that contains an extra faucet utxo
    let consensus_manager = ConsensusManagerBuilder::new(Network::LocalNet)
        .with_block(genesis_block.clone())
        .with_consensus_constants(consensus_manager.consensus_constants().clone())
        .build();

    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: 4,
        pruning_interval: 4,
    };
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = 0;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(
            MockValidator::new(true),
            // TODO: Need a test helper which adds the correct reward to a coinbase UTXO as per consensus to use the
            //       ChainBalanceValidator
            MockValidator::new(true),
        ),
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let mut prev_block = genesis_block;
        for _ in 0..4 {
            // Need coinbases for kernels and utxos
            let (block, _) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![],
                &consensus_manager,
                1.into(),
            )
            .unwrap();

            prev_block = block;
        }

        // Spend faucet UTXO
        {
            let fee = Fee::calculate(25 * uT, 1, 1, 2);
            let output = UnblindedOutput::new(faucet_value, faucet_key, None, TariScript::default(), &factories.commitment).unwrap();
            let schema = txn_schema!(from: vec![output], to: vec![faucet_value - fee], fee: 25 * uT);
            let (tx, _, _) = spend_utxos(schema);

            // Need coinbases for kernels and utxos
            let (block, _) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![tx],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
            prev_block = block;
        }

        for _ in 0..6 {
            // Need coinbases for kernels and utxos
            let (block, _) = append_block_with_coinbase(
                &factories,
                &bob_node.blockchain_db,
                &prev_block,
                vec![],
                &consensus_manager,
                1.into(),
            )
            .unwrap();
            prev_block = block;
        }

        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let network_tip = bob_db.get_chain_metadata().unwrap();
        assert_eq!(network_tip.height_of_longest_chain(), 11);
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];
        let state_event = HeaderSync::new(network_tip.clone(), sync_peers.clone())
            .next_event(&mut alice_state_machine)
            .await;
        unpack_enum!(StateEvent::HeadersSynchronized(local_metadata, sync_height) = state_event);
        // network tip - pruning horizon + offset
        assert_eq!(sync_height, 11 - 4 + 0);
        let state_event = HorizonStateSync::new(local_metadata, network_tip.clone(), sync_peers.clone(), sync_height)
            .next_event(&mut alice_state_machine)
            .await;
        assert_eq!(state_event, StateEvent::HorizonStateSynchronized);
        let state_event = BestChainMetadataBlockSyncInfo
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);

        check_final_state(&alice_db, &bob_db);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

fn check_final_state<B: BlockchainBackend>(alice_db: &BlockchainDatabase<B>, bob_db: &BlockchainDatabase<B>) {
    let network_tip = bob_db.get_chain_metadata().unwrap();

    let alice_metadata = alice_db.get_chain_metadata().unwrap();
    assert_eq!(
        alice_metadata.height_of_longest_chain(),
        network_tip.height_of_longest_chain()
    );
    assert_eq!(
        alice_metadata.best_block.as_ref().unwrap(),
        network_tip.best_block.as_ref().unwrap()
    );
    assert_eq!(
        alice_metadata.accumulated_difficulty.as_ref().unwrap(),
        network_tip.accumulated_difficulty.as_ref().unwrap()
    );

    // Check headers
    let network_tip_height = network_tip.height_of_longest_chain.unwrap_or(0);
    let block_nums = (0..=network_tip_height).collect::<Vec<u64>>();
    let alice_headers = alice_db.fetch_headers(block_nums.clone()).unwrap();
    let bob_headers = bob_db.fetch_headers(block_nums).unwrap();
    assert_eq!(alice_headers, bob_headers);

    // Check Kernel MMR nodes
    let alice_num_kernels = alice_db
        .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
        .unwrap();
    let bob_num_kernels = bob_db
        .fetch_mmr_node_count(MmrTree::Kernel, network_tip_height)
        .unwrap();
    assert_eq!(alice_num_kernels, bob_num_kernels);
    let alice_kernel_nodes = alice_db
        .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(network_tip_height))
        .unwrap();
    let bob_kernel_nodes = bob_db
        .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(network_tip_height))
        .unwrap();
    assert_eq!(alice_kernel_nodes, bob_kernel_nodes);

    // Check Kernels
    let alice_kernel_hashes = alice_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
    let bob_kernels_hashes = bob_kernel_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>();
    let alice_kernels = alice_db.fetch_kernels(alice_kernel_hashes).unwrap();
    let bob_kernels = bob_db.fetch_kernels(bob_kernels_hashes).unwrap();
    assert_eq!(alice_kernels, bob_kernels);

    // Check UTXO MMR nodes
    let alice_num_utxos = alice_db
        .fetch_mmr_node_count(MmrTree::Utxo, network_tip_height)
        .unwrap();
    let bob_num_utxos = bob_db.fetch_mmr_node_count(MmrTree::Utxo, network_tip_height).unwrap();
    assert_eq!(alice_num_utxos, bob_num_utxos);
    let alice_utxo_nodes = alice_db
        .fetch_mmr_nodes(MmrTree::Utxo, 0, alice_num_utxos, Some(network_tip_height))
        .unwrap();
    let bob_utxo_nodes = bob_db
        .fetch_mmr_nodes(MmrTree::Utxo, 0, bob_num_utxos, Some(network_tip_height))
        .unwrap();
    assert_eq!(alice_utxo_nodes, bob_utxo_nodes);
    // Check UTXOs
    let mut alice_utxos = Vec::new();
    for (hash, deleted) in alice_utxo_nodes {
        if !deleted {
            alice_utxos.push(alice_db.fetch_utxo(hash).unwrap());
        }
    }
    let mut bob_utxos = Vec::new();
    for (hash, deleted) in bob_utxo_nodes {
        if !deleted {
            bob_utxos.push(bob_db.fetch_utxo(hash).unwrap());
        }
    }
    assert_eq!(alice_utxos, bob_utxos);

    // Check RangeProof MMR nodes
    let alice_num_rps = alice_db
        .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
        .unwrap();
    let bob_num_rps = bob_db
        .fetch_mmr_node_count(MmrTree::RangeProof, network_tip_height)
        .unwrap();
    assert_eq!(alice_num_rps, bob_num_rps);
    let alice_rps_nodes = alice_db
        .fetch_mmr_nodes(MmrTree::RangeProof, 0, alice_num_rps, Some(network_tip_height))
        .unwrap();
    let bob_rps_nodes = bob_db
        .fetch_mmr_nodes(MmrTree::RangeProof, 0, bob_num_rps, Some(network_tip_height))
        .unwrap();
    assert_eq!(alice_rps_nodes, bob_rps_nodes);

    let block = alice_db.fetch_block(network_tip_height).unwrap();
    assert_eq!(block.block.header.height, network_tip_height);
    assert_eq!(block.block.header.hash(), network_tip.best_block.unwrap());
}

#[test]
fn test_pruned_mode_sync_fail_final_validation() {
    // Number of blocks to create in addition to the genesis
    const NUM_BLOCKS: u64 = 10;
    const SYNC_OFFSET: u64 = 0;
    const PRUNING_HORIZON: u64 = 4;
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let temp_dir = tempdir().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network)
        .with_emission_amounts(100_000_000.into(), 0.999, 100.into())
        .build();
    let (genesis_block, _) = create_genesis_block(&factories, &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(genesis_block.clone())
        .build();
    let blockchain_db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: 3,
        pruning_horizon: PRUNING_HORIZON,
        pruning_interval: 5,
    };
    let (alice_node, bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        blockchain_db_config,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig::default(),
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    let mut horizon_sync_config = HorizonSyncConfig::default();
    horizon_sync_config.horizon_sync_height_offset = SYNC_OFFSET;
    let state_machine_config = BaseNodeStateMachineConfig {
        block_sync_config: BlockSyncConfig::default(),
        horizon_sync_config,
        sync_peer_config: SyncPeerConfig::default(),
    };
    let shutdown = Shutdown::new();
    let mut alice_state_machine = BaseNodeStateMachine::new(
        &alice_node.blockchain_db,
        &alice_node.local_nci,
        &alice_node.outbound_nci,
        alice_node.comms.peer_manager(),
        alice_node.comms.connectivity(),
        alice_node.chain_metadata_handle.get_event_stream(),
        state_machine_config,
        SyncValidators::new(MockValidator::new(true), MockValidator::new(false)),
        shutdown.to_signal(),
    );

    runtime.block_on(async {
        let alice_db = &alice_node.blockchain_db;
        let bob_db = &bob_node.blockchain_db;
        let mut prev_block = genesis_block.clone();
        for _ in 0..NUM_BLOCKS {
            // Need coinbases for kernels and utxos
            let (block, _) =
                append_block_with_coinbase(&factories, bob_db, &prev_block, vec![], &consensus_manager, 1.into())
                    .unwrap();
            prev_block = block;
        }

        // Both nodes are running in pruned mode and can not use block sync to synchronize state. Sync horizon state
        // from genesis block to horizon_sync_height and then block sync to the tip.
        let network_tip = bob_db.get_chain_metadata().unwrap();
        assert_eq!(network_tip.effective_pruned_height, 6);
        let mut sync_peers = vec![SyncPeer {
            node_id: bob_node.node_identity.node_id().clone(),
            chain_metadata: network_tip.clone(),
        }];

        // Synchronize headers
        let state_event = HeaderSync::new(network_tip.clone(), sync_peers.clone())
            .next_event(&mut alice_state_machine)
            .await;
        unpack_enum!(StateEvent::HeadersSynchronized(local_metadata, sync_height) = state_event);

        // Sync horizon state. Final state validation will fail (MockValidator::new(false))
        assert_eq!(sync_height, NUM_BLOCKS - PRUNING_HORIZON + SYNC_OFFSET);
        let state_event = HorizonStateSync::new(local_metadata, network_tip.clone(), sync_peers.clone(), sync_height)
            .next_event(&mut alice_state_machine)
            .await;
        assert_eq!(state_event, StateEvent::HorizonStateSyncFailure);

        // Check the state was rolled back
        let node_count = alice_db.fetch_mmr_node_count(MmrTree::Kernel, sync_height).unwrap();
        assert_eq!(node_count, 1);
        let node_count = alice_db.fetch_mmr_node_count(MmrTree::Utxo, sync_height).unwrap();
        assert_eq!(node_count, 1);
        let node_count = alice_db.fetch_mmr_node_count(MmrTree::RangeProof, sync_height).unwrap();
        assert_eq!(node_count, 1);

        assert!(alice_db.get_horizon_sync_state().unwrap().is_none());
        let local_metadata = alice_db.get_chain_metadata().unwrap();
        assert!(local_metadata.best_block.is_some());

        let mut alice_state_machine = BaseNodeStateMachine::new(
            &alice_node.blockchain_db,
            &alice_node.local_nci,
            &alice_node.outbound_nci,
            alice_node.comms.peer_manager(),
            alice_node.comms.connectivity(),
            alice_node.chain_metadata_handle.get_event_stream(),
            state_machine_config,
            SyncValidators::new(MockValidator::new(true), MockValidator::new(true)),
            shutdown.to_signal(),
        );

        // Synchronize Kernels and UTXOs
        let local_metadata = alice_db.get_chain_metadata().unwrap();
        let state_event = HorizonStateSync::new(local_metadata, network_tip.clone(), sync_peers.clone(), sync_height)
            .next_event(&mut alice_state_machine)
            .await;
        assert_eq!(state_event, StateEvent::HorizonStateSynchronized);

        let alice_metadata = alice_db.get_chain_metadata().unwrap();
        // Local height should now be at the horizon sync height
        assert_eq!(alice_metadata.height_of_longest_chain(), sync_height);
        assert_eq!(alice_metadata.effective_pruned_height, sync_height);

        // Check Kernel MMR nodes after horizon sync
        let alice_num_kernels = alice_db.fetch_mmr_node_count(MmrTree::Kernel, sync_height).unwrap();
        let bob_num_kernels = bob_db.fetch_mmr_node_count(MmrTree::Kernel, sync_height).unwrap();
        assert_eq!(alice_num_kernels, bob_num_kernels);
        let alice_kernel_nodes = alice_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, alice_num_kernels, Some(sync_height))
            .unwrap();
        let bob_kernel_nodes = bob_db
            .fetch_mmr_nodes(MmrTree::Kernel, 0, bob_num_kernels, Some(sync_height))
            .unwrap();
        assert_eq!(alice_kernel_nodes, bob_kernel_nodes);

        // Synchronize full blocks
        let state_event = BestChainMetadataBlockSyncInfo
            .next_event(&mut alice_state_machine, &network_tip, &mut sync_peers)
            .await;
        assert_eq!(state_event, StateEvent::BlocksSynchronized);
        let alice_metadata = alice_db.get_chain_metadata().unwrap();
        // Local height should now be at the horizon sync height
        assert_eq!(
            alice_metadata.effective_pruned_height,
            network_tip.height_of_longest_chain() - network_tip.pruning_horizon
        );

        check_final_state(&alice_db, &bob_db);

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

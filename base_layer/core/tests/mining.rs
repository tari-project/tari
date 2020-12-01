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
    block_builders::create_genesis_block_with_coinbase_value,
    event_stream::event_stream_next,
    nodes::create_network_with_2_base_nodes_with_config,
};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        state_machine_service::states::{ListeningInfo, StateEvent, StateInfo, StatusInfo},
    },
    chain_storage::BlockchainDatabaseConfig,
    consensus::{ConsensusManagerBuilder, Network},
    mempool::{MempoolServiceConfig, TxStorageResponse},
    mining::Miner,
    proto,
    transactions::{helpers::schema_to_transaction, tari_amount::T, types::CryptoFactories},
    txn_schema,
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{services::liveness::LivenessConfig, tari_message::TariMessageType};
use tari_shutdown::Shutdown;
use tari_test_utils::async_assert_eventually;
use tempfile::tempdir;
use tokio::{runtime::Runtime, sync::broadcast};

#[test]
fn mining() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = network.create_consensus_constants();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let (block0, utxos0) =
        create_genesis_block_with_coinbase_value(&factories, 100_000_000.into(), &consensus_constants[0]);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants[0].clone())
        .with_block(block0.clone())
        .build();
    let (mut alice_node, mut bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        MmrCacheConfig { rewind_hist_len: 10 },
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );
    alice_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });
    // Bob sends Alice a transaction, the transaction is received by the mempool service. The mempool service validates
    // it and sends it to the mempool where it is added to the unconfirmed pool.
    let (tx1, _) = schema_to_transaction(&vec![txn_schema!(from: vec![utxos0.clone()], to: vec![1 * T, 1 * T])]);
    let tx1 = (*tx1[0]).clone();
    let tx1_excess_sig = tx1.body.kernels()[0].excess_sig.clone();
    runtime.block_on(async {
        bob_node
            .outbound_message_service
            .send_direct(
                alice_node.node_identity.public_key().clone(),
                OutboundDomainMessage::new(
                    TariMessageType::NewTransaction,
                    proto::types::Transaction::from(tx1.clone()),
                ),
            )
            .await
            .unwrap();
        async_assert_eventually!(
            alice_node
                .mempool
                .has_tx_with_excess_sig(tx1_excess_sig.clone())
                .unwrap(),
            expect = TxStorageResponse::UnconfirmedPool,
            max_attempts = 20,
            interval = Duration::from_millis(1000)
        );
    });
    // Setup and start the miner
    let shutdown = Shutdown::new();
    let mut miner = Miner::new(shutdown.to_signal(), consensus_manager, &alice_node.local_nci, 1);
    miner.enable_mining_flag().store(true, Ordering::Relaxed);
    let (state_event_sender, state_event_receiver) = broadcast::channel(1);
    miner.subscribe_to_node_state_events(state_event_receiver);
    miner.subscribe_to_mempool_state_events(alice_node.local_mp_interface.get_mempool_state_event_stream());
    let mut miner_utxo_stream = miner.get_utxo_receiver_channel().fuse();
    runtime.spawn(miner.mine());

    runtime.block_on(async {
        // Simulate the BlockSync event
        state_event_sender
            .send(Arc::new(StateEvent::BlocksSynchronized))
            .unwrap();
        // Wait for miner to finish mining block 1
        assert!(event_stream_next(&mut miner_utxo_stream, Duration::from_secs(20))
            .await
            .is_some());
        // Check that the mined block was submitted to the base node service and the block was added to the blockchain
        let block1 = alice_node.blockchain_db.fetch_block(1).unwrap().block().clone();
        assert_eq!(block1.body.outputs().len(), 4);

        // Check if the outputs of tx1 appeared as outputs in block1
        let mut found_tx_outputs = 0;
        for tx_output in tx1.body.outputs() {
            for block_output in block1.body.outputs() {
                if tx_output == block_output {
                    found_tx_outputs += 1;
                    break;
                }
            }
        }
        assert_eq!(found_tx_outputs, tx1.body.outputs().len());
        async_assert_eventually!(
            alice_node
                .mempool
                .has_tx_with_excess_sig(tx1_excess_sig.clone())
                .unwrap(),
            expect = TxStorageResponse::ReorgPool,
            max_attempts = 10,
            interval = Duration::from_secs(1),
        );
    });
}

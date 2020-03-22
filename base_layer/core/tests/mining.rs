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

use futures::{SinkExt, StreamExt};
use helpers::{
    block_builders::create_genesis_block_with_coinbase_value,
    event_stream::event_stream_next,
    nodes::create_network_with_2_base_nodes_with_config,
};
use std::{sync::atomic::Ordering, time::Duration};
use tari_broadcast_channel::{bounded, Publisher, Subscriber};
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption};
use tari_core::{
    base_node::{
        service::BaseNodeServiceConfig,
        states::{BaseNodeState, ListeningInfo},
    },
    consensus::{ConsensusManagerBuilder, Network},
    mempool::{MempoolServiceConfig, TxStorageResponse},
    mining::Miner,
    transactions::{helpers::schema_to_transaction, proto, tari_amount::T, types::CryptoFactories},
    txn_schema,
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{services::liveness::LivenessConfig, tari_message::TariMessageType};
use tari_shutdown::Shutdown;
use tari_test_utils::{async_assert_eventually, random::string};
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn mining() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let consensus_constants = network.create_consensus_constants();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (block0, utxos0) =
        create_genesis_block_with_coinbase_value(&factories, 100_000_000.into(), &consensus_constants);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants)
        .with_block(block0.clone())
        .build();
    let (alice_node, mut bob_node, consensus_manager) = create_network_with_2_base_nodes_with_config(
        &mut runtime,
        BaseNodeServiceConfig::default(),
        MmrCacheConfig { rewind_hist_len: 10 },
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        temp_dir.path().to_str().unwrap(),
    );

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
                OutboundEncryption::None,
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
    let (mut state_event_sender, state_event_receiver): (Publisher<BaseNodeState>, Subscriber<BaseNodeState>) =
        bounded(1);
    miner.subscribe_to_state_change(state_event_receiver);
    let miner_utxo_stream = miner.get_utxo_receiver_channel().fuse();
    runtime.spawn(miner.mine());

    runtime.block_on(async {
        // Force the base node state machine into listening state so the miner will start mining
        assert!(state_event_sender
            .send(BaseNodeState::Listening(ListeningInfo {}))
            .await
            .is_ok());
        // Wait for miner to finish mining block 1
        assert!(event_stream_next(miner_utxo_stream, Duration::from_secs(20))
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
        assert!(found_tx_outputs == tx1.body.outputs().len());
        async_assert_eventually!(
            alice_node
                .mempool
                .has_tx_with_excess_sig(tx1_excess_sig.clone())
                .unwrap(),
            expect = TxStorageResponse::ReorgPool,
            max_attempts = 10,
            interval = Duration::from_secs(1),
        );

        alice_node.comms.shutdown().await;
        bob_node.comms.shutdown().await;
    });
}

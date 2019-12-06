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
    base_node::service::BaseNodeServiceConfig,
    mempool::{
        service::{MempoolServiceConfig, MempoolServiceError},
        TxStorageResponse,
    },
    test_utils::{
        builders::{add_block_and_update_header, create_genesis_block, spend_utxos},
        node::{create_network_with_2_base_nodes_with_config, create_network_with_3_base_nodes},
    },
    tx,
    txn_schema,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_p2p::tari_message::TariMessageType;
use tari_test_utils::{async_assert_eventually, random::string};
use tari_transactions::{
    proto::types as proto,
    tari_amount::{uT, T},
    types::CryptoFactories,
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[test]
fn request_response_get_stats() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let (block0, utxo) = create_genesis_block(&factories);
    add_block_and_update_header(&bob_node.blockchain_db, block0.clone());
    add_block_and_update_header(&carol_node.blockchain_db, block0);
    let (tx1, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let tx1 = Arc::new(tx1);
    bob_node.mempool.insert(tx1.clone()).unwrap();
    carol_node.mempool.insert(tx1).unwrap();
    let (orphan1, _, _) = tx!(1*T, fee: 100*uT);
    let orphan1 = Arc::new(orphan1);
    bob_node.mempool.insert(orphan1.clone()).unwrap();
    carol_node.mempool.insert(orphan1).unwrap();
    let (orphan2, _, _) = tx!(2*T, fee: 200*uT);
    let orphan2 = Arc::new(orphan2);
    bob_node.mempool.insert(orphan2.clone()).unwrap();
    carol_node.mempool.insert(orphan2).unwrap();

    runtime.block_on(async {
        let received_stats = alice_node.outbound_mp_interface.get_stats().await.unwrap();
        assert_eq!(received_stats.total_txs, 3);
        assert_eq!(received_stats.unconfirmed_txs, 1);
        assert_eq!(received_stats.orphan_txs, 2);
        assert_eq!(received_stats.timelocked_txs, 0);
        assert_eq!(received_stats.published_txs, 0);
        assert_eq!(received_stats.total_weight, 35);
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn request_response_get_tx_state_with_excess_sig() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let (block0, utxo) = create_genesis_block(&factories);
    add_block_and_update_header(&bob_node.blockchain_db, block0.clone());
    add_block_and_update_header(&carol_node.blockchain_db, block0);
    let (tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo.clone()], to: vec![2 * T, 2 * T, 2 * T]));
    let (unpublished_tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![3 * T]));
    let (orphan_tx, _, _) = tx!(1*T, fee: 100*uT);
    let tx = Arc::new(tx);
    let orphan_tx = Arc::new(orphan_tx);
    bob_node.mempool.insert(tx.clone()).unwrap();
    carol_node.mempool.insert(tx.clone()).unwrap();
    bob_node.mempool.insert(orphan_tx.clone()).unwrap();
    carol_node.mempool.insert(orphan_tx.clone()).unwrap();

    runtime.block_on(async {
        let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
        let unpublished_tx_excess_sig = unpublished_tx.body.kernels()[0].excess_sig.clone();
        let orphan_tx_excess_sig = orphan_tx.body.kernels()[0].excess_sig.clone();
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::UnconfirmedPool
        );
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(unpublished_tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::NotStored
        );
        assert_eq!(
            alice_node
                .outbound_mp_interface
                .get_tx_state_with_excess_sig(orphan_tx_excess_sig)
                .await
                .unwrap(),
            TxStorageResponse::OrphanPool
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn receive_and_propagate_transaction() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node, carol_node) =
        create_network_with_3_base_nodes(&runtime, temp_dir.path().to_str().unwrap());

    let (block0, utxo) = create_genesis_block(&factories);
    add_block_and_update_header(&alice_node.blockchain_db, block0.clone());
    add_block_and_update_header(&bob_node.blockchain_db, block0.clone());
    add_block_and_update_header(&carol_node.blockchain_db, block0);
    let (tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
    let orphan_excess_sig = orphan.body.kernels()[0].excess_sig.clone();
    assert!(alice_node.mempool.insert(Arc::new(tx.clone())).is_ok());
    assert!(alice_node.mempool.insert(Arc::new(orphan.clone())).is_ok());

    runtime.block_on(async {
        alice_node
            .outbound_message_service
            .send_direct(
                bob_node.node_identity.public_key().clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::Transaction::from(tx)),
            )
            .await
            .unwrap();
        alice_node
            .outbound_message_service
            .send_direct(
                carol_node.node_identity.public_key().clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::Transaction::from(orphan)),
            )
            .await
            .unwrap();

        async_assert_eventually!(
            bob_node.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::UnconfirmedPool,
            max_attempts = 20,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            bob_node.mempool.has_tx_with_excess_sig(&orphan_excess_sig).unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_node.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::UnconfirmedPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_node.mempool.has_tx_with_excess_sig(&orphan_excess_sig).unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
    carol_node.comms.shutdown().unwrap();
}

#[test]
fn service_request_timeout() {
    let runtime = Runtime::new().unwrap();

    let mempool_service_config = MempoolServiceConfig {
        request_timeout: Duration::from_millis(1),
    };
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 30,
    };
    let temp_dir = TempDir::new(string(8).as_str()).unwrap();
    let (mut alice_node, bob_node) = create_network_with_2_base_nodes_with_config(
        &runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        mempool_service_config,
        temp_dir.path().to_str().unwrap(),
    );

    runtime.block_on(async {
        match alice_node.outbound_mp_interface.get_stats().await {
            Err(MempoolServiceError::RequestTimedOut) => assert!(true),
            _ => assert!(false),
        }
    });

    alice_node.comms.shutdown().unwrap();
    bob_node.comms.shutdown().unwrap();
}

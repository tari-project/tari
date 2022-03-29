//  Copyright 2020, The Tari Project
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

use std::{fmt, io, iter::repeat_with, sync::Arc};

use futures::{Sink, SinkExt, Stream, StreamExt};
use tari_common::configuration::Network;
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityEventTx},
    framing,
    memsocket::MemorySocket,
    message::MessageExt,
    peer_manager::PeerFeatures,
    protocol::{ProtocolEvent, ProtocolNotification, ProtocolNotificationTx},
    test_utils::{mocks::create_peer_connection_mock_pair, node_identity::build_node_identity},
    Bytes,
    BytesMut,
};
use tari_crypto::tari_utilities::ByteArray;
use tokio::{
    sync::{broadcast, mpsc},
    task,
};

use crate::{
    consensus::ConsensusManager,
    mempool::{
        proto,
        sync_protocol::{MempoolPeerProtocol, MempoolSyncProtocol, MAX_FRAME_SIZE, MEMPOOL_SYNC_PROTOCOL},
        Mempool,
    },
    transactions::{tari_amount::uT, test_helpers::create_tx, transaction_components::Transaction},
    validation::mocks::MockValidator,
};

pub fn create_transactions(n: usize) -> Vec<Transaction> {
    repeat_with(|| {
        let (transaction, _, _) = create_tx(5000 * uT, 3 * uT, 1, 2, 1, 3, Default::default());
        transaction
    })
    .take(n)
    .collect()
}

async fn new_mempool_with_transactions(n: usize) -> (Mempool, Vec<Transaction>) {
    let mempool = Mempool::new(
        Default::default(),
        ConsensusManager::builder(Network::LocalNet).build(),
        Box::new(MockValidator::new(true)),
    );

    let transactions = create_transactions(n);
    for txn in &transactions {
        mempool.insert(Arc::new(txn.clone())).await.unwrap();
    }

    (mempool, transactions)
}

async fn setup(
    num_txns: usize,
) -> (
    ProtocolNotificationTx<MemorySocket>,
    ConnectivityEventTx,
    Mempool,
    Vec<Transaction>,
) {
    let (protocol_notif_tx, protocol_notif_rx) = mpsc::channel(1);
    let (connectivity_events_tx, connectivity_events_rx) = broadcast::channel(10);
    let (mempool, transactions) = new_mempool_with_transactions(num_txns).await;
    let protocol = MempoolSyncProtocol::new(
        Default::default(),
        protocol_notif_rx,
        connectivity_events_rx,
        mempool.clone(),
    );

    task::spawn(protocol.run());

    (protocol_notif_tx, connectivity_events_tx, mempool, transactions)
}

#[tokio::test]
async fn empty_set() {
    let (_, connectivity_events_tx, mempool1, _) = setup(0).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_events_tx
        .send(ConnectivityEvent::PeerConnected(node2_conn))
        .unwrap();

    let substream = node1_mock.next_incoming_substream().await.unwrap();
    let framed = framing::canonical(substream, MAX_FRAME_SIZE);

    let (mempool2, _) = new_mempool_with_transactions(0).await;
    MempoolPeerProtocol::new(Default::default(), framed, node2.node_id().clone(), mempool2.clone())
        .start_responder()
        .await
        .unwrap();

    let transactions = mempool2.snapshot().await.unwrap();
    assert_eq!(transactions.len(), 0);

    let transactions = mempool1.snapshot().await.unwrap();
    assert_eq!(transactions.len(), 0);
}

#[tokio::test]
async fn synchronise() {
    let (_, connectivity_events_tx, mempool1, transactions1) = setup(5).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_events_tx
        .send(ConnectivityEvent::PeerConnected(node2_conn))
        .unwrap();

    let substream = node1_mock.next_incoming_substream().await.unwrap();
    let framed = framing::canonical(substream, MAX_FRAME_SIZE);

    let (mempool2, transactions2) = new_mempool_with_transactions(3).await;
    MempoolPeerProtocol::new(Default::default(), framed, node2.node_id().clone(), mempool2.clone())
        .start_responder()
        .await
        .unwrap();

    let transactions = get_snapshot(&mempool2).await;
    assert_eq!(transactions.len(), 8);
    assert!(transactions1.iter().all(|txn| transactions.contains(txn)));
    assert!(transactions2.iter().all(|txn| transactions.contains(txn)));

    let transactions = get_snapshot(&mempool1).await;
    assert_eq!(transactions.len(), 8);
    assert!(transactions1.iter().all(|txn| transactions.contains(txn)));
    assert!(transactions2.iter().all(|txn| transactions.contains(txn)));
}

#[tokio::test]
async fn duplicate_set() {
    let (_, connectivity_events_tx, mempool1, transactions1) = setup(2).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_events_tx
        .send(ConnectivityEvent::PeerConnected(node2_conn))
        .unwrap();

    let substream = node1_mock.next_incoming_substream().await.unwrap();
    let framed = framing::canonical(substream, MAX_FRAME_SIZE);

    let (mempool2, transactions2) = new_mempool_with_transactions(1).await;
    mempool2.insert(Arc::new(transactions1[0].clone())).await.unwrap();
    MempoolPeerProtocol::new(Default::default(), framed, node2.node_id().clone(), mempool2.clone())
        .start_responder()
        .await
        .unwrap();

    let transactions = get_snapshot(&mempool2).await;
    assert_eq!(transactions.len(), 3);
    assert!(transactions1.iter().all(|txn| transactions.contains(txn)));
    assert!(transactions2.iter().all(|txn| transactions.contains(txn)));

    let transactions = get_snapshot(&mempool1).await;
    assert_eq!(transactions.len(), 3);
    assert!(transactions1.iter().all(|txn| transactions.contains(txn)));
    assert!(transactions2.iter().all(|txn| transactions.contains(txn)));
}

#[tokio::test]
async fn responder() {
    let (protocol_notif, _, _, transactions1) = setup(2).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (sock_in, sock_out) = MemorySocket::new_pair();
    protocol_notif
        .send(ProtocolNotification::new(
            MEMPOOL_SYNC_PROTOCOL.clone(),
            ProtocolEvent::NewInboundSubstream(node1.node_id().clone(), sock_in),
        ))
        .await
        .unwrap();

    let (mempool2, transactions2) = new_mempool_with_transactions(1).await;
    mempool2.insert(Arc::new(transactions1[0].clone())).await.unwrap();
    let framed = framing::canonical(sock_out, MAX_FRAME_SIZE);
    MempoolPeerProtocol::new(Default::default(), framed, node2.node_id().clone(), mempool2.clone())
        .start_initiator()
        .await
        .unwrap();

    let transactions = get_snapshot(&mempool2).await;
    assert_eq!(transactions.len(), 3);
    assert!(transactions1.iter().all(|txn| transactions.contains(txn)));
    assert!(transactions2.iter().all(|txn| transactions.contains(txn)));

    // We cannot be sure that the mempool1 contains all the transactions at this point because the initiator protocol
    // can complete before the responder has inserted the final transaction. There is currently no mechanism to know
    // this.
}

#[tokio::test]
async fn initiator_messages() {
    let (protocol_notif, _, _, transactions1) = setup(2).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (sock_in, sock_out) = MemorySocket::new_pair();
    protocol_notif
        .send(ProtocolNotification::new(
            MEMPOOL_SYNC_PROTOCOL.clone(),
            ProtocolEvent::NewInboundSubstream(node1.node_id().clone(), sock_in),
        ))
        .await
        .unwrap();

    let mut transactions = create_transactions(2);
    transactions.push(transactions1[0].clone());
    let mut framed = framing::canonical(sock_out, MAX_FRAME_SIZE);
    // As the initiator, send an inventory
    let inventory = proto::TransactionInventory {
        items: transactions
            .iter()
            .map(|tx| tx.first_kernel_excess_sig().unwrap().get_signature().to_vec())
            .collect(),
    };
    write_message(&mut framed, inventory).await;
    // Expect 1 transaction, a "stop message" and indexes for missing transactions
    let transaction: proto::TransactionItem = read_message(&mut framed).await;
    assert!(transaction.transaction.is_some());
    let stop: proto::TransactionItem = read_message(&mut framed).await;
    assert!(stop.transaction.is_none());
    let indexes: proto::InventoryIndexes = read_message(&mut framed).await;
    assert_eq!(indexes.indexes, [0, 1]);
}

#[tokio::test]
async fn responder_messages() {
    let (_, connectivity_events_tx, _, transactions1) = setup(1).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_events_tx
        .send(ConnectivityEvent::PeerConnected(node2_conn))
        .unwrap();

    let substream = node1_mock.next_incoming_substream().await.unwrap();
    let mut framed = framing::canonical(substream, MAX_FRAME_SIZE);

    // Expect an inventory
    let inventory: proto::TransactionInventory = read_message(&mut framed).await;
    assert_eq!(inventory.items.len(), 1);
    // Send no transactions back
    let nothing = proto::TransactionItem::empty();
    write_message(&mut framed, nothing).await;
    // Send transaction indexes back
    let indexes = proto::InventoryIndexes { indexes: vec![0] };
    write_message(&mut framed, indexes).await;
    // Expect a single transaction back and a stop message
    let transaction: proto::TransactionItem = read_message(&mut framed).await;
    assert_eq!(
        transaction
            .transaction
            .unwrap()
            .body
            .unwrap()
            .kernels
            .remove(0)
            .excess_sig
            .unwrap()
            .signature,
        transactions1[0]
            .first_kernel_excess_sig()
            .unwrap()
            .get_signature()
            .to_vec()
    );
    let stop: proto::TransactionItem = read_message(&mut framed).await;
    assert!(stop.transaction.is_none());
    // Except stream to end
    assert!(framed.next().await.is_none());
}

async fn get_snapshot(mempool: &Mempool) -> Vec<Transaction> {
    mempool
        .snapshot()
        .await
        .unwrap()
        .iter()
        .map(|t| &**t)
        .cloned()
        .collect()
}

async fn read_message<S, T>(reader: &mut S) -> T
where
    S: Stream<Item = io::Result<BytesMut>> + Unpin,
    T: prost::Message + Default,
{
    let msg = reader.next().await.unwrap().unwrap();
    T::decode(&mut msg.freeze()).unwrap()
}

async fn write_message<S, T>(writer: &mut S, message: T)
where
    S: Sink<Bytes> + Unpin,
    S::Error: fmt::Debug,
    T: prost::Message,
{
    writer.send(message.to_encoded_bytes().into()).await.unwrap();
}

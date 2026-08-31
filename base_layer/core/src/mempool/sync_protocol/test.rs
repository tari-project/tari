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

// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::indexing_slicing)]
use std::{
    fmt,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures::{Sink, SinkExt, Stream, StreamExt};
use tari_common::configuration::Network;
use tari_comms::{
    Bytes,
    BytesMut,
    connectivity::ConnectivityEvent,
    framing,
    framing::CanonicalFraming,
    memsocket::MemorySocket,
    message::MessageExt,
    peer_manager::PeerFeatures,
    protocol::{ProtocolEvent, ProtocolNotification, ProtocolNotificationTx},
    test_utils::{
        mocks::{ConnectivityManagerMockState, create_connectivity_mock, create_peer_connection_mock_pair},
        node_identity::build_node_identity,
    },
};
use tari_shutdown::Shutdown;
use tari_transaction_components::{
    key_manager::KeyManager,
    tari_amount::uT,
    test_helpers::create_tx,
    transaction_components::Transaction,
};
use tari_utilities::ByteArray;
use tokio::{
    sync::{broadcast, mpsc},
    task,
};

use crate::{
    consensus::BaseNodeConsensusManager,
    mempool::{
        Mempool,
        MempoolServiceConfig,
        proto,
        sync_protocol::{
            MAX_FRAME_SIZE,
            MEMPOOL_SYNC_PROTOCOL,
            MempoolPeerProtocol,
            MempoolProtocolError,
            MempoolSyncProtocol,
        },
    },
    proto as shared_proto,
    validation::mocks::MockValidator,
};

pub fn create_transactions(n: usize) -> Vec<Transaction> {
    let key_manager = KeyManager::new_random().unwrap();
    let mut transactions = Vec::new();
    for _i in 0..n {
        let (transaction, _, _) = create_tx(5000 * uT, 3 * uT, 1, 2, 1, 3, Default::default(), &key_manager)
            .expect("Failed to get transaction");
        transactions.push(transaction);
    }
    transactions
}

async fn new_mempool_with_transactions(n: usize) -> (Mempool, Vec<Transaction>) {
    let mempool = Mempool::new(
        Default::default(),
        BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap(),
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
    ConnectivityManagerMockState,
    Mempool,
    Vec<Transaction>,
    // Returned so the caller keeps it alive: dropping it shuts the protocol down.
    Shutdown,
) {
    let (protocol_notif_tx, protocol_notif_rx) = mpsc::channel(1);
    let (mempool, transactions) = new_mempool_with_transactions(num_txns).await;
    let (connectivity, connectivity_manager_mock) = create_connectivity_mock();
    let connectivity_manager_mock_state = connectivity_manager_mock.spawn();
    let (block_event_sender, _) = broadcast::channel(1);
    let block_receiver = block_event_sender.subscribe();
    let shutdown = Shutdown::new();
    let protocol = MempoolSyncProtocol::new(
        Default::default(),
        protocol_notif_rx,
        mempool.clone(),
        connectivity,
        block_receiver,
        shutdown.to_signal(),
    );

    task::spawn(protocol.run());
    connectivity_manager_mock_state.wait_until_event_receivers_ready().await;
    (
        protocol_notif_tx,
        connectivity_manager_mock_state,
        mempool,
        transactions,
        shutdown,
    )
}

#[tokio::test]
async fn empty_set() {
    let (_, connectivity_manager_state, mempool1, _, _shutdown) = setup(0).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_manager_state.publish_event(ConnectivityEvent::PeerConnected(node2_conn.into()));

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
    let (_, connectivity_manager_state, mempool1, transactions1, _shutdown) = setup(5).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_manager_state.publish_event(ConnectivityEvent::PeerConnected(node2_conn.into()));

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
    let (_, connectivity_manager_state, mempool1, transactions1, _shutdown) = setup(2).await;
    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_manager_state.publish_event(ConnectivityEvent::PeerConnected(node2_conn.into()));

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
    let (protocol_notif, _, _, transactions1, _shutdown) = setup(2).await;

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
    let (protocol_notif, _, _, transactions1, _shutdown) = setup(2).await;

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
    let (_, connectivity_manager_state, _, transactions1, _shutdown) = setup(1).await;

    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    // This node connected to a peer, so it should open the substream
    connectivity_manager_state.publish_event(ConnectivityEvent::PeerConnected(node2_conn.into()));

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

/// A peer that takes our inventory and then goes silent — it neither sends the terminator nor
/// closes the substream. This used to park the initiator forever on an unbounded `framed.next()`,
/// and because each initiator task holds a `Mempool` clone (and through its validator, a handle on
/// the blockchain database) a single such peer pinned the node's LMDB file lock for the life of the
/// process. In the cucumber suite that stopped a restarted node from ever opening its database.
///
/// Time is paused, so the inter-message deadline elapses instantly in test time. The outer deadline
/// is a watchdog: it is strictly longer than the one under test, so if the read ever becomes
/// unbounded again this fails with a clear message instead of hanging.
#[tokio::test(start_paused = true)]
async fn initiator_gives_up_when_a_peer_stalls_mid_transaction_stream() {
    let (mempool, _transactions) = new_mempool_with_transactions(1).await;
    let peer_node = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (sock_in, sock_out) = MemorySocket::new_pair();
    // Held for the whole test: dropping it would close the substream, which is the *other* way out
    // of the read loop and not what we are testing.
    let mut peer = framing::canonical(sock_in, MAX_FRAME_SIZE);

    let framed = framing::canonical(sock_out, MAX_FRAME_SIZE);
    let initiator = task::spawn(async move {
        MempoolPeerProtocol::new(Default::default(), framed, peer_node.node_id().clone(), mempool)
            .start_initiator()
            .await
    });

    // Take the inventory the initiator sends, then answer nothing at all.
    let _inventory: proto::TransactionInventory = read_message(&mut peer).await;

    let result = tokio::time::timeout(Duration::from_secs(600), initiator)
        .await
        .expect("initiator never returned — the transaction stream read is unbounded again")
        .unwrap();

    assert!(
        matches!(result, Err(MempoolProtocolError::RecvTimeout)),
        "expected the inter-message deadline to fire, got {result:?}"
    );
}

/// Wraps a substream so that closing it never completes, modelling a peer that has stopped reading
/// but has not gone away. Reads, writes and flushes pass straight through.
struct StallingClose<T> {
    inner: T,
}

impl<T: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for StallingClose<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for StallingClose<T> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

/// A peer that keeps streaming transactions past the limit both sides agreed on must be cut off.
///
/// The per-frame deadline cannot do this on its own: it restarts for every frame, so a peer that
/// trickles items — even slowly, as here — holds the exchange, and with it the single initiator
/// permit, open indefinitely. The sender already caps what it will send; this asserts the receiver
/// enforces the same cap. Time is paused, so the 100s between items costs nothing to run.
#[tokio::test(start_paused = true)]
async fn initiator_cuts_off_a_peer_that_streams_past_the_agreed_limit() {
    let (mempool, _transactions) = new_mempool_with_transactions(0).await;
    let peer_node = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (sock_in, sock_out) = MemorySocket::new_pair();
    let mut peer = framing::canonical(sock_in, MAX_FRAME_SIZE);

    let config = MempoolServiceConfig {
        initial_sync_max_transactions: 2,
        ..Default::default()
    };
    let framed = framing::canonical(sock_out, MAX_FRAME_SIZE);
    let initiator = task::spawn(async move {
        MempoolPeerProtocol::new(config, framed, peer_node.node_id().clone(), mempool)
            .start_initiator()
            .await
    });

    let _inventory: proto::TransactionInventory = read_message(&mut peer).await;

    // Trickle transactions, never sending the terminator. The gap is under the per-frame deadline,
    // so only the item cap can stop this.
    let transactions = create_transactions(3);
    for txn in &transactions {
        tokio::time::sleep(Duration::from_secs(100)).await;
        let item = proto::TransactionItem {
            transaction: Some(txn.clone().try_into().unwrap()),
        };
        write_message(&mut peer, item).await;
    }

    let result = tokio::time::timeout(Duration::from_secs(3600), initiator)
        .await
        .expect("initiator never returned — a trickling peer is unbounded again")
        .unwrap();

    assert!(
        matches!(result, Err(MempoolProtocolError::TooManyTransactions { max: 2, .. })),
        "expected the item cap to fire, got {result:?}"
    );
}

/// A sync that exchanged everything successfully must still count as a success when only the
/// trailing `close()` fails. Reporting it as an error would skip `num_synched`, leaving the node
/// spawning initiators forever against a peer it has in fact already synced with.
#[tokio::test(start_paused = true)]
async fn successful_sync_still_succeeds_when_the_trailing_close_stalls() {
    let (protocol_notif, _, _, transactions1, _shutdown) = setup(2).await;

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

    let (mempool2, _transactions2) = new_mempool_with_transactions(1).await;
    mempool2.insert(Arc::new(transactions1[0].clone())).await.unwrap();

    // Everything is exchanged normally; only the final close never completes.
    let framed = framing::canonical(StallingClose { inner: sock_out }, MAX_FRAME_SIZE);
    let result = tokio::time::timeout(
        Duration::from_secs(3600),
        MempoolPeerProtocol::new(Default::default(), framed, node2.node_id().clone(), mempool2).start_initiator(),
    )
    .await
    .expect("initiator never returned — the trailing close is unbounded again");

    assert!(
        result.is_ok(),
        "a completed exchange must not be demoted to a failure by a stalled close, got {result:?}"
    );
}

/// `run()` must actually return when the shutdown signal fires, because everything that makes
/// shutdown deterministic — aborting the peer protocol tasks and waiting for them — happens after
/// the loop breaks. If the future were cut short instead of returning, that code would never run.
#[tokio::test]
async fn run_returns_when_shutdown_is_triggered() {
    let (protocol_notif_tx, protocol_notif_rx) = mpsc::channel(1);
    let (mempool, _transactions) = new_mempool_with_transactions(0).await;
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_state = connectivity_mock.spawn();
    let (block_event_sender, _) = broadcast::channel(1);
    let block_receiver = block_event_sender.subscribe();

    let mut shutdown = Shutdown::new();
    let protocol = MempoolSyncProtocol::<MemorySocket>::new(
        Default::default(),
        protocol_notif_rx,
        mempool,
        connectivity,
        block_receiver,
        shutdown.to_signal(),
    );
    let running = task::spawn(protocol.run());
    connectivity_state.wait_until_event_receivers_ready().await;

    shutdown.trigger();

    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .expect("run() did not return after shutdown was triggered")
        .unwrap();

    drop(protocol_notif_tx);
}

/// Encodes a transaction as a stream item, as a peer would send it.
fn transaction_item(txn: &Transaction) -> proto::TransactionItem {
    proto::TransactionItem {
        transaction: Some(shared_proto::types::Transaction::try_from(Arc::new(txn.clone())).unwrap()),
    }
}

/// Drives the initiator side of an exchange until the protocol's responder task is parked waiting
/// for transactions from us, and hands back our end of the substream. Announcing a transaction the
/// responder does not have is what makes it ask, and therefore wait.
async fn park_responder_awaiting_transactions(
    protocol_notif: &ProtocolNotificationTx<MemorySocket>,
    peer: &tari_comms::NodeIdentity,
) -> CanonicalFraming<MemorySocket> {
    let (sock_in, sock_out) = MemorySocket::new_pair();
    protocol_notif
        .send(ProtocolNotification::new(
            MEMPOOL_SYNC_PROTOCOL.clone(),
            ProtocolEvent::NewInboundSubstream(peer.node_id().clone(), sock_in),
        ))
        .await
        .unwrap();
    let mut framed = framing::canonical(sock_out, MAX_FRAME_SIZE);

    let unknown = create_transactions(1);
    let inventory = proto::TransactionInventory {
        items: unknown
            .iter()
            .map(|tx| tx.first_kernel_excess_sig().unwrap().get_signature().to_vec())
            .collect(),
    };
    write_message(&mut framed, inventory).await;

    // Drain the transactions it sends us, up to and including the terminator, then its request for
    // the transaction it is missing. After this it is inside its read loop.
    loop {
        let item: proto::TransactionItem = read_message(&mut framed).await;
        if item.transaction.is_none() {
            break;
        }
    }
    let indexes: proto::InventoryIndexes = read_message(&mut framed).await;
    assert_eq!(indexes.indexes, vec![0]);
    framed
}

/// Trickle items just inside the per-frame deadline until our end of the substream is closed,
/// then assert it was closed rather than still hanging open. Returns how long we kept it up.
async fn trickle_until_abandoned<T>(framed: &mut CanonicalFraming<T>, txn: &Transaction)
where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin {
    let item = transaction_item(txn);
    // Keep feeding items *forever*, each gap just inside STREAM_ITEM_TIMEOUT (120s) and the total
    // count far below the item cap (10_000). That is the whole point: neither of those bounds can
    // end this exchange, so if it ever ends it can only be the aggregate deadline.
    //
    // Never stop trickling of our own accord. An earlier version sent a fixed four items and then
    // waited for EOF, which passed even with PROTOCOL_TIMEOUT raised to 24h — once we fell silent,
    // the per-frame deadline closed the substream and the test read that as success. Ending the
    // loop ourselves is exactly what makes this test vacuous.
    //
    // PROTOCOL_TIMEOUT is 300s, so ~3 items should do it; the generous cap only bounds the failure
    // case, and 50 x 119s (~1.6h of virtual time) is far past any plausible aggregate bound.
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_secs(119)).await;
        if framed.send(item.to_encoded_bytes().into()).await.is_err() {
            // The peer task was dropped and took its end of the substream with it.
            return;
        }
    }
    panic!("kept trickling for 50 frames without being cut off — the aggregate deadline did not fire");
}

/// A responder that keeps receiving frames inside the per-frame deadline must still be abandoned
/// once the whole exchange exceeds `PROTOCOL_TIMEOUT`. Without an aggregate bound a peer can hold a
/// `Mempool` clone — and through it the blockchain database — for as long as it likes simply by
/// trickling.
#[tokio::test]
async fn responder_abandons_a_peer_that_exceeds_the_aggregate_deadline() {
    let (protocol_notif, _, _, _transactions, _shutdown) = setup(1).await;
    let peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut framed = park_responder_awaiting_transactions(&protocol_notif, &peer).await;

    // Set up over real time so socket round-trips cannot race the auto-advancing clock; only the
    // trickle below needs the clock moved.
    tokio::time::pause();
    let txn = create_transactions(1).remove(0);
    trickle_until_abandoned(&mut framed, &txn).await;
}

/// The same guarantee on the initiator side, which is the one that matters most: initiators are
/// serialised behind a single permit, so an unbounded exchange wedges mempool sync node-wide. This
/// also covers opening the substream, which is inside the deadline.
#[tokio::test]
async fn initiator_abandons_a_peer_that_exceeds_the_aggregate_deadline() {
    let (_, connectivity_manager_state, _, _transactions, _shutdown) = setup(1).await;
    let node1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (_node1_conn, node1_mock, node2_conn, _) =
        create_peer_connection_mock_pair(node1.to_peer(), node2.to_peer()).await;

    connectivity_manager_state.publish_event(ConnectivityEvent::PeerConnected(node2_conn.into()));

    let substream = node1_mock.next_incoming_substream().await.unwrap();
    let mut framed = framing::canonical(substream, MAX_FRAME_SIZE);
    // The initiator opens with its inventory and then waits for our transactions.
    let _inventory: proto::TransactionInventory = read_message(&mut framed).await;

    tokio::time::pause();
    let txn = create_transactions(1).remove(0);
    trickle_until_abandoned(&mut framed, &txn).await;
}

/// The M4 guarantee itself: by the time `run()` returns, the peer protocol tasks must be gone, not
/// merely told to stop. `JoinSet::drop` aborts without awaiting, so a peer task — and the `Mempool`
/// clone it holds — could otherwise outlive the service reporting itself shut down. If the task has
/// really been dropped, its end of the substream is closed and ours reads EOF.
#[tokio::test]
async fn peer_tasks_are_gone_by_the_time_run_returns() {
    let (protocol_notif_tx, protocol_notif_rx) = mpsc::channel(1);
    let (mempool, _transactions) = new_mempool_with_transactions(1).await;
    let (connectivity, connectivity_mock) = create_connectivity_mock();
    let connectivity_state = connectivity_mock.spawn();
    let (block_event_sender, _) = broadcast::channel(1);
    let block_receiver = block_event_sender.subscribe();

    let mut shutdown = Shutdown::new();
    let protocol = MempoolSyncProtocol::new(
        Default::default(),
        protocol_notif_rx,
        mempool,
        connectivity,
        block_receiver,
        shutdown.to_signal(),
    );
    let running = task::spawn(protocol.run());
    connectivity_state.wait_until_event_receivers_ready().await;

    let peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut framed = park_responder_awaiting_transactions(&protocol_notif_tx, &peer).await;

    shutdown.trigger();
    tokio::time::timeout(Duration::from_secs(30), running)
        .await
        .expect("run() did not return after shutdown was triggered")
        .unwrap();

    // `run()` has returned. The peer task must already be gone, so this reads EOF rather than
    // blocking on a substream something is still holding.
    let next = tokio::time::timeout(Duration::from_secs(5), framed.next())
        .await
        .expect("peer task still held the substream after run() returned");
    assert!(
        next.is_none(),
        "expected EOF once the peer task was joined, got {next:?}"
    );
}

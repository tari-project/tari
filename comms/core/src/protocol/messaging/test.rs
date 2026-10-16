// Copyright 2020, The Tari Project
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

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use futures::{SinkExt, StreamExt, stream::FuturesUnordered};
use tari_common_sqlite::connection::DbConnection;
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_stream, unpack_enum};
use tokio::{
    io::AsyncReadExt,
    sync::{broadcast, mpsc, oneshot, oneshot::error::TryRecvError},
    time,
    time::Instant,
};

use super::{
    extension::INBOUND_MESSAGE_BUFFER_SIZE,
    inbound::{INBOUND_PER_SESSION_INFLIGHT, STOPPING_SESSION_PERMIT_WAIT},
    outbound::{MAX_SEND_RETRIES, OUTBOUND_SEND_TIMEOUT, OutboundMessaging, QueueAccounting, QueuedOutboundMessage},
    protocol::{
        MessagingEventReceiver,
        MessagingProtocol,
        OUTBOUND_DIALING_MAX_BYTES,
        OUTBOUND_DIALING_QUEUE_MAX_BYTES,
        OUTBOUND_MESSAGE_TTL,
        OUTBOUND_QUEUE_CAPACITY,
        OUTBOUND_QUEUE_MAX_BYTES,
        OUTBOUND_TOTAL_MAX_BYTES,
    },
};
use crate::{
    Minimized,
    message::{InboundMessage, MessageTag, MessagingReplyRx, OutboundMessage},
    multiplexing::Substream,
    net_address::MultiaddressesWithStats,
    peer_manager::{
        NodeId,
        NodeIdentity,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerManager,
        create_test_peer,
        database::{MIGRATIONS, PeerDatabaseSql},
    },
    protocol::{
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        messaging::{MessagingEvent, SendFailReason},
    },
    test_utils::{
        mocks::{ConnectivityManagerMockState, create_connectivity_mock, create_peer_connection_mock_pair},
        node_id,
        node_identity::build_node_identity,
    },
    types::{CommsPublicKey, TransportProtocol},
};

static TEST_MSG1: Bytes = Bytes::from_static(b"TEST_MSG1");
static TEST_MSG2: Bytes = Bytes::from_static(b"TEST_MSG2");

static MESSAGING_PROTOCOL_ID: ProtocolId = ProtocolId::from_static(b"test/msg");

fn create_peer_manager() -> Arc<PeerManager> {
    let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
    let peers_db = PeerDatabaseSql::new(
        db_connection,
        &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
    )
    .unwrap();
    Arc::new(PeerManager::new(peers_db, TransportProtocol::get_all()).unwrap())
}

async fn spawn_messaging_protocol() -> (
    Arc<PeerManager>,
    Arc<NodeIdentity>,
    ConnectivityManagerMockState,
    mpsc::Sender<ProtocolNotification<Substream>>,
    mpsc::UnboundedSender<OutboundMessage>,
    mpsc::Receiver<InboundMessage>,
    MessagingEventReceiver,
    Shutdown,
) {
    let shutdown = Shutdown::new();

    let (requester, mock) = create_connectivity_mock();
    let mock_state = mock.get_shared_state();
    mock.spawn();

    let peer_manager = create_peer_manager();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, inbound_msg_rx) = mpsc::channel(100);
    let (events_tx, events_rx) = broadcast::channel(100);

    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    )
    .set_message_received_event_enabled(true);
    tokio::spawn(msg_proto.run());

    (
        peer_manager,
        node_identity,
        mock_state,
        proto_tx,
        request_tx,
        inbound_msg_rx,
        events_rx,
        shutdown,
    )
}

#[tokio::test]
async fn new_inbound_substream_handling() {
    let (peer_manager, _, conn_man_mock, proto_tx, outbound_msg_tx, mut inbound_msg_rx, mut events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer1 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer1.clone()).await.unwrap();

    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );

    let (_, conn1_state, conn2, _conn2_state) = create_peer_connection_mock_pair(peer1.clone(), peer2.clone()).await;

    conn_man_mock.add_active_connection(conn2).await;

    let (reply_tx, _reply_rx) = oneshot::channel();
    let out_msg = OutboundMessage {
        tag: MessageTag::new(),
        reply: reply_tx.into(),
        peer_node_id: peer1.node_id.clone(),
        body: TEST_MSG1.clone(),
    };
    outbound_msg_tx.send(out_msg).unwrap();

    let stream_theirs = conn1_state.next_incoming_substream().await.unwrap();
    proto_tx
        .send(ProtocolNotification::new(
            MESSAGING_PROTOCOL_ID.clone(),
            ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
        ))
        .await
        .unwrap();

    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(in_msg.source_peer, expected_node_id);
    assert_eq!(in_msg.body, TEST_MSG1);

    let expected_tag = in_msg.tag;
    let event = time::timeout(Duration::from_secs(5), events_rx.recv())
        .await
        .unwrap()
        .unwrap();
    unpack_enum!(MessagingEvent::MessageReceived(node_id, tag) = &event);
    assert_eq!(tag, &expected_tag);
    assert_eq!(*node_id, expected_node_id);
}

/// Regression test: a `NewInboundSubstream` notification can arrive and be processed before the
/// ConnectivityManager's pool has registered the connection it belongs to - this is the normal shape of a
/// simultaneous-dial tie break, where the winning connection's substream is negotiated before the
/// ConnectivityManager actor has processed the `PeerConnected`/tie-break event for the same peer. Previously
/// `handle_protocol_notification` looked the connection up exactly once and silently dropped the substream
/// (and every message on it) if the pool had not caught up yet. It must now wait for the pool to converge
/// rather than giving up on the first miss.
#[tokio::test]
async fn new_inbound_substream_survives_connectivity_pool_lag() {
    let (peer_manager, _, conn_man_mock, proto_tx, outbound_msg_tx, mut inbound_msg_rx, mut events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer1 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer1.clone()).await.unwrap();

    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );

    let (_, conn1_state, conn2, _conn2_state) = create_peer_connection_mock_pair(peer1.clone(), peer2.clone()).await;

    // Register the connection so `OutboundMessaging` can dial and open the substream exactly as it does in
    // `new_inbound_substream_handling`.
    conn_man_mock.add_active_connection(conn2.clone()).await;

    let (reply_tx, _reply_rx) = oneshot::channel();
    let out_msg = OutboundMessage {
        tag: MessageTag::new(),
        reply: reply_tx.into(),
        peer_node_id: peer1.node_id.clone(),
        body: TEST_MSG1.clone(),
    };
    outbound_msg_tx.send(out_msg).unwrap();

    let stream_theirs = conn1_state.next_incoming_substream().await.unwrap();

    // Now that the substream exists, withdraw the connection again - simulating the connectivity pool having
    // briefly lost track of a connection that is still perfectly alive on the wire, as happens when a
    // simultaneous-dial tie break resolves a beat behind the substream negotiation for the connection it kept.
    conn_man_mock.remove_active_connection(&expected_node_id).await;

    // Publish the substream notification BEFORE the connection pool knows about the connection again - this is
    // the race. On the buggy code path `handle_protocol_notification` would look the connection up exactly
    // once, find nothing, log "No active connection..." and drop the substream (and TEST_MSG1 with it) for
    // good.
    proto_tx
        .send(ProtocolNotification::new(
            MESSAGING_PROTOCOL_ID.clone(),
            ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
        ))
        .await
        .unwrap();

    // Only now - after the notification has had a chance to be picked up and (on the buggy path) dropped -
    // does the connectivity pool converge, exactly like a tie break resolving a beat behind the substream
    // negotiation it belongs to.
    time::sleep(Duration::from_millis(100)).await;
    conn_man_mock.add_active_connection(conn2).await;

    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .expect("connection pool lag permanently dropped the inbound substream")
        .unwrap();
    assert_eq!(in_msg.source_peer, expected_node_id);
    assert_eq!(in_msg.body, TEST_MSG1);

    let expected_tag = in_msg.tag;
    let event = time::timeout(Duration::from_secs(5), events_rx.recv())
        .await
        .unwrap()
        .unwrap();
    unpack_enum!(MessagingEvent::MessageReceived(node_id, tag) = &event);
    assert_eq!(tag, &expected_tag);
    assert_eq!(*node_id, expected_node_id);
}

#[tokio::test]
async fn send_message_request() {
    let (_, node_identity, conn_man_mock, _, request_tx, _, _, _shutdown) = spawn_messaging_protocol().await;

    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (conn1, peer_conn_mock1, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;

    // Add mock peer connection to connection manager mock for node 2
    conn_man_mock.add_active_connection(conn1).await;

    // Send a message to node
    let out_msg = OutboundMessage::new(peer_node_identity.node_id().clone(), TEST_MSG1.clone());
    request_tx.send(out_msg).unwrap();

    // Check that node got the message
    let stream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let mut framed = MessagingProtocol::framed(stream);
    let msg = framed.next().await.unwrap().unwrap();
    assert_eq!(msg, TEST_MSG1);

    // Got the call to create a substream
    assert_eq!(peer_conn_mock1.call_count(), 1);
}

#[tokio::test]
async fn send_message_dial_failed() {
    let (_, _, conn_manager_mock, _, request_tx, _, mut event_tx, _shutdown) = spawn_messaging_protocol().await;

    let node_id = node_id::random();
    let (reply_tx, reply_rx) = oneshot::channel();
    let out_msg = OutboundMessage::with_reply(node_id, TEST_MSG1.clone(), reply_tx.into());
    // Send a message to node 2
    request_tx.send(out_msg).unwrap();

    let event = event_tx.recv().await.unwrap();
    unpack_enum!(MessagingEvent::OutboundProtocolExited(_node_id) = &event);
    let reply = reply_rx.await.unwrap().unwrap_err();
    unpack_enum!(SendFailReason::PeerDialFailed = reply);

    let calls = conn_manager_mock.take_calls().await;
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|evt| evt.starts_with("DialPeer")));
}

#[tokio::test]
async fn send_message_substream_bulk_failure() {
    const NUM_MSGS: usize = 10;
    let (_, node_identity, conn_manager_mock, _, mut request_tx, _, mut events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;

    let peer_node_id = peer_node_identity.node_id();
    // Add mock peer connection to connection manager mock for node 2
    conn_manager_mock.add_active_connection(conn1).await;

    async fn send_msg(
        request_tx: &mut mpsc::UnboundedSender<OutboundMessage>,
        node_id: NodeId,
    ) -> (MessageTag, MessagingReplyRx) {
        let (reply_tx, reply_rx) = oneshot::channel();
        let out_msg = OutboundMessage::with_reply(node_id, TEST_MSG1.clone(), reply_tx.into());
        let msg_tag = out_msg.tag;
        // Send a message to node 2
        request_tx.send(out_msg).unwrap();
        (msg_tag, reply_rx)
    }

    let mut expected_out_msg_tags = Vec::with_capacity(NUM_MSGS);
    expected_out_msg_tags.push(send_msg(&mut request_tx, peer_node_id.clone()).await);

    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    // Close destination peer's channel before queuing the message to send
    peer_conn_mock2.disconnect().await.unwrap();
    drop(peer_conn_mock2);

    for _ in 0..NUM_MSGS - 1 {
        expected_out_msg_tags.push(send_msg(&mut request_tx, peer_node_id.clone()).await);
    }

    // Expect some messages to fail sending because the sender suddenly disconnected and could not be redialled.
    // Others may pass due to the race between detecting disconnection and sending
    let mut num_sent = 0usize;
    let mut num_failed = 0usize;
    for (_, reply) in expected_out_msg_tags {
        match reply.await.unwrap() {
            Ok(_) => {
                num_sent += 1;
            },
            Err(SendFailReason::PeerDialFailed) => {
                num_failed += 1;
            },
            Err(err) => unreachable!("Unexpected error {}", err),
        }
    }

    assert!(num_failed > 0);
    assert_eq!(num_sent + num_failed, NUM_MSGS);

    // Check that the outbound handler closed
    let event = time::timeout(Duration::from_secs(10), events_rx.recv())
        .await
        .unwrap()
        .unwrap();
    unpack_enum!(MessagingEvent::OutboundProtocolExited(node_id) = &event);
    assert_eq!(node_id, peer_node_id);
}

#[tokio::test]
async fn many_concurrent_send_message_requests() {
    const NUM_MSGS: usize = 100;
    let (_, _, conn_man_mock, _, request_tx, _, _, _shutdown) = spawn_messaging_protocol().await;

    let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let (conn1, peer_conn_mock1, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity1.to_peer(), node_identity2.to_peer()).await;

    let node_id2 = node_identity2.node_id();
    // Add mock peer connection to connection manager mock for node 2
    conn_man_mock.add_active_connection(conn1).await;

    // Send many messages to node
    let mut msg_tags = Vec::with_capacity(NUM_MSGS);
    let mut reply_rxs = Vec::with_capacity(NUM_MSGS);
    for _ in 0..NUM_MSGS {
        let (reply_tx, reply_rx) = oneshot::channel();
        let out_msg = OutboundMessage {
            tag: MessageTag::new(),
            reply: reply_tx.into(),
            peer_node_id: node_id2.clone(),
            body: TEST_MSG1.clone(),
        };
        msg_tags.push(out_msg.tag);
        reply_rxs.push(reply_rx);
        request_tx.send(out_msg).unwrap();
    }

    // Check that the node got the messages
    let stream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let mut framed = MessagingProtocol::framed(stream);
    let messages = collect_stream!(framed, take = NUM_MSGS, timeout = Duration::from_secs(10));
    assert_eq!(messages.len(), NUM_MSGS);

    let unordered = reply_rxs.into_iter().collect::<FuturesUnordered<_>>();
    let results = unordered.collect::<Vec<_>>().await;
    assert_eq!(
        results.into_iter().map(Result::unwrap).filter(Result::is_err).count(),
        0
    );

    // Got a single call to create a substream
    assert_eq!(peer_conn_mock1.call_count(), 1);
}

#[tokio::test]
async fn many_concurrent_send_message_requests_that_fail() {
    const NUM_MSGS: usize = 100;
    let (_, _, _, _, request_tx, _, _, _shutdown) = spawn_messaging_protocol().await;

    let node_id2 = node_id::random();

    // Send many messages to node
    let mut msg_tags = Vec::with_capacity(NUM_MSGS);
    let mut reply_rxs = Vec::with_capacity(NUM_MSGS);
    for _ in 0..NUM_MSGS {
        let (reply_tx, reply_rx) = oneshot::channel();
        let out_msg = OutboundMessage {
            tag: MessageTag::new(),
            reply: reply_tx.into(),
            peer_node_id: node_id2.clone(),
            body: TEST_MSG1.clone(),
        };
        msg_tags.push(out_msg.tag);
        reply_rxs.push(reply_rx);
        request_tx.send(out_msg).unwrap();
    }

    let unordered = reply_rxs.into_iter().collect::<FuturesUnordered<_>>();
    let results = unordered.collect::<Vec<_>>().await;
    assert!(results.into_iter().map(|r| r.unwrap()).all(|r| r.is_err()));
}

/// Regression test: a peer opening a second inbound substream while an existing session for it is still
/// running must not be rejected. `OutboundMessaging` opens a fresh substream every time it (re)establishes,
/// including when it is retrying after its previous attempt appeared to fail - most commonly simultaneous-dial
/// tie-breaking, which can cycle a connection (and with it, substreams) several times in quick succession.
/// Rejecting the new substream outright used to turn that into a tight loop: the peer's fresh substream would
/// be closed immediately, it would retry, get rejected again, and so on, starving real traffic of a working
/// session for as long as the old (stale but not-yet-finished) session lingered. The newest substream must
/// always win.
#[tokio::test]
async fn new_inbound_substream_replaces_stale_session_on_same_connection() {
    let (peer_manager, node_identity_1, conn_man_mock, proto_tx, _, mut inbound_msg_rx, _, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let peer1 = node_identity_1.to_peer();

    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer2.clone()).await.unwrap();

    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(peer1.clone(), peer2.clone()).await;

    conn_man_mock.add_active_connection(conn1).await;

    // Create connected memory sockets - we use each end of the connection as if they exist on different nodes
    // let (_, muxer_ours, mut muxer_theirs) = transport::build_multiplexed_connections().await;
    // Spawn a task to deal with incoming substreams
    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // Open first stream
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    framed_ours.send(TEST_MSG1.clone()).await.unwrap();

    // Message comes through
    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(in_msg.source_peer, expected_node_id);
    assert_eq!(in_msg.body, TEST_MSG1);

    // Open a second stream on the same connection, exactly as `OutboundMessaging` does when retrying. It must
    // be accepted, not rejected.
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();

    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .expect("the newer substream must replace the stale session, not be rejected by it")
        .unwrap();
    assert_eq!(in_msg.source_peer, expected_node_id);
    assert_eq!(in_msg.body, TEST_MSG2);

    // The first stream's session was replaced and must eventually close - sending on it will fail.
    loop {
        if let Err(e) = framed_ours.send(TEST_MSG1.clone()).await {
            assert_eq!(
                e.to_string().split(':').nth(1).map(|s| s.trim()),
                Some("connection is closed"),
                "Expected connection to be closed but got '{e}'"
            );
            break;
        }
    }

    // The second (now the live) stream keeps working.
    framed_ours2.send(TEST_MSG1.clone()).await.unwrap();
    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(in_msg.source_peer, expected_node_id);
    assert_eq!(in_msg.body, TEST_MSG1);
}

/// Regression test for the actor-wide stall this fix removes: resolving the connection for an inbound
/// substream that never becomes connected (banned peer, tie-break loser that never reconnects, spoofed
/// NodeId, ...) must not block the `MessagingProtocol` actor's `select!` loop. Before this fix,
/// `wait_for_connection` was awaited directly from `handle_protocol_notification`, parking the whole actor -
/// and with it every peer's outbound messages, retries, and other substreams - for up to
/// `CONNECTION_LOOKUP_TIMEOUT` (2s) per such notification, with no cap on how many could queue back to back.
#[tokio::test]
async fn actor_keeps_servicing_outbound_messages_while_a_substream_resolution_is_pending() {
    let (_, node_identity, conn_man_mock, proto_tx, request_tx, _inbound_msg_rx, _events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    // A NewInboundSubstream notification for a peer that will never resolve to a connection - nothing ever
    // registers it with the connectivity mock.
    let unresolvable_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let phantom_peer = Peer::new(
        pk,
        unresolvable_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    let (_, phantom_state, _, _) = create_peer_connection_mock_pair(phantom_peer, node_identity.to_peer()).await;
    let phantom_substream = phantom_state.open_substream().await.unwrap();
    proto_tx
        .send(ProtocolNotification::new(
            MESSAGING_PROTOCOL_ID.clone(),
            ProtocolEvent::NewInboundSubstream(unresolvable_node_id, phantom_substream),
        ))
        .await
        .unwrap();

    // Give the actor a moment to have picked the notification up and spawned the (up to 2s) resolution wait -
    // nowhere near the full window.
    time::sleep(Duration::from_millis(50)).await;

    // A completely unrelated, resolvable peer must still be served promptly.
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;

    let out_msg = OutboundMessage::new(peer_node_identity.node_id().clone(), TEST_MSG1.clone());
    request_tx.send(out_msg).unwrap();

    // With the actor stalled on the unresolvable substream (the pre-fix behaviour), this would not arrive
    // within `CONNECTION_LOOKUP_TIMEOUT`. It must arrive almost immediately.
    let stream = time::timeout(Duration::from_millis(500), peer_conn_mock2.next_incoming_substream())
        .await
        .expect("MessagingProtocol actor appears to be stalled servicing an unresolved substream")
        .expect("no substream opened");
    let mut framed = MessagingProtocol::framed(stream);
    let msg = framed.next().await.unwrap().unwrap();
    assert_eq!(msg, TEST_MSG1);
}

/// Regression test for the discard risk in the *previous* version of the replacement logic in
/// `spawn_inbound_handler`, which called `JoinHandle::abort()` on the stale session. Aborting cancels the task
/// wherever it happens to be, including mid-`await` on `inbound_message_tx.send` for a message that has
/// already been fully decoded off the wire - silently losing a message that was, from the network's point of
/// view, successfully delivered. This forces exactly that race deterministically: `inbound_message_tx` has
/// capacity 1 and is pre-filled, so the first session's delivery of its message is provably still blocked on
/// `send` - not merely "probably" blocked - at the moment the second substream replaces it.
#[tokio::test]
async fn replacing_a_session_does_not_discard_a_message_already_in_flight() {
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, mut inbound_msg_rx) = mpsc::channel(1);
    inbound_msg_tx
        .send(InboundMessage::new(node_id::random(), Bytes::from_static(b"FILLER")))
        .await
        .unwrap();
    let (events_tx, _events_rx) = broadcast::channel(100);

    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    )
    .set_message_received_event_enabled(true);
    tokio::spawn(msg_proto.run());

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer1 = Peer::new(
        pk,
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    // `create_peer_connection_mock_pair(a, b)` returns the first connection's `peer_node_id` set to `b`'s
    // NodeId (it is the connection *to* `b`, as dialled *from* `a`) - `peer1` (carrying `expected_node_id`)
    // must be the second argument for `conn1` to be registered under the id the substreams below arrive for.
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity.to_peer(), peer1).await;
    conn_man_mock.add_active_connection(conn1).await;

    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // First stream: write a message. Its delivery must block behind the filler until the channel is drained.
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    framed_ours.send(TEST_MSG1.clone()).await.unwrap();

    // Give the first session a chance to read and decode the message and start (and block on) delivering it.
    time::sleep(Duration::from_millis(100)).await;

    // Second stream on the same connection: replaces the first session while its delivery of TEST_MSG1 is, by
    // construction, still blocked on the full channel.
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();
    time::sleep(Duration::from_millis(100)).await;

    // Drain the filler, then both messages must still arrive - neither was discarded by the replacement.
    let filler = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(filler.body, Bytes::from_static(b"FILLER"));

    let mut bodies = Vec::new();
    for _ in 0..2 {
        let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
            .await
            .expect("a message that was already in flight when its session was replaced must not be discarded")
            .unwrap();
        assert_eq!(msg.source_peer, expected_node_id);
        bodies.push(msg.body);
    }
    bodies.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    let mut expected = vec![TEST_MSG1.clone(), TEST_MSG2.clone()];
    expected.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    assert_eq!(bodies, expected);
}

/// Regression test for `MAX_STOPPING_SESSIONS_PER_PEER`: replacing a peer's inbound session faster than the
/// previous ones can finish delivering (e.g. because the node-wide inbound channel is saturated) must not let
/// that peer accumulate an unbounded number of live-but-stopping sessions. Forces the same deterministic block
/// used by `replacing_a_session_does_not_discard_a_message_already_in_flight`: `inbound_message_tx` has
/// capacity 1 and is pre-filled, so every session's delivery blocks until the test explicitly drains it,
/// meaning none of the sessions spawned below can ever finish on their own during the test.
#[tokio::test]
async fn replacements_beyond_the_stopping_cap_are_refused_not_accumulated() {
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, mut inbound_msg_rx) = mpsc::channel(1);
    inbound_msg_tx
        .send(InboundMessage::new(node_id::random(), Bytes::from_static(b"FILLER")))
        .await
        .unwrap();
    let (events_tx, _events_rx) = broadcast::channel(100);

    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    )
    .set_message_received_event_enabled(true);
    tokio::spawn(msg_proto.run());

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer1 = Peer::new(
        pk,
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity.to_peer(), peer1).await;
    conn_man_mock.add_active_connection(conn1).await;

    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // One more attempt than `1 (current) + MAX_STOPPING_SESSIONS_PER_PEER` (= 4) can hold, each carrying a
    // distinct message.
    const ATTEMPTS: usize = 6;
    let mut bodies_sent = Vec::new();
    for i in 0..ATTEMPTS {
        let body = Bytes::from(format!("MSG{i}"));
        let stream_ours = conn1_state.open_substream().await.unwrap();
        let mut framed_ours = MessagingProtocol::framed(stream_ours);
        framed_ours.send(body.clone()).await.unwrap();
        bodies_sent.push(body);
        // Give the actor and the newly spawned session a chance to fully run (spawn, read, decode, and block
        // on delivery) before the next substream is opened, so each replacement sees the full prior state.
        time::sleep(Duration::from_millis(100)).await;
    }

    // Drain the filler, then exactly what was actually accepted (current + MAX_STOPPING_SESSIONS_PER_PEER = 5
    // of the 6 attempts) must arrive.
    let filler = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(filler.body, Bytes::from_static(b"FILLER"));

    let expected_accepted = ATTEMPTS - 1;
    let mut received = Vec::new();
    for _ in 0..expected_accepted {
        let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
            .await
            .expect("a session accepted within the stopping cap must still deliver its message")
            .unwrap();
        received.push(msg.body);
    }

    // Nothing more should ever arrive: the excess (6th) substream's message was refused before it was ever
    // decoded, not merely queued behind the others.
    assert!(
        time::timeout(Duration::from_millis(200), inbound_msg_rx.recv())
            .await
            .is_err(),
        "an extra message arrived - the stopping-session cap did not refuse the excess replacement"
    );

    let mut received_sorted = received.clone();
    received_sorted.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    let mut expected_sorted: Vec<_> = bodies_sent.iter().take(expected_accepted).cloned().collect();
    expected_sorted.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    assert_eq!(received_sorted, expected_sorted, "unexpected set of delivered messages");
    assert!(
        !received.contains(bodies_sent.last().unwrap()),
        "the message on the refused (excess) substream must never be delivered"
    );
}

/// Regression test for `MessagingProtocol::prune_inbound_session`: pruning a superseded session's bookkeeping
/// when it finishes exiting must be keyed to that specific session, not to the peer. A peer-keyed prune (e.g.
/// `active_inbound.remove(node_id)`) would remove the *whole* per-peer entry - including the live `current`
/// session that superseded it - as soon as the superseded session's exit is processed. That would drop
/// `current`'s `stop_tx` along with the rest of the entry, and a dropped `stop_tx` reads identically to a fired
/// one on the receiving end (`InboundMessaging::run` selects on `&mut self.replaced` without inspecting the
/// `Result`), so `current` would immediately - and wrongly - believe itself replaced and start draining, even
/// though nothing about it was actually superseded.
#[tokio::test]
async fn superseded_session_exiting_does_not_disturb_the_current_session() {
    let (peer_manager, node_identity_1, conn_man_mock, proto_tx, _, mut inbound_msg_rx, _, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let peer1 = node_identity_1.to_peer();

    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk.clone(),
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer2.clone()).await.unwrap();

    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(peer1.clone(), peer2.clone()).await;
    conn_man_mock.add_active_connection(conn1).await;

    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // First (soon-to-be-superseded) session.
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    framed_ours.send(TEST_MSG1.clone()).await.unwrap();
    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(in_msg.body, TEST_MSG1);

    // A second substream on the same connection replaces the first: the first becomes "stopping", the second
    // becomes "current".
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();
    let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(in_msg.body, TEST_MSG2);

    // Wait for the superseded (first) session to actually finish stopping - proven by its write side going
    // dead, which only happens once its task has exited and dropped the substream.
    loop {
        if let Err(e) = framed_ours.send(TEST_MSG1.clone()).await {
            assert_eq!(
                e.to_string().split(':').nth(1).map(|s| s.trim()),
                Some("connection is closed"),
                "Expected connection to be closed but got '{e}'"
            );
            break;
        }
    }

    // Give the actor plenty of turns to receive and act on the superseded session's
    // `MessagingEvent::InboundSessionExited` - this is exactly the window in which a peer-keyed (rather than
    // session-keyed) prune would incorrectly tear down the second (current) session's bookkeeping.
    time::sleep(Duration::from_millis(200)).await;

    // The current session must be completely unaffected: it must keep delivering messages well after the
    // superseded session's exit has been processed.
    for i in 0..5 {
        let body = Bytes::from(format!("AFTER{i}"));
        framed_ours2.send(body.clone()).await.unwrap();
        let in_msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
            .await
            .expect(
                "the current inbound session must keep working after an unrelated superseded session finished exiting",
            )
            .unwrap();
        assert_eq!(in_msg.body, body);
    }
}

/// A message body larger than the 256 KiB starting window of a yamux stream. Sending it to a peer that never reads
/// leaves the write waiting for window.
fn stalling_body() -> Bytes {
    Bytes::from(vec![0u8; 1024 * 1024])
}

/// Polls `condition` until it holds, panicking after 5 seconds
async fn wait_until<F: FnMut() -> bool>(mut condition: F) {
    time::timeout(Duration::from_secs(5), async {
        while !condition() {
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition was not met within 5 seconds");
}

#[tokio::test]
async fn outbound_queue_full_fails_new_messages_without_growth() {
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;

    let (_proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, _inbound_msg_rx) = mpsc::channel(10);
    let (events_tx, _events_rx) = broadcast::channel(10);
    // Not spawned: the test calls `send_message` directly so that it can look at the peer's queue
    let mut msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    );

    let peer = peer_node_identity.node_id().clone();
    let (reply_tx, mut stalled_reply_rx) = oneshot::channel();
    msg_proto
        .send_message(OutboundMessage::with_reply(
            peer.clone(),
            stalling_body(),
            reply_tx.into(),
        ))
        .unwrap();
    // The peer accepts the substream but never reads from it
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    // The outbound session has taken the stalling message off the queue and is stuck writing it
    wait_until(|| msg_proto.outbound_queue_len(&peer) == Some(0)).await;
    time::sleep(Duration::from_millis(100)).await;

    let mut queued_reply_rxs = Vec::with_capacity(OUTBOUND_QUEUE_CAPACITY);
    for _ in 0..OUTBOUND_QUEUE_CAPACITY {
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                TEST_MSG1.clone(),
                reply_tx.into(),
            ))
            .unwrap();
        queued_reply_rxs.push(reply_rx);
    }
    assert_eq!(msg_proto.outbound_queue_len(&peer), Some(OUTBOUND_QUEUE_CAPACITY));

    // One more fails straight away, and the queue does not grow
    let (reply_tx, reply_rx) = oneshot::channel();
    msg_proto
        .send_message(OutboundMessage::with_reply(
            peer.clone(),
            TEST_MSG1.clone(),
            reply_tx.into(),
        ))
        .unwrap();
    let reply = time::timeout(Duration::from_millis(10), reply_rx)
        .await
        .expect("a message for a full queue must fail immediately")
        .unwrap();
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
    assert_eq!(msg_proto.outbound_queue_len(&peer), Some(OUTBOUND_QUEUE_CAPACITY));

    // Everything already queued is still waiting to be sent
    assert!(matches!(stalled_reply_rx.try_recv(), Err(TryRecvError::Empty)));
    for reply_rx in &mut queued_reply_rxs {
        assert!(matches!(reply_rx.try_recv(), Err(TryRecvError::Empty)));
    }
}

#[tokio::test(start_paused = true)]
async fn outbound_send_times_out_and_session_exits() {
    let (_, node_identity, conn_man_mock, _, request_tx, _, mut events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();

    let (reply_tx, stalled_reply_rx) = oneshot::channel();
    request_tx
        .send(OutboundMessage::with_reply(
            peer.clone(),
            stalling_body(),
            reply_tx.into(),
        ))
        .unwrap();
    let (reply_tx, queued_reply_rx) = oneshot::channel();
    request_tx
        .send(OutboundMessage::with_reply(
            peer.clone(),
            TEST_MSG1.clone(),
            reply_tx.into(),
        ))
        .unwrap();

    // The peer accepts the substream but stops reading
    let mut substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let start = Instant::now();

    let reply = stalled_reply_rx.await.unwrap();
    unpack_enum!(Err(SendFailReason::SendTimeout) = reply);
    assert!(start.elapsed() >= OUTBOUND_SEND_TIMEOUT);
    // The message queued behind it fails too, rather than being retried on a new substream, with a reason that does
    // not blame the peer for a write it never got
    let reply = queued_reply_rx.await.unwrap();
    unpack_enum!(Err(SendFailReason::QueuedBehindTimeout) = reply);

    let event = time::timeout(Duration::from_secs(5), events_rx.recv())
        .await
        .unwrap()
        .unwrap();
    unpack_enum!(MessagingEvent::OutboundProtocolExited(node_id) = &event);
    assert_eq!(*node_id, peer);

    // The session closed the substream: the peer reads what arrived before the stall and then the end of it
    let mut received = Vec::new();
    let _result = time::timeout(Duration::from_secs(5), substream.read_to_end(&mut received))
        .await
        .expect("the outbound session did not close the substream");
    assert!(received.len() < stalling_body().len());
}

#[tokio::test]
async fn expired_messages_are_not_requeued_on_disconnect() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1.clone()).await;
    let peer = peer_node_identity.node_id().clone();

    let (events_tx, _events_rx) = mpsc::channel(10);
    let (messages_tx, messages_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
    let (retry_queue_tx, mut retry_queue_rx) = mpsc::channel(10);
    let outbound = OutboundMessaging::new(
        requester,
        events_tx,
        messages_rx,
        retry_queue_tx,
        peer.clone(),
        MESSAGING_PROTOCOL_ID.clone(),
        test_queue_accounting(),
    );
    let outbound_task = tokio::spawn(outbound.run());

    let live_deadline = Instant::now().checked_add(OUTBOUND_MESSAGE_TTL).unwrap();
    let expired_deadline = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let queue = |body: Bytes, deadline: Instant| {
        let (reply_tx, reply_rx) = oneshot::channel();
        let queued = QueuedOutboundMessage {
            message: OutboundMessage::with_reply(peer.clone(), body, reply_tx.into()),
            deadline,
            retries: 0,
            attempts: 0,
            retried_after_connection_loss: false,
            bytes: None,
        };
        (queued, reply_rx)
    };

    // Stall the session on a message the peer never reads
    let (stalled, _stalled_reply_rx) = queue(stalling_body(), live_deadline);
    let stalled_tag = stalled.message.tag;
    messages_tx.send(stalled).await.unwrap();
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    wait_until(|| messages_tx.capacity() == OUTBOUND_QUEUE_CAPACITY).await;

    // Queue expired and live messages behind it, then disconnect
    let (expired1, expired1_reply_rx) = queue(TEST_MSG1.clone(), expired_deadline);
    let (expired2, expired2_reply_rx) = queue(TEST_MSG1.clone(), expired_deadline);
    let (live, _live_reply_rx) = queue(TEST_MSG2.clone(), live_deadline);
    let live_tag = live.message.tag;
    messages_tx.send(expired1).await.unwrap();
    messages_tx.send(live).await.unwrap();
    messages_tx.send(expired2).await.unwrap();
    conn1.clone().disconnect(Minimized::No, "test").await.unwrap();

    time::timeout(Duration::from_secs(5), outbound_task)
        .await
        .expect("the outbound session did not exit on disconnect")
        .unwrap();

    for reply_rx in [expired1_reply_rx, expired2_reply_rx] {
        let reply = reply_rx.await.unwrap();
        unpack_enum!(Err(SendFailReason::Expired) = reply);
    }
    // Only the messages still inside their deadline were requeued
    let mut retried_tags = Vec::new();
    while let Ok(queued) = retry_queue_rx.try_recv() {
        retried_tags.push(queued.message.tag);
    }
    assert_eq!(retried_tags.len(), 2, "{retried_tags:?}");
    assert!(retried_tags.contains(&stalled_tag));
    assert!(retried_tags.contains(&live_tag));
}

/// Starts forwarding the inbound substreams of a new peer to the messaging protocol and returns the peer's node id
/// and a substream on which it can send messages
async fn connect_inbound_peer(
    node_identity: &NodeIdentity,
    conn_man_mock: &ConnectivityManagerMockState,
    proto_tx: mpsc::Sender<ProtocolNotification<Substream>>,
) -> (
    NodeId,
    tokio_util::codec::Framed<Substream, tokio_util::codec::LengthDelimitedCodec>,
) {
    let peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
    let peer_node_id = peer.node_id.clone();
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity.to_peer(), peer).await;
    conn_man_mock.add_active_connection(conn1).await;

    tokio::spawn({
        let peer_node_id = peer_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(peer_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    let stream_ours = conn1_state.open_substream().await.unwrap();
    (peer_node_id, MessagingProtocol::framed(stream_ours))
}

#[tokio::test]
async fn flooding_peer_cannot_starve_other_peers_inbound() {
    const FLOOD_MESSAGES: usize = 20;
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, mut inbound_msg_rx) = mpsc::channel(INBOUND_MESSAGE_BUFFER_SIZE);
    let (events_tx, _events_rx) = broadcast::channel(100);
    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    );
    tokio::spawn(msg_proto.run());

    let (flooder, mut flooder_stream) = connect_inbound_peer(&node_identity, &conn_man_mock, proto_tx.clone()).await;
    let (honest, mut honest_stream) = connect_inbound_peer(&node_identity, &conn_man_mock, proto_tx).await;

    // Peer A sends continuously while the consumer is not reading
    for i in 0..FLOOD_MESSAGES {
        flooder_stream.send(Bytes::from(format!("FLOOD{i}"))).await.unwrap();
    }
    time::sleep(Duration::from_millis(200)).await;
    // Peer B sends a single message
    honest_stream.send(TEST_MSG1.clone()).await.unwrap();
    time::sleep(Duration::from_millis(200)).await;

    // One consumer cycle: everything queued is A's in-flight messages plus B's message. They are held (not dropped)
    // so A's slots stay in use.
    let mut held = Vec::new();
    for _ in 0..=INBOUND_PER_SESSION_INFLIGHT {
        let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
            .await
            .expect("the honest peer's message was starved by the flooding peer")
            .unwrap();
        held.push(msg);
    }
    let from_honest = held.iter().filter(|msg| msg.source_peer == honest).count();
    let from_flooder = held.iter().filter(|msg| msg.source_peer == flooder).count();
    assert_eq!(from_honest, 1);
    assert_eq!(from_flooder, INBOUND_PER_SESSION_INFLIGHT);

    // A holds no more slots than its in-flight limit while the consumer holds its messages
    assert!(
        time::timeout(Duration::from_millis(200), inbound_msg_rx.recv())
            .await
            .is_err(),
        "the flooding peer delivered more than {INBOUND_PER_SESSION_INFLIGHT} messages that were not yet consumed"
    );

    // Once the consumer drops them, the rest of A's messages arrive
    drop(held);
    for _ in INBOUND_PER_SESSION_INFLIGHT..FLOOD_MESSAGES {
        let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(msg.source_peer, flooder);
    }
}

#[tokio::test]
async fn oversized_frame_is_reported_on_the_internal_channel() {
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, _inbound_msg_rx) = mpsc::channel(10);
    let (events_tx, mut events_rx) = broadcast::channel(100);
    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    )
    .with_ban_duration(Duration::from_secs(60));
    tokio::spawn(msg_proto.run());

    let (peer, framed) = connect_inbound_peer(&node_identity, &conn_man_mock, proto_tx).await;
    // A 9 MiB length header, over the 8 MiB frame limit
    let mut substream = framed.into_inner();
    let oversized_len = u32::try_from(9 * 1024 * 1024).unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut substream, &oversized_len.to_be_bytes())
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::flush(&mut substream).await.unwrap();

    // The actor bans the peer...
    let banned = time::timeout(Duration::from_secs(5), async {
        loop {
            let banned = conn_man_mock.take_banned_peers().await;
            if !banned.is_empty() {
                return banned;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the peer that sent an oversized frame was not banned");
    assert_eq!(banned.len(), 1);
    assert_eq!(banned.first().map(|(node_id, _, _)| node_id), Some(&peer));
    // ...and still forwards the violation to external subscribers
    let event = time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events_rx.recv().await.unwrap();
            if matches!(event, MessagingEvent::ProtocolViolation { .. }) {
                return event;
            }
        }
    })
    .await
    .unwrap();
    match event {
        MessagingEvent::ProtocolViolation { peer_node_id, .. } => assert_eq!(peer_node_id, peer),
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn messages_to_a_peer_that_resets_every_substream_fail_after_max_retries() {
    let (_, node_identity, conn_man_mock, _, request_tx, _, _, _shutdown) = spawn_messaging_protocol().await;

    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();

    // The peer resets every messaging substream we open, as soon as it arrives
    let num_substreams = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    tokio::spawn({
        let num_substreams = num_substreams.clone();
        async move {
            while let Some(substream) = peer_conn_mock2.next_incoming_substream().await {
                num_substreams.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                drop(substream);
            }
        }
    });

    // Every message is larger than the stream window, so none can be written in full before the reset arrives (a
    // small message could be written, and so reported as sent, before the peer's reset reaches us)
    const NUM_MSGS: usize = 6;
    let mut reply_rxs = Vec::new();
    for _ in 0..NUM_MSGS {
        let (reply_tx, reply_rx) = oneshot::channel();
        request_tx
            .send(OutboundMessage::with_reply(
                peer.clone(),
                stalling_body(),
                reply_tx.into(),
            ))
            .unwrap();
        reply_rxs.push(reply_rx);
    }

    for (i, reply_rx) in reply_rxs.into_iter().enumerate() {
        let reply = time::timeout(Duration::from_secs(10), reply_rx)
            .await
            .expect("a message to a peer that resets every substream must fail, not be retried until its deadline")
            .unwrap();
        match reply {
            // Only messages at the front of a session's queue get written before the reset, so a message may have
            // been written fewer times than it was retried, but never more than once per retry plus the first try
            Err(SendFailReason::MaxRetriesReached(attempts)) => {
                assert!(attempts <= MAX_SEND_RETRIES + 1);
                // The first message is written on every substream: its retry goes to a fresh session, not back
                // into the ending session's queue to be failed by its drain
                if i == 0 {
                    assert_eq!(attempts, MAX_SEND_RETRIES + 1);
                }
            },
            other => panic!("expected MaxRetriesReached, got {other:?}"),
        }
    }
    // Each substream carries at least one attempt, and no message makes more than MAX_SEND_RETRIES + 1 attempts
    time::sleep(Duration::from_millis(200)).await;
    assert!(num_substreams.load(std::sync::atomic::Ordering::SeqCst) <= NUM_MSGS * (MAX_SEND_RETRIES + 1));
}

#[tokio::test]
async fn actor_keeps_running_while_a_ban_request_is_pending() {
    let shutdown = Shutdown::new();
    // The connectivity mock is not running yet, so once its request buffer is full every ban request waits
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    for _ in 0..10 {
        requester
            .clone()
            .ban_peer_until(node_id::random(), Duration::from_secs(60), "filler")
            .await
            .unwrap();
    }

    let (_proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, _inbound_msg_rx) = mpsc::channel(10);
    let (events_tx, mut events_rx) = broadcast::channel(100);
    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    )
    .with_ban_duration(Duration::from_secs(60));
    let internal_events_tx = msg_proto.internal_event_sender();
    tokio::spawn(msg_proto.run());

    // Each violation is forwarded to subscribers only after the actor has handled the ban, so receiving them
    // promptly shows the actor is not waiting on the ConnectivityManager
    let violators = [node_id::random(), node_id::random()];
    for violator in &violators {
        internal_events_tx
            .send(MessagingEvent::ProtocolViolation {
                peer_node_id: violator.clone(),
                details: "oversized frame".to_string(),
            })
            .await
            .unwrap();
        let event = time::timeout(Duration::from_millis(500), events_rx.recv())
            .await
            .expect("the messaging actor stalled waiting for a ban request")
            .unwrap();
        match event {
            MessagingEvent::ProtocolViolation { peer_node_id, .. } => assert_eq!(&peer_node_id, violator),
            other => panic!("unexpected event {other}"),
        }
    }

    // Once the ConnectivityManager catches up, the bans still arrive
    mock.spawn();
    let mut banned = Vec::new();
    time::timeout(Duration::from_secs(5), async {
        loop {
            for (node_id, _, _) in conn_man_mock.take_banned_peers().await {
                banned.push(node_id);
            }
            if violators.iter().all(|violator| banned.contains(violator)) {
                return;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the pending ban requests were lost");
    assert_eq!(banned.len(), 12);
}

/// How a connection ends in `assert_unwritten_backlog_survives_two_connection_closes`
#[derive(Clone, Copy, Debug)]
enum ConnectionClose {
    /// We disconnect the connection
    Local,
    /// The peer closes the connection. Our substream ends first, and the connection is reported as disconnected
    /// shortly after, as the connection actor does once the yamux driver ends.
    Remote,
}

/// Runs two outbound sessions that each end with the connection closing while a write is stalled and a backlog of
/// never-written messages is queued behind it, and asserts that the backlog is requeued both times without using up
/// a retry
async fn assert_unwritten_backlog_survives_two_connection_closes(close: ConnectionClose) {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer = peer_node_identity.node_id().clone();
    let deadline = Instant::now().checked_add(OUTBOUND_MESSAGE_TTL).unwrap();
    let queue = |body: Bytes| {
        let (reply_tx, reply_rx) = oneshot::channel();
        let queued = QueuedOutboundMessage {
            message: OutboundMessage::with_reply(peer.clone(), body, reply_tx.into()),
            deadline,
            retries: 0,
            attempts: 0,
            retried_after_connection_loss: false,
            bytes: None,
        };
        (queued, reply_rx)
    };

    // Messages that are queued behind a stalled write in every session, so they are never written
    let mut backlog = Vec::new();
    let mut backlog_reply_rxs = Vec::new();
    for _ in 0..3 {
        let (queued, reply_rx) = queue(TEST_MSG1.clone());
        backlog.push(queued);
        backlog_reply_rxs.push(reply_rx);
    }
    let backlog_tags: Vec<_> = backlog.iter().map(|queued| queued.message.tag).collect();

    for round in 0..2 {
        // A fresh connection for each round, as after a tie-break or a redial
        let (conn1, _, _, peer_conn_mock2) =
            create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
        conn_man_mock.add_active_connection(conn1.clone()).await;

        let (events_tx, _events_rx) = mpsc::channel(10);
        let (messages_tx, messages_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let (retry_queue_tx, mut retry_queue_rx) = mpsc::channel(10);
        let outbound = OutboundMessaging::new(
            requester.clone(),
            events_tx,
            messages_rx,
            retry_queue_tx,
            peer.clone(),
            MESSAGING_PROTOCOL_ID.clone(),
            test_queue_accounting(),
        );
        let outbound_task = tokio::spawn(outbound.run());

        // Stall the session on a message the peer never reads, then queue the backlog behind it
        let (stalled, _stalled_reply_rx) = queue(stalling_body());
        let stalled_tag = stalled.message.tag;
        messages_tx.send(stalled).await.unwrap();
        let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
        wait_until(|| messages_tx.capacity() == OUTBOUND_QUEUE_CAPACITY).await;
        for queued in backlog.drain(..) {
            messages_tx.send(queued).await.unwrap();
        }

        match close {
            ConnectionClose::Local => {
                conn1.clone().disconnect(Minimized::No, "test").await.unwrap();
            },
            ConnectionClose::Remote => {
                peer_conn_mock2.disconnect().await.unwrap();
                time::sleep(Duration::from_millis(100)).await;
                conn1.clone().disconnect(Minimized::No, "test").await.unwrap();
            },
        }
        time::timeout(Duration::from_secs(5), outbound_task)
            .await
            .expect("the outbound session did not exit on disconnect")
            .unwrap();

        // The interrupted write uses up a retry; the never-written backlog is requeued without using one
        while let Ok(queued) = retry_queue_rx.try_recv() {
            if queued.message.tag == stalled_tag {
                assert_eq!(queued.retries, 1, "{close:?} round {round}");
            } else {
                assert_eq!(queued.retries, 0, "{close:?} round {round}");
                backlog.push(queued);
            }
        }
        let requeued_tags: Vec<_> = backlog.iter().map(|queued| queued.message.tag).collect();
        assert_eq!(requeued_tags, backlog_tags, "{close:?} round {round}");
    }

    // None of the backlog has failed
    for reply_rx in &mut backlog_reply_rxs {
        assert!(matches!(reply_rx.try_recv(), Err(TryRecvError::Empty)));
    }
}

#[tokio::test]
async fn unwritten_messages_survive_consecutive_connection_disconnects() {
    assert_unwritten_backlog_survives_two_connection_closes(ConnectionClose::Local).await;
}

#[tokio::test]
async fn unwritten_messages_survive_consecutive_remote_connection_closes() {
    assert_unwritten_backlog_survives_two_connection_closes(ConnectionClose::Remote).await;
}

#[tokio::test]
async fn an_exited_outbound_session_does_not_remove_its_successors_queue() {
    let shutdown = Shutdown::new();
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();

    // The peer resets the first messaging substream and keeps (but never reads) every later one
    let num_substreams = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    tokio::spawn({
        let num_substreams = num_substreams.clone();
        async move {
            let mut kept = Vec::new();
            while let Some(substream) = peer_conn_mock2.next_incoming_substream().await {
                if num_substreams.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    drop(substream);
                } else {
                    kept.push(substream);
                }
            }
        }
    });

    let (_proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, _inbound_msg_rx) = mpsc::channel(10);
    let (events_tx, _events_rx) = broadcast::channel(10);
    // Not spawned: the test drives the retry queue and internal events itself so that it can look at the queues
    let mut msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    );

    // Session A: the write fails when the peer resets the substream, so A requeues the message and closes its queue,
    // then waits out the disconnect grace before reporting its exit
    msg_proto
        .send_message(OutboundMessage::new(peer.clone(), stalling_body()))
        .unwrap();
    // The retry arrives during A's grace wait and spawns session B, which the peer keeps
    time::timeout(Duration::from_secs(5), msg_proto.handle_next_retry())
        .await
        .expect("the reset session did not requeue its message");
    wait_until(|| num_substreams.load(std::sync::atomic::Ordering::SeqCst) == 2).await;
    assert_eq!(msg_proto.outbound_queue_len(&peer), Some(0));

    // A's exit must leave B's queue in place
    let event = time::timeout(Duration::from_secs(5), msg_proto.handle_next_internal_event())
        .await
        .expect("session A did not exit");
    unpack_enum!(MessagingEvent::OutboundProtocolExited(node_id) = &event);
    assert_eq!(*node_id, peer);
    assert_eq!(msg_proto.outbound_queue_len(&peer), Some(0));

    // A new message goes to B rather than spawning another session
    msg_proto
        .send_message(OutboundMessage::new(peer.clone(), TEST_MSG1.clone()))
        .unwrap();
    assert_eq!(msg_proto.outbound_queue_len(&peer), Some(1));
    time::sleep(Duration::from_millis(200)).await;
    assert_eq!(num_substreams.load(std::sync::atomic::Ordering::SeqCst), 2);
}

/// A 4 MiB message body. Clones share one allocation, so queueing many of them is cheap while the byte budgets still
/// count each one's full length.
const FOUR_MIB: usize = 4 * 1024 * 1024;

fn four_mib_body() -> Bytes {
    static BODY: std::sync::OnceLock<Bytes> = std::sync::OnceLock::new();
    BODY.get_or_init(|| Bytes::from(vec![0u8; FOUR_MIB])).clone()
}

/// A messaging protocol that is not spawned, so tests can call `send_message` directly and look at its queues
fn unspawned_messaging_protocol(
    requester: crate::connectivity::ConnectivityRequester,
) -> (MessagingProtocol, Shutdown) {
    let shutdown = Shutdown::new();
    let (_proto_tx, proto_rx) = mpsc::channel(10);
    let (_request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, _inbound_msg_rx) = mpsc::channel(10);
    let (events_tx, _events_rx) = broadcast::channel(10);
    let msg_proto = MessagingProtocol::new(
        MESSAGING_PROTOCOL_ID.clone(),
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    );
    (msg_proto, shutdown)
}

/// Queues 4 MiB messages for a connected `peer` until its queue holds `OUTBOUND_QUEUE_MAX_BYTES`, and returns their
/// replies. The rest are only queued once the first has established the session: a dialing queue holds at most
/// `OUTBOUND_DIALING_QUEUE_MAX_BYTES`.
async fn fill_peer_byte_budget(msg_proto: &mut MessagingProtocol, peer: &NodeId) -> Vec<MessagingReplyRx> {
    let mut reply_rxs = Vec::new();
    for i in 0..OUTBOUND_QUEUE_MAX_BYTES / FOUR_MIB {
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                four_mib_body(),
                reply_tx.into(),
            ))
            .unwrap();
        reply_rxs.push(reply_rx);
        if i == 0 {
            wait_until(|| msg_proto.outbound_queue_established(peer) == Some(true)).await;
        }
    }
    reply_rxs
}

/// Sends a 4 MiB message to `peer` and returns its reply if it was failed straight away
async fn send_four_mib(msg_proto: &mut MessagingProtocol, peer: &NodeId) -> Option<Result<(), SendFailReason>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    msg_proto
        .send_message(OutboundMessage::with_reply(
            peer.clone(),
            four_mib_body(),
            reply_tx.into(),
        ))
        .unwrap();
    time::timeout(Duration::from_millis(10), reply_rx)
        .await
        .ok()
        .map(|reply| reply.unwrap())
}

#[tokio::test]
async fn outbound_queue_byte_budget_fails_new_messages() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    // The peer never reads, so the first message stalls and the rest wait in the queue
    let reply_rxs = fill_peer_byte_budget(&mut msg_proto, &peer).await;
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(OUTBOUND_QUEUE_MAX_BYTES));
    // Far below the 500 message cap
    assert!(reply_rxs.len() < OUTBOUND_QUEUE_CAPACITY);

    let reply = send_four_mib(&mut msg_proto, &peer)
        .await
        .expect("a message over the peer's byte budget must fail immediately");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(OUTBOUND_QUEUE_MAX_BYTES));
    assert_eq!(msg_proto.total_outbound_bytes(), OUTBOUND_QUEUE_MAX_BYTES);
}

#[tokio::test]
async fn outbound_total_byte_budget_fails_new_messages() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    // Two connected peers that will stay under their reserve: one whose 600 KiB message stalls, one that is idle
    let (busy_peer, _busy_conn, _busy_substream, _busy_reply_rx) = connect_established_peer(
        &mut msg_proto,
        &conn_man_mock,
        &node_identity,
        Bytes::from(vec![0u8; 600 * 1024]),
    )
    .await;
    assert_eq!(msg_proto.outbound_queue_bytes(&busy_peer), Some(600 * 1024));
    let (idle_peer, _idle_conn, _idle_substream, idle_reply_rx) =
        connect_established_peer(&mut msg_proto, &conn_man_mock, &node_identity, TEST_MSG1.clone()).await;
    idle_reply_rx.await.unwrap().unwrap();
    assert_eq!(msg_proto.outbound_queue_bytes(&idle_peer), Some(0));

    // Peers that never read fill the node-wide budget
    let mut _filled = Vec::new();
    for _ in 0..OUTBOUND_TOTAL_MAX_BYTES / OUTBOUND_QUEUE_MAX_BYTES {
        _filled.push(fill_established_peer(&mut msg_proto, &conn_man_mock, &node_identity).await);
    }
    assert!(msg_proto.total_outbound_bytes() + FOUR_MIB > OUTBOUND_TOTAL_MAX_BYTES);
    assert_eq!(msg_proto.dialing_outbound_bytes(), 0);

    // A queue that is still dialing gets no reserve, so it is held to the full node-wide budget
    let dialing_peer = node_id::random();
    let reply = send_four_mib(&mut msg_proto, &dialing_peer)
        .await
        .expect("a message for a dialing queue over the node-wide byte budget must fail immediately");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
    assert_eq!(msg_proto.outbound_queue_established(&dialing_peer), Some(false));

    // A connected peer whose queue is under its reserve still takes another message, even holding 600 KiB already
    assert!(
        send_four_mib(&mut msg_proto, &busy_peer).await.is_none(),
        "an established queue under its reserve must take a message when the node-wide budget is used up"
    );
    // Over its reserve, it is held to the node-wide budget again, although far under its own
    let reply = send_four_mib(&mut msg_proto, &busy_peer)
        .await
        .expect("a message over the node-wide byte budget must fail immediately");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);

    // A connected peer with an empty queue takes a message too
    assert!(send_four_mib(&mut msg_proto, &idle_peer).await.is_none());
}

#[tokio::test]
async fn queues_still_dialing_cannot_hold_more_than_the_dialing_budget() {
    // The connectivity mock is not running, so no dial completes
    let (requester, _mock) = create_connectivity_mock();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    // One reply each to many peers we cannot reach, as replies to made-up origins would be
    let mut accepted = 0usize;
    let mut refused = 0usize;
    for _ in 0..2 * OUTBOUND_DIALING_MAX_BYTES / FOUR_MIB {
        let peer = node_id::random();
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                four_mib_body(),
                reply_tx.into(),
            ))
            .unwrap();
        match time::timeout(Duration::from_millis(10), reply_rx).await {
            Ok(reply) => {
                unpack_enum!(Err(SendFailReason::QueueFull) = reply.unwrap());
                refused += 1;
            },
            Err(_) => accepted += 1,
        }
        assert_eq!(msg_proto.outbound_queue_established(&peer), Some(false));
    }
    assert_eq!(accepted, OUTBOUND_DIALING_MAX_BYTES / FOUR_MIB);
    assert_eq!(refused, OUTBOUND_DIALING_MAX_BYTES / FOUR_MIB);
    assert_eq!(msg_proto.dialing_outbound_bytes(), OUTBOUND_DIALING_MAX_BYTES);
    assert_eq!(msg_proto.total_outbound_bytes(), OUTBOUND_DIALING_MAX_BYTES);
}

#[tokio::test]
async fn establishing_moves_queued_bytes_off_the_dialing_budget() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    let (peer, _conn, _substream, _reply_rxs) =
        fill_established_peer(&mut msg_proto, &conn_man_mock, &node_identity).await;
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(OUTBOUND_QUEUE_MAX_BYTES));
    assert_eq!(msg_proto.dialing_outbound_bytes(), 0);
    assert_eq!(msg_proto.total_outbound_bytes(), OUTBOUND_QUEUE_MAX_BYTES);
}

#[tokio::test(start_paused = true)]
async fn a_dial_that_does_not_complete_releases_queued_bytes_by_the_message_deadline() {
    // The connectivity mock is not running, so the dial never completes
    let (requester, _mock) = create_connectivity_mock();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);
    let peer = node_id::random();

    // A dialing queue holds two 4 MiB messages
    let mut reply_rxs = Vec::new();
    for _ in 0..OUTBOUND_DIALING_QUEUE_MAX_BYTES / FOUR_MIB {
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                four_mib_body(),
                reply_tx.into(),
            ))
            .unwrap();
        reply_rxs.push(reply_rx);
    }
    assert_eq!(msg_proto.total_outbound_bytes(), OUTBOUND_DIALING_QUEUE_MAX_BYTES);

    let start = Instant::now();
    for reply_rx in reply_rxs {
        let reply = reply_rx.await.unwrap();
        unpack_enum!(Err(SendFailReason::PeerDialFailed) = reply);
    }
    assert!(start.elapsed() >= OUTBOUND_MESSAGE_TTL);
    assert!(start.elapsed() < OUTBOUND_MESSAGE_TTL + Duration::from_secs(1));
    wait_until(|| msg_proto.total_outbound_bytes() == 0).await;
}

#[tokio::test(start_paused = true)]
async fn outbound_bytes_are_released_after_a_ban_and_send_timeout() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    let mut reply_rxs = fill_peer_byte_budget(&mut msg_proto, &peer).await;
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let reply = send_four_mib(&mut msg_proto, &peer).await.unwrap();
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);

    // The ban drops the peer's queue; the stalled session still holds the messages until its write times out
    msg_proto.ban_peer_for_test(peer.clone());
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), None);
    let first_reply = reply_rxs.remove(0).await.unwrap();
    unpack_enum!(Err(SendFailReason::SendTimeout) = first_reply);
    for reply_rx in reply_rxs {
        let reply = reply_rx.await.unwrap();
        unpack_enum!(Err(SendFailReason::QueuedBehindTimeout) = reply);
    }
    wait_until(|| msg_proto.total_outbound_bytes() == 0).await;

    // The peer can be sent to again
    assert!(send_four_mib(&mut msg_proto, &peer).await.is_none());
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(FOUR_MIB));
}

#[tokio::test]
async fn outbound_bytes_are_released_after_a_disconnect() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1.clone()).await;
    let peer = peer_node_identity.node_id().clone();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    let reply_rxs = fill_peer_byte_budget(&mut msg_proto, &peer).await;
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    conn1.clone().disconnect(Minimized::No, "test").await.unwrap();

    // Every message is requeued, still charged to the node-wide budget while it waits to be retried. The retries go
    // to a new session, which cannot open a substream on the closed connection and fails them.
    for _ in 0..reply_rxs.len() {
        time::timeout(Duration::from_secs(5), msg_proto.handle_next_retry())
            .await
            .expect("a message was not requeued after the disconnect");
    }
    for reply_rx in reply_rxs {
        let reply = time::timeout(Duration::from_secs(5), reply_rx).await.unwrap().unwrap();
        assert!(reply.is_err());
    }
    wait_until(|| msg_proto.total_outbound_bytes() == 0).await;

    // A fresh connection to the peer takes messages again
    let (conn1, _, _, _peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    assert!(send_four_mib(&mut msg_proto, &peer).await.is_none());
}

/// Byte accounting for an outbound session that a test runs on its own
fn test_queue_accounting() -> Arc<QueueAccounting> {
    Arc::new(QueueAccounting::new(
        Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    ))
}

/// Connects a new peer that never reads, sends it `first_body`, and waits for its outbound session to establish.
/// Returns the peer, its side of the connection and the substream (to keep them open), and the first message's reply.
/// The first message must be accepted: yamux only announces a substream to the peer with its first data.
async fn connect_established_peer(
    msg_proto: &mut MessagingProtocol,
    conn_man_mock: &ConnectivityManagerMockState,
    node_identity: &NodeIdentity,
    first_body: Bytes,
) -> (
    NodeId,
    crate::test_utils::mocks::PeerConnectionMockState,
    Substream,
    MessagingReplyRx,
) {
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();

    let (reply_tx, reply_rx) = oneshot::channel();
    msg_proto
        .send_message(OutboundMessage::with_reply(peer.clone(), first_body, reply_tx.into()))
        .unwrap();
    let substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    wait_until(|| msg_proto.outbound_queue_established(&peer) == Some(true)).await;
    (peer, peer_conn_mock2, substream, reply_rx)
}

/// Connects a new peer that never reads and fills its established queue towards `OUTBOUND_QUEUE_MAX_BYTES`, as far
/// as the node-wide budget allows
async fn fill_established_peer(
    msg_proto: &mut MessagingProtocol,
    conn_man_mock: &ConnectivityManagerMockState,
    node_identity: &NodeIdentity,
) -> (
    NodeId,
    crate::test_utils::mocks::PeerConnectionMockState,
    Substream,
    Vec<MessagingReplyRx>,
) {
    let (peer, peer_conn, substream, first_reply_rx) =
        connect_established_peer(msg_proto, conn_man_mock, node_identity, four_mib_body()).await;
    let mut reply_rxs = vec![first_reply_rx];
    for _ in 1..OUTBOUND_QUEUE_MAX_BYTES / FOUR_MIB {
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                four_mib_body(),
                reply_tx.into(),
            ))
            .unwrap();
        reply_rxs.push(reply_rx);
    }
    (peer, peer_conn, substream, reply_rxs)
}

#[tokio::test]
async fn a_message_larger_than_a_frame_is_refused_when_queued() {
    let (requester, _mock) = create_connectivity_mock();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);
    let peer = node_id::random();

    let (reply_tx, reply_rx) = oneshot::channel();
    let body = Bytes::from(vec![0u8; super::MAX_FRAME_LENGTH + 1]);
    msg_proto
        .send_message(OutboundMessage::with_reply(peer.clone(), body, reply_tx.into()))
        .unwrap();
    let reply = time::timeout(Duration::from_millis(10), reply_rx)
        .await
        .expect("an oversized message must fail immediately")
        .unwrap();
    unpack_enum!(Err(SendFailReason::MessageTooLarge) = reply);
    // No session was started and nothing was charged
    assert_eq!(msg_proto.outbound_queue_len(&peer), None);
    assert_eq!(msg_proto.total_outbound_bytes(), 0);
}

#[tokio::test]
async fn a_message_the_codec_refuses_fails_alone_and_the_session_continues() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1).await;
    let peer = peer_node_identity.node_id().clone();

    let (events_tx, _events_rx) = mpsc::channel(10);
    let (messages_tx, messages_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
    let (retry_queue_tx, mut retry_queue_rx) = mpsc::channel(10);
    let outbound = OutboundMessaging::new(
        requester,
        events_tx,
        messages_rx,
        retry_queue_tx,
        peer.clone(),
        MESSAGING_PROTOCOL_ID.clone(),
        test_queue_accounting(),
    );
    tokio::spawn(outbound.run());

    // An oversized message that got past the queue check, then an ordinary one
    let deadline = Instant::now().checked_add(OUTBOUND_MESSAGE_TTL).unwrap();
    let mut reply_rxs = Vec::new();
    for body in [Bytes::from(vec![0u8; super::MAX_FRAME_LENGTH + 1]), TEST_MSG1.clone()] {
        let (reply_tx, reply_rx) = oneshot::channel();
        messages_tx
            .send(QueuedOutboundMessage {
                message: OutboundMessage::with_reply(peer.clone(), body, reply_tx.into()),
                deadline,
                retries: 0,
                attempts: 0,
                retried_after_connection_loss: false,
                bytes: None,
            })
            .await
            .unwrap();
        reply_rxs.push(reply_rx);
    }

    let substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let mut framed = MessagingProtocol::framed(substream);
    let msg = time::timeout(Duration::from_secs(5), framed.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg, TEST_MSG1);

    let oversized_reply = reply_rxs.remove(0).await.unwrap();
    unpack_enum!(Err(SendFailReason::MessageTooLarge) = oversized_reply);
    reply_rxs.remove(0).await.unwrap().unwrap();
    // Nothing was requeued: the session did not end
    assert!(retry_queue_rx.try_recv().is_err());
    assert!(!messages_tx.is_closed());
}

#[tokio::test]
async fn a_replaced_session_waiting_for_an_in_flight_slot_exits_without_the_consumer() {
    let (peer_manager, node_identity_1, conn_man_mock, proto_tx, _, mut inbound_msg_rx, _, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk,
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer2.clone()).await.unwrap();
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity_1.to_peer(), peer2).await;
    conn_man_mock.add_active_connection(conn1).await;
    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // The consumer holds the first session's messages, so it uses all of its in-flight slots
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    for _ in 0..=INBOUND_PER_SESSION_INFLIGHT {
        framed_ours.send(TEST_MSG1.clone()).await.unwrap();
    }
    let mut held = Vec::new();
    for _ in 0..INBOUND_PER_SESSION_INFLIGHT {
        held.push(
            time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    time::sleep(Duration::from_millis(100)).await;

    // A second substream replaces the first session, which must exit within STOPPING_SESSION_PERMIT_WAIT even though
    // the consumer still holds its slots
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();
    time::timeout(STOPPING_SESSION_PERMIT_WAIT + Duration::from_secs(2), async {
        loop {
            if framed_ours.send(TEST_MSG1.clone()).await.is_err() {
                break;
            }
        }
    })
    .await
    .expect("the replaced session waited for the consumer instead of exiting");

    // The new session delivers
    let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.body, TEST_MSG2);
    drop(held);
}

#[tokio::test]
async fn a_dialing_queue_cannot_hold_more_than_its_cap() {
    // The connectivity mock is not running, so no dial completes
    let (requester, _mock) = create_connectivity_mock();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);
    let peer = node_id::random();

    for _ in 0..OUTBOUND_DIALING_QUEUE_MAX_BYTES / FOUR_MIB {
        assert!(send_four_mib(&mut msg_proto, &peer).await.is_none());
    }
    let reply = send_four_mib(&mut msg_proto, &peer)
        .await
        .expect("a message over a dialing queue's cap must fail immediately");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
    assert_eq!(
        msg_proto.outbound_queue_bytes(&peer),
        Some(OUTBOUND_DIALING_QUEUE_MAX_BYTES)
    );

    // Other dialing peers still get in
    let other_peer = node_id::random();
    assert!(send_four_mib(&mut msg_proto, &other_peer).await.is_none());
    assert_eq!(
        msg_proto.dialing_outbound_bytes(),
        OUTBOUND_DIALING_QUEUE_MAX_BYTES + FOUR_MIB
    );
}

#[tokio::test]
async fn a_replaced_session_delivers_a_buffered_frame_if_a_slot_frees_up_in_time() {
    let (peer_manager, node_identity_1, conn_man_mock, proto_tx, _, mut inbound_msg_rx, _, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk,
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer2.clone()).await.unwrap();
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity_1.to_peer(), peer2).await;
    conn_man_mock.add_active_connection(conn1).await;
    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });

    // The consumer holds the first session's messages, so the last one stays buffered waiting for a slot
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    let buffered = Bytes::from_static(b"BUFFERED");
    for _ in 0..INBOUND_PER_SESSION_INFLIGHT {
        framed_ours.send(TEST_MSG1.clone()).await.unwrap();
    }
    framed_ours.send(buffered.clone()).await.unwrap();
    let mut held = Vec::new();
    for _ in 0..INBOUND_PER_SESSION_INFLIGHT {
        held.push(
            time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    time::sleep(Duration::from_millis(100)).await;

    // A second substream replaces the first session. Its message arriving shows the replacement has happened (the
    // actor tells the old session to stop in the same step that spawns the new one), and the old session is woken by
    // that signal while it waits for a slot, so it is stopping well before the slot frees up below.
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();
    let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.body, TEST_MSG2);
    drop(msg);
    time::sleep(Duration::from_millis(100)).await;
    // Nothing more has arrived: the buffered frame is still waiting for a slot
    assert!(inbound_msg_rx.try_recv().is_err());

    // A slot frees up well within the stopping session's wait, and the buffered frame arrives
    held.pop();
    let msg = time::timeout(Duration::from_secs(5), inbound_msg_rx.recv())
        .await
        .expect("a buffered frame of the replaced session was dropped")
        .unwrap();
    assert_eq!(msg.body, buffered);
}

#[tokio::test]
async fn a_requeued_backlog_is_not_held_to_the_dialing_queue_cap() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1.clone()).await;
    let peer = peer_node_identity.node_id().clone();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    // A 64 MiB backlog for a peer that never reads, then the connection drops
    let mut reply_rxs = fill_peer_byte_budget(&mut msg_proto, &peer).await;
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    conn1.clone().disconnect(Minimized::No, "test").await.unwrap();
    // The redial does not complete, so the requeued backlog stays in the new, dialing queue
    conn_man_mock.set_pending_connection(&peer).await;

    for _ in 0..reply_rxs.len() {
        time::timeout(Duration::from_secs(5), msg_proto.handle_next_retry())
            .await
            .expect("a message was not requeued after the disconnect");
    }
    assert_eq!(msg_proto.outbound_queue_established(&peer), Some(false));
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(OUTBOUND_QUEUE_MAX_BYTES));
    assert_eq!(msg_proto.dialing_outbound_bytes(), OUTBOUND_DIALING_MAX_BYTES);
    for reply_rx in &mut reply_rxs {
        assert!(matches!(reply_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    // New messages to a dialing queue are still held to its cap (and here to the full dialing budget)
    let other_peer = node_id::random();
    let reply = send_four_mib(&mut msg_proto, &other_peer)
        .await
        .expect("the dialing budget is used up by the requeued backlog");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
}

#[tokio::test]
async fn a_new_message_to_a_dialing_queue_holding_a_requeued_backlog_is_capped() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (conn1, _, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
    conn_man_mock.add_active_connection(conn1.clone()).await;
    let peer = peer_node_identity.node_id().clone();
    let (mut msg_proto, _shutdown) = unspawned_messaging_protocol(requester);

    // A 16 MiB backlog for a peer that never reads: over the dialing queue cap, under every other budget
    const BACKLOG_MESSAGES: usize = 4;
    let mut reply_rxs = Vec::new();
    for i in 0..BACKLOG_MESSAGES {
        let (reply_tx, reply_rx) = oneshot::channel();
        msg_proto
            .send_message(OutboundMessage::with_reply(
                peer.clone(),
                four_mib_body(),
                reply_tx.into(),
            ))
            .unwrap();
        reply_rxs.push(reply_rx);
        if i == 0 {
            wait_until(|| msg_proto.outbound_queue_established(&peer) == Some(true)).await;
        }
    }
    let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    conn1.clone().disconnect(Minimized::No, "test").await.unwrap();
    conn_man_mock.set_pending_connection(&peer).await;
    for _ in 0..BACKLOG_MESSAGES {
        time::timeout(Duration::from_secs(5), msg_proto.handle_next_retry())
            .await
            .expect("a message was not requeued after the disconnect");
    }
    let backlog_bytes = BACKLOG_MESSAGES * FOUR_MIB;
    assert!(backlog_bytes > OUTBOUND_DIALING_QUEUE_MAX_BYTES);
    assert_eq!(msg_proto.outbound_queue_established(&peer), Some(false));
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(backlog_bytes));

    // A new message to the same peer is held to the dialing queue cap, which the backlog is already over
    let reply = send_four_mib(&mut msg_proto, &peer)
        .await
        .expect("a new message to a dialing queue over its cap must fail immediately");
    unpack_enum!(Err(SendFailReason::QueueFull) = reply);
    assert_eq!(msg_proto.outbound_queue_bytes(&peer), Some(backlog_bytes));
    assert_eq!(msg_proto.dialing_outbound_bytes(), backlog_bytes);
    for reply_rx in &mut reply_rxs {
        assert!(matches!(reply_rx.try_recv(), Err(TryRecvError::Empty)));
    }
}

#[tokio::test]
async fn a_replaced_session_exits_even_if_the_peer_keeps_refilling_its_substream() {
    let (peer_manager, node_identity_1, conn_man_mock, proto_tx, _, mut inbound_msg_rx, _, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let peer2 = Peer::new(
        pk,
        expected_node_id.clone(),
        MultiaddressesWithStats::default(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
        Default::default(),
        Default::default(),
    );
    peer_manager.add_or_update_peer(peer2.clone()).await.unwrap();
    let (conn1, conn1_state, _, conn2_state) = create_peer_connection_mock_pair(node_identity_1.to_peer(), peer2).await;
    conn_man_mock.add_active_connection(conn1).await;
    tokio::spawn({
        let expected_node_id = expected_node_id.clone();
        async move {
            while let Some(stream_theirs) = conn2_state.next_incoming_substream().await {
                proto_tx
                    .send(ProtocolNotification::new(
                        MESSAGING_PROTOCOL_ID.clone(),
                        ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_theirs),
                    ))
                    .await
                    .unwrap();
            }
        }
    });
    // A slow consumer frees a slot well within STOPPING_SESSION_PERMIT_WAIT each time, so a session that waited for
    // each frame separately would never run out of time
    tokio::spawn(async move {
        while let Some(msg) = inbound_msg_rx.recv().await {
            time::sleep(Duration::from_millis(300)).await;
            drop(msg);
        }
    });

    // The peer writes on the first substream for as long as it can, so it always has frames buffered
    let stream_ours = conn1_state.open_substream().await.unwrap();
    let mut framed_ours = MessagingProtocol::framed(stream_ours);
    framed_ours.send(TEST_MSG1.clone()).await.unwrap();
    let writer = tokio::spawn(async move { while framed_ours.send(TEST_MSG1.clone()).await.is_ok() {} });
    time::sleep(Duration::from_millis(100)).await;

    // A second substream replaces the first session, which must stop within its drain deadline although the peer
    // keeps refilling the old substream
    let stream_ours2 = conn1_state.open_substream().await.unwrap();
    let mut framed_ours2 = MessagingProtocol::framed(stream_ours2);
    framed_ours2.send(TEST_MSG2.clone()).await.unwrap();
    time::timeout(STOPPING_SESSION_PERMIT_WAIT + Duration::from_secs(2), writer)
        .await
        .expect("the replaced session kept draining a substream the peer kept refilling")
        .unwrap();
}

#[tokio::test]
async fn an_in_flight_message_that_loses_its_connection_twice_fails_as_connection_lost() {
    let (requester, mock) = create_connectivity_mock();
    let conn_man_mock = mock.get_shared_state();
    mock.spawn();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer = peer_node_identity.node_id().clone();

    let (reply_tx, reply_rx) = oneshot::channel();
    let mut in_flight = QueuedOutboundMessage {
        message: OutboundMessage::with_reply(peer.clone(), stalling_body(), reply_tx.into()),
        deadline: Instant::now().checked_add(OUTBOUND_MESSAGE_TTL).unwrap(),
        retries: 0,
        attempts: 0,
        retried_after_connection_loss: false,
        bytes: None,
    };

    for round in 0..=MAX_SEND_RETRIES {
        // A fresh connection each round, which we then close while the message is being written (as a tie-break or
        // the connection reaper would)
        let (conn1, _, _, peer_conn_mock2) =
            create_peer_connection_mock_pair(node_identity.to_peer(), peer_node_identity.to_peer()).await;
        conn_man_mock.add_active_connection(conn1.clone()).await;
        let (events_tx, _events_rx) = mpsc::channel(10);
        let (messages_tx, messages_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let (retry_queue_tx, mut retry_queue_rx) = mpsc::channel(10);
        let outbound = OutboundMessaging::new(
            requester.clone(),
            events_tx,
            messages_rx,
            retry_queue_tx,
            peer.clone(),
            MESSAGING_PROTOCOL_ID.clone(),
            test_queue_accounting(),
        );
        let outbound_task = tokio::spawn(outbound.run());

        messages_tx.send(in_flight).await.unwrap();
        let _substream = peer_conn_mock2.next_incoming_substream().await.unwrap();
        wait_until(|| messages_tx.capacity() == OUTBOUND_QUEUE_CAPACITY).await;
        conn1.clone().disconnect(Minimized::No, "test").await.unwrap();
        time::timeout(Duration::from_secs(5), outbound_task)
            .await
            .expect("the outbound session did not exit on disconnect")
            .unwrap();

        if round < MAX_SEND_RETRIES {
            in_flight = retry_queue_rx
                .try_recv()
                .expect("the interrupted message was not requeued");
            assert!(in_flight.retried_after_connection_loss);
        } else {
            assert!(retry_queue_rx.try_recv().is_err());
            break;
        }
    }

    let reply = time::timeout(Duration::from_secs(5), reply_rx).await.unwrap().unwrap();
    match reply {
        Err(SendFailReason::ConnectionLost(attempts)) => assert_eq!(attempts, MAX_SEND_RETRIES + 1),
        other => panic!("expected ConnectionLost, got {other:?}"),
    }
}

#[tokio::test]
async fn a_message_requeued_after_a_connection_loss_whose_redial_fails_fails_as_connection_lost() {
    // No connection to the peer, so the session's dial fails
    let (requester, mock) = create_connectivity_mock();
    mock.spawn();
    let peer = node_id::random();
    let (events_tx, _events_rx) = mpsc::channel(10);
    let (messages_tx, messages_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
    let (retry_queue_tx, _retry_queue_rx) = mpsc::channel(10);
    let outbound = OutboundMessaging::new(
        requester,
        events_tx,
        messages_rx,
        retry_queue_tx,
        peer.clone(),
        MESSAGING_PROTOCOL_ID.clone(),
        test_queue_accounting(),
    );

    let deadline = Instant::now().checked_add(OUTBOUND_MESSAGE_TTL).unwrap();
    let mut reply_rxs = Vec::new();
    for retried_after_connection_loss in [true, false] {
        let (reply_tx, reply_rx) = oneshot::channel();
        messages_tx
            .send(QueuedOutboundMessage {
                message: OutboundMessage::with_reply(peer.clone(), TEST_MSG1.clone(), reply_tx.into()),
                deadline,
                retries: 1,
                attempts: 1,
                retried_after_connection_loss,
                bytes: None,
            })
            .await
            .unwrap();
        reply_rxs.push(reply_rx);
    }
    time::timeout(Duration::from_secs(5), outbound.run()).await.unwrap();

    // The message that is being retried because its connection was lost reports that
    let reply = reply_rxs.remove(0).await.unwrap();
    match reply {
        Err(SendFailReason::ConnectionLost(attempts)) => assert_eq!(attempts, 1),
        other => panic!("expected ConnectionLost, got {other:?}"),
    }
    // Any other message reports the failed dial
    let reply = reply_rxs.remove(0).await.unwrap();
    unpack_enum!(Err(SendFailReason::PeerDialFailed) = reply);
}

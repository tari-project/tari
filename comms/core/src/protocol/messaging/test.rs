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
    sync::{broadcast, mpsc, oneshot},
    time,
};

use super::protocol::{MessagingEventReceiver, MessagingProtocol};
use crate::{
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

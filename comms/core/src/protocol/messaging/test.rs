// Copyright 2020, The Taiji Project
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
use futures::{stream::FuturesUnordered, SinkExt, StreamExt};
use rand::rngs::OsRng;
use tari_crypto::keys::PublicKey;
use taiji_shutdown::Shutdown;
use taiji_test_utils::{collect_stream, unpack_enum};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use super::protocol::{MessagingEvent, MessagingEventReceiver, MessagingProtocol, MESSAGING_PROTOCOL};
use crate::{
    message::{InboundMessage, MessageTag, MessagingReplyRx, OutboundMessage},
    multiplexing::Substream,
    net_address::MultiaddressesWithStats,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManager},
    protocol::{messaging::SendFailReason, ProtocolEvent, ProtocolNotification},
    test_utils::{
        mocks::{create_connectivity_mock, create_peer_connection_mock_pair, ConnectivityManagerMockState},
        node_id,
        node_identity::build_node_identity,
        transport,
    },
    types::{CommsDatabase, CommsPublicKey},
};

static TEST_MSG1: Bytes = Bytes::from_static(b"TEST_MSG1");

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

    let peer_manager = PeerManager::new(CommsDatabase::new(), None).map(Arc::new).unwrap();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (request_tx, request_rx) = mpsc::unbounded_channel();
    let (inbound_msg_tx, inbound_msg_rx) = mpsc::channel(100);
    let (events_tx, events_rx) = broadcast::channel(100);

    let msg_proto = MessagingProtocol::new(
        requester,
        proto_rx,
        request_rx,
        events_tx,
        inbound_msg_tx,
        shutdown.to_signal(),
    );
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
    let (peer_manager, _, _, proto_tx, _, mut inbound_msg_rx, mut events_rx, _shutdown) =
        spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
    peer_manager
        .add_peer(Peer::new(
            pk.clone(),
            expected_node_id.clone(),
            MultiaddressesWithStats::default(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
            Default::default(),
            Default::default(),
        ))
        .await
        .unwrap();

    // Create connected memory sockets - we use each end of the connection as if they exist on different nodes
    let (_, muxer_ours, mut muxer_theirs) = transport::build_multiplexed_connections().await;

    // Notify the messaging protocol that a new substream has been established that wants to talk the messaging.
    let stream_ours = muxer_ours.get_yamux_control().open_stream().await.unwrap();
    proto_tx
        .send(ProtocolNotification::new(
            MESSAGING_PROTOCOL.clone(),
            ProtocolEvent::NewInboundSubstream(expected_node_id.clone(), stream_ours),
        ))
        .await
        .unwrap();

    let stream_theirs = muxer_theirs.incoming_mut().next().await.unwrap();
    let mut framed_theirs = MessagingProtocol::framed(stream_theirs);

    framed_theirs.send(TEST_MSG1.clone()).await.unwrap();

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
    unpack_enum!(MessagingEvent::MessageReceived(node_id, tag) = &*event);
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
    unpack_enum!(MessagingEvent::OutboundProtocolExited(_node_id) = &*event);
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
    unpack_enum!(MessagingEvent::OutboundProtocolExited(node_id) = &*event);
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

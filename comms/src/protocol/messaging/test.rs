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

use super::messaging::{Messaging, MessagingEvent, MessagingRequest, PROTOCOL_MESSAGING};
use crate::{
    connection::NetAddressesWithStats,
    message::{MessageExt, MessageFlags},
    outbound_message_service::OutboundMessage,
    peer_manager::{AsyncPeerManager, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    proto::envelope::Envelope,
    protocol::{ProtocolEvent, ProtocolNotification},
    test_utils::{
        create_connection_manager_mock,
        create_peer_connection_mock_pair,
        node_id,
        node_identity::build_node_identity,
        peer_manager::build_peer_manager,
        transport,
        ConnectionManagerMockState,
    },
    types::{CommsPublicKey, CommsSubstream},
};
use bytes::Bytes;
use futures::{
    channel::{mpsc, oneshot},
    stream,
    stream::FuturesUnordered,
    SinkExt,
    StreamExt,
};
use prost::Message;
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_crypto::keys::PublicKey;
use tari_test_utils::{collect_stream, unpack_enum};
use tokio::runtime::Handle;
use tokio_macros as runtime;

const TEST_MSG1: Bytes = Bytes::from_static(b"TEST_MSG1");

async fn spawn_messaging_protocol() -> (
    AsyncPeerManager,
    Arc<NodeIdentity>,
    ConnectionManagerMockState,
    mpsc::Sender<ProtocolNotification<CommsSubstream>>,
    mpsc::Sender<MessagingRequest>,
    mpsc::Receiver<MessagingEvent>,
) {
    let rt_handle = Handle::current();

    let (requester, mock) = create_connection_manager_mock(10);
    let mock_state = mock.get_shared_state();
    rt_handle.spawn(mock.run());

    let peer_manager: AsyncPeerManager = build_peer_manager().into();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);
    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (request_tx, request_rx) = mpsc::channel(10);
    let (events_tx, events_rx) = mpsc::channel(100);

    let msg_proto = Messaging::new(
        rt_handle.clone(),
        requester,
        peer_manager.clone(),
        node_identity.clone(),
        proto_rx,
        request_rx,
        events_tx,
    );
    rt_handle.spawn(msg_proto.run());

    (peer_manager, node_identity, mock_state, proto_tx, request_tx, events_rx)
}

// This tests the behaviour of futures mpsc channels. Specifically, it ensures that if
// a Receiver closes the channel it is still able to receive all the items currently in the
// buffer. This behaviour is required for the OutboundMessaging component to work.
#[runtime::test_basic]
async fn channel_prerequisite_rx_close() {
    let (mut tx, mut rx) = mpsc::channel(100);

    tx.send_all(&mut stream::iter(0u8..100).map(Ok)).await.unwrap();

    rx.close();

    let mut count = 0;
    while let Some(_) = rx.next().await {
        count += 1;
    }
    assert_eq!(count, 100);
    assert_eq!(tx.is_closed(), true);
}

#[runtime::test_basic]
async fn new_inbound_substream_handling() {
    let (peer_manager, _, _, mut proto_tx, _, mut events_rx) = spawn_messaging_protocol().await;

    let expected_node_id = node_id::random();
    // Create connected memory sockets - we use each end of the connection as if they exist on different nodes
    let (_, muxer_ours, mut muxer_theirs) = transport::build_multiplexed_connections().await;

    // Notify the messaging protocol that a new substream has been established that wants to talk the messaging.
    let stream_ours = muxer_ours.get_yamux_control().open_stream().await.unwrap();
    proto_tx
        .send(ProtocolNotification::new(
            PROTOCOL_MESSAGING.into(),
            ProtocolEvent::NewInboundSubstream(Box::new(expected_node_id.clone()), stream_ours),
        ))
        .await
        .unwrap();

    let stream_theirs = muxer_theirs.incoming_mut().next().await.unwrap().unwrap();
    let mut framed_theirs = Messaging::framed(stream_theirs);

    let (sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
    let envelope = Envelope::construct_signed(&sk, &pk, TEST_MSG1, MessageFlags::empty()).unwrap();
    peer_manager
        .add_peer(Peer::new(
            pk,
            expected_node_id.clone(),
            NetAddressesWithStats::default(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_CLIENT,
        ))
        .await
        .unwrap();

    framed_theirs
        .send(Bytes::copy_from_slice(&envelope.to_encoded_bytes().unwrap()))
        .await
        .unwrap();

    unpack_enum!(MessagingEvent::MessageReceived(node_id, msg) = events_rx.next().await.unwrap());
    assert_eq!(msg.body, TEST_MSG1);
    assert_eq!(*node_id, expected_node_id);
}

#[runtime::test_basic]
async fn send_message_request() {
    let (_, node_identity, conn_man_mock, _, mut request_tx, _) = spawn_messaging_protocol().await;

    let peer_node_id = node_id::random();

    let (conn1, peer_conn_mock1, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(1, node_identity.node_id().clone(), peer_node_id.clone()).await;

    // Add mock peer connection to connection manager mock for node 2
    conn_man_mock.add_active_connection(peer_node_id.clone(), conn1).await;

    // Send a message to node
    let (reply_tx, reply_rx) = oneshot::channel();
    let out_msg = OutboundMessage::new(peer_node_id, MessageFlags::NONE, TEST_MSG1);
    request_tx
        .send(MessagingRequest::SendMessage(out_msg, reply_tx))
        .await
        .unwrap();
    // Reply is Ok
    reply_rx.await.unwrap().unwrap();

    // Check that node got the message
    let stream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let mut framed = Messaging::framed(stream);
    let msg = framed.next().await.unwrap().unwrap();
    let msg = Envelope::decode(msg).unwrap();
    assert_eq!(msg.body, TEST_MSG1);

    // Got the call to create a substream
    assert_eq!(peer_conn_mock1.call_count(), 1);
}

#[runtime::test_basic]
async fn send_message_dial_failed() {
    let (_, _, conn_manager_mock, _, mut request_tx, mut event_tx) = spawn_messaging_protocol().await;

    let node_id = node_id::random();
    let out_msg = OutboundMessage::new(node_id, MessageFlags::NONE, TEST_MSG1);
    let expected_out_msg_tag = out_msg.tag;
    // Send a message to node 2
    let (reply_tx, reply_rx) = oneshot::channel();
    request_tx
        .send(MessagingRequest::SendMessage(out_msg, reply_tx))
        .await
        .unwrap();
    // Reply is Ok (message was queued)
    reply_rx.await.unwrap().unwrap();

    let event = event_tx.next().await.unwrap();
    unpack_enum!(MessagingEvent::SendMessageFailed(out_msg) = event);
    assert_eq!(out_msg.tag, expected_out_msg_tag);

    assert_eq!(conn_manager_mock.call_count(), 1);
}

#[runtime::test_basic]
async fn many_concurrent_send_message_requests() {
    const NUM_MSGS: usize = 100;
    let (_, _, conn_man_mock, _, mut request_tx, mut events_rx) = spawn_messaging_protocol().await;

    let node_id1 = node_id::random();
    let node_id2 = node_id::random();

    let (conn1, peer_conn_mock1, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(1, node_id1.clone(), node_id2.clone()).await;

    // Add mock peer connection to connection manager mock for node 2
    conn_man_mock.add_active_connection(node_id2.clone(), conn1).await;

    // Send many messages to node
    let replies = FuturesUnordered::new();
    let mut msg_tags = Vec::with_capacity(NUM_MSGS);
    for _ in 0..NUM_MSGS {
        let (reply_tx, reply_rx) = oneshot::channel();
        let out_msg = OutboundMessage::new(node_id2.clone(), MessageFlags::NONE, TEST_MSG1);
        msg_tags.push(out_msg.tag);
        request_tx
            .send(MessagingRequest::SendMessage(out_msg, reply_tx))
            .await
            .unwrap();
        replies.push(reply_rx);
    }

    collect_stream!(replies, take = NUM_MSGS, timeout = Duration::from_secs(5))
        .into_iter()
        .map(Result::unwrap)
        .for_each(drop);

    // Check that the node got the messages
    let stream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let framed = Messaging::framed(stream);
    let messages = collect_stream!(framed, take = NUM_MSGS, timeout = Duration::from_secs(10));
    assert_eq!(messages.len(), NUM_MSGS);

    // Check that we got message success events
    events_rx.close();
    let events = collect_stream!(events_rx, timeout = Duration::from_secs(10));
    for event in events {
        unpack_enum!(MessagingEvent::SendMessageSucceeded(tag) = event);
        // Assert that each tag is emitted only once
        let index = msg_tags.iter().position(|t| t == &tag).unwrap();
        msg_tags.remove(index);
    }

    // Got a single call to create a substream
    assert_eq!(peer_conn_mock1.call_count(), 1);
}

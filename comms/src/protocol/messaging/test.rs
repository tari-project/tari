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

use super::{MessagingEvent, MessagingProtocol, MessagingRequest, PROTOCOL_MESSAGING};
use crate::{
    protocol::{messaging::MessagingProtocolError, ProtocolEvent, ProtocolNotification},
    test_utils::{
        create_connection_manager_mock,
        create_peer_connection_mock_pair,
        node_id,
        transport,
        ConnectionManagerMockState,
    },
    types::CommsSubstream,
};
use bytes::Bytes;
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
    StreamExt,
};
use tari_test_utils::unpack_enum;
use tokio::runtime::Handle;
use tokio_macros as runtime;

const TEST_MSG1: Bytes = Bytes::from_static(b"TEST_MSG1");

async fn spawn_messaging_protocol() -> (
    ConnectionManagerMockState,
    mpsc::Sender<ProtocolNotification<CommsSubstream>>,
    mpsc::Sender<MessagingRequest>,
    mpsc::Receiver<MessagingEvent>,
) {
    let rt_handle = Handle::current();

    let (requester, mock) = create_connection_manager_mock(10);
    let mock_state = mock.get_shared_state();
    rt_handle.spawn(mock.run());

    let (proto_tx, proto_rx) = mpsc::channel(10);
    let (request_tx, request_rx) = mpsc::channel(10);
    let (events_tx, events_rx) = mpsc::channel(10);

    let msg_proto = MessagingProtocol::new(rt_handle.clone(), requester, proto_rx, request_rx, events_tx);
    rt_handle.spawn(msg_proto.run());

    (mock_state, proto_tx, request_tx, events_rx)
}

#[runtime::test_basic]
async fn new_inbound_substream_handling() {
    let (_, mut proto_tx, _, mut events_rx) = spawn_messaging_protocol().await;

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
    let mut framed_theirs = MessagingProtocol::framed(stream_theirs);
    framed_theirs.send(TEST_MSG1).await.unwrap();

    unpack_enum!(MessagingEvent::MessageReceived(node_id, msg) = events_rx.next().await.unwrap());
    assert_eq!(msg, TEST_MSG1);
    assert_eq!(*node_id, expected_node_id);
}

#[runtime::test_basic]
async fn send_message_request() {
    let (conn_man_mock, _, mut request_tx, _) = spawn_messaging_protocol().await;

    let node_id1 = node_id::random();
    let node_id2 = node_id::random();

    let (conn1, peer_conn_mock1, _, peer_conn_mock2) =
        create_peer_connection_mock_pair(1, node_id1.clone(), node_id2.clone()).await;

    // Add mock peer connection to connection manager mock for node 2
    conn_man_mock.add_active_connection(node_id2.clone(), conn1).await;

    // Send a message to node 2
    let (reply_tx, reply_rx) = oneshot::channel();
    request_tx
        .send(MessagingRequest::SendMessage(
            Box::new(node_id2.clone()),
            TEST_MSG1,
            reply_tx,
        ))
        .await
        .unwrap();
    reply_rx.await.unwrap().unwrap();

    // Check that node 2 got the message
    let stream = peer_conn_mock2.next_incoming_substream().await.unwrap();
    let mut framed = MessagingProtocol::framed(stream);
    let msg = framed.next().await.unwrap().unwrap();
    assert_eq!(msg, TEST_MSG1);

    // Got the call to create a substream
    assert_eq!(peer_conn_mock1.call_count(), 1);
}

#[runtime::test_basic]
async fn send_message_not_connected() {
    let (_, _, mut request_tx, _) = spawn_messaging_protocol().await;

    let node_id = node_id::random();

    // Send a message to node 2
    let (reply_tx, reply_rx) = oneshot::channel();
    request_tx
        .send(MessagingRequest::SendMessage(
            Box::new(node_id.clone()),
            TEST_MSG1,
            reply_tx,
        ))
        .await
        .unwrap();

    unpack_enum!(MessagingProtocolError::PeerNotConnected = reply_rx.await.unwrap().unwrap_err());
}

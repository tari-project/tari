//  Copyright 2019 The Tari Project
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

use crate::support::{
    factories::{self, TestFactory},
    helpers::{asserts::assert_change, streams::stream_assert_count},
};
use futures::{channel::mpsc, executor::block_on, StreamExt};
use std::time::Duration;
use tari_comms::{
    connection::{
        peer_connection::PeerConnectionProtocolMessage,
        types::{ConnectionDirection, Linger},
        Connection,
        CurveEncryption,
        PeerConnection,
        PeerConnectionContextBuilder,
        PeerConnectionError,
        ZmqContext,
    },
    message::FrameSet,
};

#[test]
fn connection_in() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let (consumer_tx, consumer_rx) = mpsc::channel(1);

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_direction(ConnectionDirection::Inbound)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::listen(context).unwrap();
    conn.wait_listening_or_failure(Duration::from_millis(1000)).unwrap();

    // Connect to the inbound connection and send a message
    let connection_identity = b"conn-ident".to_vec();
    let peer_identity = b"peer-ident".to_vec();
    conn.allow_identity(connection_identity.clone(), peer_identity.clone())
        .unwrap();
    let sender = Connection::new(&ctx, ConnectionDirection::Outbound)
        .set_identity(&connection_identity)
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .establish(&addr)
        .unwrap();
    sender
        .send(&[&[PeerConnectionProtocolMessage::Identify as u8]])
        .unwrap();
    sender
        .send(&[&[PeerConnectionProtocolMessage::Message as u8], &[1u8]])
        .unwrap();

    // Receive the message from the consumer channel
    let frames: Vec<FrameSet> = block_on(consumer_rx.take(1).collect());

    assert_eq!(frames[0][0], peer_identity);
    assert_eq!(frames[0][1], vec![1u8]);
    conn.send_to_identity(peer_identity, vec![vec![111u8]]).unwrap();
    let reply = sender.receive(100).unwrap();
    assert_eq!(reply, vec![vec![PeerConnectionProtocolMessage::Message as u8], vec![
        111u8
    ]],);
}

#[test]
fn connection_out() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, ConnectionDirection::Inbound)
        .set_name("Test sender")
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .establish(&addr)
        .unwrap();

    let connection_identity = b"conn-identity".to_vec();
    let peer_identity = b"peer-identity".to_vec();
    let (consumer_tx, consumer_rx) = mpsc::channel(10);
    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_peer_identity(peer_identity.clone())
        .set_connection_identity(connection_identity.clone())
        .set_direction(ConnectionDirection::Outbound)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .set_address(addr.clone())
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::connect(context).unwrap();

    conn.wait_connected_or_failure(Duration::from_millis(2000)).unwrap();

    conn.send(vec![vec![123u8]]).unwrap();

    let data = sender.receive(2000).unwrap();
    assert_eq!(vec![123u8], data[2]);

    sender
        .send(&[
            connection_identity.as_slice(),
            &[PeerConnectionProtocolMessage::Message as u8],
            &[1],
            &[2],
        ])
        .unwrap();

    let frames: Vec<FrameSet> = block_on(consumer_rx.take(1).collect());
    assert_eq!(frames[0][0], peer_identity);
    assert_eq!(frames[0][1], vec![1]);
    assert_eq!(frames[0][2], vec![2]);
}

#[test]
fn connection_wait_connect_shutdown() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let receiver = Connection::new(&ctx, ConnectionDirection::Inbound)
        .establish(&addr)
        .unwrap();

    let (consumer_tx, _consumer_rx) = mpsc::channel(10);

    let context = PeerConnectionContextBuilder::new()
        .set_peer_identity(b"dummy-remote-identity".to_vec())
        .set_connection_identity(b"123".to_vec())
        .set_direction(ConnectionDirection::Outbound)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_address(addr)
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::connect(context).unwrap();

    conn.wait_connected_or_failure(Duration::from_millis(2000)).unwrap();

    conn.shutdown().unwrap();

    assert!(
        conn.wait_disconnected(Duration::from_millis(2000)).is_ok(),
        "Failed to shut down in 100ms"
    );

    drop(receiver);
}

#[test]
fn connection_wait_connect_failed() {
    let addr = factories::net_address::create().use_os_port().build().unwrap();
    let ctx = ZmqContext::new();

    let (consumer_tx, _consumer_rx) = mpsc::channel(10);

    // This has nothing to connect to
    let context = PeerConnectionContextBuilder::new()
        .set_peer_identity(b"dummy-remote-identity".to_vec())
        .set_connection_identity(b"123".to_vec())
        .set_direction(ConnectionDirection::Outbound)
        .set_max_retry_attempts(1)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_address(addr.clone())
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::connect(context).unwrap();

    let err = conn.wait_connected_or_failure(Duration::from_millis(2000)).unwrap_err();

    assert!(conn.is_failed());
    match err {
        PeerConnectionError::OperationFailed(s) => {
            assert!(s.contains(PeerConnectionError::ExceededMaxConnectRetryCount.to_string().as_str()))
        },
        _ => panic!("Unexpected connection error '{}'", err),
    }
}

#[test]
fn connection_disconnect() {
    let addr = factories::net_address::create().use_os_port().build().unwrap();
    let ctx = ZmqContext::new();

    let (consumer_tx, _consumer_rx) = mpsc::channel(10);

    // Connect to the inbound connection and send a message
    let sender = Connection::new(&ctx, ConnectionDirection::Inbound)
        .set_linger(Linger::Indefinitely)
        .establish(&addr)
        .unwrap();

    let addr = sender.get_connected_address().clone().unwrap().into();

    // Initialize and start peer connection
    let identity = b"123".to_vec();
    let context = PeerConnectionContextBuilder::new()
        .set_peer_identity(b"dummy-remote-identity".to_vec())
        .set_connection_identity(identity.clone())
        .set_direction(ConnectionDirection::Outbound)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_address(addr)
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::connect(context).unwrap();

    conn.wait_connected_or_failure(Duration::from_millis(5000)).unwrap();

    sender
        .send(&[identity.as_slice(), &[PeerConnectionProtocolMessage::Identify as u8]])
        .unwrap();
    sender
        .send(
            &[identity.as_slice(), &[PeerConnectionProtocolMessage::Message as u8], &[
                123u8,
            ]],
        )
        .unwrap();
    drop(sender);

    conn.wait_disconnected(Duration::from_millis(5000)).unwrap();
}

#[test]
fn connection_stats() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (inbound_tx, _inbound_rx) = mpsc::channel(10);
    let (sender_tx, _sender_rx) = mpsc::channel(10);

    // Connect to the sender (peer)
    let conn_identity = b"conn-ident".to_vec();
    let peer_identity = b"peer-ident".to_vec();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_direction(ConnectionDirection::Inbound)
        .set_context(&ctx)
        .set_message_sink_channel(inbound_tx)
        .set_address(addr.clone())
        .finish()
        .unwrap();

    let (inbound_conn, _) = PeerConnection::listen(context).unwrap();
    inbound_conn
        .wait_listening_or_failure(Duration::from_millis(3000))
        .unwrap();
    assert!(inbound_conn.is_listening());

    inbound_conn
        .allow_identity(conn_identity.clone(), peer_identity.clone())
        .unwrap();

    let initial_stats = inbound_conn.get_connection_stats();

    let outbound_context = PeerConnectionContextBuilder::new()
        .set_context(&ctx)
        .set_connection_identity(conn_identity.clone())
        .set_peer_identity(peer_identity.clone())
        .set_direction(ConnectionDirection::Outbound)
        .set_message_sink_channel(sender_tx)
        .set_address(addr)
        .finish()
        .unwrap();
    let (outbound_conn, _) = PeerConnection::connect(outbound_context).unwrap();

    outbound_conn
        .wait_connected_or_failure(Duration::from_millis(5000))
        .unwrap();

    outbound_conn.send(vec![vec![1u8]]).unwrap();
    outbound_conn.send(vec![vec![2u8]]).unwrap();
    outbound_conn.send(vec![vec![3u8]]).unwrap();
    outbound_conn.send(vec![vec![4u8]]).unwrap();

    let sender = inbound_conn.get_peer_sender(peer_identity.clone()).unwrap();
    sender.send(vec![vec![10u8]]).unwrap();
    sender.send(vec![vec![11u8]]).unwrap();
    sender.send(vec![vec![12u8]]).unwrap();

    // Assert that receive stats update
    assert_change(
        || {
            let stats = inbound_conn.get_connection_stats();
            stats.messages_recv()
        },
        4,
        50,
    );

    assert_change(
        || {
            let stats = inbound_conn.get_connection_stats();
            stats.messages_sent()
        },
        3,
        25,
    );

    let stats = inbound_conn.get_connection_stats();
    assert!(stats.last_activity() > initial_stats.last_activity());
}

#[test]
fn ignore_invalid_message_types() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let (consumer_tx, consumer_rx) = mpsc::channel(10);

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_connection_identity(b"123".to_vec())
        .set_direction(ConnectionDirection::Inbound)
        .set_context(&ctx)
        .set_message_sink_channel(consumer_tx)
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .finish()
        .unwrap();

    let (conn, _) = PeerConnection::listen(context).unwrap();
    conn.wait_listening_or_failure(Duration::from_millis(1000)).unwrap();

    // Connect to the inbound connection and send a message
    let sender = Connection::new(&ctx, ConnectionDirection::Outbound)
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .establish(&addr)
        .unwrap();

    assert!(!conn.is_connected());
    sender
        .send(&[&[PeerConnectionProtocolMessage::Identify as u8]])
        .unwrap();
    // Send invalid peer connection message type
    sender.send(&[&[255], &[1u8]]).unwrap();
    sender
        .send(&[&[PeerConnectionProtocolMessage::Message as u8], &[1u8]])
        .unwrap();

    let result = stream_assert_count(consumer_rx, 2, 500);
    assert!(result.is_err());
}

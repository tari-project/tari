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
    helpers::asserts::assert_change,
};
use std::time::Duration;
use tari_comms::connection::{
    peer_connection::PeerConnectionProtoMessage,
    types::{Direction, Linger},
    Connection,
    ConnectionError,
    CurveEncryption,
    InprocAddress,
    NetAddress,
    PeerConnection,
    PeerConnectionContextBuilder,
    PeerConnectionError,
    ZmqContext,
};

#[test]
fn connection_in() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let consumer_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();
    conn.start(context).unwrap();
    conn.wait_listening_or_failure(&Duration::from_millis(1000)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .establish(&consumer_addr)
        .unwrap();

    // Connect to the inbound connection and send a message
    let sender = Connection::new(&ctx, Direction::Outbound)
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .establish(&addr)
        .unwrap();
    sender.send(&[&[PeerConnectionProtoMessage::Identify as u8]]).unwrap();
    sender
        .send(&[&[PeerConnectionProtoMessage::Message as u8], &[1u8]])
        .unwrap();

    // Receive the message from the consumer socket
    let frames = consumer.receive(2000).unwrap();
    assert_eq!("123".as_bytes().to_vec(), frames[1]);
    assert_eq!(vec![1u8], frames[2]);

    conn.send(vec![vec![111u8]]).unwrap();

    let reply = sender.receive(100).unwrap();
    assert_eq!(
        vec![vec![PeerConnectionProtoMessage::Message as u8], vec![111u8]],
        reply
    );
}

#[test]
fn connection_out() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let consumer_addr = InprocAddress::random();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, Direction::Inbound)
        .set_name("Test sender")
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .establish(&addr)
        .unwrap();

    let conn_id = "123".as_bytes();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id(conn_id.clone())
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();
    conn.wait_connected_or_failure(&Duration::from_millis(2000)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .set_name("Test message sink")
        .establish(&consumer_addr)
        .unwrap();

    conn.send(vec![vec![123u8]]).unwrap();

    let ident = sender.receive(2000).unwrap();
    assert_eq!(vec![PeerConnectionProtoMessage::Identify as u8], ident[1]);
    let data = sender.receive(2000).unwrap();
    assert_eq!(vec![123u8], data[2]);

    sender
        .send(&[data[0].as_slice(), &[PeerConnectionProtoMessage::Message as u8], &[
            123u8,
        ]])
        .unwrap();
    let frames = consumer.receive(2000).unwrap();
    assert_eq!(conn_id.to_vec(), frames[1]);
    assert_eq!(vec![123u8], frames[2]);
}

#[test]
fn connection_wait_connect_shutdown() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let receiver = Connection::new(&ctx, Direction::Inbound).establish(&addr).unwrap();

    let consumer_addr = InprocAddress::random();

    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    conn.wait_connected_or_failure(&Duration::from_millis(2000)).unwrap();

    conn.shutdown().unwrap();

    assert!(
        conn.wait_disconnected(&Duration::from_millis(2000)).is_ok(),
        "Failed to shut down in 100ms"
    );

    drop(receiver);
}

#[test]
fn connection_wait_connect_failed() {
    let addr = factories::net_address::create().use_os_port().build().unwrap();
    let ctx = ZmqContext::new();

    let consumer_addr = InprocAddress::random();

    // This has nothing to connect to
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Outbound)
        .set_max_retry_attempts(1)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    let err = conn
        .wait_connected_or_failure(&Duration::from_millis(2000))
        .unwrap_err();

    assert!(conn.is_failed());
    match err {
        ConnectionError::PeerError(err) => match err {
            PeerConnectionError::ExceededMaxConnectRetryCount => {},
            _ => panic!("Unexpected connection error '{}'", err),
        },
        _ => panic!("Unexpected connection error '{}'", err),
    }
}

#[test]
fn connection_pause_resume() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let consumer_addr = InprocAddress::random();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, Direction::Outbound)
        .set_linger(Linger::Indefinitely)
        .establish(&addr)
        .unwrap();
    let conn_id = "123".as_bytes();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id(conn_id.clone())
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    conn.wait_listening_or_failure(&Duration::from_millis(2000)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .establish(&consumer_addr)
        .unwrap();

    let msg_type_frame = &[PeerConnectionProtoMessage::Message as u8];
    sender.send(&[&[PeerConnectionProtoMessage::Identify as u8]]).unwrap();
    sender.send(&[msg_type_frame, &[1u8]]).unwrap();

    let frames = consumer.receive(2000).unwrap();
    assert_eq!(conn_id.to_vec(), frames[1]);
    assert_eq!(vec![1u8], frames[2]);

    // Pause the connection
    conn.pause().unwrap();

    sender.send(&[msg_type_frame, &[2u8]]).unwrap();
    sender.send(&[msg_type_frame, &[3u8]]).unwrap();
    sender.send(&[msg_type_frame, &[4u8]]).unwrap();

    let err = consumer.receive(3000).unwrap_err();
    assert!(err.is_timeout());

    // Resume connection
    conn.resume().unwrap();

    // Should receive all the pending messages
    let frames = consumer.receive(3000).unwrap();
    assert_eq!(vec![2u8], frames[2]);
    let frames = consumer.receive(3000).unwrap();
    assert_eq!(vec![3u8], frames[2]);
    let frames = consumer.receive(3000).unwrap();
    assert_eq!(vec![4u8], frames[2]);
}

#[test]
fn connection_disconnect() {
    let addr = factories::net_address::create().use_os_port().build().unwrap();
    let ctx = ZmqContext::new();

    let consumer_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();
    conn.start(context).unwrap();
    conn.wait_listening_or_failure(&Duration::from_millis(1000)).unwrap();
    let addr = NetAddress::from(conn.get_connected_address().unwrap());

    {
        // Connect to the inbound connection and send a message
        let sender = Connection::new(&ctx, Direction::Outbound)
            .set_linger(Linger::Indefinitely)
            .establish(&addr)
            .unwrap();
        sender.send(&[&[PeerConnectionProtoMessage::Identify as u8]]).unwrap();
        sender
            .send(&[&[PeerConnectionProtoMessage::Message as u8], &[123u8]])
            .unwrap();
    }

    conn.wait_disconnected(&Duration::from_millis(2000)).unwrap();
}

#[test]
fn connection_stats() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let consumer_addr = InprocAddress::random();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, Direction::Outbound)
        .set_linger(Linger::Indefinitely)
        .establish(&addr)
        .unwrap();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123".as_bytes())
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    let initial_stats = conn.connection_stats();
    let msg_type_frame = &[PeerConnectionProtoMessage::Message as u8];

    sender.send(&[&[PeerConnectionProtoMessage::Identify as u8]]).unwrap();
    sender.send(&[msg_type_frame, &[1u8]]).unwrap();
    sender.send(&[msg_type_frame, &[2u8]]).unwrap();
    sender.send(&[msg_type_frame, &[3u8]]).unwrap();
    sender.send(&[msg_type_frame, &[4u8]]).unwrap();

    conn.wait_connected_or_failure(&Duration::from_millis(2000)).unwrap();

    conn.send(vec![vec![10u8]]).unwrap();
    conn.send(vec![vec![11u8]]).unwrap();
    conn.send(vec![vec![12u8]]).unwrap();

    // Assert that receive stats update
    assert_change(
        || {
            let stats = conn.connection_stats();
            stats.messages_recv()
        },
        4,
        40,
    );

    assert_change(
        || {
            let stats = conn.connection_stats();
            stats.messages_sent()
        },
        3,
        20,
    );

    let stats = conn.connection_stats();
    assert!(stats.last_activity() > initial_stats.last_activity());
}

#[test]
fn ignore_invalid_message_types() {
    let addr = factories::net_address::create().build().unwrap();
    let ctx = ZmqContext::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let consumer_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_message_sink_address(consumer_addr.clone())
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();
    conn.start(context).unwrap();
    conn.wait_listening_or_failure(&Duration::from_millis(1000)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .establish(&consumer_addr)
        .unwrap();

    // Connect to the inbound connection and send a message
    let sender = Connection::new(&ctx, Direction::Outbound)
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .establish(&addr)
        .unwrap();

    assert!(!conn.is_connected());
    sender.send(&[&[PeerConnectionProtoMessage::Identify as u8]]).unwrap();
    assert_change(|| conn.is_connected(), true, 10);
    // Send invalid peer connection message type
    sender.send(&[&[255], &[1u8]]).unwrap();
    sender
        .send(&[&[PeerConnectionProtoMessage::Message as u8], &[1u8]])
        .unwrap();

    // Receive the message from the consumer socket
    let frames = consumer.receive(2000).unwrap();
    assert_eq!("123".as_bytes().to_vec(), frames[1]);
    assert_eq!(vec![1u8], frames[2]);

    // Test no more messages to receive. Since we have received above, the invalid message
    // should be already ready to receive (10ms) if it was forwarded by the peer connection.
    assert!(consumer.receive(10).is_err());
}

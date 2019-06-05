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

use std::{str::FromStr, time::Duration};
use tari_comms::connection::{
    Connection,
    ConnectionError,
    Context,
    CurveEncryption,
    Direction,
    InprocAddress,
    Linger,
    NetAddress,
    PeerConnection,
    PeerConnectionContextBuilder,
    PeerConnectionError,
};

#[test]
fn connection_in() {
    let addr = NetAddress::from_str("127.0.0.1:9888").unwrap();
    let ctx = Context::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let consumer_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_consumer_address(consumer_addr.clone())
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let conn = PeerConnection::new();
    conn.start(context).unwrap();
    conn.wait_connected_or_failure(Duration::from_millis(1000)).unwrap();

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
    sender.send(&[&[1u8]]).unwrap();

    // Receive the message from the consumer socket
    let frames = consumer.receive(2000).unwrap();
    assert_eq!("123".as_bytes().to_vec(), frames[1]);
    assert_eq!(vec![1u8], frames[2]);

    conn.send(vec![vec![111u8]]).unwrap();

    let reply = sender.receive(100).unwrap();
    assert_eq!(vec![vec![111u8]], reply);
}

#[test]
fn connection_out() {
    let addr = NetAddress::from_str("127.0.0.1:9884").unwrap();
    let ctx = Context::new();

    let (server_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let (client_sk, client_pk) = CurveEncryption::generate_keypair().unwrap();

    let consumer_addr = InprocAddress::random();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, Direction::Inbound)
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .establish(&addr)
        .unwrap();

    let conn_id = "123".as_bytes();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id(conn_id.clone())
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_consumer_address(consumer_addr.clone())
        .set_curve_encryption(CurveEncryption::Client {
            server_public_key: server_pk,
            secret_key: client_sk,
            public_key: client_pk,
        })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();
    conn.wait_connected_or_failure(Duration::from_millis(100)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .establish(&consumer_addr)
        .unwrap();

    conn.send(vec![vec![123u8]]).unwrap();

    let data = sender.receive(2000).unwrap();
    assert_eq!(vec![123u8], data[1]);
    sender.send(&[data[0].as_slice(), &[123u8]]).unwrap();

    let frames = consumer.receive(100).unwrap();
    assert_eq!(conn_id.to_vec(), frames[1]);
    assert_eq!(vec![123u8], frames[2]);
}

#[test]
fn connection_wait_connect_shutdown() {
    let addr = NetAddress::from_str("127.0.0.1:10100").unwrap();
    let ctx = Context::new();

    let receiver = Connection::new(&ctx, Direction::Inbound).establish(&addr).unwrap();

    let consumer_addr = InprocAddress::random();

    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_consumer_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    conn.wait_connected_or_failure(Duration::from_millis(100)).unwrap();

    conn.shutdown().unwrap();

    assert!(
        conn.wait_disconnected(Duration::from_millis(100)).is_ok(),
        "Failed to shut down in 100ms"
    );

    drop(receiver);
}

#[test]
fn connection_wait_connect_failed() {
    let addr = NetAddress::from_str("127.0.0.1:0").unwrap();
    let ctx = Context::new();

    let consumer_addr = InprocAddress::random();

    // This has nothing to connect to
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Outbound)
        .set_max_retry_attempts(1)
        .set_context(&ctx)
        .set_consumer_address(consumer_addr.clone())
        .set_address(addr.clone())
        .build()
        .unwrap();

    let conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    let err = conn.wait_connected_or_failure(Duration::from_millis(2000)).unwrap_err();

    assert!(conn.is_failed());
    match err {
        ConnectionError::PeerError(err) => match err {
            PeerConnectionError::ConnectFailed => {},
            _ => panic!("Unexpected connection error '{}'", err),
        },
        _ => panic!("Unexpected connection error '{}'", err),
    }
}

#[test]
fn connection_pause_resume() {
    let addr = NetAddress::from_str("127.0.0.1:9873").unwrap();
    let ctx = Context::new();

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
        .set_consumer_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();

    conn.wait_connected_or_failure(Duration::from_millis(100)).unwrap();

    // Connect the message consumer
    let consumer = Connection::new(&ctx, Direction::Inbound)
        .establish(&consumer_addr)
        .unwrap();

    sender.send(&[&[1u8]]).unwrap();

    let frames = consumer.receive(200).unwrap();
    assert_eq!(conn_id.to_vec(), frames[1]);
    assert_eq!(vec![1u8], frames[2]);

    // Pause the connection
    conn.pause().unwrap();

    sender.send(&[&[2u8]]).unwrap();
    sender.send(&[&[3u8]]).unwrap();
    sender.send(&[&[4u8]]).unwrap();

    let err = consumer.receive(100).unwrap_err();
    assert!(err.is_timeout());

    // Resume connection
    conn.resume().unwrap();

    // Should receive all the pending messages
    let frames = consumer.receive(100).unwrap();
    assert_eq!(vec![2u8], frames[2]);
    let frames = consumer.receive(100).unwrap();
    assert_eq!(vec![3u8], frames[2]);
    let frames = consumer.receive(100).unwrap();
    assert_eq!(vec![4u8], frames[2]);
}

#[test]
fn connection_disconnect() {
    let addr = NetAddress::from_str("127.0.0.1:0").unwrap();
    let ctx = Context::new();

    let consumer_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_id("123")
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_consumer_address(consumer_addr.clone())
        .set_address(addr)
        .build()
        .unwrap();

    let conn = PeerConnection::new();
    conn.start(context).unwrap();
    conn.wait_connected_or_failure(Duration::from_millis(1000)).unwrap();
    let addr = NetAddress::from(conn.get_connected_address().unwrap());

    {
        // Connect to the inbound connection and send a message
        let sender = Connection::new(&ctx, Direction::Outbound)
            .set_linger(Linger::Indefinitely)
            .establish(&addr)
            .unwrap();
        sender.send(&[&[123u8]]).unwrap();
    }

    conn.wait_disconnected(Duration::from_millis(2000)).unwrap();
}

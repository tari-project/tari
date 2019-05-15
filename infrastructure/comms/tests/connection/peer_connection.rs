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

use crate::support::utils::find_available_tcp_net_address;
use tari_comms::connection::{
    connection::Connection,
    peer_connection::PeerConnection,
    types::Direction,
    zmq::{curve_keypair, Context, CurveEncryption, InprocAddress},
    PeerConnectionContextBuilder,
};

#[test]
fn connection_in() {
    let addr = find_available_tcp_net_address("127.0.0.1").unwrap();
    let ctx = Context::new();

    let (server_sk, server_pk) = curve_keypair::generate().unwrap();
    let (client_sk, client_pk) = curve_keypair::generate().unwrap();

    let receiver_addr = InprocAddress::random();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_direction(Direction::Inbound)
        .set_context(&ctx)
        .set_consumer_address(receiver_addr.clone())
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();
    conn.start(context).unwrap();

    // Connect the message receiver
    let receiver = Connection::new(&ctx, Direction::Inbound)
        .establish(&receiver_addr)
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
    sender.send(&[&[123u8]]).unwrap();

    // Receive the message from the receiver socket
    let frames = receiver.receive(2000).unwrap();
    assert_eq!(vec![123u8], frames[1]);

    conn.send(vec![vec![111u8]]).unwrap();

    let reply = sender.receive(100).unwrap();
    assert_eq!(vec![vec![111u8]], reply);
}

#[test]
fn connection_out() {
    let addr = find_available_tcp_net_address("127.0.0.1").unwrap();
    let ctx = Context::new();

    let (server_sk, server_pk) = curve_keypair::generate().unwrap();
    let (client_sk, client_pk) = curve_keypair::generate().unwrap();

    let receiver_addr = InprocAddress::random();

    // Connect to the sender (peer)
    let sender = Connection::new(&ctx, Direction::Inbound)
        .set_curve_encryption(CurveEncryption::Server { secret_key: server_sk })
        .establish(&addr)
        .unwrap();

    // Initialize and start peer connection
    let context = PeerConnectionContextBuilder::new()
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_consumer_address(receiver_addr.clone())
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
    assert!(conn.is_connected());

    // Connect the message receiver
    let receiver = Connection::new(&ctx, Direction::Inbound)
        .establish(&receiver_addr)
        .unwrap();

    conn.send(vec![vec![123u8]]).unwrap();

    let data = sender.receive(2000).unwrap();
    assert_eq!(data[1], vec![123u8]);
    sender.send(&[data[0].as_slice(), &[123u8]]).unwrap();

    let frames = receiver.receive(100).unwrap();
    assert_eq!(vec![123u8], frames[1]);
}

#[test]
fn connection_shutdown() {
    let addr = find_available_tcp_net_address("127.0.0.1").unwrap();
    let ctx = Context::new();

    let receiver_addr = InprocAddress::random();

    let context = PeerConnectionContextBuilder::new()
        .set_direction(Direction::Outbound)
        .set_context(&ctx)
        .set_consumer_address(receiver_addr.clone())
        .set_address(addr.clone())
        .build()
        .unwrap();

    let mut conn = PeerConnection::new();

    assert!(!conn.is_connected());
    conn.start(context).unwrap();
    assert!(conn.is_connected());
}

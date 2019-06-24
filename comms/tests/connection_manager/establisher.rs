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

//    use tari_comms::
use crate::support::{
    factories::{self, Factory},
    helpers::ConnectionMessageCounter,
};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{Connection, Context, CurveEncryption, Direction, InprocAddress, NetAddress},
    connection_manager::{ConnectionManagerError, PeerConnectionConfig},
};

fn make_peer_connection_config(context: &Context, consumer_address: InprocAddress) -> PeerConnectionConfig {
    PeerConnectionConfig {
        context: context.clone(),
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
        consumer_address,
        socks_proxy_address: None,
    }
}

#[test]
fn establish_control_service_connection_fail() {
    let context = Context::new();
    let peers = factories::peer::create_many(2).build().unwrap();
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_peers(peers.clone())
            .build()
            .unwrap(),
    );
    let config = make_peer_connection_config(&context, InprocAddress::random());

    let example_peer = &peers[0];

    let establisher = ConnectionEstablisher::new(config, peer_manager);
    let result = establisher.establish_control_service_connection(example_peer);

    match result {
        Ok(_) => panic!("Unexpected success result"),
        Err(ConnectionManagerError::MaxConnnectionAttemptsExceeded) => {},
        Err(err) => panic!("Unexpected error type: {:?}", err),
    }
}

#[test]
fn establish_control_service_connection_succeed() {
    let context = Context::new();
    let address = factories::net_address::create().use_os_port().build().unwrap();

    // Setup a connection to act as an endpoint for a peers control service
    let dummy_conn = Connection::new(&context, Direction::Inbound)
        .establish(&address)
        .unwrap();

    let address: NetAddress = dummy_conn.get_connected_address().clone().unwrap().into();

    let example_peer = factories::peer::create()
        .with_net_addresses(vec![address])
        .build()
        .unwrap();

    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    let config = make_peer_connection_config(&context, InprocAddress::random());
    let establisher = ConnectionEstablisher::new(config, peer_manager);
    establisher.establish_control_service_connection(&example_peer).unwrap();
}

#[test]
fn establish_peer_connection_outbound() {
    let context = Context::new();
    let consumer_address = InprocAddress::random();

    // Setup a message counter to count the number of messages sent to the consumer address
    let msg_counter = ConnectionMessageCounter::new(&context);
    msg_counter.start(consumer_address.clone());

    // Setup a peer connection
    let (other_peer_conn, _, peer_curve_pk) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_consumer_address(consumer_address.clone())
                .with_context(&context)
                .with_direction(Direction::Inbound),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_listening_or_failure(&Duration::from_millis(200))
        .unwrap();

    let address: NetAddress = other_peer_conn.get_connected_address().unwrap().into();

    let example_peer = factories::peer::create()
        .with_net_addresses(vec![address.clone()])
        .build()
        .unwrap();

    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    let config = make_peer_connection_config(&context, InprocAddress::random());
    let establisher = ConnectionEstablisher::new(config, peer_manager);
    let (entry, peer_conn_handle) = establisher
        .establish_outbound_peer_connection(&example_peer, &address, peer_curve_pk)
        .unwrap();

    entry.connection.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    entry.connection.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

    entry.connection.shutdown().unwrap();
    entry
        .connection
        .wait_disconnected(&Duration::from_millis(1000))
        .unwrap();

    assert_eq!(msg_counter.count(), 2);

    peer_conn_handle.join().unwrap().unwrap();
}

#[test]
fn establish_peer_connection_inbound() {
    let context = Context::new();
    let consumer_address = InprocAddress::random();

    let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();

    let example_peer = factories::peer::create().build().unwrap();

    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    // Setup a message counter to count the number of messages sent to the consumer address
    let msg_counter = ConnectionMessageCounter::new(&context);
    msg_counter.start(consumer_address.clone());

    // Create a connection establisher
    let config = make_peer_connection_config(&context, consumer_address.clone());
    let establisher = ConnectionEstablisher::new(config, peer_manager);
    let (entry, peer_conn_handle) = establisher
        .establish_inbound_peer_connection(&example_peer, secret_key)
        .unwrap();

    entry
        .connection
        .wait_listening_or_failure(&Duration::from_millis(2000))
        .unwrap();
    let address: NetAddress = entry.connection.get_connected_address().unwrap().into();

    // Setup a peer connection which will connect to our established inbound peer connection
    let (other_peer_conn, _, _) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_context(&context)
                .with_address(address)
                .with_server_public_key(public_key.clone())
                .with_direction(Direction::Outbound),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_connected_or_failure(&Duration::from_millis(2000))
        .unwrap();
    // Start sending messages
    other_peer_conn.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    other_peer_conn.send(vec!["TARI".as_bytes().to_vec()]).unwrap();
    let _ = other_peer_conn.shutdown();
    other_peer_conn.wait_disconnected(&Duration::from_millis(1000)).unwrap();

    assert_eq!(msg_counter.count(), 2);

    peer_conn_handle.join().unwrap().unwrap();
}

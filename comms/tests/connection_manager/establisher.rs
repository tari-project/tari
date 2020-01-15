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
    helpers::{streams::stream_assert_count, ConnectionMessageCounter},
};
use futures::channel::mpsc::channel;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    connection::{ConnectionDirection, CurveEncryption, ZmqContext},
    connection_manager::{
        deprecated::establisher::ConnectionEstablisher,
        ConnectionManagerError,
        PeerConnectionConfig,
    },
    control_service::messages::{MessageHeader, MessageType, PongMessage},
    message::{Envelope, MessageExt, MessageFlags},
    utils::{crypt, multiaddr::socketaddr_to_multiaddr},
    wrap_in_envelope_body,
};
use tari_utilities::{thread_join::ThreadJoinWithTimeout, ByteArray};

fn make_peer_connection_config() -> PeerConnectionConfig {
    PeerConnectionConfig {
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        max_connections: 10,
        listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        max_connect_retries: 3,
        socks_proxy_address: None,
    }
}

// This tries to break the establisher by sending malformed messages. The establisher should
// disregard the malformed message and continue to try other addresses. Once all
// addresses fail, the correct error should be returned.
#[test]
fn establish_control_service_connection_fail() {
    let context = ZmqContext::new();

    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();

    let peers = factories::peer::create_many(2)
        .with_factory(factories::peer::create().with_net_addresses_factory(factories::net_address::create_many(2)))
        .build()
        .unwrap();

    // Setup a connection counter to act as a 'junk' endpoint for a peers control service.
    let mut msg_counter1 = ConnectionMessageCounter::new(&context);
    msg_counter1.set_response(vec!["JUNK".as_bytes().to_vec()]);
    msg_counter1.start(peers[0].addresses[0].net_address.clone());

    let mut msg_counter2 = ConnectionMessageCounter::new(&context);
    msg_counter2.set_response(vec!["JUNK".as_bytes().to_vec()]);
    msg_counter2.start(peers[0].addresses[1].net_address.clone());

    let (tx, _rx) = channel(10);
    let config = make_peer_connection_config();

    let example_peer = &peers[0];

    let establisher = ConnectionEstablisher::new(context.clone(), node_identity, config, tx);
    match establisher.connect_control_service_client(example_peer) {
        Ok(_) => panic!("Unexpected success result"),
        Err(ConnectionManagerError::ControlServiceFailedConnectionAllAddresses) => {},
        Err(err) => panic!("Unexpected error type: {:?}", err),
    }

    msg_counter1.assert_count(1, 20);
    msg_counter2.assert_count(1, 20);
}

#[test]
fn establish_control_service_connection_succeed() {
    let context = ZmqContext::new();
    let address = factories::net_address::create().build().unwrap();
    // The node attempting to connect
    let node_identity1 = factories::node_identity::create().build().map(Arc::new).unwrap();
    // The node being connected to
    let node_identity2 = factories::node_identity::create().build().map(Arc::new).unwrap();

    let example_peer = factories::peer::create()
        .with_public_key(node_identity2.public_key().clone())
        .with_net_addresses(vec![address])
        .build()
        .unwrap();

    // Setup a connection counter to act as a control service sending back a pong
    let pong_response = {
        let body = wrap_in_envelope_body!(MessageHeader::new(MessageType::Pong), PongMessage {})
            .unwrap()
            .to_encoded_bytes()
            .unwrap();

        let shared_secret = crypt::generate_ecdh_secret(node_identity2.secret_key(), node_identity1.public_key());
        let encrypted_body = crypt::encrypt(&shared_secret, &body).unwrap();

        let envelope = Envelope::construct_signed(
            node_identity1.secret_key(),
            node_identity1.public_key(),
            encrypted_body,
            MessageFlags::ENCRYPTED,
        )
        .unwrap();
        envelope.to_encoded_bytes().unwrap()
    };

    let mut msg_counter1 = ConnectionMessageCounter::new(&context);
    msg_counter1.set_response(vec![pong_response]);

    let address = example_peer.addresses[0].net_address.clone();
    msg_counter1.start(address);

    let (tx, _rx) = channel(10);
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity1, config, tx);
    let client = establisher.connect_control_service_client(&example_peer).unwrap();
    client.ping_pong(Duration::from_millis(3000)).unwrap();

    msg_counter1.assert_count(2, 20);
}

#[test]
fn establish_peer_connection_outbound() {
    let context = ZmqContext::new();
    let node_identity_in = factories::node_identity::create().build().map(Arc::new).unwrap();
    let node_identity_out = factories::node_identity::create().build().map(Arc::new).unwrap();
    let (tx_inbound, rx_inbound) = channel(10);
    // Setup a peer connection
    let (peer_curve_sk, peer_curve_pk) = CurveEncryption::generate_keypair().unwrap();
    let (other_peer_conn, other_peer_conn_handle) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_context(&context)
                .with_direction(ConnectionDirection::Inbound)
                .with_message_sink_channel(tx_inbound)
                .with_curve_keypair((peer_curve_sk, peer_curve_pk.clone())),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_listening_or_failure(Duration::from_millis(2000))
        .unwrap();

    let address = other_peer_conn
        .get_address()
        .as_ref()
        .map(socketaddr_to_multiaddr)
        .unwrap();
    assert_ne!(address.to_string(), "127.0.0.1:0");

    let remote_peer = factories::peer::create()
        .with_net_addresses(vec![address.clone()])
        .with_public_key(node_identity_in.public_key().clone())
        .with_node_id(node_identity_in.node_id().clone())
        .build()
        .unwrap();

    other_peer_conn
        .allow_identity(
            node_identity_out.node_id().to_vec(),
            node_identity_out.node_id().to_vec(),
        )
        .unwrap();

    let (tx_outbound2, _rx_outbound) = channel(10);
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity_out.clone(), config, tx_outbound2);
    let (connection, peer_conn_handle) = establisher
        .establish_outbound_peer_connection(
            address,
            peer_curve_pk,
            node_identity_out.node_id().to_vec(),
            remote_peer.node_id.to_vec(),
        )
        .unwrap();
    connection.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    connection.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

    connection.shutdown().unwrap();
    connection.wait_disconnected(Duration::from_millis(3000)).unwrap();

    other_peer_conn.shutdown().unwrap();
    other_peer_conn_handle
        .timeout_join(Duration::from_millis(3000))
        .unwrap();

    let (_, _messages) = stream_assert_count(rx_inbound, 2, 500).unwrap();

    peer_conn_handle.timeout_join(Duration::from_millis(3000)).unwrap();
}

#[test]
fn establish_peer_connection_inbound() {
    let context = ZmqContext::new();
    let node_identity = factories::node_identity::create().build().map(Arc::new).unwrap();

    let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();

    let (tx, rx) = channel(10);
    // Create a connection establisher
    let config = make_peer_connection_config();
    let establisher = ConnectionEstablisher::new(context.clone(), node_identity.clone(), config, tx);
    let (connection, peer_conn_handle) = establisher.establish_peer_listening_connection(secret_key).unwrap();
    let peer_identity = b"peer-identity".to_vec();
    let connection_identity = b"conn-identity".to_vec();
    connection
        .allow_identity(connection_identity.clone(), peer_identity.clone())
        .unwrap();

    connection
        .wait_listening_or_failure(Duration::from_millis(3000))
        .unwrap();

    let address = connection.get_address().as_ref().map(socketaddr_to_multiaddr).unwrap();

    // Setup a peer connection which will connect to our established inbound peer connection
    let (other_tx, _other_rx) = channel(10);
    let (other_peer_conn, other_peer_conn_handle) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_direction(ConnectionDirection::Outbound)
                .with_peer_identity(peer_identity.clone())
                .with_connection_identity(connection_identity.clone())
                .with_context(&context)
                .with_server_public_key(public_key.clone())
                .with_message_sink_channel(other_tx)
                .with_address(address),
        )
        .build()
        .unwrap();

    other_peer_conn
        .wait_connected_or_failure(Duration::from_millis(3000))
        .unwrap();

    // Start sending messages
    other_peer_conn.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    other_peer_conn.send(vec!["TARI".as_bytes().to_vec()]).unwrap();
    other_peer_conn.shutdown().unwrap();
    other_peer_conn.wait_disconnected(Duration::from_millis(3000)).unwrap();

    let (_arc_rx, _items) = stream_assert_count(rx, 2, 500).unwrap();

    connection.shutdown().unwrap();
    peer_conn_handle.timeout_join(Duration::from_millis(3000)).unwrap();
    other_peer_conn_handle
        .timeout_join(Duration::from_millis(3000))
        .unwrap();
}

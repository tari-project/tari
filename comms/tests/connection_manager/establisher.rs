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
    helpers::ConnectionMessageCounter,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tari_comms::{
    connection::{Connection, CurveEncryption, Direction, InprocAddress, NetAddress, ZmqContext},
    connection_manager::{establisher::ConnectionEstablisher, ConnectionManagerError, PeerConnectionConfig},
};
use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

fn make_peer_connection_config(message_sink_address: InprocAddress) -> PeerConnectionConfig {
    PeerConnectionConfig {
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
        message_sink_address,
        socks_proxy_address: None,
    }
}

fn get_path(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name);
    path.to_str().unwrap().to_string()
}

fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
    let path = get_path(name);
    let _ = std::fs::create_dir(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_environment_size(10)
        .set_max_number_of_databases(2)
        .add_database(name, lmdb_zero::db::CREATE)
        .build()
}

fn clean_up_datastore(name: &str) {
    std::fs::remove_dir_all(get_path(name)).unwrap();
}

#[test]
fn establish_control_service_connection_fail() {
    let context = ZmqContext::new();
    let peers = factories::peer::create_many(2).build().unwrap();
    let database_name = "establisher_establish_control_service_connection_fail"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(peers.clone())
            .build()
            .unwrap(),
    );
    let config = make_peer_connection_config(InprocAddress::random());

    let example_peer = &peers[0];

    let establisher = ConnectionEstablisher::new(context, config, peer_manager);
    let result = establisher.establish_control_service_connection(example_peer);

    match result {
        Ok(_) => panic!("Unexpected success result"),
        Err(ConnectionManagerError::MaxConnnectionAttemptsExceeded) => {},
        Err(err) => panic!("Unexpected error type: {:?}", err),
    }

    clean_up_datastore(database_name);
}

#[test]
fn establish_control_service_connection_succeed() {
    let context = ZmqContext::new();
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

    let database_name = "establisher_establish_control_service_connection_succeed"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    let config = make_peer_connection_config(InprocAddress::random());
    let establisher = ConnectionEstablisher::new(context, config, peer_manager);
    establisher.establish_control_service_connection(&example_peer).unwrap();

    clean_up_datastore(database_name);
}

#[test]
fn establish_peer_connection_outbound() {
    let context = ZmqContext::new();
    let msg_sink_address = InprocAddress::random();

    // Setup a message counter to count the number of messages sent to the consumer address
    let msg_counter = ConnectionMessageCounter::new(&context);
    msg_counter.start(msg_sink_address.clone());

    // Setup a peer connection
    let (other_peer_conn, _, peer_curve_pk) = factories::peer_connection::create()
        .with_peer_connection_context_factory(
            factories::peer_connection_context::create()
                .with_message_sink_address(msg_sink_address.clone())
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

    let database_name = "establisher_establish_peer_connection_outbound"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    let config = make_peer_connection_config(InprocAddress::random());
    let establisher = ConnectionEstablisher::new(context.clone(), config, peer_manager);
    let (connection, peer_conn_handle) = establisher
        .establish_outbound_peer_connection(example_peer.node_id.clone().into(), address, peer_curve_pk)
        .unwrap();

    connection.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
    connection.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

    connection.shutdown().unwrap();
    connection.wait_disconnected(&Duration::from_millis(1000)).unwrap();

    assert_eq!(msg_counter.count(), 2);

    peer_conn_handle.timeout_join(Duration::from_millis(100)).unwrap();

    clean_up_datastore(database_name);
}

#[test]
fn establish_peer_connection_inbound() {
    let context = ZmqContext::new();
    let msg_sink_address = InprocAddress::random();

    let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();

    let example_peer = factories::peer::create().build().unwrap();

    let database_name = "establish_peer_connection_inbound"; // Note: every test should have unique database
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let peer_manager = Arc::new(
        factories::peer_manager::create()
            .with_database(database)
            .with_peers(vec![example_peer.clone()])
            .build()
            .unwrap(),
    );

    // Setup a message counter to count the number of messages sent to the consumer address
    let msg_counter = ConnectionMessageCounter::new(&context);
    msg_counter.start(msg_sink_address.clone());

    // Create a connection establisher
    let config = make_peer_connection_config(msg_sink_address.clone());
    let establisher = ConnectionEstablisher::new(context.clone(), config, peer_manager);
    let (connection, peer_conn_handle) = establisher
        .establish_inbound_peer_connection(example_peer.node_id.clone().into(), secret_key)
        .unwrap();

    connection
        .wait_listening_or_failure(&Duration::from_millis(2000))
        .unwrap();
    let address: NetAddress = connection.get_connected_address().unwrap().into();

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

    peer_conn_handle.timeout_join(Duration::from_millis(100)).unwrap();

    clean_up_datastore(database_name);
}

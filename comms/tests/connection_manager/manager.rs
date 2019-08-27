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
use std::{path::PathBuf, sync::Arc, thread, time::Duration};
use tari_comms::{
    connection::{types::Linger, InprocAddress, ZmqContext},
    connection_manager::PeerConnectionConfig,
    control_service::{ControlService, ControlServiceConfig},
    peer_manager::{Peer, PeerManager},
    types::CommsDatabase,
};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBError, LMDBStore},
    LMDBWrapper,
};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

fn make_peer_connection_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
    PeerConnectionConfig {
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        max_connections: 10,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 5,
        message_sink_address: consumer_address,
        socks_proxy_address: None,
    }
}

fn make_peer_manager(peers: Vec<Peer>, database: CommsDatabase) -> Arc<PeerManager> {
    Arc::new(
        factories::peer_manager::create()
            .with_peers(peers)
            .with_database(database)
            .build()
            .unwrap(),
    )
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

fn pause() {
    thread::sleep(Duration::from_millis(200));
}

#[test]
#[allow(non_snake_case)]
fn establish_peer_connection() {
    let context = ZmqContext::new();

    let node_A_identity = Arc::new(factories::node_identity::create().build().unwrap());

    let node_B_consumer_address = InprocAddress::random();
    let node_B_msg_counter = ConnectionMessageCounter::new(&context);
    node_B_msg_counter.start(node_B_consumer_address.clone());

    //---------------------------------- Node B Setup --------------------------------------------//

    let node_B_control_port_address = factories::net_address::create().build().unwrap();
    let node_B_identity = Arc::new(
        factories::node_identity::create()
            .with_control_service_address(node_B_control_port_address.clone())
            .build()
            .unwrap(),
    );

    let node_B_peer = factories::peer::create()
        .with_net_addresses(vec![node_B_control_port_address.clone()])
        .with_public_key(node_B_identity.identity.public_key.clone())
        .build()
        .unwrap();

    // Node B knows no peers
    let node_B_database_name = "connection_manager_node_B_peer_manager";
    let datastore = init_datastore(node_B_database_name).unwrap();
    let database = datastore.get_handle(node_B_database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let node_B_peer_manager = make_peer_manager(vec![], database);
    let node_B_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .with_node_identity(node_B_identity.clone())
            .with_peer_manager(node_B_peer_manager)
            .with_peer_connection_config(make_peer_connection_config(node_B_consumer_address.clone()))
            .build()
            .unwrap(),
    );

    // Start node B's control service
    let node_B_control_service = ControlService::new(context.clone(), node_B_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: node_B_control_port_address,
        requested_connection_timeout: Duration::from_millis(5000),
    })
    .serve(node_B_connection_manager)
    .unwrap();

    // Give the control service a moment to start up
    pause();

    //---------------------------------- Node A setup --------------------------------------------//

    let node_A_consumer_address = InprocAddress::random();

    // Add node B to node A's peer manager
    let node_A_database_name = "connection_manager_node_A_peer_manager"; // Note: every test should have unique database
    let datastore = init_datastore(node_A_database_name).unwrap();
    let database = datastore.get_handle(node_A_database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let node_A_peer_manager = make_peer_manager(vec![node_B_peer.clone()], database);
    let node_A_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .with_node_identity(node_A_identity.clone())
            .with_peer_manager(node_A_peer_manager)
            .with_peer_connection_config(make_peer_connection_config(node_A_consumer_address))
            .build()
            .unwrap(),
    );

    //------------------------------ Negotiate connection to node B -----------------------------------//

    let node_B_peer_copy = node_B_peer.clone();
    let node_A_connection_manager_cloned = node_A_connection_manager.clone();
    let handle1 = thread::spawn(move || -> Result<(), String> {
        let to_node_B_conn = node_A_connection_manager_cloned
            .establish_connection_to_peer(&node_B_peer)
            .map_err(|err| format!("{:?}", err))?;
        to_node_B_conn.set_linger(Linger::Indefinitely).unwrap();
        to_node_B_conn
            .send(vec!["THREAD1".as_bytes().to_vec()])
            .map_err(|err| format!("{:?}", err))?;
        Ok(())
    });

    let node_A_connection_manager_cloned = node_A_connection_manager.clone();
    let handle2 = thread::spawn(move || -> Result<(), String> {
        let to_node_B_conn = node_A_connection_manager_cloned
            .establish_connection_to_peer(&node_B_peer_copy)
            .map_err(|err| format!("{:?}", err))?;
        to_node_B_conn.set_linger(Linger::Indefinitely).unwrap();
        to_node_B_conn
            .send(vec!["THREAD2".as_bytes().to_vec()])
            .map_err(|err| format!("{:?}", err))?;
        Ok(())
    });

    handle1.timeout_join(Duration::from_millis(2000)).unwrap();
    handle2.timeout_join(Duration::from_millis(2000)).unwrap();

    // Give the peer connections a moment to receive and the message sink connections to send
    pause();

    node_B_control_service.shutdown().unwrap();
    node_B_control_service
        .timeout_join(Duration::from_millis(1000))
        .unwrap();

    assert_eq!(node_A_connection_manager.get_active_connection_count(), 1);
    node_B_msg_counter.assert_count(2, 20);

    match Arc::try_unwrap(node_A_connection_manager) {
        Ok(manager) => manager.shutdown().into_iter().map(|r| r.unwrap()).collect::<Vec<()>>(),
        Err(_) => panic!("Unable to unwrap connection manager from Arc"),
    };

    clean_up_datastore(node_A_database_name);
    clean_up_datastore(node_B_database_name);
}

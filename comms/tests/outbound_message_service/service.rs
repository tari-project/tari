// Copyright 2019 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use crate::support::{
    factories::{self, TestFactory},
    helpers::streams::stream_assert_count,
};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
    StreamExt,
};
use std::{fs, path::PathBuf, sync::Arc, thread, time::Duration};
use tari_comms::{
    connection::ZmqContext,
    connection_manager::{create_connection_manager_actor, ConnectionManager, PeerConnectionConfig},
    control_service::{ControlService, ControlServiceConfig},
    message::MessageFlags,
    outbound_message_service::{OutboundMessage, OutboundMessageService},
    peer_manager::{Peer, PeerManager},
    types::CommsDatabase,
};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBError, LMDBStore},
    LMDBWrapper,
};
use tokio::runtime::Runtime;

fn make_peer_connection_config() -> PeerConnectionConfig {
    PeerConnectionConfig {
        peer_connection_establish_timeout: Duration::from_secs(5),
        max_message_size: 1024,
        max_connections: 10,
        host: "127.0.0.1".parse().unwrap(),
        max_connect_retries: 3,
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
    let _ = fs::create_dir(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_environment_size(10)
        .set_max_number_of_databases(2)
        .add_database(name, lmdb_zero::db::CREATE)
        .build()
}

fn clean_up_datastore(name: &str) {
    fs::remove_dir_all(get_path(name)).unwrap();
}

/// This tests a message being sent through to the OMP where a peer (Node B) is awaiting alive and accepting
/// connections.
#[test]
#[allow(non_snake_case)]
fn outbound_message_pool_no_retry() {
    let rt = Runtime::new().unwrap();
    let context = ZmqContext::new();
    let node_identity = Arc::new(factories::node_identity::create().build().unwrap());

    //---------------------------------- Node B Setup --------------------------------------------//
    let node_B_control_port_address = factories::net_address::create().build().unwrap();

    let node_B_peer = factories::peer::create()
        .with_net_addresses(vec![node_B_control_port_address.clone()])
        // Set node B's secret key to be the same as node A's so that we can generate the same shared secret
        .with_public_key(node_identity.identity.public_key.clone())
        .build()
        .unwrap();

    // Node B knows no peers
    let node_B_database_name = "omp_node_B_peer_manager"; // Note: every test should have unique database
    let datastore = init_datastore(node_B_database_name).unwrap();
    let database = datastore.get_handle(node_B_database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let node_B_peer_manager = make_peer_manager(vec![], database);
    let (message_sink_tx_b, message_sink_rx_b) = mpsc::channel(10);
    let node_B_connection_manager = Arc::new(ConnectionManager::new(
        context.clone(),
        node_identity.clone(),
        node_B_peer_manager,
        make_peer_connection_config(),
        message_sink_tx_b.clone(),
    ));

    // Start node B's control service
    let node_B_control_service = ControlService::new(context.clone(), node_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: node_B_control_port_address,
        requested_connection_timeout: Duration::from_millis(2000),
    })
    .serve(Arc::clone(&node_B_connection_manager))
    .unwrap();

    let (_, node_B_connection_manager_actor) = create_connection_manager_actor(10, node_B_connection_manager);
    rt.spawn(node_B_connection_manager_actor.start());

    //---------------------------------- Node A setup --------------------------------------------//

    // Add node B to node A's peer manager
    let node_A_database_name = "omp_node_A_peer_manager"; // Note: every test should have unique database
    let datastore = init_datastore(node_A_database_name).unwrap();
    let database = datastore.get_handle(node_A_database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let node_A_peer_manager = make_peer_manager(vec![node_B_peer.clone()], database);
    let (message_sink_tx_a, _message_sink_rx_a) = mpsc::channel(10);
    let node_A_connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_peer_manager(node_A_peer_manager.clone())
            .with_peer_connection_config(make_peer_connection_config())
            .with_message_sink_sender(message_sink_tx_a)
            .build()
            .unwrap(),
    );

    let (node_A_connection_manager_requester, node_A_connection_manager_actor) =
        create_connection_manager_actor(10, node_A_connection_manager);
    rt.spawn(node_A_connection_manager_actor.start());

    // Setup Node A OMS
    let (outbound_tx, outbound_rx) = mpsc::unbounded();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let oms = OutboundMessageService::new(
        Default::default(),
        outbound_rx,
        node_identity,
        node_A_connection_manager_requester,
        shutdown_rx,
    );
    rt.spawn(oms.start());

    // Spawn 8 message sending tasks
    for _ in 0..8 {
        let mut sink_clone = outbound_tx.clone();
        let node_id = node_B_peer.node_id.clone();
        rt.spawn(async move {
            sink_clone
                .send(OutboundMessage::new(node_id, MessageFlags::NONE, vec![0]))
                .await
                .unwrap()
        });
    }

    let (_, _) = stream_assert_count(message_sink_rx_b.fuse(), 8, 8000).unwrap();
    node_B_control_service.shutdown().unwrap();
    node_B_control_service
        .timeout_join(Duration::from_millis(3000))
        .unwrap();

    shutdown_tx.send(()).unwrap();

    clean_up_datastore(node_A_database_name);
    clean_up_datastore(node_B_database_name);
}

/// This tests the reliability of the OMP.
///
/// This test is quite slow as it has to allow time for messages to send after a backoff period.
///
/// 1. A message is sent through to node A's OMP,
/// 2. Node B is offline so the message is sent to the message retry service
/// 3. Node B comes online (control service is started up)
/// 4. The message retry service eventually sends the messages
/// 5. Assert that all messages have been received
#[test]
#[allow(non_snake_case)]
fn test_outbound_message_pool_fail_and_retry() {
    let rt = Runtime::new().unwrap();
    let context = ZmqContext::new();

    let node_A_identity = factories::node_identity::create().build().map(Arc::new).unwrap();
    //---------------------------------- Node B Setup --------------------------------------------//

    let (message_sink_tx_b, message_sink_rx_b) = mpsc::channel(10);

    let node_B_control_port_address = factories::net_address::create().build().unwrap();

    let node_B_identity = factories::node_identity::create()
        .with_control_service_address(node_B_control_port_address.clone())
        .build()
        .map(Arc::new)
        .unwrap();

    let node_B_peer = factories::peer::create()
        .with_net_addresses(vec![node_B_control_port_address.clone()])
        // Set node B's secret key to be the same as node A's so that we can generate the same shared secret
        .with_public_key(node_B_identity.identity.public_key.clone())
        .build()
        .unwrap();

    //---------------------------------- Node A setup --------------------------------------------//

    // Add node B to node A's peer manager
    let database_name = "omp_test_outbound_message_pool_fail_and_retry1"; // Note: every test should have unique
    let datastore = init_datastore(database_name).unwrap();
    let database = datastore.get_handle(database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let (message_sink_tx_a, _message_sink_rx_a) = mpsc::channel(10);
    let node_A_peer_manager = factories::peer_manager::create()
        .with_peers(vec![node_B_peer.clone()])
        .with_database(database)
        .build()
        .map(Arc::new)
        .unwrap();
    let node_A_connection_manager = factories::connection_manager::create()
        .with_context(context.clone())
        .with_node_identity(node_A_identity.clone())
        .with_peer_manager(node_A_peer_manager.clone())
        .with_peer_connection_config(make_peer_connection_config())
        .with_message_sink_sender(message_sink_tx_a)
        .build()
        .map(Arc::new)
        .unwrap();

    let (node_A_connection_manager_requester, node_A_connection_manager_actor) =
        create_connection_manager_actor(10, node_A_connection_manager);
    rt.spawn(node_A_connection_manager_actor.start());

    // Setup Node A OMS
    let (outbound_tx, outbound_rx) = mpsc::unbounded();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let oms = OutboundMessageService::new(
        Default::default(),
        outbound_rx,
        node_A_identity,
        node_A_connection_manager_requester,
        shutdown_rx,
    );
    rt.spawn(oms.start());

    // Spawn 8 message sending tasks
    for _ in 0..8 {
        let mut sink_clone = outbound_tx.clone();
        let node_id = node_B_peer.node_id.clone();
        rt.spawn(async move {
            sink_clone
                .send(OutboundMessage::new(node_id, MessageFlags::empty(), vec![0]))
                .await
                .unwrap()
        });
    }

    // Keep this thread sleep, it's testing send retry
    thread::sleep(Duration::from_millis(1500));

    // Later, start node B's control service and test if we receive messages
    let node_B_database_name = "omp_node_B_peer_manager"; // Note: every test should have unique database
    let datastore = init_datastore(node_B_database_name).unwrap();
    let database = datastore.get_handle(node_B_database_name).unwrap();
    let database = LMDBWrapper::new(Arc::new(database));
    let node_B_peer_manager = make_peer_manager(vec![], database);
    let node_B_connection_manager = factories::connection_manager::create()
        .with_context(context.clone())
        .with_node_identity(node_B_identity.clone())
        .with_peer_manager(node_B_peer_manager.clone())
        .with_peer_connection_config(make_peer_connection_config())
        .with_message_sink_sender(message_sink_tx_b)
        .build()
        .map(Arc::new)
        .unwrap();

    // Start node B's control service
    let node_B_control_service = ControlService::new(context.clone(), node_B_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: node_B_control_port_address,
        requested_connection_timeout: Duration::from_millis(2000),
    })
    .serve(node_B_connection_manager)
    .unwrap();

    // We wait for the message to retry sending
    let (_, _) = stream_assert_count(message_sink_rx_b.fuse(), 5, 3000).unwrap();
    node_B_control_service.shutdown().unwrap();
    node_B_control_service
        .timeout_join(Duration::from_millis(3000))
        .unwrap();

    shutdown_tx.send(()).unwrap();

    clean_up_datastore(database_name);
}

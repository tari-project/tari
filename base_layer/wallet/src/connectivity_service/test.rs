//  Copyright 2021, The Taiji Project
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

use core::convert;
use std::{iter, sync::Arc};

use futures::future;
use taiji_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::{
        mock::{MockRpcImpl, MockRpcServer},
        RpcPoolClient,
    },
    test_utils::{
        mocks::{create_connectivity_mock, ConnectivityManagerMockState},
        node_identity::build_node_identity,
    },
};
use taiji_shutdown::Shutdown;
use taiji_test_utils::runtime::spawn_until_shutdown;
use tokio::{
    sync::{mpsc, Barrier},
    task,
};

use super::service::WalletConnectivityService;
use crate::{
    connectivity_service::{OnlineStatus, WalletConnectivityHandle, WalletConnectivityInterface},
    util::watch::Watch,
};

async fn setup() -> (
    WalletConnectivityHandle,
    MockRpcServer<MockRpcImpl>,
    ConnectivityManagerMockState,
    Shutdown,
) {
    let (tx, rx) = mpsc::channel(1);
    let base_node_watch = Watch::new(None);
    let online_status_watch = Watch::new(OnlineStatus::Offline);
    let handle = WalletConnectivityHandle::new(tx, base_node_watch.clone(), online_status_watch.get_receiver());
    let (connectivity, mock) = create_connectivity_mock();
    let mock_state = mock.spawn();
    // let peer_manager = create_peer_manager(tempdir().unwrap());
    let service = WalletConnectivityService::new(
        Default::default(),
        rx,
        base_node_watch.get_receiver(),
        online_status_watch,
        connectivity,
    );
    let shutdown = spawn_until_shutdown(service.start());

    let mock_svc = MockRpcImpl::new();
    let mut mock_server = MockRpcServer::new(mock_svc, build_node_identity(PeerFeatures::COMMUNICATION_NODE));
    mock_server.serve();

    (handle, mock_server, mock_state, shutdown)
}

#[tokio::test]
async fn it_dials_peer_when_base_node_is_set() {
    let (mut handle, mock_server, mock_state, _shutdown) = setup().await;
    let base_node_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;

    // Set the mock to defer returning a result for the peer connection
    mock_state.set_pending_connection(base_node_peer.node_id()).await;
    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer.to_peer());

    // Wait for connection request
    mock_state.await_call_count(1).await;
    mock_state.expect_dial_peer(base_node_peer.node_id()).await;

    // Now a connection will given to the service
    mock_state.add_active_connection(conn).await;

    let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
    assert!(rpc_client.is_connected());
}

#[tokio::test]
async fn it_resolves_many_pending_rpc_session_requests() {
    let (mut handle, mock_server, mock_state, _shutdown) = setup().await;
    let base_node_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;

    // Set the mock to defer returning a result for the peer connection
    mock_state.set_pending_connection(base_node_peer.node_id()).await;

    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer.to_peer());

    let pending_requests = iter::repeat_with(|| {
        let mut handle = handle.clone();
        task::spawn(async move {
            let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
            rpc_client.is_connected()
        })
    })
    .take(10)
    // Eagerly call `obtain_base_node_rpc_client`
    .collect::<Vec<_>>();

    // Now a connection will given to the service
    mock_state.add_active_connection(conn).await;

    let results = future::join_all(pending_requests).await;
    assert!(results.into_iter().map(Result::unwrap).all(convert::identity));
}

#[tokio::test]
async fn it_changes_to_a_new_base_node() {
    let (mut handle, mock_server, mock_state, _shutdown) = setup().await;
    let base_node_peer1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let conn1 = mock_server.create_mockimpl_connection(base_node_peer1.to_peer()).await;
    let base_node_peer2 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let conn2 = mock_server.create_mockimpl_connection(base_node_peer2.to_peer()).await;

    mock_state.add_active_connection(conn1).await;
    mock_state.add_active_connection(conn2).await;

    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer1.to_peer());

    mock_state.await_call_count(1).await;
    mock_state.expect_dial_peer(base_node_peer1.node_id()).await;
    assert!(mock_state.count_calls_containing("DialPeer").await >= 1);
    let _result = mock_state.take_calls().await;

    let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
    assert!(rpc_client.is_connected());

    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer2.to_peer());

    mock_state.await_call_count(1).await;
    mock_state.expect_dial_peer(base_node_peer2.node_id()).await;

    let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
    assert!(rpc_client.is_connected());
}

#[tokio::test]
async fn it_gracefully_handles_connect_fail_reconnect() {
    let (mut handle, mock_server, mock_state, _shutdown) = setup().await;
    let base_node_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let mut conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;

    // Set the mock to defer returning a result for the peer connection
    mock_state.set_pending_connection(base_node_peer.node_id()).await;

    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer.to_peer());

    // Now a connection will given to the service
    mock_state.add_active_connection(conn.clone()).await;
    // Empty out all the calls
    let _result = mock_state.take_calls().await;
    conn.disconnect().await.unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let pending_request = task::spawn({
        let mut handle = handle.clone();
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
            assert!(rpc_client.is_connected());
        }
    });

    mock_state.await_call_count(1).await;
    mock_state.expect_dial_peer(base_node_peer.node_id()).await;

    // Make sure that the task has actually started before continuing, otherwise we may not be testing the client asking
    // for a client session before a connection is made
    barrier.wait().await;

    // "Establish" a new connections
    let conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;
    mock_state.add_active_connection(conn).await;

    pending_request.await.unwrap();
}

#[tokio::test]
async fn it_gracefully_handles_multiple_connection_failures() {
    let (mut handle, mock_server, mock_state, _shutdown) = setup().await;
    let base_node_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;

    // Initiate a connection to the base node
    handle.set_base_node(base_node_peer.to_peer());

    // Now a connection will given to the service
    mock_state.add_active_connection(conn.clone()).await;
    let barrier = Arc::new(Barrier::new(2));

    let pending_request = task::spawn({
        let mut handle = handle.clone();
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            let rpc_client = handle.obtain_base_node_wallet_rpc_client().await.unwrap();
            assert!(rpc_client.is_connected());
        }
    });

    mock_state.await_call_count(1).await;
    mock_state.expect_dial_peer(base_node_peer.node_id()).await;

    barrier.wait().await;

    // Peer has failed up until this point, but finally the base node "comes online"
    let conn = mock_server.create_mockimpl_connection(base_node_peer.to_peer()).await;
    mock_state.add_active_connection(conn).await;

    // Still able to get a base node rpc client
    pending_request.await.unwrap();
}

//  Copyright 2020, The Tari Project
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
use super::{
    config::ConnectivityConfig,
    connection_pool::ConnectionStatus,
    manager::ConnectivityManager,
    requester::{ConnectivityEvent, ConnectivityRequester},
    selection::ConnectivitySelection,
};
use crate::{
    connection_manager::ConnectionManagerError,
    peer_manager::{Peer, PeerFeatures},
    runtime,
    runtime::task,
    test_utils::{
        mocks::{create_connection_manager_mock, create_peer_connection_mock_pair, ConnectionManagerMockState},
        node_identity::{build_many_node_identities, build_node_identity},
        test_node::build_peer_manager,
    },
    ConnectionManagerEvent,
    NodeIdentity,
    PeerManager,
};
use futures::{channel::mpsc, future};
use std::{sync::Arc, time::Duration};
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_stream, streams, unpack_enum};
use tokio::sync::broadcast;

fn setup_connectivity_manager(
    config: ConnectivityConfig,
) -> (
    ConnectivityRequester,
    broadcast::Receiver<Arc<ConnectivityEvent>>,
    Arc<NodeIdentity>,
    Arc<PeerManager>,
    ConnectionManagerMockState,
    Shutdown,
) {
    let peer_manager = build_peer_manager();
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let (cm_requester, mock) = create_connection_manager_mock();
    let cm_mock_state = mock.get_shared_state();
    task::spawn(mock.run());
    let shutdown = Shutdown::new();

    let (request_tx, request_rx) = mpsc::channel(1);
    let (event_tx, event_rx) = broadcast::channel(10);
    let requester = ConnectivityRequester::new(request_tx, event_tx.clone());
    ConnectivityManager {
        config,
        event_tx,
        request_rx,
        node_identity: node_identity.clone(),
        connection_manager: cm_requester,
        peer_manager: peer_manager.clone(),
        shutdown_signal: shutdown.to_signal(),
    }
    .create()
    .spawn();

    (
        requester,
        event_rx,
        node_identity,
        peer_manager,
        cm_mock_state,
        shutdown,
    )
}

async fn add_test_peers(peer_manager: &PeerManager, n: usize) -> Vec<Peer> {
    let node_identities = build_many_node_identities(n, PeerFeatures::COMMUNICATION_NODE);
    let peer_iter = node_identities.iter().map(|n| n.to_peer());

    let mut peers = Vec::with_capacity(n);
    for peer in peer_iter {
        peers.push(peer.clone());
        peer_manager.add_peer(peer).await.unwrap();
    }
    peers
}

#[runtime::test_basic]
async fn connecting_peers() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(Default::default());
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(
        peers
            .iter()
            .cloned()
            .map(|peer| create_peer_connection_mock_pair(1, peer, node_identity.to_peer())),
    )
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    let mut events = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = &*events.remove(0).unwrap());

    // All connections succeeded
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    }

    let _events = collect_stream!(event_stream, take = 11, timeout = Duration::from_secs(10));

    let connection_states = connectivity.get_all_connection_states().await.unwrap();
    assert_eq!(connection_states.len(), 10);

    for state in connection_states {
        assert_eq!(state.status(), ConnectionStatus::Connected);
    }
}

#[runtime::test_basic]
async fn add_many_managed_peers() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(Default::default());
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(
        (0..5)
            .map(|i| peers[i].clone())
            .map(|peer| create_peer_connection_mock_pair(1, node_identity.to_peer(), peer)),
    )
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .add_managed_peers(peers.iter().map(|p| p.node_id.clone()).collect())
        .await
        .unwrap();

    let mut events = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = &*events.remove(0).unwrap());

    // First 5 succeeded
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    }

    // 7-10 have failed, the rest are still connecting
    for i in 7..10 {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnectFailed(
            Box::new(peers[i].node_id.clone()),
            ConnectionManagerError::ConnectFailedMaximumAttemptsReached,
        ));
    }

    let events = collect_stream!(event_stream, take = 9, timeout = Duration::from_secs(10));
    let n = events
        .iter()
        .find_map(|event| match &**event.as_ref().unwrap() {
            ConnectivityEvent::ConnectivityStateOnline(n) => Some(n),
            ConnectivityEvent::ConnectivityStateDegraded(_) => None,
            ConnectivityEvent::PeerConnected(_) => None,
            e => panic!("Unexpected ConnectivityEvent {:?}", e),
        })
        .unwrap();
    assert!(*n > 1);

    let connection_states = connectivity.get_all_connection_states().await.unwrap();
    assert_eq!(connection_states.len(), 10);

    for i in 0..5 {
        let state = connection_states
            .iter()
            .find(|s| s.node_id() == &peers[i].node_id)
            .unwrap();
        assert_eq!(state.status(), ConnectionStatus::Connected);
        // Check the connection matches the expected peer
        assert_eq!(state.connection().unwrap().peer_node_id(), &peers[i].node_id);
    }
    for i in 5..6 {
        let state = connection_states
            .iter()
            .find(|s| s.node_id() == &peers[i].node_id)
            .unwrap();
        assert_eq!(state.status(), ConnectionStatus::Connecting);
        assert!(state.connection().is_none());
    }
    for i in 7..10 {
        let state = connection_states
            .iter()
            .find(|s| s.node_id() == &peers[i].node_id)
            .unwrap();
        assert_eq!(state.status(), ConnectionStatus::Failed);
        assert!(state.connection().is_none());
    }
}

#[runtime::test_basic]
async fn online_then_offline() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(Default::default());
    let peers = add_test_peers(&peer_manager, 8).await;
    let clients = build_many_node_identities(2, PeerFeatures::COMMUNICATION_CLIENT);
    for peer in &clients {
        peer_manager.add_peer(peer.to_peer()).await.unwrap();
    }

    let client_connections = future::join_all(
        clients
            .iter()
            .map(|peer| create_peer_connection_mock_pair(1, node_identity.to_peer(), peer.to_peer())),
    )
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();

    let connections = future::join_all(
        (0..5)
            .map(|i| peers[i].clone())
            .map(|peer| create_peer_connection_mock_pair(1, node_identity.to_peer(), peer)),
    )
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .add_managed_peers(peers.iter().map(|p| p.node_id.clone()).collect())
        .await
        .unwrap();
    connectivity
        .add_managed_peers(clients.iter().map(|p| p.node_id().clone()).collect())
        .await
        .unwrap();

    let mut events = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = &*events.remove(0).unwrap());

    for conn in connections.iter().skip(1) {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    }
    for conn in &client_connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    }

    connectivity
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnectFailed(
        connections[0].peer_node_id().clone().into(),
        ConnectionManagerError::InvalidStaticPublicKey,
    ));

    for conn in connections.iter().skip(1) {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
            conn.peer_node_id().clone().into(),
        ));
    }

    streams::assert_in_stream(
        &mut event_stream,
        |item| match &*item.unwrap() {
            ConnectivityEvent::ConnectivityStateDegraded(2) => true,
            _ => false,
        },
        Duration::from_secs(10),
    )
    .await;

    // Still online because we have client connections
    assert_eq!(
        connectivity.get_connectivity_status().await.unwrap().is_offline(),
        false
    );

    // Disconnect client connections
    for conn in &client_connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
            conn.peer_node_id().clone().into(),
        ));
    }

    streams::assert_in_stream(
        &mut event_stream,
        |item| match &*item.unwrap() {
            ConnectivityEvent::ConnectivityStateOffline => true,
            _ => false,
        },
        Duration::from_secs(10),
    )
    .await;

    let is_offline = connectivity.get_connectivity_status().await.unwrap().is_offline();
    assert!(is_offline);
}

#[runtime::test_basic]
async fn ban_peer() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(Default::default());
    let peer = add_test_peers(&peer_manager, 1).await.pop().unwrap();
    let (conn, _, _, _) = create_peer_connection_mock_pair(1, node_identity.to_peer(), peer.clone()).await;

    let mut events = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = &*events.remove(0).unwrap());

    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    let mut events = collect_stream!(event_stream, take = 2, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::PeerConnected(_conn) = &*events.remove(0).unwrap());
    unpack_enum!(ConnectivityEvent::ConnectivityStateOnline(_n) = &*events.remove(0).unwrap());

    let conn = connectivity.get_connection(peer.node_id.clone()).await.unwrap();
    assert!(conn.is_some());

    connectivity
        .ban_peer(peer.node_id.clone(), Duration::from_secs(3600), "".to_string())
        .await
        .unwrap();

    // We can always expect a single PeerBanned because we do not publish a disconnected event from the connection
    // manager In a real system, peer disconnect and peer banned events may happen in any order and should always be
    // completely fine.
    let event = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10))
        .pop()
        .unwrap()
        .unwrap();

    unpack_enum!(ConnectivityEvent::PeerBanned(node_id) = &*event);
    assert_eq!(node_id, &peer.node_id);

    let peer = peer_manager.find_by_node_id(&peer.node_id).await.unwrap();
    assert!(peer.is_banned());

    let conn = connectivity.get_connection(peer.node_id.clone()).await.unwrap();
    assert!(conn.is_none());
}

#[runtime::test_basic]
async fn peer_selection() {
    let config = ConnectivityConfig {
        min_connectivity: 1.0,
        ..Default::default()
    };
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(config);
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(
        peers
            .iter()
            .cloned()
            .map(|peer| create_peer_connection_mock_pair(1, peer, node_identity.to_peer())),
    )
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .add_managed_peers(peers.iter().take(5).map(|p| p.node_id.clone()).collect())
        .await
        .unwrap();

    let mut events = collect_stream!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = &*events.remove(0).unwrap());
    // 10 connections
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone()));
    }

    // Wait for all peers to be connected (i.e. for the connection manager events to be received)
    let mut _events = collect_stream!(event_stream, take = 12, timeout = Duration::from_secs(10));

    let conns = connectivity
        .select_connections(ConnectivitySelection::random_nodes(10, vec![connections[0]
            .peer_node_id()
            .clone()]))
        .await
        .unwrap();
    assert_eq!(conns.len(), 9);
    assert!(conns.iter().all(|c| c.peer_node_id() != connections[0].peer_node_id()));

    let mut conns = connectivity
        .select_connections(ConnectivitySelection::closest_to(
            connections.last().unwrap().peer_node_id().clone(),
            5,
            vec![],
        ))
        .await
        .unwrap();
    assert_eq!(conns.len(), 5);
    for i in 9usize..=5 {
        let c = conns.remove(0);
        assert_eq!(c.peer_node_id(), connections[i].peer_node_id());
    }
}

//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

#![allow(clippy::indexing_slicing)]
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{StreamExt, future};
use tari_shutdown::Shutdown;
use tari_test_utils::{collect_try_recv, streams, unpack_enum};
use tokio::sync::{broadcast, mpsc};

use super::{
    config::ConnectivityConfig,
    connection_pool::ConnectionStatus,
    manager::ConnectivityManager,
    requester::{ConnectivityEvent, ConnectivityRequester},
    selection::ConnectivitySelection,
};
use crate::{
    Minimized,
    NodeIdentity,
    PeerManager,
    RefKind,
    connection_manager::{ConnectionManagerError, ConnectionManagerEvent},
    connectivity::ConnectivityEventRx,
    peer_manager::{Peer, PeerFeatures, PeerFlags},
    test_utils::{
        build_peer_manager,
        mocks::{ConnectionManagerMockState, create_connection_manager_mock, create_peer_connection_mock_pair},
        node_identity::{build_many_node_identities, build_node_identity},
    },
};

#[allow(clippy::type_complexity)]
fn setup_connectivity_manager(
    config: ConnectivityConfig,
) -> (
    ConnectivityRequester,
    ConnectivityEventRx,
    Arc<NodeIdentity>,
    Arc<PeerManager>,
    ConnectionManagerMockState,
    Shutdown,
) {
    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let peer_manager = build_peer_manager(&node_identity.to_peer()).unwrap();
    setup_connectivity_manager_with_peer_manager(config, node_identity, peer_manager)
}

#[allow(clippy::type_complexity)]
fn setup_connectivity_manager_with_peer_manager(
    config: ConnectivityConfig,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
) -> (
    ConnectivityRequester,
    ConnectivityEventRx,
    Arc<NodeIdentity>,
    Arc<PeerManager>,
    ConnectionManagerMockState,
    Shutdown,
) {
    let (cm_requester, mock) = create_connection_manager_mock();
    let cm_mock_state = mock.get_shared_state();
    tokio::spawn(mock.run());
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
        peer_manager.add_or_update_peer(peer).await.unwrap();
    }
    peers
}

#[tokio::test]
async fn connecting_peers() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(Default::default());
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(
        peers
            .iter()
            .cloned()
            .map(|peer| create_peer_connection_mock_pair(peer, node_identity.to_peer())),
    )
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));

    // All connections succeeded
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }

    let _events = collect_try_recv!(event_stream, take = 11, timeout = Duration::from_secs(10));

    let connection_states = connectivity.get_all_connection_states().await.unwrap();
    assert_eq!(connection_states.len(), 10);

    for state in connection_states {
        assert_eq!(state.status(), ConnectionStatus::Connected);
    }
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn online_then_offline_then_online() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(ConnectivityConfig {
            min_connectivity: 2,
            ..Default::default()
        });
    let peers = add_test_peers(&peer_manager, 8).await;
    let clients = build_many_node_identities(2, PeerFeatures::COMMUNICATION_CLIENT);
    for peer in &clients {
        peer_manager.add_or_update_peer(peer.to_peer()).await.unwrap();
    }

    let client_connections = future::join_all(clients.iter().map(|peer| {
        let value = node_identity.clone();
        async move { create_peer_connection_mock_pair(value.to_peer(), peer.to_peer()).await }
    }))
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();

    let connections = future::join_all((0..5).map(|i| peers[i].clone()).map(|peer| {
        let value = node_identity.clone();
        async move { create_peer_connection_mock_pair(value.to_peer(), peer).await }
    }))
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .dial_many_peers(peers.iter().map(|p| p.node_id.clone()), RefKind::Weak)
        .await
        .collect::<Vec<_>>()
        .await;

    connectivity
        .dial_many_peers(clients.iter().map(|p| p.node_id().clone()), RefKind::Weak)
        .await
        .collect::<Vec<_>>()
        .await;

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));

    for conn in connections.iter().skip(1) {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }
    for conn in &client_connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }

    connectivity
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnectFailed(
        connections[0].peer_node_id().clone(),
        ConnectionManagerError::InvalidStaticPublicKey,
    ));

    for conn in connections.iter().skip(1) {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
            conn.id(),
            conn.peer_node_id().clone(),
            Minimized::No,
        ));
    }

    streams::assert_in_broadcast(
        &mut event_stream,
        |item| match item {
            ConnectivityEvent::ConnectivityStateOnline(2) => Some(()),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;

    // Still online because we have client connections
    assert!(!connectivity.get_connectivity_status().await.unwrap().is_offline());

    // Disconnect client connections
    for conn in &client_connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
            conn.id(),
            conn.peer_node_id().clone(),
            Minimized::No,
        ));
    }

    streams::assert_in_broadcast(
        &mut event_stream,
        |item| match item {
            ConnectivityEvent::ConnectivityStateOffline => Some(()),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;

    let is_offline = connectivity.get_connectivity_status().await.unwrap().is_offline();
    assert!(is_offline);

    // Create a fresh set of connections since the previous connections are now in a disconnected state
    let connections = future::join_all(
        (0..5)
            .map(|i| peers[i].clone())
            .map(|peer| create_peer_connection_mock_pair(node_identity.to_peer(), peer)),
    )
    .await
    .into_iter()
    .map(|(conn, _, _, _)| conn)
    .collect::<Vec<_>>();
    for conn in connections.iter().skip(1) {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }

    streams::assert_in_broadcast(
        &mut event_stream,
        |item| match item {
            ConnectivityEvent::ConnectivityStateOnline(2) => Some(()),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;

    assert!(connectivity.get_connectivity_status().await.unwrap().is_online());
}

#[tokio::test]
async fn ban_peer() {
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(ConnectivityConfig {
            min_connectivity: 1,
            ..Default::default()
        });
    let peer = add_test_peers(&peer_manager, 1).await.pop().unwrap();
    let (conn, _, _, _) = create_peer_connection_mock_pair(node_identity.to_peer(), peer.clone()).await;

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));

    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    let mut events = collect_try_recv!(event_stream, take = 2, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::PeerConnected(_conn) = events.remove(0));
    unpack_enum!(ConnectivityEvent::ConnectivityStateOnline(_n) = events.remove(0));

    let conn = connectivity
        .get_connection(peer.node_id.clone(), RefKind::Weak)
        .await
        .unwrap();
    assert!(conn.is_some());

    connectivity
        .ban_peer_until(peer.node_id.clone(), Duration::from_secs(3600), "".to_string())
        .await
        .unwrap();

    // We can always expect a single PeerBanned because we do not publish a disconnected event from the connection
    // manager In a real system, peer disconnect and peer banned events may happen in any order and should always be
    // completely fine.
    let event = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10))
        .pop()
        .unwrap();

    unpack_enum!(ConnectivityEvent::PeerBanned(node_id) = event);
    assert_eq!(node_id, peer.node_id);

    let peer = peer_manager.find_by_node_id(&peer.node_id).await.unwrap().unwrap();
    assert!(peer.is_banned());

    let conn = connectivity
        .get_connection(peer.node_id.clone(), RefKind::Weak)
        .await
        .unwrap();
    assert!(conn.is_none());
}

#[tokio::test]
async fn peer_selection() {
    let config = ConnectivityConfig {
        min_connectivity: 1,
        ..Default::default()
    };
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(config);
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(peers.iter().map(|peer| {
        let value = node_identity.clone();
        let peer = peer.clone();
        async move { create_peer_connection_mock_pair(peer, value.to_peer()).await }
    }))
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .dial_many_peers(peers.iter().take(5).map(|p| p.node_id.clone()), RefKind::Weak)
        .await
        .collect::<Vec<_>>()
        .await;

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));
    // 10 connections
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }

    // Wait for all peers to be connected (i.e. for the connection manager events to be received)
    let _events = collect_try_recv!(event_stream, take = 11, timeout = Duration::from_secs(10));

    let conns = connectivity
        .select_connections(ConnectivitySelection::random_nodes(10, vec![
            connections[0].peer_node_id().clone(),
        ]))
        .await
        .unwrap();
    assert_eq!(conns.len(), 9);
    assert!(conns.iter().all(|c| c.peer_node_id() != connections[0].peer_node_id()));

    let conns = connectivity
        .select_connections(ConnectivitySelection::random_nodes(5, vec![]))
        .await
        .unwrap();
    assert_eq!(conns.len(), 5);
}

#[tokio::test]
async fn pool_management() {
    let config = ConnectivityConfig {
        min_connectivity: 1,
        connection_pool_refresh_interval: Duration::from_secs(10),
        reaper_min_inactive_age: Duration::from_secs(10),
        is_connection_reaping_enabled: true,
        ..Default::default()
    };
    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(config);
    let peers = add_test_peers(&peer_manager, 10).await;

    let connections = future::join_all(peers.iter().map(|peer| {
        let value = node_identity.clone();
        let peer = peer.clone();
        async move { create_peer_connection_mock_pair(peer, value.to_peer()).await }
    }))
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    connectivity
        .dial_many_peers(peers.iter().take(5).map(|p| p.node_id.clone()), RefKind::Weak)
        .await
        .collect::<Vec<_>>()
        .await;

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));
    // 10 connections
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }

    // Wait for all peers to be connected (i.e. for the connection manager events to be received)
    collect_try_recv!(event_stream, take = 11, timeout = Duration::from_secs(10));

    let mut important_connection = connectivity
        .get_connection(connections[0].peer_node_id().clone(), RefKind::Weak)
        .await
        .unwrap()
        .unwrap();

    // Drop all connections references
    for mut conn in connections {
        if conn != important_connection {
            assert_eq!(conn.handle_count(), 2);
            // The peer connection mock does not "automatically" publish event to connectivity manager
            conn.disconnect(Minimized::No, "unit test").await.unwrap();
            cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
                conn.id(),
                conn.peer_node_id().clone(),
                Minimized::No,
            ));
        }
    }

    assert_eq!(important_connection.handle_count(), 2);

    let events = collect_try_recv!(event_stream, take = 9, timeout = Duration::from_secs(10));
    for event in events {
        unpack_enum!(ConnectivityEvent::PeerDisconnected(..) = event);
    }

    assert_eq!(important_connection.handle_count(), 2);

    let conns = connectivity.get_active_connections().await.unwrap();

    assert_eq!(conns.len(), 1);
    important_connection
        .disconnect(Minimized::No, "unit test")
        .await
        .unwrap();
    cm_mock_state.publish_event(ConnectionManagerEvent::PeerDisconnected(
        important_connection.id(),
        important_connection.peer_node_id().clone(),
        Minimized::No,
    ));
    drop(important_connection);

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::PeerDisconnected(..) = events.remove(0));
    let conns = connectivity.get_active_connections().await.unwrap();
    assert!(conns.is_empty());
}

#[tokio::test]
async fn seed_peer_release() {
    let config = ConnectivityConfig {
        min_connectivity: 1,
        connection_pool_refresh_interval: Duration::from_millis(100),
        max_seed_peer_age: Duration::from_secs(3),
        ..Default::default()
    };

    let (mut connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager(config);

    let peers = add_test_peers(&peer_manager, 2).await;

    // Peer 0 = SEED
    let mut seed_peer = peers[0].clone();
    seed_peer.add_flags(PeerFlags::SEED);
    peer_manager.add_or_update_peer(seed_peer.clone()).await.unwrap();

    // Peer 1 = NORMAL
    let normal_peer = peers[1].clone();

    // Connect to both
    let connections = future::join_all([seed_peer.clone(), normal_peer.clone()].into_iter().map(|peer| {
        let my_id = node_identity.clone();
        async move { create_peer_connection_mock_pair(peer, my_id.to_peer()).await }
    }))
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    let seed_conn = &connections[0];
    let normal_conn = &connections[1];

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(5));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));

    // Simulate connections
    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(seed_conn.clone().into()));
    cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(normal_conn.clone().into()));

    // Wait for events to propagate
    let conn_events = collect_try_recv!(event_stream, take = 2, timeout = Duration::from_secs(5));
    assert_eq!(conn_events.len(), 2, "Expected 2 connection events");

    // Verify Initial State (Age < 3s)
    let conns = connectivity.get_active_connections().await.unwrap();
    assert_eq!(conns.len(), 2, "Both peers should be connected initially");

    // Sleep 3.5s to cross the 3s threshold + allow refresh cycle
    tokio::time::sleep(Duration::from_millis(3500)).await;

    // Verify Disconnection
    let conns = connectivity.get_active_connections().await.unwrap();
    assert_eq!(conns.len(), 1, "Seed peer should have been disconnected");
    assert_eq!(
        conns[0].peer_node_id(),
        &normal_peer.node_id,
        "Remaining peer should be the normal peer"
    );
}

/// The ConnectivityManager must keep answering `select_connections` even while the peer database is
/// unresponsive.
///
/// `handle_dial_peer` consults the peer database (`is_peer_banned`) before doing anything else, so a
/// contended peer database used to wedge the actor there. Once it is wedged `select_connections`
/// never answers, the DHT blocks mid-propagation, and every outbound message holds a pipeline slot
/// until it times out — which is how this surfaced in production, four layers downstream.
///
/// The runtime has a single worker so that a peer database call which blocks its caller's *thread*
/// (rather than merely its task) is fatal to the test, as it was to the node.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn select_connections_is_served_while_the_peer_database_is_slow() {
    use diesel::connection::SimpleConnection;
    use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};

    use crate::peer_manager::database::{MIGRATIONS, PeerDatabaseSql};

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
    let temp_dir = tempfile::tempdir().unwrap();
    let db_url = DbConnectionUrl::File(temp_dir.path().join("slow_peers.db"));
    // A long busy timeout on purpose: if a peer database call is allowed to block its caller, it
    // blocks for far longer than this test is prepared to wait.
    // A small pool and a long busy timeout on purpose. Writers that are stuck on SQLite's single
    // write lock hold their pooled connections while they wait, so the pool empties and every
    // subsequent call - reads included, WAL or not - blocks on the r2d2 checkout instead. That is the
    // shape of the production failure: `is_peer_banned` is a read, and it still wedged the actor.
    let db_connection =
        DbConnection::connect_and_migrate_with_busy_timeout(&db_url, MIGRATIONS, Some(4), Duration::from_secs(15))
            .unwrap();
    let peers_db = PeerDatabaseSql::new(db_connection.clone(), &node_identity.to_peer()).unwrap();
    let peer_manager = Arc::new(PeerManager::new(peers_db, crate::types::TransportProtocol::get_all()).unwrap());

    let (connectivity, mut event_stream, node_identity, peer_manager, cm_mock_state, _shutdown) =
        setup_connectivity_manager_with_peer_manager(Default::default(), node_identity, peer_manager);

    let peers = add_test_peers(&peer_manager, 3).await;
    let connections = future::join_all(
        peers
            .iter()
            .cloned()
            .map(|peer| create_peer_connection_mock_pair(peer, node_identity.to_peer())),
    )
    .await
    .into_iter()
    .map(|(_, _, conn, _)| conn)
    .collect::<Vec<_>>();

    let mut events = collect_try_recv!(event_stream, take = 1, timeout = Duration::from_secs(10));
    unpack_enum!(ConnectivityEvent::ConnectivityStateInitialized = events.remove(0));
    for conn in &connections {
        cm_mock_state.publish_event(ConnectionManagerEvent::PeerConnected(conn.clone().into()));
    }
    let _events = collect_try_recv!(event_stream, take = 4, timeout = Duration::from_secs(10));

    // Take SQLite's write lock on one pooled connection, then pile on more writes than the pool has
    // connections left. Every one of them parks holding (or waiting for) a connection, so the peer
    // database stops answering anything at all.
    let mut lock_holder = db_connection.get_pooled_connection().unwrap();
    lock_holder.batch_execute("BEGIN IMMEDIATE;").unwrap();

    // The probe runs on a real OS thread, outside the runtime's single worker, and both timestamps
    // are taken there. That is the whole point: when a worker is parked by a blocking call the
    // runtime's timers stop too, so a task that measures its own latency (or a `tokio::time::timeout`)
    // sees nothing at all - it is not scheduled until after the stall has passed.
    let handle = tokio::runtime::Handle::current();
    let mut probe_connectivity = connectivity.clone();
    let probe = std::thread::spawn(move || {
        // Let the writers below saturate the peer database first.
        std::thread::sleep(Duration::from_millis(400));
        let started = Instant::now();
        let result =
            handle.block_on(probe_connectivity.select_connections(ConnectivitySelection::random_nodes(3, vec![])));
        (started.elapsed(), result)
    });

    let writers = (0..6)
        .map(|_| {
            let peer_manager = peer_manager.clone();
            let peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
            tokio::spawn(async move { peer_manager.add_or_update_peer(peer).await })
        })
        .collect::<Vec<_>>();

    // Ask for a dial too. This is the request that reaches the peer database first, from the
    // connectivity manager's own request loop, and it is where the actor used to wedge.
    let dial = tokio::spawn({
        let connectivity = connectivity.clone();
        let node_id = peers[0].node_id.clone();
        async move { connectivity.dial_peer(node_id, RefKind::Weak).await }
    });

    let (elapsed, result) = tokio::task::spawn_blocking(move || probe.join().expect("probe thread panicked"))
        .await
        .unwrap();
    let conns = result.unwrap();
    assert_eq!(conns.len(), 3);
    // Bounded by `PEER_LOOKUP_TIMEOUT` (the shed), not by the peer database's busy timeout.
    assert!(
        elapsed < Duration::from_secs(10),
        "the connectivity manager took {elapsed:.1?} to answer select_connections while the peer database was \
         saturated; it should shed the peer lookup, not park on it"
    );

    lock_holder.batch_execute("COMMIT;").unwrap();
    drop(lock_holder);
    let _result = dial.await.unwrap();
    for writer in writers {
        // Under this much artificial contention an individual write may legitimately give up with
        // "database is locked". What matters is that none of them took the actor down with them.
        let _result = writer.await.unwrap();
    }
}

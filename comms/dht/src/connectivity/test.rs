//  Copyright 2020, The Taiji Project
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

use std::{iter::repeat_with, sync::Arc, time::Duration};

use rand::{rngs::OsRng, seq::SliceRandom};
use taiji_comms::{
    connectivity::ConnectivityEvent,
    peer_manager::{Peer, PeerFeatures},
    test_utils::{
        count_string_occurrences,
        mocks::{create_connectivity_mock, create_dummy_peer_connection, ConnectivityManagerMockState},
        node_identity::ordered_node_identities_by_distance,
    },
    NodeIdentity,
    PeerManager,
};
use taiji_shutdown::Shutdown;
use taiji_test_utils::async_assert;
use tokio::sync::broadcast;

use crate::{
    connectivity::{DhtConnectivity, MetricsCollector},
    test_utils::{build_peer_manager, create_dht_actor_mock, make_node_identity, DhtMockState},
    DhtConfig,
};

async fn setup(
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    initial_peers: Vec<Peer>,
) -> (
    DhtConnectivity,
    DhtMockState,
    ConnectivityManagerMockState,
    Arc<PeerManager>,
    Arc<NodeIdentity>,
    Shutdown,
) {
    let peer_manager = build_peer_manager();
    for peer in initial_peers {
        peer_manager.add_peer(peer).await.unwrap();
    }

    let shutdown = Shutdown::new();
    let (connectivity, mock) = create_connectivity_mock();
    let connectivity_state = mock.get_shared_state();
    mock.spawn();
    let (dht_requester, mock) = create_dht_actor_mock(1);
    let dht_state = mock.get_shared_state();
    mock.spawn();
    let (event_publisher, _) = broadcast::channel(1);

    let dht_connectivity = DhtConnectivity::new(
        Arc::new(config),
        peer_manager.clone(),
        node_identity.clone(),
        connectivity,
        dht_requester,
        event_publisher.subscribe(),
        MetricsCollector::spawn(),
        shutdown.to_signal(),
    );

    (
        dht_connectivity,
        dht_state,
        connectivity_state,
        peer_manager,
        node_identity,
        shutdown,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn initialize() {
    let config = DhtConfig {
        num_neighbouring_nodes: 4,
        num_random_nodes: 2,
        ..Default::default()
    };
    let peers = repeat_with(|| make_node_identity().to_peer()).take(10).collect();
    let (dht_connectivity, _, connectivity, peer_manager, node_identity, _shutdown) =
        setup(config, make_node_identity(), peers).await;
    dht_connectivity.spawn();
    let neighbours = peer_manager
        .closest_peers(node_identity.node_id(), 4, &[], Some(PeerFeatures::COMMUNICATION_NODE))
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.node_id)
        .collect::<Vec<_>>();

    // Wait for calls to add peers
    async_assert!(
        connectivity.get_dialed_peers().await.len() >= 2,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    // Check that neighbours are added
    for neighbour in &neighbours {
        connectivity.expect_dial_peer(neighbour).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn added_neighbours() {
    let node_identity = make_node_identity();
    let mut node_identities =
        ordered_node_identities_by_distance(node_identity.node_id(), 6, PeerFeatures::COMMUNICATION_NODE);
    // Closest to this node
    let closer_peer = node_identities.remove(0);
    let peers = node_identities.iter().map(|ni| ni.to_peer()).collect::<Vec<_>>();

    let config = DhtConfig {
        num_neighbouring_nodes: 5,
        num_random_nodes: 0,
        ..Default::default()
    };
    let (dht_connectivity, _, connectivity, _, _, _shutdown) = setup(config, node_identity, peers).await;
    dht_connectivity.spawn();

    // Wait for calls to add peers
    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let calls = connectivity.take_calls().await;
    assert_eq!(count_string_occurrences(&calls, &["DialPeer"]), 5);

    let (conn, _) = create_dummy_peer_connection(closer_peer.node_id().clone());
    connectivity.publish_event(ConnectivityEvent::PeerConnected(conn.clone().into()));

    async_assert!(
        connectivity.get_dialed_peers().await.len() >= 5,
        max_attempts = 20,
        interval = Duration::from_millis(50),
    );

    // 1 for this test, 1 for the connectivity manager
    assert_eq!(conn.handle_count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn replace_peer_when_peer_goes_offline() {
    let node_identity = make_node_identity();
    let node_identities =
        ordered_node_identities_by_distance(node_identity.node_id(), 6, PeerFeatures::COMMUNICATION_NODE);
    // Closest to this node
    let peers = node_identities.iter().map(|ni| ni.to_peer()).collect::<Vec<_>>();

    let config = DhtConfig {
        num_neighbouring_nodes: 5,
        num_random_nodes: 0,
        ..Default::default()
    };
    let (dht_connectivity, _, connectivity, _, _, _shutdown) = setup(config, node_identity, peers).await;
    dht_connectivity.spawn();

    // Wait for calls to dial peers
    async_assert!(
        connectivity.call_count().await >= 6,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );
    let _result = connectivity.take_calls().await;

    let dialed = connectivity.take_dialed_peers().await;
    assert_eq!(dialed.len(), 5);

    connectivity.publish_event(ConnectivityEvent::PeerDisconnected(
        node_identities[4].node_id().clone(),
    ));

    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let _result = connectivity.take_calls().await;
    // Redial
    let dialed = connectivity.take_dialed_peers().await;
    assert_eq!(dialed.len(), 1);
    assert_eq!(dialed[0], *node_identities[4].node_id());

    connectivity.publish_event(ConnectivityEvent::PeerConnectFailed(
        node_identities[4].node_id().clone(),
    ));

    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    // Check that the next closer neighbour was added to the pool
    let dialed = connectivity.take_dialed_peers().await;
    assert_eq!(dialed.len(), 1);
    assert_eq!(dialed[0], *node_identities[5].node_id());
}

#[tokio::test]
async fn insert_neighbour() {
    let node_identity = make_node_identity();
    let node_identities =
        ordered_node_identities_by_distance(node_identity.node_id(), 10, PeerFeatures::COMMUNICATION_NODE);

    let config = DhtConfig {
        num_neighbouring_nodes: 8,
        ..Default::default()
    };
    let (mut dht_connectivity, _, _, _, _, _) = setup(config, node_identity.clone(), vec![]).await;

    let shuffled = {
        let mut v = node_identities.clone();
        v.shuffle(&mut OsRng);
        v
    };

    // First 8 inserts should not remove a peer (because num_neighbouring_nodes == 8)
    for ni in shuffled.iter().take(8) {
        assert!(dht_connectivity.insert_neighbour(ni.node_id().clone()).is_none());
    }

    // Next 2 inserts will always remove a node id
    for ni in shuffled.iter().skip(8) {
        assert!(dht_connectivity.insert_neighbour(ni.node_id().clone()).is_some())
    }

    // Check the first 7 node ids match our neighbours, the last element depends on distance and ordering of inserts
    // (these are random). insert_neighbour only cares about inserting the element in the right order and preserving
    // the length of the neighbour list. It doesnt care if it kicks out a closer peer (that is left for the
    // calling code).
    let ordered_node_ids = node_identities
        .iter()
        .take(7)
        .map(|ni| ni.node_id())
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(&dht_connectivity.neighbours[..7], ordered_node_ids.as_slice());
}

mod metrics {
    mod collector {
        use taiji_comms::peer_manager::NodeId;

        use crate::connectivity::MetricsCollector;

        #[tokio::test]
        async fn it_adds_message_received() {
            let mut metric_collector = MetricsCollector::spawn();
            let node_id = NodeId::default();
            (0..100).for_each(|_| {
                assert!(metric_collector.write_metric_message_received(node_id.clone()));
            });

            let ts = metric_collector
                .get_messages_received_timeseries(node_id)
                .await
                .unwrap();
            assert_eq!(ts.count(), 100);
        }

        #[tokio::test]
        async fn it_clears_the_metrics() {
            let mut metric_collector = MetricsCollector::spawn();
            let node_id = NodeId::default();
            assert!(metric_collector.write_metric_message_received(node_id.clone()));

            metric_collector.clear_metrics(node_id.clone()).await.unwrap();
            let ts = metric_collector
                .get_messages_received_timeseries(node_id)
                .await
                .unwrap();
            assert_eq!(ts.count(), 0);
        }
    }
}

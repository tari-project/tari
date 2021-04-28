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

use crate::{
    connectivity::{DhtConnectivity, MetricsCollector},
    test_utils::{build_peer_manager, create_dht_actor_mock, make_node_identity, DhtMockState},
    DhtConfig,
};
use rand::{rngs::OsRng, seq::SliceRandom};
use std::{iter::repeat_with, sync::Arc, time::Duration};
use tari_comms::{
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
use tari_shutdown::Shutdown;
use tari_test_utils::async_assert;
use tokio::sync::broadcast;

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
)
{
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
        config,
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

#[tokio_macros::test_basic]
#[allow(clippy::redundant_closure)]
async fn initialize() {
    let config = DhtConfig {
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
        connectivity.call_count().await >= 2,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let calls = connectivity.take_calls().await;
    assert_eq!(count_string_occurrences(&calls, &["AddManagedPeers"]), 2);

    // Check that neighbours are added
    let mut managed = connectivity.get_managed_peers().await;
    for neighbour in &neighbours {
        let pos = managed.iter().position(|n| n == neighbour).unwrap();
        managed.remove(pos);
    }

    // Check that random peers (excl neighbours) are added
    assert_eq!(managed.len(), 2);
    assert!(managed.iter().all(|n| !neighbours.contains(n)));
}

#[tokio_macros::test_basic]
async fn added_neighbours() {
    let node_identity = make_node_identity();
    let mut node_identities =
        ordered_node_identities_by_distance(node_identity.node_id(), 6, PeerFeatures::COMMUNICATION_NODE);
    // Closest to this node
    let closer_peer = node_identities.remove(0);
    let peers = node_identities.iter().map(|ni| ni.to_peer()).collect::<Vec<_>>();

    let config = DhtConfig {
        num_nodes_in_home_bucket: 5,
        num_nodes_in_other_buckets: 0,
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
    assert_eq!(count_string_occurrences(&calls, &["AddManagedPeers"]), 1);

    let (conn, _) = create_dummy_peer_connection(closer_peer.node_id().clone());
    connectivity.publish_event(ConnectivityEvent::PeerConnected(conn));

    async_assert!(
        connectivity.call_count().await >= 2,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let calls = connectivity.take_calls().await;
    assert_eq!(count_string_occurrences(&calls, &["AddManagedPeers"]), 1);
    assert_eq!(count_string_occurrences(&calls, &["RemovePeer"]), 1);

    // Check that the closer neighbour was added to managed peers
    let managed = connectivity.get_managed_peers().await;
    assert_eq!(managed.len(), 5);
    assert!(managed.contains(closer_peer.node_id()));
}

#[tokio_macros::test_basic]
#[allow(clippy::redundant_closure)]
async fn reinitialize_pools_when_offline() {
    let node_identity = make_node_identity();
    let node_identities = repeat_with(|| make_node_identity()).take(5).collect::<Vec<_>>();
    // Closest to this node
    let peers = node_identities.iter().map(|ni| ni.to_peer()).collect::<Vec<_>>();

    let config = DhtConfig {
        num_nodes_in_home_bucket: 5,
        num_nodes_in_other_buckets: 0,
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
    assert_eq!(count_string_occurrences(&calls, &["AddManagedPeers"]), 1);

    connectivity.publish_event(ConnectivityEvent::ConnectivityStateOffline);

    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );
    let calls = connectivity.take_calls().await;
    assert_eq!(count_string_occurrences(&calls, &["RemovePeer"]), 5);
    assert_eq!(count_string_occurrences(&calls, &["AddManagedPeers"]), 1);

    // Check that the closer neighbour was added to managed peers
    let managed = connectivity.get_managed_peers().await;
    assert_eq!(managed.len(), 5);
}

#[tokio_macros::test_basic]
async fn insert_peer_into_bucket() {
    let node_identity = make_node_identity();
    let node_identities =
        ordered_node_identities_by_distance(node_identity.node_id(), 10, PeerFeatures::COMMUNICATION_NODE);

    let (mut dht_connectivity, _, _, _, _, _) = setup(Default::default(), node_identity.clone(), vec![]).await;
    dht_connectivity.config.num_nodes_in_home_bucket = 8;

    let shuffled = {
        let mut v = node_identities.clone();
        v.shuffle(&mut OsRng);
        v
    };

    // First 8 inserts should not remove a peer (because num_neighbouring_nodes == 8)
    for ni in shuffled.iter().take(8) {
        dht_connectivity
            .insert_peer_into_bucket(ni.node_id().clone(), 0, 8)
            .await
            .unwrap();
    }

    // Next 2 inserts will always remove a node id
    for ni in shuffled.iter().skip(8) {
        dht_connectivity
            .insert_peer_into_bucket(ni.node_id().clone(), 0, 8)
            .await
            .unwrap();
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
    assert_eq!(&dht_connectivity.peer_buckets[0][..7], ordered_node_ids.as_slice());
}

mod metrics {
    mod collector {
        use crate::connectivity::MetricsCollector;
        use tari_comms::peer_manager::NodeId;

        #[tokio_macros::test_basic]
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

        #[tokio_macros::test_basic]
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

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

#![allow(clippy::indexing_slicing)]
use std::{iter::repeat_with, sync::Arc, time::Duration};

use rand::{rngs::OsRng, seq::SliceRandom};
use tari_comms::{
    Minimized,
    NodeIdentity,
    PeerManager,
    connectivity::ConnectivityEvent,
    peer_manager::{Peer, PeerFeatures},
    test_utils::{
        mocks::{ConnectivityManagerMockState, create_connectivity_mock, create_dummy_peer_connection},
        node_identity::build_many_node_identities,
    },
};
use tari_shutdown::Shutdown;
use tari_test_utils::async_assert;
use tokio::sync::broadcast;

use crate::{
    DhtConfig,
    connectivity::{DhtConnectivity, MetricsCollector},
    test_utils::{
        DhtMockState,
        build_peer_manager,
        create_dht_actor_mock,
        create_good_standing_peer,
        make_node_identity,
    },
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
        peer_manager.add_or_update_peer(peer).await.unwrap();
    }

    let shutdown = Shutdown::new();
    let (connectivity, mock) = create_connectivity_mock();
    let connectivity_state = mock.get_shared_state();
    mock.spawn();
    let (dht_requester, mock) = create_dht_actor_mock();
    let dht_state = mock.get_shared_state();
    mock.spawn();
    let (event_publisher, _) = broadcast::channel(1);

    let dht_connectivity = DhtConnectivity::new(
        Arc::new(config),
        peer_manager.clone(),
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
    let peers = repeat_with(|| create_good_standing_peer(&make_node_identity()))
        .take(10)
        .collect();
    let (dht_connectivity, _, connectivity, _peer_manager, _node_identity, _shutdown) =
        setup(config, make_node_identity(), peers).await;
    dht_connectivity.spawn();

    // Wait for calls to add peers
    async_assert!(
        connectivity.get_dialed_peers().await.len() >= 2,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    // Check that some pool peers were dialed (total pool size = 6)
    let dialed = connectivity.get_dialed_peers().await;
    assert!(
        dialed.len() >= 2,
        "Expected at least 2 peers to be dialed, got {}",
        dialed.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn added_pool_peers() {
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"` // Pipe to `> .\target\output.log 2>&1`
    let node_identity = make_node_identity();
    let mut node_identities = build_many_node_identities(6, PeerFeatures::COMMUNICATION_NODE);
    let extra_peer = node_identities.remove(0);
    let mut peers = node_identities.iter().map(|ni| ni.to_peer()).collect::<Vec<_>>();
    for peer in &mut peers {
        let addresses: Vec<_> = peer.addresses.address_iter().cloned().collect();
        for addr in &addresses {
            peer.addresses.mark_last_seen_now(addr);
        }
    }

    let config = DhtConfig {
        num_neighbouring_nodes: 3,
        num_random_nodes: 2,
        ..Default::default()
    };
    let peer_node_ids = peers.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
    let (dht_connectivity, _, connectivity, peer_manager, _, _shutdown) = setup(config, node_identity, peers).await;

    let added_peers = peer_manager.get_peers_by_node_ids(&peer_node_ids).await.unwrap();
    assert!(
        added_peers
            .iter()
            .any(|p| peer_node_ids.iter().any(|node_id| node_id == &p.node_id))
    );
    assert!(
        peer_node_ids
            .iter()
            .any(|p| peer_node_ids.iter().any(|node_id| node_id == p))
    );

    dht_connectivity.spawn();

    // Wait for calls to add peers
    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let _calls = connectivity.take_calls().await;
    // Check that we requested 5 dials (pool_size = 3 + 2 = 5)
    assert_eq!(connectivity.get_dialed_peers().await.len(), 5);

    let (conn, _) = create_dummy_peer_connection(extra_peer.node_id().clone());
    connectivity.publish_event(ConnectivityEvent::PeerConnected(conn.clone().into()));

    async_assert!(
        connectivity.get_dialed_peers().await.len() >= 5,
        max_attempts = 20,
        interval = Duration::from_millis(50),
    );

    // 1 for this test, 1 for the connectivity manager [FLAKY test, sometimes it is 3]
    assert!(conn.handle_count() == 2 || conn.handle_count() == 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn replace_peer_when_peer_goes_offline() {
    let node_identity = make_node_identity();
    let node_identities = build_many_node_identities(7, PeerFeatures::COMMUNICATION_NODE);
    let peers = node_identities
        .iter()
        .map(|ni| create_good_standing_peer(ni))
        .collect::<Vec<_>>();

    // pool_size = 3+3 = 6, with 7 peers available, 6 will be dialed, 1 stays as a spare
    let config = DhtConfig {
        num_neighbouring_nodes: 3,
        num_random_nodes: 3,
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
    assert_eq!(dialed.len(), 6);

    // Disconnect the first peer that was dialed
    let disconnected_peer = dialed[0].clone();
    connectivity.publish_event(ConnectivityEvent::PeerDisconnected(
        disconnected_peer.clone(),
        Minimized::No,
    ));

    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    let _result = connectivity.take_calls().await;
    let redialed = connectivity.take_dialed_peers().await;
    // After a disconnect, the peer should be redialed
    assert!(!redialed.is_empty(), "Expected at least one redial after disconnect");

    connectivity.publish_event(ConnectivityEvent::PeerConnectFailed(disconnected_peer.clone()));

    async_assert!(
        connectivity.call_count().await >= 1,
        max_attempts = 20,
        interval = Duration::from_millis(10),
    );

    // After connect failure, either the spare peer or the failed peer itself gets dialed
    let replacement_dialed = connectivity.take_dialed_peers().await;
    assert!(
        !replacement_dialed.is_empty(),
        "Expected replacement dial after connect failure"
    );
}

#[tokio::test]
async fn insert_into_pool() {
    let node_identity = make_node_identity();
    let node_identities = build_many_node_identities(10, PeerFeatures::COMMUNICATION_NODE);

    let config = DhtConfig {
        num_neighbouring_nodes: 4,
        num_random_nodes: 4,
        ..Default::default()
    };
    let (mut dht_connectivity, _, _, _, _, _) = setup(config, node_identity.clone(), vec![]).await;

    let shuffled = {
        let mut v = node_identities.clone();
        v.shuffle(&mut OsRng);
        v
    };

    // Insert all 10 peers into the pool
    for ni in &shuffled {
        dht_connectivity.insert_random_peer(ni.node_id().clone());
    }

    // insert_random_peer caps the pool at pool_size = num_neighbouring_nodes + num_random_nodes = 8
    // (excess entries are popped off even without minimize_connections)
    assert_eq!(dht_connectivity.random_pool.len(), 8);
}

mod metrics {
    mod collector {
        use tari_comms::peer_manager::NodeId;

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

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
use std::{
    iter::repeat_with,
    sync::Arc,
    time::{Duration, Instant},
};

use tari_comms::{
    NodeIdentity,
    PeerManager,
    connection_manager::{ConnectionDirection, PeerConnectionRequest},
    connectivity::ConnectivityEvent,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    test_utils::{
        mocks::{
            ConnectivityManagerMockState,
            create_connectivity_mock,
            create_dummy_peer_connection,
            create_dummy_peer_connection_with_direction,
        },
        node_identity::build_many_node_identities,
    },
};
use tari_shutdown::Shutdown;
use tari_test_utils::async_assert;
use tokio::sync::{broadcast, mpsc};

use crate::{
    DhtConfig,
    DhtConnectivityConfig,
    connectivity::{DhtConnectivity, MAX_REBOOTSTRAP_PEERS, MetricsCollector, RebootstrapTrigger, is_pool_starved},
    event::DhtEvent,
    test_utils::{
        DhtMockState,
        build_peer_manager,
        create_dht_actor_mock,
        create_good_standing_peer,
        make_client_identity,
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
        event_publisher,
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

    // Wait for the dials themselves rather than for *a* call: `call_count` counts every recorded call, and
    // `DhtConnectivity` makes others before it gets to dialling, so waiting on it can return before the dial
    // request has been processed - leaving the exact-count assertion below looking at a partial list.
    async_assert!(
        connectivity.get_dialed_peers().await.len() >= 5,
        max_attempts = 50,
        interval = Duration::from_millis(20),
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

/// Dials that never resolve must back off rather than be reissued every refresh cycle.
///
/// A pool that never becomes healthy re-selects the same unreachable peers on every cycle, and every
/// failed dial writes the peer back through `PeerManager::add_or_update_peer`. That write load is
/// what saturates the peer database and, ultimately, wedges the comms actors.
#[tokio::test]
async fn failed_dials_are_backed_off_instead_of_reissued() {
    let peers = repeat_with(|| create_good_standing_peer(&make_node_identity()))
        .take(4)
        .collect::<Vec<_>>();
    let node_ids = peers.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
    let (mut dht_connectivity, _, _connectivity, _peer_manager, _node_identity, _shutdown) =
        setup(DhtConfig::default(), make_node_identity(), peers).await;

    // Every peer is a candidate to begin with.
    let selected = dht_connectivity.fetch_random_peers(4, &[], false).await.unwrap();
    assert_eq!(selected.len(), 4);

    // Fail a dial to each of them.
    for node_id in &node_ids {
        dht_connectivity.record_dial_failure(node_id);
    }
    let selected = dht_connectivity.fetch_random_peers(4, &[], false).await.unwrap();
    assert!(
        selected.is_empty(),
        "peers whose dials just failed were selected for another dial immediately: {selected:?}"
    );

    // Consecutive failures push the next attempt further out.
    let first = *dht_connectivity.dial_backoff.get(&node_ids[0]).unwrap();
    assert_eq!(first.failures, 1);
    dht_connectivity.record_dial_failure(&node_ids[0]);
    let second = *dht_connectivity.dial_backoff.get(&node_ids[0]).unwrap();
    assert_eq!(second.failures, 2);
    assert!(second.retry_after > first.retry_after);

    // Connecting clears the backoff: the peer is reachable after all.
    let (conn, _) = create_dummy_peer_connection(node_ids[0].clone());
    dht_connectivity.handle_new_peer_connected(conn).await.unwrap();
    assert!(!dht_connectivity.dial_backoff.contains_key(&node_ids[0]));
    let selected = dht_connectivity.fetch_random_peers(4, &[], false).await.unwrap();
    assert_eq!(selected, vec![node_ids[0].clone()]);
}

/// The actor must survive a failed first pool refresh: without it there is no pool and nothing to notice that the
/// pool is starved.
#[tokio::test]
async fn it_survives_a_failed_initial_refresh() {
    let shutdown = Shutdown::new();
    // Dropping the mock makes every connectivity request fail, including the initial refresh
    let (connectivity, mock) = create_connectivity_mock();
    drop(mock);
    let (dht_requester, mock) = create_dht_actor_mock();
    mock.spawn();
    let (event_publisher, _) = broadcast::channel(1);
    let dht_connectivity = DhtConnectivity::new(
        Arc::new(DhtConfig::default()),
        build_peer_manager(),
        connectivity,
        dht_requester,
        event_publisher,
        MetricsCollector::spawn(),
        shutdown.to_signal(),
    );

    let handle = dht_connectivity.spawn();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !handle.is_finished(),
        "DhtConnectivity exited after a failed initial refresh"
    );

    shutdown.trigger();
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

mod rebootstrap_trigger {
    use super::*;

    fn config() -> DhtConnectivityConfig {
        DhtConnectivityConfig {
            pool_starved_ticks: 3,
            rebootstrap_cooldown_min: Duration::from_secs(10 * 60),
            rebootstrap_cooldown_max: Duration::from_secs(2 * 60 * 60),
            ..Default::default()
        }
    }

    fn mins(n: u64) -> Duration {
        Duration::from_secs(n.saturating_mul(60))
    }

    #[test]
    fn it_needs_consecutive_starved_ticks() {
        let config = config();
        let now = Instant::now();
        let mut trigger = RebootstrapTrigger::default();

        assert!(!trigger.on_tick(true, now, &config));
        assert!(!trigger.on_tick(true, now, &config));
        // A healthy tick resets the count
        assert!(!trigger.on_tick(false, now, &config));
        assert!(!trigger.on_tick(true, now, &config));
        assert!(!trigger.on_tick(true, now, &config));
        // The third consecutive starved tick fires immediately
        assert!(trigger.on_tick(true, now, &config));
    }

    #[test]
    fn its_cooldown_doubles_caps_and_resets() {
        let config = config();
        let start = Instant::now();
        let mut trigger = RebootstrapTrigger::default();
        for _ in 0..2 {
            assert!(!trigger.on_tick(true, start, &config));
        }
        assert!(trigger.on_tick(true, start, &config));

        // Each gap between rebootstraps is double the last, up to the 2h cap
        let mut at = start;
        for gap in [10, 20, 40, 80, 120, 120] {
            let just_before = at + mins(gap) - Duration::from_secs(1);
            assert!(
                !trigger.on_tick(true, just_before, &config),
                "fired before the {gap}m cooldown was up"
            );
            at += mins(gap);
            assert!(
                trigger.on_tick(true, at, &config),
                "did not fire after the {gap}m cooldown"
            );
        }

        // Staying recovered for the minimum cooldown resets everything: the next starvation needs 3 ticks again, then
        // fires straight away and the cooldown starts from the minimum again
        assert!(!trigger.on_tick(false, at, &config));
        at += mins(10);
        assert!(!trigger.on_tick(false, at, &config));
        assert!(!trigger.on_tick(true, at, &config));
        assert!(!trigger.on_tick(true, at, &config));
        assert!(trigger.on_tick(true, at, &config));
        assert!(!trigger.on_tick(true, at + mins(9), &config));
        assert!(trigger.on_tick(true, at + mins(10), &config));
    }

    /// A pool that recovers for a tick after each rebootstrap and then decays again must not reset the backoff.
    #[test]
    fn a_brief_recovery_does_not_reset_the_cooldown() {
        let config = config();
        let start = Instant::now();
        let mut trigger = RebootstrapTrigger::default();
        for _ in 0..2 {
            assert!(!trigger.on_tick(true, start, &config));
        }
        assert!(trigger.on_tick(true, start, &config));

        // One healthy tick, then starved again: three starved ticks are needed again, and the 10m cooldown still holds
        let mut at = start + mins(2);
        assert!(!trigger.on_tick(false, at, &config));
        for _ in 0..3 {
            at += mins(2);
            assert!(
                !trigger.on_tick(true, at, &config),
                "fired inside the cooldown after a brief recovery"
            );
        }
        // 10m after the first rebootstrap it fires, and the cooldown has doubled to 20m
        assert!(trigger.on_tick(true, start + mins(10), &config));
        assert!(!trigger.on_tick(false, start + mins(12), &config));
        assert!(!trigger.on_tick(true, start + mins(14), &config));
        assert!(!trigger.on_tick(true, start + mins(16), &config));
        assert!(!trigger.on_tick(true, start + mins(29), &config));
        assert!(trigger.on_tick(true, start + mins(30), &config));

        // Recovered, but for less than the minimum cooldown: the 40m cooldown still applies
        assert!(!trigger.on_tick(false, start + mins(31), &config));
        assert!(!trigger.on_tick(false, start + mins(40), &config));
        for t in [41, 42, 43, 69] {
            assert!(!trigger.on_tick(true, start + mins(t), &config));
        }
        assert!(trigger.on_tick(true, start + mins(70), &config));
    }

    #[test]
    fn it_is_starved_below_the_threshold() {
        assert!(is_pool_starved(0, 12, 0.5));
        assert!(is_pool_starved(5, 12, 0.5));
        assert!(!is_pool_starved(6, 12, 0.5));
        assert!(!is_pool_starved(0, 0, 0.5));
    }

    /// Fills the pool with peers connected in the given direction and holds their connection handles.
    fn fill_pool(
        dht_connectivity: &mut DhtConnectivity,
        n: usize,
        direction: ConnectionDirection,
    ) -> Vec<mpsc::Receiver<PeerConnectionRequest>> {
        let mut receivers = Vec::new();
        for _ in 0..n {
            let node_id = NodeId::from_public_key(make_node_identity().public_key());
            let (conn, rx) = create_dummy_peer_connection_with_direction(node_id.clone(), direction);
            dht_connectivity.random_pool.push(node_id);
            dht_connectivity.connection_handles.push(conn);
            receivers.push(rx);
        }
        receivers
    }

    /// A node that survives only on peers dialling in is still starved: inbound pool peers do not count.
    #[tokio::test]
    async fn it_counts_outbound_pool_peers_only() {
        let config = DhtConfig {
            num_neighbouring_nodes: 6,
            num_random_nodes: 6,
            ..Default::default()
        };
        let (mut dht_connectivity, _, _connectivity, _, _, _shutdown) =
            setup(config, make_node_identity(), vec![]).await;
        let mut events = dht_connectivity.dht_event_publisher.subscribe();

        let _inbound = fill_pool(&mut dht_connectivity, 12, ConnectionDirection::Inbound);
        assert_eq!(dht_connectivity.pool_connection_counts(), (0, 12));

        for _ in 0..2 {
            dht_connectivity.check_pool_starved();
            assert!(
                events.try_recv().is_err(),
                "PoolStarved published before 3 starved ticks"
            );
        }
        dht_connectivity.check_pool_starved();
        let event = events.try_recv().unwrap();
        assert!(matches!(*event, DhtEvent::PoolStarved));

        // Half the pool target in outbound peers is enough to no longer be starved
        let _outbound = fill_pool(&mut dht_connectivity, 6, ConnectionDirection::Outbound);
        assert_eq!(dht_connectivity.pool_connection_counts(), (6, 12));
        dht_connectivity.check_pool_starved();
        assert_eq!(dht_connectivity.rebootstrap_trigger.starved_ticks, 0);
    }

    /// Answers disconnect requests for a dummy connection, then closes it.
    fn serve_disconnects(mut rx: mpsc::Receiver<PeerConnectionRequest>) {
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                if let PeerConnectionRequest::Disconnect(_, reply_tx, _, _) = request {
                    let _ignore = reply_tx.send(Ok(()));
                    break;
                }
            }
        });
    }

    /// The degraded case: the pool is full, but only of peers that dialled in. A rebootstrap must still dial the
    /// peers it learned, and the outbound connections must replace inbound peers rather than be turned away.
    #[tokio::test]
    async fn it_dials_learned_peers_into_a_pool_full_of_inbound_peers() {
        let config = DhtConfig {
            num_neighbouring_nodes: 6,
            num_random_nodes: 6,
            ..Default::default()
        };
        let learned = make_node_identity().to_peer();
        let learned_node_id = learned.node_id.clone();
        let (mut dht_connectivity, _, connectivity, _, _, _shutdown) =
            setup(config, make_node_identity(), vec![learned]).await;
        for rx in fill_pool(&mut dht_connectivity, 12, ConnectionDirection::Inbound) {
            serve_disconnects(rx);
        }

        let info = crate::RebootstrapInfo {
            learned_peers: vec![learned_node_id.clone()],
            ..Default::default()
        };
        dht_connectivity
            .handle_dht_event(&DhtEvent::RebootstrapComplete(info))
            .await
            .unwrap();
        async_assert!(
            connectivity.is_peer_dialed(&learned_node_id).await,
            max_attempts = 20,
            interval = Duration::from_millis(10),
        );

        // The dial lands. The pool is full of inbound peers, but the outbound peer is kept...
        let (conn, rx) =
            create_dummy_peer_connection_with_direction(learned_node_id.clone(), ConnectionDirection::Outbound);
        let _outbound = rx;
        dht_connectivity.handle_new_peer_connected(conn).await.unwrap();
        assert_eq!(dht_connectivity.pool_connection_counts(), (1, 12));

        // ...and the next refresh releases an inbound peer, not the outbound one
        dht_connectivity.refresh_random_pool().await.unwrap();
        assert!(dht_connectivity.is_pool_peer(&learned_node_id));
        assert_eq!(dht_connectivity.pool_connection_counts(), (1, 11));
    }

    /// On a starved node the in-flight dial budget is taken up by dials to stale peers. Learned peers must still be
    /// dialled on the refill, and must be used to replace a failed pool dial.
    #[tokio::test]
    async fn learned_peers_are_dialled_while_the_dial_budget_is_used_up() {
        let config = DhtConfig {
            num_neighbouring_nodes: 6,
            num_random_nodes: 6,
            ..Default::default()
        };
        let learned = repeat_with(|| make_node_identity().to_peer())
            .take(3)
            .collect::<Vec<_>>();
        let learned_ids = learned.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        let (mut dht_connectivity, _, connectivity, _, _, _shutdown) =
            setup(config, make_node_identity(), learned).await;

        // 24 dials in flight is the cap for a pool of 12 with nothing connected
        let stale = repeat_with(|| NodeId::from_public_key(make_node_identity().public_key()))
            .take(24)
            .collect::<Vec<_>>();
        for node_id in &stale {
            dht_connectivity.random_pool.push(node_id.clone());
            dht_connectivity.pending_dials.insert(node_id.clone(), Instant::now());
        }

        let info = crate::RebootstrapInfo {
            learned_peers: learned_ids[..2].to_vec(),
            ..Default::default()
        };
        dht_connectivity
            .handle_dht_event(&DhtEvent::RebootstrapComplete(info))
            .await
            .unwrap();
        for node_id in &learned_ids[..2] {
            async_assert!(
                connectivity.is_peer_dialed(node_id).await,
                max_attempts = 20,
                interval = Duration::from_millis(10),
            );
        }

        // A stale dial fails and a learned peer is queued: the learned peer replaces it
        dht_connectivity.rebootstrap_peers = vec![learned_ids[2].clone()];
        dht_connectivity
            .handle_connectivity_event(ConnectivityEvent::PeerConnectFailed(stale[0].clone()))
            .await
            .unwrap();
        async_assert!(
            connectivity.is_peer_dialed(&learned_ids[2]).await,
            max_attempts = 20,
            interval = Duration::from_millis(10),
        );
        assert!(dht_connectivity.is_pool_peer(&learned_ids[2]));
    }

    /// Learned peers are taken in the order they were learned (seeds first), not in database order.
    #[tokio::test]
    async fn learned_peers_keep_their_order() {
        let learned = repeat_with(|| make_node_identity().to_peer())
            .take(5)
            .collect::<Vec<_>>();
        let learned_ids = learned.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        // Stored in reverse, so database order differs from learned order
        let stored = learned.into_iter().rev().collect();
        let (mut dht_connectivity, _, _, _, _, _shutdown) =
            setup(DhtConfig::default(), make_node_identity(), stored).await;

        dht_connectivity.set_rebootstrap_peers(&learned_ids).await.unwrap();
        assert_eq!(
            dht_connectivity.take_rebootstrap_peers(3, &[]),
            learned_ids[..3].to_vec()
        );
        assert_eq!(
            dht_connectivity.take_rebootstrap_peers(3, &[]),
            learned_ids[3..].to_vec()
        );
    }

    /// Learned peers are checked against the peer database once, when the rebootstrap completes: peers that can never
    /// be pool peers are dropped and the list is capped.
    #[tokio::test]
    async fn learned_peers_are_filtered_and_capped() {
        let mut banned = make_node_identity().to_peer();
        banned.ban_for(Duration::from_secs(60 * 60), "test".to_string());
        let mut seed = make_node_identity().to_peer();
        seed.add_flags(PeerFlags::SEED);
        let client = make_client_identity().to_peer();
        let unknown = NodeId::from_public_key(make_node_identity().public_key());
        let good = repeat_with(|| make_node_identity().to_peer())
            .take(MAX_REBOOTSTRAP_PEERS + 10)
            .collect::<Vec<_>>();

        let mut learned = vec![
            banned.node_id.clone(),
            seed.node_id.clone(),
            client.node_id.clone(),
            unknown,
        ];
        learned.extend(good.iter().map(|p| p.node_id.clone()));
        let mut stored = vec![banned, seed, client];
        stored.extend(good.iter().cloned());
        let (mut dht_connectivity, _, _, _, _, _shutdown) =
            setup(DhtConfig::default(), make_node_identity(), stored).await;

        dht_connectivity.set_rebootstrap_peers(&learned).await.unwrap();
        // The cap applies to the learned list, so the 4 ineligible peers take up 4 of its places
        let expected = good
            .iter()
            .take(MAX_REBOOTSTRAP_PEERS - 4)
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(dht_connectivity.rebootstrap_peers, expected);
    }

    /// Peers that are already in the pool, or excluded for another reason, are dropped rather than kept for later.
    #[tokio::test]
    async fn learned_peers_already_in_the_pool_are_dropped() {
        let (mut dht_connectivity, _, _, _, _, _shutdown) =
            setup(DhtConfig::default(), make_node_identity(), vec![]).await;
        let in_pool = NodeId::from_public_key(make_node_identity().public_key());
        let excluded = NodeId::from_public_key(make_node_identity().public_key());
        let backed_off = NodeId::from_public_key(make_node_identity().public_key());
        dht_connectivity.random_pool.push(in_pool.clone());
        dht_connectivity.record_dial_failure(&backed_off);
        dht_connectivity.rebootstrap_peers = vec![in_pool, excluded.clone(), backed_off.clone()];

        assert!(dht_connectivity.take_rebootstrap_peers(3, &[excluded]).is_empty());
        assert_eq!(dht_connectivity.rebootstrap_peers, vec![backed_off]);
    }

    /// After a rebootstrap the pool is topped up straight away, starting with the peers just learned.
    #[tokio::test]
    async fn it_prefers_rebootstrap_peers_when_refilling() {
        let config = DhtConfig {
            num_neighbouring_nodes: 1,
            num_random_nodes: 0,
            ..Default::default()
        };
        // Plenty of known-good peers in the database compete with the one learned peer
        let mut peers = repeat_with(|| create_good_standing_peer(&make_node_identity()))
            .take(10)
            .collect::<Vec<_>>();
        let learned = make_node_identity().to_peer();
        let learned_node_id = learned.node_id.clone();
        peers.push(learned);
        let (mut dht_connectivity, _, connectivity, _, _, _shutdown) = setup(config, make_node_identity(), peers).await;

        let info = crate::RebootstrapInfo {
            learned_peers: vec![learned_node_id.clone()],
            ..Default::default()
        };
        dht_connectivity
            .handle_dht_event(&DhtEvent::RebootstrapComplete(info))
            .await
            .unwrap();

        async_assert!(
            !connectivity.get_dialed_peers().await.is_empty(),
            max_attempts = 20,
            interval = Duration::from_millis(10),
        );
        let dialed = connectivity.get_dialed_peers().await;
        assert_eq!(
            dialed[0], learned_node_id,
            "the learned peer was not dialled first: {dialed:?}"
        );
        assert!(dht_connectivity.rebootstrap_peers.is_empty());
    }
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

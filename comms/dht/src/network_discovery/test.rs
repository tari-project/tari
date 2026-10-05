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
use std::{sync::Arc, time::Duration};

use tari_comms::{
    NodeIdentity,
    PeerManager,
    peer_manager::{Peer, PeerFeatures},
    test_utils::{
        mocks::{ConnectivityManagerMockState, create_connectivity_mock},
        node_identity::build_node_identity,
    },
};
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tokio::sync::broadcast;

use super::{DhtNetworkDiscovery, NetworkDiscoveryConfig};
use crate::{
    DhtConfig,
    event::DhtEvent,
    test_utils::{build_peer_manager, make_node_identity},
};

mod state_machine {
    use super::*;

    async fn setup(
        mut config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        initial_peers: Vec<Peer>,
    ) -> (
        DhtNetworkDiscovery,
        ConnectivityManagerMockState,
        Arc<PeerManager>,
        Arc<NodeIdentity>,
        broadcast::Receiver<Arc<DhtEvent>>,
        Shutdown,
    ) {
        // Every test needs these to be enabled
        config.network_discovery.enabled = true;

        let peer_manager = build_peer_manager();
        for peer in initial_peers {
            peer_manager.add_or_update_peer(peer).await.unwrap();
        }

        let shutdown = Shutdown::new();
        let (connectivity, mock) = create_connectivity_mock();
        let connectivity_state = mock.get_shared_state();
        mock.spawn();
        // let (dht_requester, mock) = create_dht_actor_mock(1);
        // let dht_state = mock.get_shared_state();
        // mock.spawn();

        let (event_tx, event_rx) = broadcast::channel(2);

        let network_discovery = DhtNetworkDiscovery::new(
            Arc::new(config),
            node_identity.clone(),
            peer_manager.clone(),
            connectivity,
            event_tx,
            None,
            shutdown.to_signal(),
        );

        (
            network_discovery,
            connectivity_state,
            peer_manager,
            node_identity,
            event_rx,
            shutdown,
        )
    }

    #[tokio::test]
    async fn it_shuts_down() {
        let (discovery, _, _, _, _, shutdown) = setup(Default::default(), make_node_identity(), vec![]).await;

        shutdown.trigger();
        tokio::time::timeout(Duration::from_secs(5), discovery.run())
            .await
            .unwrap();
    }
}

mod discovery_ready {
    use tari_comms::test_utils::{mocks::ConnectivityManagerMock, node_identity::build_many_node_identities};
    use tokio::sync::RwLock;

    use super::*;
    use crate::{
        BootstrapMethod,
        network_discovery::{
            DhtNetworkDiscoveryRoundInfo,
            ready::DiscoveryReady,
            state_machine::{NetworkDiscoveryContext, StateEvent},
        },
    };
    fn setup(
        config: NetworkDiscoveryConfig,
    ) -> (
        Arc<NodeIdentity>,
        Arc<PeerManager>,
        ConnectivityManagerMock,
        DiscoveryReady,
        NetworkDiscoveryContext,
    ) {
        let peer_manager = build_peer_manager();
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let (event_tx, _) = broadcast::channel(1);
        let context = NetworkDiscoveryContext {
            config: Arc::new(DhtConfig {
                network_discovery: config,
                ..Default::default()
            }),
            peer_manager: peer_manager.clone(),
            connectivity,
            node_identity: node_identity.clone(),
            num_rounds: Default::default(),
            all_attempted_peers: Default::default(),
            event_tx,
            last_round: Default::default(),
            bootstrap_method: Arc::new(RwLock::new(BootstrapMethod::None)),
            bootstrap_started_at: Arc::new(RwLock::new(None)),
            seed_peer_provider: None,
            inbound_learned: Default::default(),
            pool_peers: Default::default(),
        };

        let ready = DiscoveryReady::new(context.clone());
        (node_identity, peer_manager, connectivity_mock, ready, context)
    }

    #[tokio::test]
    async fn it_begins_aggressive_discovery() {
        let (_, pm, _, mut ready, _) = setup(Default::default());
        let node_identities = build_many_node_identities(1, PeerFeatures::COMMUNICATION_NODE);
        for identity in node_identities {
            let mut peer = identity.to_peer();
            let addresses: Vec<_> = peer.addresses.address_iter().cloned().collect();
            for addr in &addresses {
                peer.addresses.mark_last_seen_now(addr);
            }
            pm.add_or_update_peer(peer).await.unwrap();
        }
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::BeginDiscovery(params) = state_event);
        assert_eq!(
            params.num_peers_to_request,
            NetworkDiscoveryConfig::default().max_peers_to_sync_per_round
        );
    }

    #[tokio::test]
    async fn it_idles_if_no_sync_peers() {
        let (_, _, _, mut ready, _) = setup(Default::default());
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::Idle = state_event);
    }

    #[tokio::test]
    async fn it_idles_if_num_rounds_reached() {
        let config = NetworkDiscoveryConfig {
            min_desired_peers: 0,
            idle_after_num_rounds: 0,
            initial_peer_sync_delay: None,
            ..Default::default()
        };
        let (_, _, _, mut ready, context) = setup(config);
        context
            .set_last_round(DhtNetworkDiscoveryRoundInfo {
                num_new_peers: 1,
                num_duplicate_peers: 0,
                num_succeeded: 1,
                sync_peers: vec![],
                ..Default::default()
            })
            .await;
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::Idle = state_event);
    }

    #[tokio::test]
    async fn it_transitions_to_idle() {
        let config = NetworkDiscoveryConfig {
            min_desired_peers: 0,
            idle_after_num_rounds: 0,
            initial_peer_sync_delay: None,
            ..Default::default()
        };
        let (_, _, _, mut ready, context) = setup(config);
        context
            .set_last_round(DhtNetworkDiscoveryRoundInfo {
                num_succeeded: 1,
                ..Default::default()
            })
            .await;
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::Idle = state_event);
    }
}

mod rebootstrap {
    use std::{
        collections::HashSet,
        iter::repeat_with,
        sync::atomic::{AtomicUsize, Ordering},
        time::Instant,
    };

    use tari_comms::{
        PeerConnection,
        connection_manager::{ConnectionDirection, PeerConnectionRequest},
        peer_manager::{NodeId, PeerFlags},
        protocol::rpc::RpcError,
        test_utils::{
            mocks::{ConnectivityManagerMockState, create_dummy_peer_connection_with_direction},
            node_identity::build_many_node_identities,
        },
    };
    use tokio::sync::RwLock;

    use super::*;
    use crate::{
        BootstrapMethod,
        network_discovery::{
            MAX_LEARNED_PEERS,
            NetworkDiscoveryError,
            RebootstrapInfo,
            SeedPeerProvider,
            discovering::Discovering,
            on_connect::OnConnect,
            ready::DiscoveryReady,
            rebootstrap::{
                Rebootstrap,
                SourceKind,
                SourceResult,
                ban_on_offence,
                interleave_sources,
                refresh_seed_peers,
                source_kind,
                store_peers,
            },
            seed_strap::SeedStrap,
            state_machine::{DiscoveryParams, NetworkDiscoveryContext, State, StateEvent},
        },
        proto::rpc::PeerInfo,
        rpc::UnvalidatedPeerInfo,
    };

    fn node_ids(n: usize) -> Vec<NodeId> {
        (0..n)
            .map(|_| NodeId::from_public_key(make_node_identity().public_key()))
            .collect()
    }

    fn source(kind: SourceKind, stored: Vec<NodeId>) -> SourceResult {
        SourceResult {
            node_id: NodeId::from_public_key(make_node_identity().public_key()),
            kind,
            stored,
        }
    }

    /// One source returning far more peers than the others cannot fill the learned list.
    #[test]
    fn one_source_cannot_fill_the_learned_list() {
        let flood = node_ids(250);
        let sources = vec![
            source(SourceKind::Outbound, flood.clone()),
            source(SourceKind::Seed, node_ids(20)),
            source(SourceKind::Outbound, node_ids(20)),
            source(SourceKind::Seed, node_ids(20)),
        ];
        let (learned, _, kept) = interleave_sources(&sources);
        assert!(learned.len() <= MAX_LEARNED_PEERS);
        // 4 sources: at most 50 each
        assert_eq!(kept, vec![50, 20, 20, 20]);
        assert_eq!(learned.iter().filter(|node_id| flood.contains(node_id)).count(), 50);
    }

    /// Sources take turns in the order seeds, outbound, inbound.
    #[test]
    fn sources_are_interleaved_seeds_then_outbound_then_inbound() {
        let inbound = node_ids(2);
        let outbound = node_ids(2);
        let seed = node_ids(2);
        let sources = vec![
            source(SourceKind::Inbound, inbound.clone()),
            source(SourceKind::Outbound, outbound.clone()),
            source(SourceKind::Seed, seed.clone()),
        ];
        let (learned, _, _) = interleave_sources(&sources);
        assert_eq!(learned, vec![
            seed[0].clone(),
            outbound[0].clone(),
            inbound[0].clone(),
            seed[1].clone(),
            outbound[1].clone(),
            inbound[1].clone(),
        ]);
    }

    /// Inbound sources together make up at most a quarter of the learned list, and duplicates are kept once.
    #[test]
    fn the_inbound_share_is_capped() {
        let shared = node_ids(1);
        let mut first = shared.clone();
        first.extend(node_ids(149));
        let sources = vec![
            source(SourceKind::Inbound, first),
            source(SourceKind::Inbound, node_ids(150)),
            source(SourceKind::Outbound, shared),
        ];
        let (learned, _, kept) = interleave_sources(&sources);
        assert_eq!(kept.iter().take(2).sum::<usize>(), MAX_LEARNED_PEERS / 4);
        assert_eq!(kept.get(2), Some(&1));
        assert_eq!(learned.len(), MAX_LEARNED_PEERS / 4 + 1);
    }

    /// A connected peer that relays a stream of invalid peer records is banned, the same way Discovering does it.
    #[tokio::test]
    async fn a_connected_peer_relaying_invalid_records_is_banned() {
        let (context, mock, _events) = context(None);
        let source = NodeId::from_public_key(make_node_identity().public_key());
        let invalid = (0..10)
            .map(|_| PeerInfo {
                public_key: vec![1u8; 3],
                claims: vec![],
            })
            .collect();
        let err = store_peers(&context, &source, invalid).await.unwrap_err();
        assert!(matches!(err, NetworkDiscoveryError::TooManyInvalidPeersReceived));

        ban_on_offence(&context, source.clone(), &err).await;
        let mut banned = Vec::new();
        for _ in 0..50 {
            banned = mock.take_banned_peers().await;
            if !banned.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(banned.len(), 1);
        assert_eq!(banned[0].0, source);
    }

    /// A resolved seed whose key is already stored as an ordinary peer is synced from this round but not stored as a
    /// seed (nor overwritten), and only seeds of the current resolution are synced from.
    #[tokio::test]
    async fn seed_resolution_syncs_known_peers_and_replaces_old_seeds() {
        let known_identity = make_node_identity();
        let known = known_identity.to_peer();
        let known_id = known.node_id.clone();
        let mut known_resolved = known_identity.to_peer();
        known_resolved.features = PeerFeatures::COMMUNICATION_CLIENT;
        let mut old_seed = make_node_identity().to_peer();
        old_seed.add_flags(PeerFlags::SEED);
        let old_seed_id = old_seed.node_id.clone();
        let new_seed = make_node_identity().to_peer();
        let new_seed_id = new_seed.node_id.clone();
        let provider = Arc::new(TestSeedPeerProvider {
            seeds: vec![known_resolved, new_seed],
        });
        let (context, mock, _events) = context(Some(provider));
        context.peer_manager.add_or_update_peer(known).await.unwrap();
        context.peer_manager.add_or_update_peer(old_seed).await.unwrap();

        let accepted = refresh_seed_peers(&context).await.unwrap();
        assert_eq!(accepted, vec![known_id.clone(), new_seed_id.clone()]);
        let stored = context.peer_manager.find_by_node_id(&known_id).await.unwrap().unwrap();
        assert!(!stored.is_seed());
        assert_eq!(stored.features, PeerFeatures::COMMUNICATION_NODE);

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        assert!(mock.is_peer_dialed(&new_seed_id).await);
        assert!(mock.is_peer_dialed(&known_id).await);
        assert!(!mock.is_peer_dialed(&old_seed_id).await);
    }

    /// A dummy connection to a new peer that counts how often it is asked to open a substream (i.e. synced from).
    fn counting_connection(direction: ConnectionDirection) -> (PeerConnection, tokio::task::JoinHandle<usize>) {
        let node_id = NodeId::from_public_key(make_node_identity().public_key());
        let (conn, mut requests) = create_dummy_peer_connection_with_direction(node_id, direction);
        let counter = tokio::spawn(async move {
            let mut count = 0usize;
            while let Some(request) = requests.recv().await {
                if matches!(request, PeerConnectionRequest::OpenSubstream { .. }) {
                    count = count.saturating_add(1);
                }
            }
            count
        });
        (conn, counter)
    }

    /// Outbound connections fill the connected-peer slots before any inbound one, and a connected seed is left to
    /// the seed path.
    #[tokio::test]
    async fn connected_peer_sync_prefers_outbound_and_skips_seeds() {
        let (mut context, mock, _events) = context(None);
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.rebootstrap_connected_peers = 3;
        // The seed path takes the connected seed, so the connected-peer path must leave it out
        config.network_discovery.max_seed_peer_sync_count = 1;
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);

        let mut outbound = Vec::new();
        let mut inbound = Vec::new();
        for _ in 0..2 {
            let (conn, counter) = counting_connection(ConnectionDirection::Outbound);
            mock.add_active_connection(conn.clone()).await;
            outbound.push((conn.peer_node_id().clone(), counter));
        }
        for _ in 0..3 {
            let (conn, counter) = counting_connection(ConnectionDirection::Inbound);
            mock.add_active_connection(conn.clone()).await;
            inbound.push((conn.peer_node_id().clone(), counter));
        }
        let (seed_conn, seed_counter) = counting_connection(ConnectionDirection::Outbound);
        let mut seed = make_node_identity().to_peer();
        seed.node_id = seed_conn.peer_node_id().clone();
        seed.add_flags(PeerFlags::SEED);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();
        let seed_id = seed_conn.peer_node_id().clone();
        mock.add_active_connection(seed_conn).await;

        let _event = Rebootstrap::new(context.clone()).next_event().await;

        // Drop every handle so the counters finish
        let all = outbound
            .iter()
            .chain(inbound.iter())
            .map(|(node_id, _)| node_id.clone());
        for node_id in all.chain(Some(seed_id)) {
            mock.remove_active_connection(&node_id).await;
        }
        let mut synced_outbound = 0usize;
        for (_, counter) in outbound {
            synced_outbound = synced_outbound.saturating_add(
                tokio::time::timeout(Duration::from_secs(5), counter)
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
        let mut synced_inbound = 0usize;
        for (_, counter) in inbound {
            synced_inbound = synced_inbound.saturating_add(
                tokio::time::timeout(Duration::from_secs(5), counter)
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
        let synced_seed = tokio::time::timeout(Duration::from_secs(5), seed_counter)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(synced_outbound, 2, "both outbound peers should be synced from");
        assert_eq!(
            synced_inbound, 1,
            "only the remaining slot should go to an inbound peer"
        );
        assert_eq!(
            synced_seed, 1,
            "the seed should be synced from once, by the seed path only"
        );
    }

    /// A stored seed this node is connected to, and that is not in the current resolution.
    async fn connected_stored_seed(
        context: &NetworkDiscoveryContext,
        mock: &ConnectivityManagerMockState,
    ) -> (NodeId, tokio::task::JoinHandle<usize>) {
        let (conn, counter) = counting_connection(ConnectionDirection::Outbound);
        let mut seed = make_node_identity().to_peer();
        seed.node_id = conn.peer_node_id().clone();
        seed.add_flags(PeerFlags::SEED);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();
        let node_id = conn.peer_node_id().clone();
        mock.add_active_connection(conn).await;
        (node_id, counter)
    }

    async fn times_synced(
        mock: &ConnectivityManagerMockState,
        node_id: &NodeId,
        counter: tokio::task::JoinHandle<usize>,
    ) -> usize {
        mock.remove_active_connection(node_id).await;
        tokio::time::timeout(Duration::from_secs(5), counter)
            .await
            .unwrap()
            .unwrap()
    }

    /// A connected stored seed is synced from even when the current resolution does not include it.
    #[tokio::test]
    async fn a_connected_seed_outside_the_resolution_is_synced_from() {
        let resolved = make_node_identity().to_peer();
        let resolved_id = resolved.node_id.clone();
        let provider = Arc::new(TestSeedPeerProvider { seeds: vec![resolved] });
        let (mut context, mock, _events) = context(Some(provider));
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);
        let (connected_id, counter) = connected_stored_seed(&context, &mock).await;

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        assert!(mock.is_peer_dialed(&resolved_id).await);
        assert_eq!(times_synced(&mock, &connected_id, counter).await, 1);
    }

    /// When nothing resolves, a connected stored seed goes first and the other stored seeds fill the remaining slots.
    #[tokio::test]
    async fn stored_seeds_fill_the_slots_left_by_a_connected_seed() {
        let (mut context, mock, _events) = context(None);
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.max_seed_peer_sync_count = 3;
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);
        let (connected_id, counter) = connected_stored_seed(&context, &mock).await;
        let mut others = Vec::new();
        for _ in 0..2 {
            let mut seed = make_node_identity().to_peer();
            seed.add_flags(PeerFlags::SEED);
            others.push(seed.node_id.clone());
            context.peer_manager.add_or_update_peer(seed).await.unwrap();
        }

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        for node_id in &others {
            assert!(mock.is_peer_dialed(node_id).await, "a stored seed was left out");
        }
        assert_eq!(times_synced(&mock, &connected_id, counter).await, 1);
    }

    /// With more resolved seeds than slots, the connected seed always gets one of them.
    #[tokio::test]
    async fn a_connected_seed_always_gets_a_slot() {
        let resolved = repeat_with(|| make_node_identity().to_peer())
            .take(3)
            .collect::<Vec<_>>();
        let resolved_ids = resolved.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        let provider = Arc::new(TestSeedPeerProvider { seeds: resolved });
        let (mut context, mock, _events) = context(Some(provider));
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.max_seed_peer_sync_count = 1;
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);
        let (connected_id, counter) = connected_stored_seed(&context, &mock).await;

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        for node_id in &resolved_ids {
            assert!(
                !mock.is_peer_dialed(node_id).await,
                "a resolved seed took the only slot"
            );
        }
        assert_eq!(times_synced(&mock, &connected_id, counter).await, 1);
    }

    /// An outbound connection to a peer that an inbound source told us about still counts as inbound.
    #[test]
    fn a_laundered_peer_counts_as_inbound() {
        let laundered = NodeId::from_public_key(make_node_identity().public_key());
        let (outbound_to_laundered, _rx1) =
            create_dummy_peer_connection_with_direction(laundered.clone(), ConnectionDirection::Outbound);
        let (outbound, _rx2) = create_dummy_peer_connection_with_direction(
            NodeId::from_public_key(make_node_identity().public_key()),
            ConnectionDirection::Outbound,
        );
        let (inbound, _rx3) = create_dummy_peer_connection_with_direction(
            NodeId::from_public_key(make_node_identity().public_key()),
            ConnectionDirection::Inbound,
        );
        let inbound_learned = [laundered].into_iter().collect::<HashSet<_>>();
        assert_eq!(
            source_kind(&outbound_to_laundered, &inbound_learned),
            SourceKind::Inbound
        );
        assert_eq!(source_kind(&outbound, &inbound_learned), SourceKind::Outbound);
        assert_eq!(source_kind(&inbound, &inbound_learned), SourceKind::Inbound);
    }

    /// A stored seed that only dialled in, and is not in the current resolution, gets no seed slot: it is synced from
    /// as an ordinary inbound peer instead.
    #[tokio::test]
    async fn an_inbound_only_seed_outside_the_resolution_is_not_prioritised() {
        let resolved = make_node_identity().to_peer();
        let resolved_id = resolved.node_id.clone();
        let provider = Arc::new(TestSeedPeerProvider { seeds: vec![resolved] });
        let (mut context, mock, _events) = context(Some(provider));
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.max_seed_peer_sync_count = 1;
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);

        let (conn, counter) = counting_connection(ConnectionDirection::Inbound);
        let mut seed = make_node_identity().to_peer();
        seed.node_id = conn.peer_node_id().clone();
        seed.add_flags(PeerFlags::SEED);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();
        let seed_id = conn.peer_node_id().clone();
        mock.add_active_connection(conn).await;

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        // The seed slot went to the resolved seed...
        assert!(mock.is_peer_dialed(&resolved_id).await);
        assert!(!mock.is_peer_dialed(&seed_id).await);
        // ...and the inbound seed was synced from by the connected-peer path
        assert_eq!(times_synced(&mock, &seed_id, counter).await, 1);
    }

    /// Only the first `MAX_STORED_PER_SOURCE` valid peers from one source are written to the peer database.
    #[tokio::test]
    async fn a_source_can_store_only_so_many_peers() {
        let (context, _mock, _events) = context(None);
        let source = NodeId::from_public_key(make_node_identity().public_key());
        let peers = build_many_node_identities(60, PeerFeatures::COMMUNICATION_NODE)
            .into_iter()
            .map(|identity| UnvalidatedPeerInfo::from_peer_limited_claims(identity.to_peer(), 5, 5).into())
            .collect::<Vec<PeerInfo>>();
        let stored = store_peers(&context, &source, peers).await.unwrap();
        assert_eq!(stored.len(), 50);
        assert_eq!(context.peer_manager.count().await, 50);
    }

    /// Discovering bans a peer it dialled for an RPC failure, but not a peer it was already connected to.
    #[tokio::test]
    async fn discovering_does_not_ban_an_existing_connection_for_an_rpc_failure() {
        let (context, mock, _events) = context(None);
        let mut discovering = Discovering::new(
            DiscoveryParams {
                peers: vec![],
                num_peers_to_request: 10,
            },
            context,
        );
        let peer = NodeId::from_public_key(make_node_identity().public_key());
        let err = || Err::<(), _>(NetworkDiscoveryError::RpcError(RpcError::ReplyTimeout));
        let _ignore = discovering.ban_on_offence(peer.clone(), err(), true).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(mock.take_banned_peers().await.is_empty());

        // Bad peer data is banned for either way
        let _ignore = discovering
            .ban_on_offence(
                peer.clone(),
                Err::<(), _>(NetworkDiscoveryError::TooManyInvalidPeersReceived),
                true,
            )
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(mock.take_banned_peers().await.len(), 1);
    }

    /// If the current resolution leaves nothing to sync from, the stored seeds are used.
    #[tokio::test]
    async fn stored_seeds_are_used_when_the_resolution_leaves_nothing() {
        let mut banned_seed = make_node_identity().to_peer();
        banned_seed.ban_for(Duration::from_secs(60 * 60), "test".to_string());
        let mut old_seed = make_node_identity().to_peer();
        old_seed.add_flags(PeerFlags::SEED);
        let old_seed_id = old_seed.node_id.clone();
        let provider = Arc::new(TestSeedPeerProvider {
            seeds: vec![banned_seed.clone()],
        });
        let (context, mock, _events) = context(Some(provider));
        context.peer_manager.add_or_update_peer(banned_seed).await.unwrap();
        context.peer_manager.add_or_update_peer(old_seed).await.unwrap();

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        assert!(mock.is_peer_dialed(&old_seed_id).await);
    }

    /// RPC failures are routine over Tor and a starved node's connections may be its last ones, so only bad peer
    /// data is banned for.
    #[tokio::test]
    async fn rpc_failures_from_connected_peers_are_not_banned_for() {
        let (context, mock, _events) = context(None);
        let peer = NodeId::from_public_key(make_node_identity().public_key());
        ban_on_offence(
            &context,
            peer.clone(),
            &NetworkDiscoveryError::RpcError(RpcError::ReplyTimeout),
        )
        .await;
        ban_on_offence(&context, peer.clone(), &NetworkDiscoveryError::Timeout {
            operation: "connect_rpc".to_string(),
            peer: peer.to_string(),
            duration: "1s".to_string(),
        })
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(mock.take_banned_peers().await.is_empty());
    }

    struct TestSeedPeerProvider {
        seeds: Vec<Peer>,
    }

    #[tari_comms::async_trait]
    impl SeedPeerProvider for TestSeedPeerProvider {
        async fn resolve_seed_peers(&self) -> Vec<Peer> {
            self.seeds.clone()
        }
    }

    fn context(
        seed_peer_provider: Option<Arc<dyn SeedPeerProvider>>,
    ) -> (
        NetworkDiscoveryContext,
        ConnectivityManagerMockState,
        broadcast::Receiver<Arc<DhtEvent>>,
    ) {
        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let (event_tx, event_rx) = broadcast::channel(10);
        let context = NetworkDiscoveryContext {
            config: Arc::new(DhtConfig::default_local_test()),
            peer_manager: build_peer_manager(),
            connectivity,
            node_identity: make_node_identity(),
            num_rounds: Default::default(),
            all_attempted_peers: Default::default(),
            event_tx,
            last_round: Default::default(),
            bootstrap_method: Arc::new(RwLock::new(BootstrapMethod::None)),
            bootstrap_started_at: Arc::new(RwLock::new(None)),
            seed_peer_provider,
            inbound_learned: Default::default(),
            pool_peers: Default::default(),
        };
        (context, mock_state, event_rx)
    }

    fn discovery(context: &NetworkDiscoveryContext, shutdown: &Shutdown) -> DhtNetworkDiscovery {
        DhtNetworkDiscovery::new(
            context.config.clone(),
            context.node_identity.clone(),
            context.peer_manager.clone(),
            context.connectivity.clone(),
            context.event_tx.clone(),
            context.seed_peer_provider.clone(),
            shutdown.to_signal(),
        )
    }

    #[tokio::test]
    async fn pool_starved_from_every_state_rebootstraps_then_becomes_ready() {
        let (context, _mock, mut events) = context(None);
        let shutdown = Shutdown::new();
        let mut discovery = discovery(&context, &shutdown);

        let states = vec![
            State::Initializing,
            State::SeedStrap(SeedStrap::new(context.clone())),
            State::Ready(DiscoveryReady::new(context.clone())),
            State::Discovering(Discovering::new(
                DiscoveryParams {
                    peers: vec![],
                    num_peers_to_request: 10,
                },
                context.clone(),
            )),
            State::Waiting(Duration::from_secs(60).into()),
            State::OnConnect(OnConnect::new(context.clone())),
        ];
        for state in states {
            let name = state.to_string();
            let state = discovery.transition(state, StateEvent::PoolStarved).await;
            assert!(state.is_rebootstrap(), "{name} went to {state} instead of Rebootstrap");

            let info = RebootstrapInfo {
                learned_peers: vec![NodeId::default()],
                ..Default::default()
            };
            let state = discovery
                .transition(state, StateEvent::RebootstrapComplete(info.clone()))
                .await;
            assert!(
                state.is_ready(),
                "Rebootstrap (from {name}) went to {state} instead of Ready"
            );

            let event = events.try_recv().unwrap();
            match &*event {
                DhtEvent::RebootstrapComplete(published) => assert_eq!(*published, info),
                event => panic!("unexpected event {event:?}"),
            }
        }
    }

    /// A rebootstrap that pre-empts an unfinished primary bootstrap (here, the wait for a first connection) runs to
    /// completion, publishes its result and completes the primary bootstrap.
    #[tokio::test]
    async fn a_rebootstrap_completes_an_unfinished_bootstrap() {
        let mut seed = make_node_identity().to_peer();
        seed.add_flags(PeerFlags::SEED);
        let seed_node_id = seed.node_id.clone();
        let (mut context, mock, mut events) = context(None);
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.enabled = true;
        // The seed dial never resolves, so the rebootstrap runs into the bootstrap timeout
        config.network_discovery.bootstrap_timeout = Duration::from_millis(300);
        context.config = Arc::new(config);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();
        mock.set_pending_connection(&seed_node_id).await;

        let shutdown = Shutdown::new();
        let discovery = discovery(&context, &shutdown);
        let handle = tokio::spawn(discovery.run());
        // Let the state machine subscribe to DHT events before signalling
        tokio::time::sleep(Duration::from_millis(50)).await;
        context.event_tx.send(Arc::new(DhtEvent::PoolStarved)).unwrap();

        let mut rebootstraps = 0;
        let mut bootstrap_completed = false;
        tokio::time::timeout(Duration::from_secs(5), async {
            while rebootstraps == 0 || !bootstrap_completed {
                match &*events.recv().await.unwrap() {
                    DhtEvent::RebootstrapComplete(_) => rebootstraps += 1,
                    DhtEvent::PrimaryBootstrapComplete => bootstrap_completed = true,
                    _ => {},
                }
            }
        })
        .await
        .expect("the rebootstrap did not complete");
        assert!(mock.is_peer_dialed(&seed_node_id).await);

        shutdown.trigger();
        handle.await.unwrap();
    }

    /// Counts how often the seeds are resolved, i.e. how many rebootstraps ran.
    struct CountingSeedPeerProvider {
        seeds: Vec<Peer>,
        resolutions: Arc<AtomicUsize>,
    }

    #[tari_comms::async_trait]
    impl SeedPeerProvider for CountingSeedPeerProvider {
        async fn resolve_seed_peers(&self) -> Vec<Peer> {
            self.resolutions.fetch_add(1, Ordering::SeqCst);
            self.seeds.clone()
        }
    }

    /// A PoolStarved published while a rebootstrap is running is answered by that rebootstrap; it must not start
    /// another one once the first finishes.
    #[tokio::test]
    async fn pool_starved_during_a_rebootstrap_does_not_queue_another() {
        let seed = make_node_identity().to_peer();
        let seed_node_id = seed.node_id.clone();
        let resolutions = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(CountingSeedPeerProvider {
            seeds: vec![seed],
            resolutions: resolutions.clone(),
        });
        let (mut context, mock, mut events) = context(Some(provider));
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.enabled = true;
        // The seed dial never resolves, so each rebootstrap runs for its per-source deadline (250ms)
        config.network_discovery.bootstrap_timeout = Duration::from_millis(500);
        context.config = Arc::new(config);
        mock.set_pending_connection(&seed_node_id).await;

        let shutdown = Shutdown::new();
        let discovery = discovery(&context, &shutdown);
        let handle = tokio::spawn(discovery.run());
        tokio::time::sleep(Duration::from_millis(50)).await;
        context.event_tx.send(Arc::new(DhtEvent::PoolStarved)).unwrap();
        // Mid-rebootstrap
        tokio::time::sleep(Duration::from_millis(100)).await;
        context.event_tx.send(Arc::new(DhtEvent::PoolStarved)).unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if matches!(&*events.recv().await.unwrap(), DhtEvent::RebootstrapComplete(_)) {
                    break;
                }
            }
        })
        .await
        .expect("the rebootstrap did not complete");
        // Give a queued second rebootstrap time to start and run
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(resolutions.load(Ordering::SeqCst), 1, "a second rebootstrap ran");

        shutdown.trigger();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn pool_starved_does_not_restart_a_rebootstrap() {
        let (context, _mock, _events) = context(None);
        let shutdown = Shutdown::new();
        let mut discovery = discovery(&context, &shutdown);
        let state = discovery
            .transition(
                State::Rebootstrap(Rebootstrap::new(context.clone())),
                StateEvent::PoolStarved,
            )
            .await;
        assert!(state.is_rebootstrap());
    }

    #[tokio::test]
    async fn it_upserts_resolved_seeds_and_completes() {
        let seed = make_node_identity().to_peer();
        let seed_node_id = seed.node_id.clone();
        let provider = Arc::new(TestSeedPeerProvider { seeds: vec![seed] });
        let (context, mock, _events) = context(Some(provider));

        // The seed cannot be dialled (the mock has no connection for it) and nothing is connected, so nothing is
        // learned, but the resolved seed is stored and dialled.
        let event = Rebootstrap::new(context.clone()).next_event().await;
        let StateEvent::RebootstrapComplete(info) = event else {
            panic!("unexpected event {event}");
        };
        assert_eq!(info.seeds_resolved, 1);
        assert_eq!(info.seeds_synced, 0);
        assert!(info.learned_peers.is_empty());

        let stored = context
            .peer_manager
            .find_by_node_id(&seed_node_id)
            .await
            .unwrap()
            .unwrap();
        assert!(stored.flags.contains(PeerFlags::SEED));
        assert!(mock.is_peer_dialed(&seed_node_id).await);
    }

    /// A connection to a seed that already existed (e.g. the proactive dialer's, possibly this node's only one) is
    /// used for the sync and then left up.
    #[tokio::test]
    async fn it_leaves_an_existing_seed_connection_up() {
        let mut seed = make_node_identity().to_peer();
        seed.add_flags(PeerFlags::SEED);
        let seed_node_id = seed.node_id.clone();
        let (mut context, mock, _events) = context(None);
        let mut config = DhtConfig::default_local_test();
        config.network_discovery.bootstrap_rpc_connect_timeout = Duration::from_millis(100);
        context.config = Arc::new(config);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();

        let (conn, mut requests) =
            create_dummy_peer_connection_with_direction(seed_node_id.clone(), ConnectionDirection::Outbound);
        mock.add_active_connection(conn).await;
        // Record what is asked of the connection. RPC substreams are never answered, so the sync times out.
        let recorder = tokio::spawn(async move {
            let mut seen = Vec::new();
            while let Some(request) = requests.recv().await {
                match request {
                    PeerConnectionRequest::OpenSubstream { .. } => seen.push("open_substream"),
                    PeerConnectionRequest::Disconnect(_, reply_tx, _, _) => {
                        seen.push("disconnect");
                        let _ignore = reply_tx.send(Ok(()));
                    },
                }
            }
            seen
        });

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        assert!(mock.is_peer_dialed(&seed_node_id).await);
        // Drop every handle so the recorder finishes
        mock.remove_active_connection(&seed_node_id).await;
        let seen = tokio::time::timeout(Duration::from_secs(5), recorder)
            .await
            .unwrap()
            .unwrap();

        // The rebootstrap tried to sync over the connection, but did not hang up on it
        // Only by the seed path: connected seeds are left out of the connected-peer sync
        assert_eq!(
            seen.iter().filter(|r| **r == "open_substream").count(),
            1,
            "the seed connection was not used exactly once: {seen:?}"
        );
        assert!(
            !seen.contains(&"disconnect"),
            "an existing seed connection was disconnected"
        );
    }

    #[tokio::test]
    async fn it_does_not_dial_banned_seeds() {
        let mut seed = make_node_identity().to_peer();
        seed.add_flags(PeerFlags::SEED);
        seed.ban_for(Duration::from_secs(60 * 60), "test".to_string());
        let seed_node_id = seed.node_id.clone();
        let (context, mock, _events) = context(None);
        context.peer_manager.add_or_update_peer(seed).await.unwrap();

        let _event = Rebootstrap::new(context.clone()).next_event().await;
        assert!(!mock.is_peer_dialed(&seed_node_id).await);
    }

    #[tokio::test]
    async fn on_connect_resyncs_a_peer_once_the_ttl_has_passed() {
        let (context, _mock, _events) = context(None);
        let ttl = context.config.network_discovery.on_connect_resync_ttl;
        let mut on_connect = OnConnect::new(context);
        let node_id = NodeId::from_public_key(make_node_identity().public_key());
        let other = NodeId::from_public_key(make_node_identity().public_key());

        let now = Instant::now();
        assert!(!on_connect.is_recently_synced(&node_id, now));
        on_connect.mark_synced(node_id.clone(), now);
        assert!(on_connect.is_recently_synced(&node_id, now));
        assert!(on_connect.is_recently_synced(&node_id, now + ttl - Duration::from_secs(1)));
        assert!(!on_connect.is_recently_synced(&node_id, now + ttl));
        assert!(!on_connect.is_recently_synced(&other, now));

        // Expired entries are forgotten when another peer is recorded
        on_connect.mark_synced(other.clone(), now + ttl);
        assert!(on_connect.is_recently_synced(&other, now + ttl));
        assert_eq!(on_connect.prev_synced.len(), 1);
    }
}

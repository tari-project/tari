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
use std::{sync::Arc, time::Duration};

use tari_comms::{
    peer_manager::{Peer, PeerFeatures},
    test_utils::{
        mocks::{create_connectivity_mock, ConnectivityManagerMockState},
        node_identity::build_node_identity,
    },
    NodeIdentity,
    PeerManager,
};
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tokio::sync::broadcast;

use super::{DhtNetworkDiscovery, NetworkDiscoveryConfig};
use crate::{
    event::DhtEvent,
    test_utils::{build_peer_manager, make_node_identity},
    DhtConfig,
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
        let (discovery, _, _, _, _, mut shutdown) = setup(Default::default(), make_node_identity(), vec![]).await;

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
        network_discovery::{
            ready::DiscoveryReady,
            state_machine::{NetworkDiscoveryContext, StateEvent},
            DhtNetworkDiscoveryRoundInfo,
        },
        BootstrapMethod,
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

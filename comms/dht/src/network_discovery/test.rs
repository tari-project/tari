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
use super::{DhtNetworkDiscovery, NetworkDiscoveryConfig};
use crate::{
    event::DhtEvent,
    proto::rpc::GetPeersResponse,
    rpc,
    rpc::DhtRpcServiceMock,
    test_utils::{build_peer_manager, make_node_identity},
    DhtConfig,
};
use futures::StreamExt;
use std::{iter, sync::Arc, time::Duration};
use tari_comms::{
    connectivity::ConnectivityStatus,
    peer_manager::{Peer, PeerFeatures},
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
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
    )
    {
        // Every test needs these to be enabled
        config.network_discovery.enabled = true;

        let peer_manager = build_peer_manager();
        for peer in initial_peers {
            peer_manager.add_peer(peer).await.unwrap();
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
            config,
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

    #[tokio_macros::test_basic]
    async fn it_fetches_peers() {
        const NUM_PEERS: usize = 3;
        let config = DhtConfig {
            num_neighbouring_nodes: 4,
            network_discovery: NetworkDiscoveryConfig {
                min_desired_peers: NUM_PEERS,
                ..Default::default()
            },
            ..DhtConfig::default_local_test()
        };
        let peers = iter::repeat_with(|| make_node_identity().to_peer())
            .map(|p| GetPeersResponse { peer: Some(p.into()) })
            .take(NUM_PEERS)
            .collect();
        let (discovery_actor, connectivity_mock, peer_manager, node_identity, mut event_rx, _shutdown) =
            setup(config, make_node_identity(), vec![]).await;

        let mock = DhtRpcServiceMock::new();
        let service = rpc::DhtService::new(mock.clone());
        let protocol_name = service.as_protocol_name();

        let mut mock_server = MockRpcServer::new(service, node_identity.clone());
        let peer_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        // Add the peer that we'll sync from
        peer_manager.add_peer(peer_node_identity.to_peer()).await.unwrap();
        mock_server.serve();

        // Create a connection to the RPC mock and then make it available to the connectivity manager mock
        let connection = mock_server
            .create_connection(peer_node_identity.to_peer(), protocol_name.into())
            .await;

        connectivity_mock
            .set_connectivity_status(ConnectivityStatus::Online(NUM_PEERS))
            .await;
        connectivity_mock.add_active_connection(connection).await;

        mock.get_peers.set_response(Ok(peers)).await;
        discovery_actor.spawn();

        let event = event_rx.next().await.unwrap().unwrap();
        unpack_enum!(DhtEvent::NetworkDiscoveryPeersAdded(info) = &*event);
        assert_eq!(info.has_new_neighbours(), true);
        assert_eq!(info.num_new_neighbours, NUM_PEERS);
        assert_eq!(info.num_new_peers, NUM_PEERS);
        assert_eq!(info.num_duplicate_peers, 0);
        assert_eq!(info.num_succeeded, 1);
        assert_eq!(info.sync_peers, vec![peer_node_identity.node_id().clone()]);
    }

    #[tokio_macros::test_basic]
    async fn it_shuts_down() {
        let (discovery, _, _, _, _, mut shutdown) = setup(Default::default(), make_node_identity(), vec![]).await;

        shutdown.trigger().unwrap();
        tokio::time::timeout(Duration::from_secs(5), discovery.run())
            .await
            .unwrap();
    }
}

mod discovery_ready {
    use super::*;
    use crate::network_discovery::{
        ready::DiscoveryReady,
        state_machine::{NetworkDiscoveryContext, StateEvent},
        DhtNetworkDiscoveryRoundInfo,
    };
    use tari_comms::test_utils::mocks::ConnectivityManagerMock;

    fn setup(
        config: NetworkDiscoveryConfig,
        last_discovery: Option<DhtNetworkDiscoveryRoundInfo>,
    ) -> (
        Arc<NodeIdentity>,
        Arc<PeerManager>,
        ConnectivityManagerMock,
        DiscoveryReady,
    )
    {
        let peer_manager = build_peer_manager();
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let (event_tx, _) = broadcast::channel(1);
        let context = NetworkDiscoveryContext {
            config: DhtConfig {
                network_discovery: config,
                ..Default::default()
            },
            peer_manager: peer_manager.clone(),
            connectivity,
            node_identity: node_identity.clone(),
            num_rounds: Default::default(),
            event_tx,
        };

        let ready = match last_discovery {
            Some(r) => DiscoveryReady::new(context, r),
            None => DiscoveryReady::initial(context),
        };
        (node_identity, peer_manager, connectivity_mock, ready)
    }

    #[tokio_macros::test_basic]
    async fn it_begins_aggressive_discovery() {
        let config = NetworkDiscoveryConfig {
            min_desired_peers: 10,
            ..Default::default()
        };
        let (_, _, _, mut ready) = setup(config, None);
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::BeginDiscovery(params) = state_event);
        assert!(params.num_peers_to_request.is_none());
    }

    #[tokio_macros::test_basic]
    async fn it_idles_if_num_rounds_reached() {
        let config = NetworkDiscoveryConfig {
            min_desired_peers: 0,
            idle_after_num_rounds: 0,
            ..Default::default()
        };
        let (_, _, _, mut ready) = setup(
            config,
            Some(DhtNetworkDiscoveryRoundInfo {
                num_new_neighbours: 1,
                num_new_peers: 1,
                num_duplicate_peers: 0,
                num_succeeded: 1,
                sync_peers: vec![],
            }),
        );
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::Idle = state_event);
    }

    #[tokio_macros::test_basic]
    async fn it_transitions_to_on_connect() {
        let config = NetworkDiscoveryConfig {
            min_desired_peers: 0,
            idle_after_num_rounds: 0,
            ..Default::default()
        };
        let (_, _, _, mut ready) = setup(config, Some(Default::default()));
        let state_event = ready.next_event().await;
        unpack_enum!(StateEvent::OnConnectMode = state_event);
    }
}

//  Copyright 2021, The Tari Project
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

use std::{env, time::Duration};

use tari_shutdown::Shutdown;
use tari_test_utils::{async_assert_eventually, unpack_enum};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::{
    connection_manager::PeerConnection,
    protocol::{
        rpc::{
            test::{
                greeting_service::{GreetingClient, GreetingServer, GreetingService, SlowStreamRequest},
                mock::create_mocked_rpc_context,
            },
            NamedProtocolService,
            RpcServer,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
    },
    test_utils::mocks::{new_peer_connection_mock_pair, PeerConnectionMockState},
};

async fn setup(num_concurrent_sessions: usize) -> (PeerConnection, PeerConnectionMockState, Shutdown) {
    let (conn1, conn1_state, conn2, conn2_state) = new_peer_connection_mock_pair().await;
    let (notif_tx, notif_rx) = mpsc::channel(1);
    let shutdown = Shutdown::new();
    let (context, _) = create_mocked_rpc_context();

    tokio::spawn(
        RpcServer::builder()
            .with_maximum_simultaneous_sessions(num_concurrent_sessions)
            .finish()
            .add_service(GreetingServer::new(GreetingService::default()))
            .serve(notif_rx, context),
    );

    tokio::spawn(async move {
        while let Some(stream) = conn2_state.next_incoming_substream().await {
            notif_tx
                .send(ProtocolNotification::new(
                    ProtocolId::from_static(GreetingClient::PROTOCOL_NAME),
                    ProtocolEvent::NewInboundSubstream(conn2.peer_node_id().clone(), stream),
                ))
                .await
                .unwrap();
        }
    });

    (conn1, conn1_state, shutdown)
}

mod lazy_pool {
    use super::*;
    use crate::protocol::rpc::client::pool::{LazyPool, RpcClientPoolError};

    #[tokio::test]
    async fn it_connects_lazily() {
        let (conn, mock_state, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 2, Default::default());
        assert_eq!(mock_state.num_open_substreams(), 0);
        let _conn1 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        let _conn2 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 2);
    }

    #[tokio::test]
    async fn it_reuses_unused_connections() {
        let (conn, mock_state, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 2, Default::default());
        let _rpc_client_lease = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(pool.refresh_num_active_connections(), 1);
        async_assert_eventually!(mock_state.num_open_substreams(), expect = 1);
        let _second_lease = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(pool.refresh_num_active_connections(), 2);
        async_assert_eventually!(mock_state.num_open_substreams(), expect = 2);
    }

    #[tokio::test]
    async fn it_reuses_least_used_connections() {
        let (conn, mock_state, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 2, Default::default());
        let conn1 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        let conn2 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 2);
        let conn3 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(conn3.lease_count(), 2);
        assert!((conn1.lease_count() == 1) ^ (conn2.lease_count() == 1));
        assert_eq!(mock_state.num_open_substreams(), 2);
        let conn4 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(conn4.lease_count(), 2);
        assert_eq!(mock_state.num_open_substreams(), 2);

        assert_eq!(conn1.lease_count(), 2);
        assert_eq!(conn2.lease_count(), 2);
        assert_eq!(conn3.lease_count(), 2);
    }

    #[tokio::test]
    async fn it_reuses_used_connections_if_necessary() {
        let (conn, mock_state, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 1, Default::default());
        let conn1 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        let conn2 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        drop(conn1);
        drop(conn2);
    }

    #[tokio::test]
    async fn it_gracefully_handles_insufficient_server_sessions() {
        let (conn, mock_state, _shutdown) = setup(1).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 2, Default::default());
        let conn1 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        let conn2 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        assert_eq!(conn1.lease_count(), 2);
        assert_eq!(conn2.lease_count(), 2);
    }

    #[tokio::test]
    async fn it_prunes_disconnected_sessions() {
        let (conn, mock_state, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(conn, 2, Default::default());
        let mut client1 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 1);
        let _client2 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(mock_state.num_open_substreams(), 2);
        client1.close().await;
        drop(client1);
        async_assert_eventually!(mock_state.num_open_substreams(), expect = 1);
        assert_eq!(pool.refresh_num_active_connections(), 1);
        let _client3 = pool.get_least_used_or_connect().await.unwrap();
        assert_eq!(pool.refresh_num_active_connections(), 2);
        assert_eq!(mock_state.num_open_substreams(), 2);
    }

    #[tokio::test]
    async fn it_fails_when_peer_connected_disconnects() {
        let (mut peer_conn, _, _shutdown) = setup(2).await;
        let mut pool = LazyPool::<GreetingClient>::new(peer_conn.clone(), 2, Default::default());
        let mut _conn1 = pool.get_least_used_or_connect().await.unwrap();
        peer_conn.disconnect().await.unwrap();
        let err = pool.get_least_used_or_connect().await.unwrap_err();
        unpack_enum!(RpcClientPoolError::PeerConnectionDropped { .. } = err);
    }
}

mod last_request_latency {
    use super::*;

    #[tokio::test]
    async fn it_returns_the_latency_until_the_first_response() {
        let (mut conn, _, _shutdown) = setup(1).await;

        let mut client = conn.connect_rpc::<GreetingClient>().await.unwrap();

        let resp = client
            .slow_stream(SlowStreamRequest {
                num_items: 100,
                item_size: 10,
                delay_ms: 10,
            })
            .await
            .unwrap();

        resp.collect::<Vec<_>>().await.into_iter().for_each(|r| {
            r.unwrap();
        });

        let latency = client.get_last_request_latency().unwrap();
        // CI could be really slow, so to prevent flakiness exclude the assert
        if env::var("CI").is_err() {
            assert!(latency < Duration::from_millis(100));
        }
    }
}

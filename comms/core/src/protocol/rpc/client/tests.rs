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
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        rpc::{
            NamedProtocolService,
            RpcServer,
            test::{
                greeting_service::{GreetingClient, GreetingServer, GreetingService, SlowStreamRequest},
                mock::create_mocked_rpc_context,
            },
        },
    },
    test_utils::mocks::{PeerConnectionMockState, new_peer_connection_mock_pair},
};

async fn setup(num_concurrent_sessions: usize) -> (PeerConnection, PeerConnectionMockState, Shutdown) {
    setup_with_service(num_concurrent_sessions, GreetingService::default()).await
}

async fn setup_with_service(
    num_concurrent_sessions: usize,
    service: GreetingService,
) -> (PeerConnection, PeerConnectionMockState, Shutdown) {
    let (conn1, conn1_state, conn2, conn2_state) = new_peer_connection_mock_pair().await;
    let (notif_tx, notif_rx) = mpsc::channel(1);
    let shutdown = Shutdown::new();
    let (context, _) = create_mocked_rpc_context();

    tokio::spawn(
        RpcServer::builder()
            .with_maximum_simultaneous_sessions(num_concurrent_sessions)
            .finish()
            .add_service(GreetingServer::new(service))
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
    use crate::{
        Minimized,
        protocol::rpc::client::pool::{LazyPool, RpcClientPoolError},
    };

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
        peer_conn.disconnect(Minimized::No, "unit test").await.unwrap();
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

mod stream_buffer_size {
    use super::*;
    use crate::protocol::rpc::client::{RpcClientBuilder, RpcClientConfig};

    #[test]
    fn it_defaults_to_five() {
        assert_eq!(RpcClientConfig::default().stream_buffer_size, 5);
        let builder = RpcClientBuilder::<GreetingClient>::new();
        assert_eq!(builder.config.stream_buffer_size, 5);
    }

    #[test]
    fn it_is_set_by_the_builder() {
        let builder = RpcClientBuilder::<GreetingClient>::new().with_stream_buffer_size(100);
        assert_eq!(builder.config.stream_buffer_size, 100);
    }

    #[tokio::test]
    async fn a_zero_buffer_size_still_streams() {
        let (mut conn, _, _shutdown) = setup(1).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new().with_stream_buffer_size(0))
            .await
            .unwrap();
        let resp = client
            .slow_stream(SlowStreamRequest {
                num_items: 10,
                item_size: 10,
                delay_ms: 1,
            })
            .await
            .unwrap();
        let items = resp.collect::<Vec<_>>().await;
        assert_eq!(items.len(), 10);
        assert!(items.iter().all(|r| r.is_ok()));
    }
}

mod max_response_size {
    use super::*;
    use crate::protocol::{
        rpc,
        rpc::{
            RpcError,
            RpcPoolClient,
            RpcStatusCode,
            client::{RpcClientBuilder, RpcClientConfig},
            test::greeting_service::SayHelloRequest,
        },
    };

    #[test]
    fn it_defaults_to_the_frame_size() {
        assert_eq!(RpcClientConfig::default().max_response_size, rpc::RPC_MAX_FRAME_SIZE);
        let builder = RpcClientBuilder::<GreetingClient>::new();
        assert_eq!(builder.config.max_response_size, rpc::RPC_MAX_FRAME_SIZE);
    }

    #[test]
    fn it_is_set_by_the_builder() {
        let builder = RpcClientBuilder::<GreetingClient>::new().with_max_response_size(1234);
        assert_eq!(builder.config.max_response_size, 1234);
    }

    #[tokio::test]
    async fn items_within_the_cap_are_received() {
        let (mut conn, _, _shutdown) = setup(1).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new().with_max_response_size(100))
            .await
            .unwrap();
        // Each item is encoded as a protobuf `bytes` value: 2 bytes of overhead for 90 bytes
        let items = client
            .slow_stream(SlowStreamRequest {
                num_items: 5,
                item_size: 90,
                delay_ms: 1,
            })
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(items.len(), 5);
        assert!(items.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn a_streamed_item_over_the_cap_ends_the_request_and_the_session() {
        let (mut conn, _, _shutdown) = setup(1).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new().with_max_response_size(100))
            .await
            .unwrap();

        let items = client
            .slow_stream(SlowStreamRequest {
                num_items: 10,
                item_size: 1000,
                delay_ms: 1,
            })
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        // The caller gets an error in place of the item, and nothing after it
        assert_eq!(items.len(), 1);
        let err = items.into_iter().next().unwrap().unwrap_err();
        unpack_enum!(RpcError::RequestFailed(err) = err);
        assert_eq!(err.as_status_code(), RpcStatusCode::ProtocolError);
        assert!(err.details().contains("too large"), "{}", err.details());

        // As on other protocol errors, the session is closed so that the rest of the stream is never read as the
        // response to a later request
        async_assert_eventually!(client.is_connected(), expect = false);
        let err = client
            .say_hello(SayHelloRequest {
                name: "Ruby".to_string(),
                language: 0,
            })
            .await
            .unwrap_err();
        unpack_enum!(RpcError::ClientClosed = err);
    }

    #[tokio::test]
    async fn the_final_response_is_not_capped() {
        let (mut conn, _, _shutdown) = setup(1).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new().with_max_response_size(1))
            .await
            .unwrap();
        // A unary response is a single FIN message, which is never queued behind others
        let resp = client
            .say_hello(SayHelloRequest {
                name: "Ruby".to_string(),
                language: 0,
            })
            .await
            .unwrap();
        assert_eq!(resp.greeting, "Sawubona Ruby");
    }
}

mod buffered_response_bytes {
    use std::time::Duration;

    use super::*;
    use crate::protocol::rpc::client::{RPC_CLIENT_MAX_BUFFERED_RESPONSE_BYTES, RpcClientBuilder};

    const MIB: usize = 1024 * 1024;

    /// A caller that stops reading slows the server down at the byte budget, well before the stream buffer is full
    #[tokio::test]
    async fn a_slow_caller_slows_the_server_down() {
        const ITEM_SIZE: usize = 2 * MIB;
        const NUM_ITEMS: usize = 60;
        let service = GreetingService::default();
        let (mut conn, _, _shutdown) = setup_with_service(1, service.clone()).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new().with_stream_buffer_size(100))
            .await
            .unwrap();
        let mut stream = client
            .slow_stream(SlowStreamRequest {
                num_items: u32::try_from(NUM_ITEMS).unwrap(),
                item_size: u32::try_from(ITEM_SIZE).unwrap(),
                delay_ms: 0,
            })
            .await
            .unwrap();

        // Read one item, then stop reading
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.len(), ITEM_SIZE);
        tokio::time::sleep(Duration::from_secs(1)).await;
        let num_sent = service.slow_stream_items_sent();
        // The client buffers at most the budget plus one item. The rest of what the server sent is held by the server
        // (one queued, one prefetched and one being written) or in the substream's flow control window.
        const MAX_BUFFERED_ITEMS: usize = RPC_CLIENT_MAX_BUFFERED_RESPONSE_BYTES / ITEM_SIZE + 1;
        const MAX_IN_FLIGHT_ITEMS: usize = 3 + 16 * MIB / ITEM_SIZE;
        assert!(
            num_sent <= 1 + MAX_BUFFERED_ITEMS + MAX_IN_FLIGHT_ITEMS,
            "the server sent {num_sent} items to a caller that stopped reading"
        );
        assert!(num_sent >= MAX_BUFFERED_ITEMS, "the server sent only {num_sent} items");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            service.slow_stream_items_sent(),
            num_sent,
            "the server was not held back"
        );

        // Reading again lets the rest through
        let mut num_received = 1usize;
        while let Some(item) = stream.next().await {
            assert_eq!(item.unwrap().len(), ITEM_SIZE);
            num_received = num_received.saturating_add(1);
        }
        assert_eq!(num_received, NUM_ITEMS);
    }

    #[tokio::test]
    async fn items_that_fill_the_budget_still_stream() {
        let (mut conn, _, _shutdown) = setup(1).await;
        let mut client = conn
            .connect_rpc_using_builder(RpcClientBuilder::<GreetingClient>::new())
            .await
            .unwrap();
        // Each item is a fifth of the budget, so the budget, not the stream buffer size, is what holds them back
        let items = client
            .slow_stream(SlowStreamRequest {
                num_items: 10,
                item_size: u32::try_from(RPC_CLIENT_MAX_BUFFERED_RESPONSE_BYTES / 5).unwrap(),
                delay_ms: 0,
            })
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(items.len(), 10);
        assert!(items.iter().all(|r| r.is_ok()));
    }
}

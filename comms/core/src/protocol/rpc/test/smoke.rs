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

use futures::StreamExt;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tari_utilities::hex::Hex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{RwLock, mpsc},
    task,
    time,
};
use tokio_stream::Stream;

use crate::{
    NodeIdentity,
    Substream,
    framing,
    multiplexing::{Control, Yamux},
    peer_manager::NodeId,
    protocol::{
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        rpc,
        rpc::{
            RpcError,
            RpcPoolClient,
            RpcServer,
            RpcServerBuilder,
            RpcStatusCode,
            context::RpcCommsBackend,
            error::HandshakeRejectReason,
            handshake::RpcHandshakeError,
            server::NamedProtocolService,
            test::{
                greeting_service::{
                    GreetingClient,
                    GreetingRpc,
                    GreetingServer,
                    GreetingService,
                    SayHelloRequest,
                    SlowGreetingService,
                    SlowStreamRequest,
                },
                mock::create_mocked_rpc_context,
            },
        },
    },
    test_utils::{node_identity::build_node_identity, transport::build_multiplexed_connections},
};

pub(super) async fn setup_service_with_builder<T: GreetingRpc>(
    service_impl: T,
    builder: RpcServerBuilder,
) -> (
    mpsc::Sender<ProtocolNotification<Substream>>,
    task::JoinHandle<()>,
    RpcCommsBackend,
    Shutdown,
) {
    let (notif_tx, notif_rx) = mpsc::channel(10);
    let shutdown = Shutdown::new();
    let (context, _) = create_mocked_rpc_context();
    let server_hnd = task::spawn({
        let context = context.clone();
        let shutdown_signal = shutdown.to_signal();
        async move {
            let fut = builder
                .finish()
                .add_service(GreetingServer::new(service_impl))
                .serve(notif_rx, context);

            tokio::select! {
                biased;
                _ = shutdown_signal => {},
                r = fut => r.unwrap(),
            }
        }
    });

    (notif_tx, server_hnd, context, shutdown)
}

pub(super) async fn setup_service<T: GreetingRpc>(
    service_impl: T,
    num_concurrent_sessions: usize,
) -> (
    mpsc::Sender<ProtocolNotification<Substream>>,
    task::JoinHandle<()>,
    RpcCommsBackend,
    Shutdown,
) {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(num_concurrent_sessions)
        .with_minimum_client_deadline(Duration::from_secs(0));
    setup_service_with_builder(service_impl, builder).await
}

fn spawn_inbound(
    mut inbound: impl Stream<Item = Substream> + Unpin + Send + 'static,
    notif_tx: mpsc::Sender<ProtocolNotification<Substream>>,
    node_id: NodeId,
) -> task::JoinHandle<()> {
    task::spawn(async move {
        while let Some(stream) = inbound.next().await {
            notif_tx
                .send(ProtocolNotification::new(
                    ProtocolId::from_static(GreetingClient::PROTOCOL_NAME),
                    ProtocolEvent::NewInboundSubstream(node_id.clone(), stream),
                ))
                .await
                .unwrap();
        }
    })
}

pub(super) async fn setup<T: GreetingRpc>(
    service_impl: T,
    num_concurrent_sessions: usize,
) -> (Control, Yamux, task::JoinHandle<()>, Arc<NodeIdentity>, Shutdown) {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(num_concurrent_sessions)
        .with_minimum_client_deadline(Duration::from_secs(0));
    setup_with_builder(service_impl, builder).await
}

pub(super) async fn setup_with_builder<T: GreetingRpc>(
    service_impl: T,
    builder: RpcServerBuilder,
) -> (Control, Yamux, task::JoinHandle<()>, Arc<NodeIdentity>, Shutdown) {
    let (notif_tx, server_hnd, context, shutdown) = setup_service_with_builder(service_impl, builder).await;
    let (_, inbound, outbound) = build_multiplexed_connections().await;
    let inbound_control = inbound.get_yamux_control();

    let node_identity = build_node_identity(Default::default());
    let node_id = node_identity.node_id().clone();
    spawn_inbound(inbound.into_incoming(), notif_tx.clone(), node_id);

    // Notify that a peer wants to speak the greeting RPC protocol
    context
        .peer_manager()
        .add_or_update_peer(node_identity.to_peer())
        .await
        .unwrap();

    (inbound_control, outbound, server_hnd, node_identity, shutdown)
}

#[tokio::test]
async fn request_response_errors_and_streaming() {
    let (_inbound, outbound, server_hnd, node_identity, shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .with_deadline_grace_period(Duration::from_secs(5))
        .with_handshake_timeout(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // Latency is available "for free" as part of the connect protocol
    assert!(client.get_last_request_latency().is_some());

    let resp = client
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .unwrap();
    assert_eq!(resp.greeting, "Jambo Yathvan");

    let resp = client.get_greetings(4).await.unwrap();
    let greetings = resp.map(|r| r.unwrap()).collect::<Vec<_>>().await;
    assert_eq!(greetings, ["Sawubona", "Jambo", "Bonjour", "Hello"]);

    let err = client.return_error().await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::NotImplemented);
    assert_eq!(status.details(), "I haven't gotten to this yet :(");

    let stream = client.streaming_error("Gurglesplurb".to_string()).await.unwrap();
    let err = stream
        // StreamExt::collect has a Default trait bound which Result<_, _> cannot satisfy
        // so we must first collect the results into a Vec
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<String, _>>()
        .unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(status.details(), "What does 'Gurglesplurb' mean?");

    let stream = client.streaming_error2().await.unwrap();
    let results = stream.collect::<Vec<_>>().await;
    assert_eq!(results.len(), 2);
    let first_reply = results.first().unwrap().as_ref().unwrap();
    assert_eq!(first_reply, "This is ok");

    let second_reply = results.get(1).unwrap().as_ref().unwrap_err();
    unpack_enum!(RpcError::RequestFailed(second_reply) = second_reply);
    assert_eq!(second_reply.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(second_reply.details(), "This is a problem");

    let pk_hex = client.get_public_key_hex().await.unwrap();
    assert_eq!(pk_hex, node_identity.public_key().to_hex());

    client.close().await;

    let err = client
        .say_hello(SayHelloRequest {
            name: String::new(),
            language: 0,
        })
        .await
        .unwrap_err();

    match err {
        // Because of the race between closing the request stream and sending on that stream in the above call
        // We can either get "this client was closed" or "the request you made was cancelled".
        // If we delay some small time, we'll probably always get the former (but arbitrary delays cause flakiness and
        // should be avoided)
        RpcError::ClientClosed | RpcError::RequestCancelled => {},
        err => panic!("Unexpected error {err:?}"),
    }

    shutdown.trigger();
    server_hnd.await.unwrap();
}

#[tokio::test]
async fn concurrent_requests() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let mut cloned_client = client.clone();
    let spawned1 = task::spawn(async move {
        cloned_client
            .say_hello(SayHelloRequest {
                name: "Madeupington".to_string(),
                language: 2,
            })
            .await
            .unwrap()
    });
    let mut cloned_client = client.clone();
    let spawned2 = task::spawn(async move {
        let resp = cloned_client.get_greetings(5).await.unwrap().collect::<Vec<_>>().await;
        resp.into_iter().map(Result::unwrap).collect::<Vec<_>>()
    });
    let resp = client
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .unwrap();
    assert_eq!(resp.greeting, "Jambo Yathvan");

    assert_eq!(spawned1.await.unwrap().greeting, "Bonjour Madeupington");
    assert_eq!(spawned2.await.unwrap(), GreetingService::DEFAULT_GREETINGS[..5]);
}

#[tokio::test]
async fn response_too_big() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    // The frame limit applies to what the client reads, i.e. responses (requests have their own, smaller cap)
    let framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // RPC_MAX_FRAME_SIZE bytes will always be too large because of the overhead of the RpcResponse proto message
    let err = client
        .reply_with_msg_of_size(rpc::max_response_payload_size() as u64 - 4)
        .await
        .unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    unpack_enum!(RpcStatusCode::MalformedResponse = status.as_status_code());

    // Check that the exact frame size boundary works and that the session is still going
    let _string = client
        .reply_with_msg_of_size(rpc::max_response_payload_size() as u64 - 5)
        .await
        .unwrap();
}

#[tokio::test]
async fn ping_latency() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder().connect(framed).await.unwrap();

    let latency = client.ping().await.unwrap();
    // This is plenty (typically would be < 1ms over MemorySocket), however CI can be very slow, so to prevent flakiness
    // we leave a wide berth
    assert!(latency.as_secs() < 5);
}

#[tokio::test]
async fn server_shutdown_before_connect() {
    let (_inbound, outbound, _, _, shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    shutdown.trigger();

    let err = GreetingClient::connect(framed).await.unwrap_err();
    assert!(matches!(
        err,
        RpcError::HandshakeError(RpcHandshakeError::ServerClosedRequest)
    ));
}

#[tokio::test]
async fn timeout() {
    let delay = Arc::new(RwLock::new(Duration::from_secs(10)));
    let (_inbound, outbound, _, _, _shutdown) = setup(SlowGreetingService::new(delay.clone()), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(1))
        .with_deadline_grace_period(Duration::from_secs(1))
        .connect(framed)
        .await
        .unwrap();

    let err = client.say_hello(Default::default()).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::Timeout);

    *delay.write().await = Duration::from_secs(0);

    // The server should have hit the deadline and "reset" by waiting for another request without sending a response.
    // Test that this happens by checking that the next request is furnished correctly
    let resp = client.say_hello(Default::default()).await.unwrap();
    assert_eq!(resp.greeting, "took a while to load");
}

/// A peer asking for a deadline beyond the server's ceiling must be held to the ceiling, and must
/// be *told* when the request runs past it - not left waiting out its own, much longer, deadline.
#[tokio::test]
async fn client_deadline_is_capped_and_the_client_is_told() {
    let delay = Arc::new(RwLock::new(Duration::from_secs(60)));
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(1)
        .with_minimum_client_deadline(Duration::from_secs(0))
        .with_maximum_client_deadline(Duration::from_secs(1));
    let (_inbound, outbound, _, _, _shutdown) =
        setup_with_builder(SlowGreetingService::new(delay.clone()), builder).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);

    // Ask for far more than the server allows. If the cap were not applied on the wire, or if the
    // server went silent instead of replying, this request would not resolve until the client's own
    // deadline expires - well past the timeout below.
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(600))
        .with_deadline_grace_period(Duration::from_secs(60))
        .connect(framed)
        .await
        .unwrap();

    let result = time::timeout(Duration::from_secs(10), client.say_hello(Default::default()))
        .await
        .expect("client was not told about the capped deadline and waited on its own instead");

    let err = result.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.as_status_code(), RpcStatusCode::Timeout);

    // The session survives: the next request is served normally.
    *delay.write().await = Duration::from_secs(0);
    let resp = client.say_hello(Default::default()).await.unwrap();
    assert_eq!(resp.greeting, "took a while to load");
}

#[tokio::test]
async fn unknown_protocol() {
    let (notif_tx, _, _, _shutdown) = setup_service(GreetingService::new(&[]), 1).await;

    let (_, inbound, mut outbound) = build_multiplexed_connections().await;
    let mut in_substream = inbound.get_yamux_control().open_stream().await.unwrap();
    // To avoid having to spawn a inbound task, we can just write to the stream directly to initiate a substream
    in_substream.write_all(b"hello").await.unwrap();

    let node_identity = build_node_identity(Default::default());

    // This case should never happen because protocols are preregistered with the connection manager and so a
    // protocol notification should never be sent out if it is unrecognised. However it is still not a bad
    // idea to test the behaviour.
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"this-is-junk"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), in_substream),
        ))
        .await
        .unwrap();

    let mut out_socket = outbound.incoming_mut().next().await.unwrap();
    // Read "hello"
    out_socket.read_exact(&mut [0u8; 5]).await.unwrap();
    let framed = framing::canonical(out_socket, 1024);
    let err = GreetingClient::connect(framed).await.unwrap_err();
    assert!(matches!(
        err,
        RpcError::HandshakeError(RpcHandshakeError::Rejected(HandshakeRejectReason::ProtocolNotSupported))
    ));
}

#[tokio::test]
async fn rejected_no_sessions_available() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 0).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder().connect(framed).await.unwrap_err();
    assert!(matches!(
        err,
        RpcError::HandshakeError(RpcHandshakeError::Rejected(
            HandshakeRejectReason::NoServerSessionsAvailable(_)
        ))
    ));
}

#[tokio::test]
async fn stream_still_works_after_cancel() {
    let service_impl = GreetingService::default();
    let (_inbound, outbound, _, _, _shutdown) = setup(service_impl.clone(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // Ask for a stream, but immediately throw away the receiver
    client
        .slow_stream(SlowStreamRequest {
            num_items: 100,
            item_size: 100,
            delay_ms: 10,
        })
        .await
        .unwrap();
    // Request was sent
    assert_eq!(service_impl.call_count(), 1);

    // Subsequent call still works
    let resp = client
        .slow_stream(SlowStreamRequest {
            num_items: 100,
            item_size: 100,
            delay_ms: 10,
        })
        .await
        .unwrap();

    resp.collect::<Vec<_>>().await.into_iter().for_each(|r| {
        r.unwrap();
    });
}

/// A server stream holds no client handle, so callers routinely drop the `RpcClient` and keep reading the stream (e.g.
/// DHT network discovery's `get_peers`). Dropping the last client handle must not cut that stream short: every item and
/// then the end of the stream must still arrive.
#[tokio::test]
async fn stream_completes_after_client_is_dropped() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    const NUM_ITEMS: u32 = 20;
    let stream = client
        .slow_stream(SlowStreamRequest {
            num_items: NUM_ITEMS,
            item_size: 100,
            delay_ms: 10,
        })
        .await
        .unwrap();
    drop(client);

    let items = time::timeout(Duration::from_secs(10), stream.collect::<Vec<_>>())
        .await
        .expect("stream did not end after the client was dropped");
    assert_eq!(items.len(), NUM_ITEMS as usize);
    for item in items {
        assert_eq!(item.unwrap().len(), 100);
    }
}

/// In contrast to dropping the client, an explicit `close()` cuts an in-flight stream short.
#[tokio::test]
async fn stream_ends_early_after_client_is_closed() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    const NUM_ITEMS: u32 = 1000;
    let stream = client
        .slow_stream(SlowStreamRequest {
            num_items: NUM_ITEMS,
            item_size: 100,
            delay_ms: 10,
        })
        .await
        .unwrap();
    client.close().await;

    let items = time::timeout(Duration::from_secs(10), stream.collect::<Vec<_>>())
        .await
        .expect("stream did not end after the client was closed");
    assert!(items.len() < NUM_ITEMS as usize);
}

/// `close()` must interrupt a stream that is waiting for a slow frame, not only take effect when the next frame
/// arrives.
#[tokio::test]
async fn close_interrupts_a_stream_waiting_for_a_slow_frame() {
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(30))
        .connect(framed)
        .await
        .unwrap();

    // The server sleeps this long before every item, so no frame arrives while the client is being closed
    const FRAME_DELAY: Duration = Duration::from_secs(10);
    let stream = client
        .slow_stream(SlowStreamRequest {
            num_items: 2,
            item_size: 100,
            delay_ms: u64::try_from(FRAME_DELAY.as_millis()).unwrap(),
        })
        .await
        .unwrap();
    client.close().await;

    let items = time::timeout(Duration::from_secs(3), stream.collect::<Vec<_>>())
        .await
        .expect("close() did not interrupt a stream waiting for a slow frame");
    assert!(items.is_empty());
}

/// A peer that opens a streaming request and then stops draining its yamux window used to park the
/// server, and keep its session slot, indefinitely.
///
/// Two separate bounds are needed. The read timeout does not cover `framed.send`, and the outer
/// session loop's idle timer does not tick while a request is being handled - so the write parks.
/// Bounding only the write is not enough either: `run()` then tries to close the substream
/// gracefully, and `EarlyClose::poll_close` returns `Pending` for a peer that is merely silent, so
/// the task parks one level up instead. Either way the session's `BoundedExecutor` permit is held
/// until `start()` returns, which is what actually locks other peers out of the node.
///
/// So the assertion here is the one that matters: another session can still be opened.
#[tokio::test]
async fn a_peer_that_stops_reading_does_not_hold_its_session_slot() {
    const NUM_ITEMS: u32 = 512;
    // A single global session, so the second handshake below succeeds only if the first session's
    // slot was genuinely reclaimed.
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(1)
        .with_minimum_client_deadline(Duration::from_secs(0));
    let (_inbound, outbound, _, _, _shutdown) = setup_with_builder(GreetingService::default(), builder).await;

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    let mut stalled_client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(2))
        .with_deadline_grace_period(Duration::from_secs(1))
        .connect(framed)
        .await
        .unwrap();

    // Far more data than a yamux window holds, produced as fast as the server can send it.
    let _stalled_stream = stalled_client
        .slow_stream(SlowStreamRequest {
            num_items: NUM_ITEMS,
            item_size: 64 * 1024,
            delay_ms: 0,
        })
        .await
        .unwrap();

    // Never drain it. Wait out the write deadline and the close timeout with room to spare, and do
    // not touch `_stalled_stream` - reading it would unblock the server and mask the bug.
    time::sleep(Duration::from_secs(10)).await;

    // The slot must be free for somebody else.
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .expect("session slot was not reclaimed from the stalled peer");

    let resp = client
        .say_hello(SayHelloRequest {
            name: "Norman".to_string(),
            language: 0,
        })
        .await
        .unwrap();
    assert_eq!(resp.greeting, "Sawubona Norman");
}

#[tokio::test]
async fn stream_interruption_handling() {
    let service_impl = GreetingService::default();
    let (_inbound, outbound, _, _, _shutdown) = setup(service_impl.clone(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let mut resp = client
        .slow_stream(SlowStreamRequest {
            num_items: 10000,
            item_size: 100,
            delay_ms: 100,
        })
        .await
        .unwrap();

    let _buffer = resp.next().await.unwrap().unwrap();
    // Drop it before the stream is finished
    drop(resp);

    // Subsequent call still works, without waiting
    let mut resp = client
        .slow_stream(SlowStreamRequest {
            num_items: 100,
            item_size: 100,
            delay_ms: 1,
        })
        .await
        .unwrap();

    let next_fut = resp.next();
    tokio::pin!(next_fut);
    // Allow 10 seconds, if the previous stream is still streaming, it will take a while for this stream to start and
    // the timeout will expire
    time::timeout(Duration::from_secs(10), next_fut)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn max_global_sessions() {
    let builder = RpcServer::builder().with_maximum_simultaneous_sessions(1);
    let (muxer, _outbound, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, inbound, outbound) = build_multiplexed_connections().await;

    let node_identity = build_node_identity(Default::default());
    // Notify that a peer wants to speak the greeting RPC protocol
    context
        .peer_manager()
        .add_or_update_peer(node_identity.to_peer())
        .await
        .unwrap();

    spawn_inbound(inbound.into_incoming(), muxer.clone(), node_identity.node_id().clone());

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap_err();

    unpack_enum!(RpcError::HandshakeError(err) = err);
    unpack_enum!(
        RpcHandshakeError::Rejected(HandshakeRejectReason::NoServerSessionsAvailable(
            "session limit reached"
        )) = err
    );

    client.close().await;

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let _client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();
}

#[tokio::test]
async fn idle_sessions_are_closed_and_their_slot_reclaimed() {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(1)
        .with_minimum_client_deadline(Duration::from_secs(0))
        .with_idle_session_timeout(Duration::from_secs(1));
    let (muxer, _outbound, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, inbound, outbound) = build_multiplexed_connections().await;

    let node_identity = build_node_identity(Default::default());
    context
        .peer_manager()
        .add_or_update_peer(node_identity.to_peer())
        .await
        .unwrap();
    spawn_inbound(inbound.into_incoming(), muxer.clone(), node_identity.node_id().clone());

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // A session that keeps making requests is never closed, even once it has been alive for longer
    // than the idle timeout - each request restarts the clock.
    for _ in 0..3 {
        time::sleep(Duration::from_millis(400)).await;
        client.say_hello(Default::default()).await.unwrap();
    }
    assert!(client.is_connected());

    // Once it goes quiet for longer than the timeout, the server closes the session out from under
    // it. The client only notices when it next tries to use it - it does not read the substream
    // while no request is in flight.
    time::sleep(Duration::from_millis(2500)).await;
    client.say_hello(Default::default()).await.unwrap_err();

    // ...and the slot it held in the global session limit of 1 is free again.
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let _client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();
}

#[tokio::test]
async fn max_per_client_sessions() {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(3)
        .with_maximum_sessions_per_client(1)
        .with_cull_oldest_peer_rpc_connection_on_full(false);
    let (muxer, _outbound, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, inbound, outbound) = build_multiplexed_connections().await;

    let node_identity = build_node_identity(Default::default());
    // Notify that a peer wants to speak the greeting RPC protocol
    context
        .peer_manager()
        .add_or_update_peer(node_identity.to_peer())
        .await
        .unwrap();
    spawn_inbound(inbound.into_incoming(), muxer.clone(), node_identity.node_id().clone());

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap_err();

    unpack_enum!(RpcError::HandshakeError(err) = err);
    unpack_enum!(
        RpcHandshakeError::Rejected(HandshakeRejectReason::NoServerSessionsAvailable(
            "session limit reached"
        )) = err
    );

    drop(client);

    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let _client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();
}

/// Requests larger than `RPC_MAX_REQUEST_SIZE` are answered with `BadRequest` before they are decoded; a request of
/// exactly that size is handled normally, and the session carries on after a rejection.
#[tokio::test]
async fn oversized_requests_are_rejected_before_decoding() {
    use futures::SinkExt;
    use prost::Message;

    use crate::{
        proto,
        protocol::rpc::{handshake::Handshake, message::RpcMessageFlags},
    };

    /// An encoded `say_hello` request (method 1) of exactly `size` bytes
    fn say_hello_request(request_id: u32, size: usize) -> Vec<u8> {
        let encode = |name_len: usize| {
            let payload = SayHelloRequest {
                name: "a".repeat(name_len),
                language: 0,
            }
            .encode_to_vec();
            proto::rpc::RpcRequest {
                request_id,
                method: 1,
                flags: 0,
                deadline: 10,
                payload,
            }
            .encode_to_vec()
        };
        // Varint lengths make the overhead depend slightly on the size, so converge on it
        let mut name_len = size;
        loop {
            let len = encode(name_len).len();
            if len == size {
                return encode(name_len);
            }
            name_len = (name_len + size).checked_sub(len).unwrap();
        }
    }

    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let mut framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    Handshake::new(&mut framed).perform_client_handshake().await.unwrap();

    // One byte over the limit: rejected before it is decoded or passed to the service, with the request id echoed back
    // so that the client can match the response
    let oversized = say_hello_request(7, rpc::RPC_MAX_REQUEST_SIZE + 1);
    framed.send(oversized.into()).await.unwrap();
    let resp = proto::rpc::RpcResponse::decode(framed.next().await.unwrap().unwrap()).unwrap();
    assert_eq!(resp.request_id, 7);
    assert_eq!(RpcStatusCode::from(resp.status), RpcStatusCode::BadRequest);
    assert!(
        RpcMessageFlags::from_bits(u8::try_from(resp.flags).unwrap())
            .unwrap()
            .is_fin()
    );
    let details = String::from_utf8(resp.payload).unwrap();
    assert!(details.contains("request size exceeded"), "{details}");

    // Exactly at the limit: handled by the service
    let at_limit = say_hello_request(8, rpc::RPC_MAX_REQUEST_SIZE);
    assert_eq!(at_limit.len(), rpc::RPC_MAX_REQUEST_SIZE);
    framed.send(at_limit.into()).await.unwrap();
    let resp = proto::rpc::RpcResponse::decode(framed.next().await.unwrap().unwrap()).unwrap();
    assert_eq!(resp.request_id, 8);
    assert_eq!(RpcStatusCode::from(resp.status), RpcStatusCode::Ok);
}

/// An oversized message received while the server is streaming a response is ignored without being decoded, and the
/// stream carries on.
#[tokio::test]
async fn oversized_messages_during_a_stream_are_ignored() {
    use futures::SinkExt;
    use prost::Message;

    use crate::{
        proto,
        protocol::rpc::{handshake::Handshake, message::RpcMessageFlags},
    };

    // Enough items that the server reads the (large) interrupting frame completely while still streaming: it reads
    // pending input once per streamed item
    const NUM_ITEMS: u32 = 300;
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let mut framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    Handshake::new(&mut framed).perform_client_handshake().await.unwrap();

    // slow_stream (method 8)
    let request = proto::rpc::RpcRequest {
        request_id: 1,
        method: 8,
        flags: 0,
        deadline: 10,
        payload: SlowStreamRequest {
            num_items: NUM_ITEMS,
            item_size: 10,
            delay_ms: 10,
        }
        .encode_to_vec(),
    };
    framed.send(request.encode_to_vec().into()).await.unwrap();
    let first = proto::rpc::RpcResponse::decode(framed.next().await.unwrap().unwrap()).unwrap();
    assert_eq!(RpcStatusCode::from(first.status), RpcStatusCode::Ok);

    // A message larger than the request cap, mid-stream (not a FIN)
    let oversized = proto::rpc::RpcRequest {
        request_id: 2,
        method: 1,
        flags: 0,
        deadline: 10,
        payload: vec![0; rpc::RPC_MAX_REQUEST_SIZE],
    };
    framed.send(oversized.encode_to_vec().into()).await.unwrap();

    // The rest of the stream still arrives
    let mut items = 1;
    loop {
        let resp = proto::rpc::RpcResponse::decode(framed.next().await.unwrap().unwrap()).unwrap();
        assert_eq!(RpcStatusCode::from(resp.status), RpcStatusCode::Ok);
        if !resp.payload.is_empty() {
            items += 1;
        }
        if RpcMessageFlags::from_bits(u8::try_from(resp.flags).unwrap())
            .unwrap()
            .is_fin()
        {
            break;
        }
    }
    assert_eq!(items, NUM_ITEMS);
}

/// When the client reads slowly, sends block on the yamux window and the server holds the next item while it waits.
/// Every item must still arrive exactly once and in order, followed by the end of the stream.
#[tokio::test]
async fn a_slow_reader_receives_every_item_of_a_prefetched_stream() {
    const NUM_ITEMS: u32 = 20;
    const ITEM_SIZE: u32 = 1024 * 1024;
    let (_inbound, outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = outbound.get_yamux_control().open_stream().await.unwrap();
    let framed = framing::canonical(socket, rpc::RPC_MAX_FRAME_SIZE);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(10))
        .with_stream_buffer_size(1)
        .connect(framed)
        .await
        .unwrap();

    let mut resp = client
        .slow_stream(SlowStreamRequest {
            num_items: NUM_ITEMS,
            item_size: ITEM_SIZE,
            delay_ms: 0,
        })
        .await
        .unwrap();

    // `slow_stream` stamps each item's index into its first 4 bytes
    let mut indices = Vec::new();
    while let Some(item) = resp.next().await {
        let item = item.unwrap();
        assert_eq!(item.len(), ITEM_SIZE as usize);
        indices.push(u32::from_le_bytes(item[..4].try_into().unwrap()));
        time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(indices, (0..NUM_ITEMS).collect::<Vec<_>>());

    // The session is still usable afterwards
    let resp = client
        .say_hello(SayHelloRequest {
            name: "Ruby".to_string(),
            language: 0,
        })
        .await
        .unwrap();
    assert_eq!(resp.greeting, "Sawubona Ruby");
}

/// Connects a new peer to the server behind `notif_tx`, returning its identity and its end of the connection
async fn connect_peer(
    notif_tx: &mpsc::Sender<ProtocolNotification<Substream>>,
    context: &RpcCommsBackend,
) -> (Arc<NodeIdentity>, Yamux) {
    let (_, inbound, outbound) = build_multiplexed_connections().await;
    let node_identity = build_node_identity(Default::default());
    context
        .peer_manager()
        .add_or_update_peer(node_identity.to_peer())
        .await
        .unwrap();
    spawn_inbound(
        inbound.into_incoming(),
        notif_tx.clone(),
        node_identity.node_id().clone(),
    );
    (node_identity, outbound)
}

/// Opens a substream for the greeting protocol and never completes the handshake. It sends one byte (the start of a
/// frame length prefix): yamux opens a stream lazily, so the server only learns of it when data arrives.
async fn open_silent_substream(peer: &Yamux) -> Substream {
    let mut substream = peer.get_yamux_control().open_stream().await.unwrap();
    substream.write_all(&[0]).await.unwrap();
    substream.flush().await.unwrap();
    substream
}

/// Whether the server has closed (or dropped) `substream` within `wait`
async fn is_closed_within(substream: &mut Substream, wait: Duration) -> bool {
    let mut buf = [0u8; 16];
    match time::timeout(wait, substream.read(&mut buf)).await {
        // EOF or an error: the server dropped it
        Ok(Ok(0)) | Ok(Err(_)) => true,
        Ok(Ok(n)) => panic!("unexpected {n} byte(s) on a substream that never sent a handshake"),
        Err(_elapsed) => false,
    }
}

async fn connect_greeting_client(peer: &Yamux) -> Result<GreetingClient, RpcError> {
    let socket = peer.get_yamux_control().open_stream().await.unwrap();
    GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .with_handshake_timeout(Duration::from_secs(5))
        .connect(framing::canonical(socket, 1024))
        .await
}

/// Handshakes run off the accept loop: a peer holding substreams open without ever handshaking does not delay another
/// peer's session, even with a long handshake timeout
#[tokio::test]
async fn silent_substreams_do_not_stall_other_peers() {
    let builder = RpcServer::builder().with_handshake_timeout(Duration::from_secs(60));
    let (notif_tx, _server, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;

    let (_, silent_peer) = connect_peer(&notif_tx, &context).await;
    let mut silent = Vec::new();
    for _ in 0..2 {
        silent.push(open_silent_substream(&silent_peer).await);
    }

    let (_, honest_peer) = connect_peer(&notif_tx, &context).await;
    let mut client = time::timeout(Duration::from_secs(5), connect_greeting_client(&honest_peer))
        .await
        .expect("the session was held up behind the silent handshakes")
        .unwrap();
    let resp = client
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .unwrap();
    assert_eq!(resp.greeting, "Jambo Yathvan");

    // The silent substreams are still waiting for their handshakes
    for substream in &mut silent {
        assert!(!is_closed_within(substream, Duration::from_millis(200)).await);
    }
}

/// Pending handshakes are capped per peer and in total: a substream over either cap is refused at once with an
/// explicit `NoServerSessionsAvailable` rejection (not dropped, which a client pool would treat as a failure to
/// connect). A handshake that times out releases its slot.
#[tokio::test]
async fn pending_handshakes_are_capped_and_released_on_timeout() {
    let builder = RpcServer::builder()
        .with_handshake_timeout(Duration::from_secs(2))
        .with_maximum_pending_handshakes_per_client(2)
        .with_maximum_pending_handshakes(3);
    let (notif_tx, _server, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;

    // Peer A fills its own cap; its next session is refused
    let (_, peer_a) = connect_peer(&notif_tx, &context).await;
    let mut a1 = open_silent_substream(&peer_a).await;
    let mut a2 = open_silent_substream(&peer_a).await;
    assert_explicitly_rejected(&peer_a).await;

    // Peer B takes the last global slot; peer C is then refused by the global cap
    let (_, peer_b) = connect_peer(&notif_tx, &context).await;
    let mut b1 = open_silent_substream(&peer_b).await;
    assert!(!is_closed_within(&mut b1, Duration::from_millis(200)).await);
    let (_, peer_c) = connect_peer(&notif_tx, &context).await;
    assert_explicitly_rejected(&peer_c).await;
    assert!(!is_closed_within(&mut a1, Duration::from_millis(10)).await);

    // Once the pending handshakes time out, they are closed and their slots released: A and C can both connect
    for substream in [&mut a1, &mut a2, &mut b1] {
        assert!(
            is_closed_within(substream, Duration::from_secs(5)).await,
            "handshake did not time out"
        );
    }
    let _client_a = connect_greeting_client(&peer_a).await.unwrap();
    let _client_c = connect_greeting_client(&peer_c).await.unwrap();
}

/// A handshake in progress does not outlive the server: shutting it down aborts the handshake task and closes the
/// substream, well before the handshake timeout
#[tokio::test]
async fn pending_handshakes_end_with_the_server() {
    let builder = RpcServer::builder().with_handshake_timeout(Duration::from_secs(60));
    let (notif_tx, server, context, shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, peer) = connect_peer(&notif_tx, &context).await;
    let mut silent = open_silent_substream(&peer).await;
    assert!(!is_closed_within(&mut silent, Duration::from_millis(200)).await);

    shutdown.trigger();
    server.await.unwrap();
    assert!(is_closed_within(&mut silent, Duration::from_secs(5)).await);
}

/// A handshake frame (length prefix and `RpcSession`)
fn handshake_frame() -> Vec<u8> {
    use crate::{message::MessageExt, proto};

    let handshake = proto::rpc::RpcSession {
        supported_versions: vec![0],
    }
    .to_encoded_bytes();
    let mut frame = u32::try_from(handshake.len()).unwrap().to_be_bytes().to_vec();
    frame.extend_from_slice(&handshake);
    frame
}

/// Starts a handshake and holds it open after its first byte. Waits for the server to have taken it in (and, if it is
/// accepted, reserved its session).
async fn hold_handshake(peer: &Yamux) -> Substream {
    let mut held = peer.get_yamux_control().open_stream().await.unwrap();
    held.write_all(handshake_frame().get(..1).unwrap()).await.unwrap();
    held.flush().await.unwrap();
    time::sleep(Duration::from_millis(200)).await;
    held
}

/// Completes a held handshake and asserts it is accepted and its session stays open
async fn complete_held_handshake(mut held: Substream) -> framing::CanonicalFraming<Substream> {
    use prost::Message;

    use crate::proto;

    held.write_all(handshake_frame().get(1..).unwrap()).await.unwrap();
    held.flush().await.unwrap();
    let mut held = framing::canonical(held, 1024);
    let reply = time::timeout(Duration::from_secs(5), held.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let reply = proto::rpc::RpcSessionReply::decode(reply.freeze()).unwrap();
    assert_eq!(
        reply.session_result,
        Some(proto::rpc::rpc_session_reply::SessionResult::AcceptedVersion(0))
    );
    assert!(
        time::timeout(Duration::from_millis(500), held.next()).await.is_err(),
        "an accepted handshake was dropped"
    );
    held
}

/// Asserts that connecting fails with an explicit `NoServerSessionsAvailable` rejection, which a client pool treats as
/// "use an existing session"
async fn assert_explicitly_rejected(peer: &Yamux) {
    let Err(rejection) = connect_greeting_client(peer).await else {
        panic!("the session was not rejected");
    };
    unpack_enum!(RpcError::HandshakeError(rejection) = rejection);
    unpack_enum!(RpcHandshakeError::Rejected(HandshakeRejectReason::NoServerSessionsAvailable(_)) = rejection);
}

/// With one session slot left, two overlapping handshakes from one peer get exactly one session and one explicit
/// rejection. The first handshake is held open while the second arrives, so they overlap. Neither may be accepted and
/// then dropped, which the client would only see as the substream closing on its first request (`ServerClosedRequest`,
/// blamed on the server).
async fn assert_one_session_and_one_explicit_rejection(builder: RpcServerBuilder) {
    let (notif_tx, _server, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, peer) = connect_peer(&notif_tx, &context).await;
    // Takes all but one slot
    let _first = connect_greeting_client(&peer).await.unwrap();

    let held = hold_handshake(&peer).await;
    // The second handshake, while the first is pending: explicitly rejected
    assert_explicitly_rejected(&peer).await;
    // The first completes: it is accepted, and its session stays open
    complete_held_handshake(held).await;
}

#[tokio::test]
async fn concurrent_handshakes_at_the_global_limit_are_rejected_not_dropped() {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(2)
        .with_unlimited_sessions_per_client();
    assert_one_session_and_one_explicit_rejection(builder).await;
}

#[tokio::test]
async fn concurrent_handshakes_at_the_per_client_limit_are_rejected_not_dropped() {
    let builder = RpcServer::builder()
        .with_maximum_simultaneous_sessions(100)
        .with_maximum_sessions_per_client(2);
    assert_one_session_and_one_explicit_rejection(builder).await;
}

/// In cull mode, the cull decision counts accepted handshakes: a new request culls the oldest running sessions when
/// that leaves room for it, alongside the handshakes already accepted, and is then accepted. It is never refused after
/// sessions were culled for it.
#[tokio::test]
async fn culling_counts_accepted_handshakes() {
    let builder = RpcServer::builder()
        .with_maximum_sessions_per_client(2)
        .with_cull_oldest_peer_rpc_connection_on_full(true);
    let (notif_tx, _server, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, peer) = connect_peer(&notif_tx, &context).await;
    let _oldest = connect_greeting_client(&peer).await.unwrap();
    let mut second = connect_greeting_client(&peer).await.unwrap();

    // At the limit: this culls the oldest session and is accepted (and reserved)
    let held = hold_handshake(&peer).await;
    // One running session and one accepted handshake: this culls the second session and is accepted
    let mut newest = connect_greeting_client(&peer).await.unwrap();
    let greeting = newest
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .unwrap();
    assert_eq!(greeting.greeting, "Jambo Yathvan");
    second
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .expect_err("the second session was not culled");
    complete_held_handshake(held).await;
}

/// In cull mode, accepted handshakes are never culled: once they alone fill the peer's limit, a further request is
/// explicitly rejected, and the accepted handshakes still get their sessions
#[tokio::test]
async fn culling_cannot_make_room_past_accepted_handshakes() {
    let builder = RpcServer::builder()
        .with_maximum_sessions_per_client(2)
        .with_cull_oldest_peer_rpc_connection_on_full(true);
    let (notif_tx, _server, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, peer) = connect_peer(&notif_tx, &context).await;
    let mut running = connect_greeting_client(&peer).await.unwrap();

    // One running session: accepted without culling
    let held_a = hold_handshake(&peer).await;
    // One running and one accepted: culls the running session, accepted
    let held_b = hold_handshake(&peer).await;
    running
        .say_hello(SayHelloRequest {
            name: "Yathvan".to_string(),
            language: 1,
        })
        .await
        .expect_err("the running session was not culled");

    // Two accepted handshakes fill the limit: rejected
    assert_explicitly_rejected(&peer).await;
    let _a = complete_held_handshake(held_a).await;
    let _b = complete_held_handshake(held_b).await;
}

/// A client is only told its session was accepted once the server counts it. With the in-memory transport the server's
/// status query races the session's registration as closely as it can: on every one of many connects, the count read
/// straight after `connect` returns already includes the new session.
#[tokio::test]
async fn a_session_is_counted_before_the_client_is_told_it_was_accepted() {
    const NUM_SESSIONS: usize = 50;
    let (notif_tx, notif_rx) = mpsc::channel(10);
    let (context, _) = create_mocked_rpc_context();
    let server = RpcServer::builder()
        .with_maximum_sessions_per_client(NUM_SESSIONS)
        .with_maximum_simultaneous_sessions(NUM_SESSIONS)
        .finish();
    let mut handle = server.get_handle();
    let _server = task::spawn({
        let context = context.clone();
        async move {
            server
                .add_service(GreetingServer::new(GreetingService::default()))
                .serve(notif_rx, context)
                .await
                .unwrap();
        }
    });
    let (node_identity, peer) = connect_peer(&notif_tx, &context).await;

    let mut clients = Vec::with_capacity(NUM_SESSIONS);
    for expected in 1..=NUM_SESSIONS {
        clients.push(connect_greeting_client(&peer).await.unwrap());
        let counted = handle
            .get_num_active_sessions_for(node_identity.node_id().clone())
            .await
            .unwrap();
        assert_eq!(
            counted, expected,
            "the client was told it was accepted before its session was counted"
        );
    }
}

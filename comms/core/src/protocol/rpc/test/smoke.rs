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
    let (_inbound, outbound, server_hnd, node_identity, mut shutdown) = setup(GreetingService::default(), 1).await;
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
    let status = stream
        // StreamExt::collect has a Default trait bound which Result<_, _> cannot satisfy
        // so we must first collect the results into a Vec
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<String, _>>()
        .unwrap_err();
    assert_eq!(status.as_status_code(), RpcStatusCode::BadRequest);
    assert_eq!(status.details(), "What does 'Gurglesplurb' mean?");

    let stream = client.streaming_error2().await.unwrap();
    let results = stream.collect::<Vec<_>>().await;
    assert_eq!(results.len(), 2);
    let first_reply = results.first().unwrap().as_ref().unwrap();
    assert_eq!(first_reply, "This is ok");

    let second_reply = results.get(1).unwrap().as_ref().unwrap_err();
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

    let framed = framing::canonical(socket, rpc::max_request_size());
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
    let (_inbound, outbound, _, _, mut shutdown) = setup(GreetingService::new(&[]), 1).await;
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

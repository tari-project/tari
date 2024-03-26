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

use futures::StreamExt;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{mpsc, RwLock},
    task,
    time,
};

use crate::{
    framing,
    multiplexing::Yamux,
    protocol::{
        rpc,
        rpc::{
            context::RpcCommsBackend,
            error::HandshakeRejectReason,
            handshake::RpcHandshakeError,
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
            RpcError,
            RpcServer,
            RpcServerBuilder,
            RpcStatusCode,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
    },
    test_utils::{node_identity::build_node_identity, transport::build_multiplexed_connections},
    NodeIdentity,
    Substream,
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

pub(super) async fn setup<T: GreetingRpc>(
    service_impl: T,
    num_concurrent_sessions: usize,
) -> (Yamux, Yamux, task::JoinHandle<()>, Arc<NodeIdentity>, Shutdown) {
    let (notif_tx, server_hnd, context, shutdown) = setup_service(service_impl, num_concurrent_sessions).await;
    let (_, inbound, outbound) = build_multiplexed_connections().await;
    let substream = outbound.get_yamux_control().open_stream().await.unwrap();

    let node_identity = build_node_identity(Default::default());
    // Notify that a peer wants to speak the greeting RPC protocol
    context.peer_manager().add_peer(node_identity.to_peer()).await.unwrap();
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"/test/greeting/1.0"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), substream),
        ))
        .await
        .unwrap();

    (inbound, outbound, server_hnd, node_identity, shutdown)
}

#[tokio::test]
async fn request_response_errors_and_streaming() {
    let (mut muxer, _outbound, server_hnd, node_identity, mut shutdown) = setup(GreetingService::default(), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

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
        err => panic!("Unexpected error {:?}", err),
    }

    shutdown.trigger();
    server_hnd.await.unwrap();
}

#[tokio::test]
async fn concurrent_requests() {
    let (mut muxer, _outbound, _, _, _shutdown) = setup(GreetingService::default(), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

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
    let (mut muxer, _outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

    let framed = framing::canonical(socket, rpc::max_response_size());
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // RPC_MAX_FRAME_SIZE bytes will always be too large because of the overhead of the RpcResponse proto message
    let err = client
        .reply_with_msg_of_size(rpc::max_response_payload_size() as u64 + 1)
        .await
        .unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    unpack_enum!(RpcStatusCode::MalformedResponse = status.as_status_code());

    // Check that the exact frame size boundary works and that the session is still going
    let _string = client
        .reply_with_msg_of_size(rpc::max_response_payload_size() as u64 - 9)
        .await
        .unwrap();
}

#[tokio::test]
async fn ping_latency() {
    let (mut muxer, _outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder().connect(framed).await.unwrap();

    let latency = client.ping().await.unwrap();
    // This is plenty (typically would be < 1ms over MemorySocket), however CI can be very slow, so to prevent flakiness
    // we leave a wide berth
    assert!(latency.as_secs() < 5);
}

#[tokio::test]
async fn server_shutdown_before_connect() {
    let (mut muxer, _outbound, _, _, mut shutdown) = setup(GreetingService::new(&[]), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();
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
    let (mut muxer, _outbound, _, _, _shutdown) = setup(SlowGreetingService::new(delay.clone()), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();
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

#[tokio::test]
async fn unknown_protocol() {
    let (notif_tx, _, _, _shutdown) = setup_service(GreetingService::new(&[]), 1).await;

    let (_, inbound, mut outbound) = build_multiplexed_connections().await;
    let in_substream = inbound.get_yamux_control().open_stream().await.unwrap();

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

    let out_socket = outbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(out_socket, 1024);
    let err = GreetingClient::connect(framed).await.unwrap_err();
    assert!(matches!(
        err,
        RpcError::HandshakeError(RpcHandshakeError::Rejected(HandshakeRejectReason::ProtocolNotSupported))
    ));
}

#[tokio::test]
async fn rejected_no_sessions_available() {
    let (mut muxer, _outbound, _, _, _shutdown) = setup(GreetingService::new(&[]), 0).await;
    let socket = muxer.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder().connect(framed).await.unwrap_err();
    assert!(matches!(
        err,
        RpcError::HandshakeError(RpcHandshakeError::Rejected(HandshakeRejectReason::NoSessionsAvailable))
    ));
}

#[tokio::test]
async fn stream_still_works_after_cancel() {
    let service_impl = GreetingService::default();
    let (mut muxer, _outbound, _, _, _shutdown) = setup(service_impl.clone(), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

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

#[tokio::test]
async fn stream_interruption_handling() {
    let service_impl = GreetingService::default();
    let (mut muxer, _outbound, _, _, _shutdown) = setup(service_impl.clone(), 1).await;
    let socket = muxer.incoming_mut().next().await.unwrap();

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
    let (_, mut inbound, outbound) = build_multiplexed_connections().await;

    let node_identity = build_node_identity(Default::default());
    // Notify that a peer wants to speak the greeting RPC protocol
    context.peer_manager().add_peer(node_identity.to_peer()).await.unwrap();

    for _ in 0..2 {
        let substream = outbound.get_yamux_control().open_stream().await.unwrap();
        muxer
            .send(ProtocolNotification::new(
                ProtocolId::from_static(b"/test/greeting/1.0"),
                ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), substream),
            ))
            .await
            .unwrap();
    }

    let socket = inbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let socket = inbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap_err();

    unpack_enum!(RpcError::HandshakeError(err) = err);
    unpack_enum!(RpcHandshakeError::Rejected(HandshakeRejectReason::NoSessionsAvailable) = err);

    client.close().await;
    let substream = outbound.get_yamux_control().open_stream().await.unwrap();
    muxer
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"/test/greeting/1.0"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), substream),
        ))
        .await
        .unwrap();
    let socket = inbound.incoming_mut().next().await.unwrap();
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
        .with_maximum_sessions_per_client(1);
    let (muxer, _outbound, context, _shutdown) = setup_service_with_builder(GreetingService::default(), builder).await;
    let (_, mut inbound, outbound) = build_multiplexed_connections().await;

    let node_identity = build_node_identity(Default::default());
    // Notify that a peer wants to speak the greeting RPC protocol
    context.peer_manager().add_peer(node_identity.to_peer()).await.unwrap();
    for _ in 0..2 {
        let substream = outbound.get_yamux_control().open_stream().await.unwrap();
        muxer
            .send(ProtocolNotification::new(
                ProtocolId::from_static(b"/test/greeting/1.0"),
                ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), substream),
            ))
            .await
            .unwrap();
    }

    let socket = inbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    let socket = inbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap_err();

    unpack_enum!(RpcError::HandshakeError(err) = err);
    unpack_enum!(RpcHandshakeError::Rejected(HandshakeRejectReason::NoSessionsAvailable) = err);

    drop(client);
    let substream = outbound.get_yamux_control().open_stream().await.unwrap();
    muxer
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"/test/greeting/1.0"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), substream),
        ))
        .await
        .unwrap();
    let socket = inbound.incoming_mut().next().await.unwrap();
    let framed = framing::canonical(socket, 1024);
    let _client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();
}

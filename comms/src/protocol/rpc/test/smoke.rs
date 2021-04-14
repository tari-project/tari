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

use crate::{
    framing,
    memsocket::MemorySocket,
    protocol::{
        rpc::{
            body::Streaming,
            context::RpcCommsBackend,
            error::HandshakeRejectReason,
            message::Request,
            test::mock::create_mocked_rpc_context,
            Response,
            RpcError,
            RpcServer,
            RpcStatus,
            RpcStatusCode,
            RPC_MAX_FRAME_SIZE,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
    },
    runtime,
    runtime::task,
    test_utils::node_identity::build_node_identity,
    NodeIdentity,
};
use async_trait::async_trait;
use futures::{channel::mpsc, stream, SinkExt, StreamExt};
use std::{iter, sync::Arc, time::Duration};
use tari_crypto::tari_utilities::hex::Hex;
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tokio::{sync::RwLock, time};

#[async_trait]
// #[tari_rpc(protocol_name = "/tari/greeting/1.0", server_struct = GreetingServer, client_struct = GreetingClient)]
pub trait GreetingRpc: Send + Sync + 'static {
    // #[rpc(method = 1)]
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus>;
    // #[rpc(method = 2)]
    async fn return_error(&self, request: Request<()>) -> Result<Response<()>, RpcStatus>;
    // #[rpc(method = 3)]
    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus>;
    // #[rpc(method = 4)]
    async fn streaming_error(&self, request: Request<String>) -> Result<Streaming<String>, RpcStatus>;
    // #[rpc(method = 5)]
    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus>;
    // #[rpc(method = 6)]
    async fn get_public_key_hex(&self, _: Request<()>) -> Result<String, RpcStatus>;
    // #[rpc(method = 7)]
    async fn reply_with_msg_of_size(&self, request: Request<u64>) -> Result<Vec<u8>, RpcStatus>;
}

async fn setup_service<T: GreetingRpc>(
    service: T,
    num_concurrent_sessions: usize,
) -> (
    mpsc::Sender<ProtocolNotification<MemorySocket>>,
    task::JoinHandle<Result<(), RpcError>>,
    RpcCommsBackend,
    Shutdown,
)
{
    let (notif_tx, notif_rx) = mpsc::channel(1);
    let shutdown = Shutdown::new();
    let (context, _) = create_mocked_rpc_context();
    let server_hnd = task::spawn(
        RpcServer::new()
            .with_maximum_simultaneous_sessions(num_concurrent_sessions)
            .with_minimum_client_deadline(Duration::from_secs(0))
            .with_shutdown_signal(shutdown.to_signal())
            .add_service(GreetingServer::new(service))
            .serve(notif_rx, context.clone()),
    );
    (notif_tx, server_hnd, context, shutdown)
}

async fn setup<T: GreetingRpc>(
    service: T,
    num_concurrent_sessions: usize,
) -> (
    MemorySocket,
    task::JoinHandle<Result<(), RpcError>>,
    Arc<NodeIdentity>,
    Shutdown,
)
{
    let (mut notif_tx, server_hnd, context, shutdown) = setup_service(service, num_concurrent_sessions).await;
    let (inbound, outbound) = MemorySocket::new_pair();
    let node_identity = build_node_identity(Default::default());

    // Notify that a peer wants to speak the greeting RPC protocol
    context.peer_manager().add_peer(node_identity.to_peer()).await.unwrap();
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"/test/greeting/1.0"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), inbound),
        ))
        .await
        .unwrap();

    (outbound, server_hnd, node_identity, shutdown)
}

#[runtime::test_basic]
async fn request_reponse_errors_and_streaming() // a.k.a  smoke test
{
    let greetings = &["Sawubona", "Jambo", "Bonjour", "Hello", "Molo", "Olá"];
    let (socket, server_hnd, node_identity, mut shutdown) = setup(GreetingService::new(greetings), 1).await;

    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(5))
        .connect(framed)
        .await
        .unwrap();

    // Latency is available "for free" as part of the connect protocol
    assert!(client.get_last_request_latency().await.unwrap().is_some());

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
    assert_eq!(status.status_code(), RpcStatusCode::NotImplemented);
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
    assert_eq!(status.status_code(), RpcStatusCode::BadRequest);
    assert_eq!(status.details(), "What does 'Gurglesplurb' mean?");

    let stream = client.streaming_error2().await.unwrap();
    let results = stream.collect::<Vec<_>>().await;
    assert_eq!(results.len(), 2);
    let first_reply = results.get(0).unwrap().as_ref().unwrap();
    assert_eq!(first_reply, "This is ok");

    let second_reply = results.get(1).unwrap().as_ref().unwrap_err();
    assert_eq!(second_reply.status_code(), RpcStatusCode::BadRequest);
    assert_eq!(second_reply.details(), "This is a problem");

    let pk_hex = client.get_public_key_hex().await.unwrap();
    assert_eq!(pk_hex, node_identity.public_key().to_hex());

    client.close();

    let err = client
        .say_hello(SayHelloRequest {
            name: String::new(),
            language: 0,
        })
        .await
        .unwrap_err();

    unpack_enum!(RpcError::ClientClosed = err);

    shutdown.trigger().unwrap();
    server_hnd.await.unwrap().unwrap();
}

#[runtime::test_basic]
async fn response_too_big() {
    let (socket, _, _, _shutdown) = setup(GreetingService::new(&[]), 1).await;

    let framed = framing::canonical(socket, RPC_MAX_FRAME_SIZE);
    let mut client = GreetingClient::builder().connect(framed).await.unwrap();

    // RPC_MAX_FRAME_SIZE bytes will always be too large because of the overhead of the RpcResponse proto message
    let err = client
        .reply_with_msg_of_size(RPC_MAX_FRAME_SIZE as u64)
        .await
        .unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    unpack_enum!(RpcStatusCode::MalformedResponse = status.status_code());

    // Check that the exact frame size boundary works and that the session is still going
    // Take off 14 bytes for the RpcResponse overhead (i.e request_id + status + flags + msg field + vec_char(len(msg)))
    let max_size = RPC_MAX_FRAME_SIZE - 14;
    let _ = client.reply_with_msg_of_size(max_size as u64).await.unwrap();
}

#[runtime::test_basic]
async fn server_shutdown_after_connect() {
    let (socket, _, _, mut shutdown) = setup(GreetingService::new(&[]), 1).await;
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::connect(framed).await.unwrap();
    shutdown.trigger().unwrap();

    let err = client.say_hello(Default::default()).await.unwrap_err();
    unpack_enum!(RpcError::RequestCancelled = err);
}

#[runtime::test_basic]
async fn server_shutdown_before_connect() {
    let (socket, _, _, mut shutdown) = setup(GreetingService::new(&[]), 1).await;
    let framed = framing::canonical(socket, 1024);
    shutdown.trigger().unwrap();

    let err = GreetingClient::connect(framed).await.unwrap_err();
    unpack_enum!(RpcError::ServerClosedRequest = err);
}

#[runtime::test_basic]
async fn timeout() {
    let delay = Arc::new(RwLock::new(Duration::from_secs(10)));
    let (socket, _, _, _shutdown) = setup(SlowGreetingService::new(delay.clone()), 1).await;
    let framed = framing::canonical(socket, 1024);
    let mut client = GreetingClient::builder()
        .with_deadline(Duration::from_secs(1))
        .with_deadline_grace_period(Duration::from_secs(1))
        .connect(framed)
        .await
        .unwrap();

    let err = client.say_hello(Default::default()).await.unwrap_err();
    unpack_enum!(RpcError::RequestFailed(status) = err);
    assert_eq!(status.status_code(), RpcStatusCode::Timeout);

    *delay.write().await = Duration::from_secs(0);

    // The server should have hit the deadline and "reset" by waiting for another request without sending a response.
    // Test that this happens by checking that the next request is furnished correctly
    let resp = client.say_hello(Default::default()).await.unwrap();
    assert_eq!(resp.greeting, "took a while to load");
}

#[runtime::test_basic]
async fn unknown_protocol() {
    let (mut notif_tx, _, _, _shutdown) = setup_service(GreetingService::new(&[]), 1).await;

    let (inbound, socket) = MemorySocket::new_pair();
    let node_identity = build_node_identity(Default::default());

    // This case should never happen because protocols are preregistered with the connection manager and so a
    // protocol notification should never be sent out if it is unrecognised. However it is still not a bad
    // idea to test the behaviour.
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"this-is-junk"),
            ProtocolEvent::NewInboundSubstream(node_identity.node_id().clone(), inbound),
        ))
        .await
        .unwrap();

    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::connect(framed).await.unwrap_err();
    unpack_enum!(RpcError::HandshakeRejected(reason) = err);
    unpack_enum!(HandshakeRejectReason::ProtocolNotSupported = reason);
}

#[runtime::test_basic]
async fn rejected_no_sessions_available() {
    let (socket, _, _, _shutdown) = setup(GreetingService::new(&[]), 0).await;
    let framed = framing::canonical(socket, 1024);
    let err = GreetingClient::builder().connect(framed).await.unwrap_err();
    unpack_enum!(RpcError::HandshakeRejected(reason) = err);
    unpack_enum!(HandshakeRejectReason::NoSessionsAvailable = reason);
}

//---------------------------------- Greeting Service --------------------------------------------//

pub struct GreetingService {
    greetings: Vec<String>,
}

impl GreetingService {
    pub fn new(greetings: &[&str]) -> Self {
        Self {
            greetings: greetings.iter().map(ToString::to_string).collect(),
        }
    }
}

#[async_trait]
impl GreetingRpc for GreetingService {
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus> {
        let msg = request.message();
        let greeting = self
            .greetings
            .get(msg.language as usize)
            .ok_or_else(|| RpcStatus::bad_request(format!("{} is not a valid language identifier", msg.language)))?;

        let greeting = format!("{} {}", greeting, msg.name);
        Ok(Response::new(SayHelloResponse { greeting }))
    }

    async fn return_error(&self, _: Request<()>) -> Result<Response<()>, RpcStatus> {
        Err(RpcStatus::not_implemented("I haven't gotten to this yet :("))
    }

    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
        let (mut tx, rx) = mpsc::channel(1);
        let num = *request.message();
        let greetings = self.greetings[..num as usize].to_vec();
        task::spawn(async move {
            let iter = greetings.into_iter().map(Ok);
            let mut stream = stream::iter(iter)
                // "Extra" Result::Ok is to satisfy send_all
                .map(Ok);
            match tx.send_all(&mut stream).await {
                Ok(_) => {},
                Err(_err) => {
                    // Log error
                },
            }
        });

        Ok(Streaming::new(rx))
    }

    async fn streaming_error(&self, request: Request<String>) -> Result<Streaming<String>, RpcStatus> {
        Err(RpcStatus::bad_request(format!(
            "What does '{}' mean?",
            request.message()
        )))
    }

    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus> {
        let (mut tx, rx) = mpsc::channel(2);
        tx.send(Ok("This is ok".to_string())).await.unwrap();
        tx.send(Err(RpcStatus::bad_request("This is a problem"))).await.unwrap();

        Ok(Streaming::new(rx))
    }

    async fn get_public_key_hex(&self, req: Request<()>) -> Result<String, RpcStatus> {
        let context = req.context();
        let peer = context.fetch_peer().await?;
        Ok(peer.public_key.to_hex())
    }

    async fn reply_with_msg_of_size(&self, request: Request<u64>) -> Result<Vec<u8>, RpcStatus> {
        let size = request.into_message() as usize;
        Ok(iter::repeat(0).take(size).collect())
    }
}

pub struct SlowGreetingService {
    delay: Arc<RwLock<Duration>>,
}

impl SlowGreetingService {
    pub fn new(delay: Arc<RwLock<Duration>>) -> Self {
        Self { delay }
    }
}

#[async_trait]
impl GreetingRpc for SlowGreetingService {
    async fn say_hello(&self, _: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus> {
        let delay = *self.delay.read().await;
        time::delay_for(delay).await;
        Ok(Response::new(SayHelloResponse {
            greeting: "took a while to load".to_string(),
        }))
    }

    async fn return_error(&self, _: Request<()>) -> Result<Response<()>, RpcStatus> {
        unimplemented!()
    }

    async fn get_greetings(&self, _: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
        unimplemented!()
    }

    async fn streaming_error(&self, _: Request<String>) -> Result<Streaming<String>, RpcStatus> {
        unimplemented!()
    }

    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus> {
        unimplemented!()
    }

    async fn get_public_key_hex(&self, _: Request<()>) -> Result<String, RpcStatus> {
        unimplemented!()
    }

    async fn reply_with_msg_of_size(&self, _: Request<u64>) -> Result<Vec<u8>, RpcStatus> {
        unimplemented!()
    }
}

#[derive(prost::Message)]
pub struct SayHelloRequest {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(uint32, tag = "2")]
    language: u32,
}

#[derive(prost::Message)]
pub struct SayHelloResponse {
    #[prost(string, tag = "1")]
    greeting: String,
}

// This is approximately what is generated from the #[tari_rpc(...)] macro.
mod __rpc_deps {
    pub use crate::protocol::rpc::__macro_reexports::*;
}

pub struct GreetingServer<T> {
    inner: Arc<T>,
}

impl<T: GreetingRpc> GreetingServer<T> {
    pub fn new(service: T) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }
}

impl<T: GreetingRpc> __rpc_deps::Service<Request<__rpc_deps::Bytes>> for GreetingServer<T> {
    type Error = RpcStatus;
    type Future = __rpc_deps::BoxFuture<'static, Result<Response<__rpc_deps::Body>, RpcStatus>>;
    type Response = Response<__rpc_deps::Body>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<__rpc_deps::Bytes>) -> Self::Future {
        use __rpc_deps::IntoBody;
        let inner = self.inner.clone();
        match req.method().id() {
            // say_hello
            1 => {
                let fut = async move {
                    let resp = inner.say_hello(req.decode()?).await?;
                    Ok(resp.map(IntoBody::into_body))
                };
                Box::pin(fut)
            },
            // return_error
            2 => {
                let fut = async move {
                    let resp = inner.return_error(req.decode()?).await?;
                    Ok(resp.map(IntoBody::into_body))
                };
                Box::pin(fut)
            },
            // get_greetings
            3 => {
                let fut = async move {
                    let resp = inner.get_greetings(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },
            // streaming_error
            4 => {
                let fut = async move {
                    let resp = inner.streaming_error(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },
            // streaming_error2
            5 => {
                let fut = async move {
                    let resp = inner.streaming_error2(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },
            // get_public_key_hex
            6 => {
                let fut = async move {
                    let resp = inner.get_public_key_hex(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },
            // reply_with_msg_of_size
            7 => {
                let fut = async move {
                    let resp = inner.reply_with_msg_of_size(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },

            id => Box::pin(__rpc_deps::future::ready(Err(RpcStatus::unsupported_method(format!(
                "Method identifier `{}` is not recognised or supported",
                id
            ))))),
        }
    }
}

impl<T> Clone for GreetingServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> __rpc_deps::NamedProtocolService for GreetingServer<T> {
    const PROTOCOL_NAME: &'static [u8] = b"/test/greeting/1.0";
}

/// A service maker for GreetingServer
impl<T> __rpc_deps::Service<ProtocolId> for GreetingServer<T>
where T: GreetingRpc
{
    type Error = RpcError;
    type Future = __rpc_deps::future::Ready<Result<Self::Response, Self::Error>>;
    type Response = Self;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ProtocolId) -> Self::Future {
        __rpc_deps::future::ready(Ok(self.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct GreetingClient {
    inner: __rpc_deps::RpcClient,
}

impl __rpc_deps::NamedProtocolService for GreetingClient {
    const PROTOCOL_NAME: &'static [u8] = b"/test/greeting/1.0";
}

impl GreetingClient {
    pub async fn connect<TSubstream>(framed: __rpc_deps::CanonicalFraming<TSubstream>) -> Result<Self, RpcError>
    where TSubstream: __rpc_deps::AsyncRead + __rpc_deps::AsyncWrite + Unpin + Send + 'static {
        let inner = __rpc_deps::RpcClient::connect(Default::default(), framed).await?;
        Ok(Self { inner })
    }

    pub fn builder() -> __rpc_deps::RpcClientBuilder<Self> {
        __rpc_deps::RpcClientBuilder::new()
    }

    pub async fn say_hello(&mut self, request: SayHelloRequest) -> Result<SayHelloResponse, RpcError> {
        self.inner.request_response(request, 1).await
    }

    pub async fn return_error(&mut self) -> Result<(), RpcError> {
        self.inner.request_response((), 2).await
    }

    pub async fn get_greetings(&mut self, request: u32) -> Result<__rpc_deps::ClientStreaming<String>, RpcError> {
        self.inner.server_streaming(request, 3).await
    }

    pub async fn streaming_error(&mut self, request: String) -> Result<__rpc_deps::ClientStreaming<String>, RpcError> {
        self.inner.server_streaming(request, 4).await
    }

    pub async fn streaming_error2(&mut self) -> Result<__rpc_deps::ClientStreaming<String>, RpcError> {
        self.inner.server_streaming((), 5).await
    }

    pub async fn get_public_key_hex(&mut self) -> Result<String, RpcError> {
        self.inner.request_response((), 6).await
    }

    pub async fn reply_with_msg_of_size(&mut self, request: u64) -> Result<String, RpcError> {
        self.inner.request_response(request, 7).await
    }

    pub async fn get_last_request_latency(&mut self) -> Result<Option<Duration>, RpcError> {
        self.inner.get_last_request_latency().await
    }

    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl From<__rpc_deps::RpcClient> for GreetingClient {
    fn from(inner: __rpc_deps::RpcClient) -> Self {
        Self { inner }
    }
}

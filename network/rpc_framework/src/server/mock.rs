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

use std::{
    collections::HashMap,
    future,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use bytes::Bytes;
use futures::future::BoxFuture;
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task,
};
use tower::{make::MakeService, Service};

use crate::{
    notify::ProtocolNotificationTx,
    server::{handle::RpcServerRequest, PeerRpcServer, RpcServerError},
    Body,
    NamedProtocolService,
    Request,
    Response,
    RpcError,
    RpcServer,
    RpcStatus,
    Streaming,
};

pub struct RpcRequestMock {}

impl RpcRequestMock {
    pub fn new() -> Self {
        Self {}
    }

    pub fn request_no_context<T>(&self, msg: T) -> Request<T> {
        Request::new(0.into(), msg)
    }
}

/// # RpcMock trait
///
/// Some common utilities used to mock out an Rpc service.
///
/// Currently, there is a fair amount of manual boilerplate involved. The intention is to discover what the
/// requirements/edge cases are for mocking out RPC services and create a proc macro to generate the
/// boilerplate.
///
/// ## Usage
///
///
/// ```edition2018
/// # use tari_comms::protocol::rpc::mock::{RpcMock, RpcMockMethodState};
/// struct MyServiceMock {
///     // Each method has a field where it's call state (requests, number of calls etc) and canned response are stored
///     my_method: RpcMockMethodState<(), ()>,
/// }
/// // impl MyServiceTrait for MySericeMock {
/// //     async fn my_method(&self, request: Request<()>) -> Result<Response<()>, RpcStatus> {
/// //         self.request_response(request, &self.my_method).await
/// // }
/// impl RpcMock for MyServiceMock {};
/// ```
#[async_trait]
pub trait RpcMock {
    async fn request_response<TReq, TResp>(
        &self,
        request: Request<TReq>,
        method_state: &RpcMockMethodState<TReq, TResp>,
    ) -> Result<Response<TResp>, RpcStatus>
    where
        TReq: Send + Sync,
        TResp: Send + Sync + Clone,
    {
        method_state.requests.write().await.push(request.into_message());
        let resp = method_state.response.read().await.clone()?;
        Ok(Response::new(resp))
    }

    async fn server_streaming<TReq, TResp>(
        &self,
        request: Request<TReq>,
        method_state: &RpcMockMethodState<TReq, Vec<TResp>>,
    ) -> Result<Streaming<TResp>, RpcStatus>
    where
        TReq: Send + Sync,
        TResp: Send + Sync + Clone,
    {
        method_state.requests.write().await.push(request.into_message());
        let resp = method_state.response.read().await.clone()?;
        let (tx, rx) = mpsc::channel(resp.len());

        #[allow(clippy::match_wild_err_arm)]
        for msg in resp {
            match tx.send(msg).await {
                Ok(_) => {},
                // This is done because tokio mpsc channels give the item back to you in the error, and our item doesn't
                // impl Debug, so we can't use unwrap, expect etc
                Err(_) => panic!("send error"),
            }
        }
        Ok(Streaming::new(rx))
    }
}

#[derive(Debug, Clone)]
pub struct RpcMockMethodState<TReq, TResp> {
    requests: Arc<RwLock<Vec<TReq>>>,
    response: Arc<RwLock<Result<TResp, RpcStatus>>>,
}

impl<TReq, TResp> RpcMockMethodState<TReq, TResp> {
    pub async fn request_count(&self) -> usize {
        self.requests.read().await.len()
    }

    pub async fn set_response(&self, response: Result<TResp, RpcStatus>) {
        *self.response.write().await = response;
    }
}

impl<TReq, TResp: Default> Default for RpcMockMethodState<TReq, TResp> {
    fn default() -> Self {
        Self {
            requests: Default::default(),
            response: Arc::new(RwLock::new(Ok(Default::default()))),
        }
    }
}

pub struct MockRpcServer<TSvc> {
    inner: Option<PeerRpcServer<TSvc>>,
    protocol_tx: ProtocolNotificationTx<Substream>,
    our_node: Arc<NodeIdentity>,
    #[allow(dead_code)]
    request_tx: mpsc::Sender<RpcServerRequest>,
}

impl<TSvc> MockRpcServer<TSvc>
where
    TSvc: MakeService<
            ProtocolId,
            Request<Bytes>,
            MakeError = RpcServerError,
            Response = Response<Body>,
            Error = RpcStatus,
        > + Send
        + Sync
        + 'static,
    TSvc::Service: Send + 'static,
    <TSvc::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    TSvc::Future: Send + 'static,
{
    pub fn new(service: TSvc, our_node: Arc<NodeIdentity>) -> Self {
        let (protocol_tx, protocol_rx) = mpsc::channel(1);
        let (request_tx, request_rx) = mpsc::channel(1);

        Self {
            inner: Some(PeerRpcServer::new(
                RpcServer::builder(),
                service,
                protocol_rx,
                MockCommsProvider,
                request_rx,
            )),
            our_node,
            protocol_tx,
            request_tx,
        }
    }

    /// Create a PeerConnection that can open a substream to this mock server, notifying the server of the given
    /// protocol_id.
    pub async fn create_connection(&self, peer: Peer, protocol_id: ProtocolId) -> PeerConnection {
        let peer_node_id = peer.node_id.clone();
        let (_, our_conn_mock, peer_conn, _) = create_peer_connection_mock_pair(peer, self.our_node.to_peer()).await;

        let protocol_tx = self.protocol_tx.clone();
        task::spawn(async move {
            while let Some(substream) = our_conn_mock.next_incoming_substream().await {
                let proto_notif = ProtocolNotification::new(
                    protocol_id.clone(),
                    ProtocolEvent::NewInboundSubstream(peer_node_id.clone(), substream),
                );
                protocol_tx.send(proto_notif).await.unwrap();
            }
        });

        peer_conn
    }

    pub fn serve(&mut self) -> task::JoinHandle<Result<(), RpcServerError>> {
        let inner = self.inner.take().expect("can only call `serve` once");
        task::spawn(inner.serve())
    }
}

impl MockRpcServer<MockRpcImpl> {
    pub async fn create_mockimpl_connection(&self, peer: Peer) -> PeerConnection {
        // MockRpcImpl accepts any protocol
        self.create_connection(peer, ProtocolId::new()).await
    }
}

#[derive(Clone, Default)]
pub struct MockRpcImpl {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    accepted_calls: HashMap<u32, Response<Bytes>>,
}

impl MockRpcImpl {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Service<Request<Bytes>> for MockRpcImpl {
    type Error = RpcStatus;
    type Future = BoxFuture<'static, Result<Response<Body>, RpcStatus>>;
    type Response = Response<Body>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Bytes>) -> Self::Future {
        let state = self.state.clone();
        Box::pin(async move {
            let method_id = req.method().id();
            match state.lock().await.accepted_calls.get(&method_id) {
                Some(resp) => Ok(resp.clone().map(Body::single)),
                None => Err(RpcStatus::unsupported_method(&format!(
                    "Method identifier `{}` is not recognised or supported",
                    method_id
                ))),
            }
        })
    }
}

impl NamedProtocolService for MockRpcImpl {
    const PROTOCOL_NAME: &'static [u8] = b"mock-service";
}

/// A service maker for GreetingServer
impl Service<ProtocolId> for MockRpcImpl {
    type Error = RpcServerError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;
    type Response = Self;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ProtocolId) -> Self::Future {
        future::ready(Ok(self.clone()))
    }
}

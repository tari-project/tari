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

use crate::{
    connectivity::ConnectivitySelection,
    peer_manager::{NodeId, Peer},
    protocol::{
        rpc::{
            context::{RequestContext, RpcCommsBackend, RpcCommsProvider},
            server::{handle::RpcServerRequest, PeerRpcServer, RpcServerError},
            Body,
            Request,
            Response,
            RpcError,
            RpcServer,
            RpcStatus,
            Streaming,
        },
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        ProtocolNotificationTx,
    },
    test_utils::mocks::{create_connectivity_mock, create_peer_connection_mock_pair, ConnectivityManagerMockState},
    NodeIdentity,
    PeerConnection,
    PeerManager,
    Substream,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{channel::mpsc, stream, SinkExt};
use std::sync::Arc;
use tokio::{sync::RwLock, task};
use tower::Service;
use tower_make::MakeService;

pub struct RpcRequestMock {
    comms_provider: RpcCommsBackend,
    connectivity_mock_state: ConnectivityManagerMockState,
}

impl RpcRequestMock {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let connectivity_mock_state = connectivity_mock.get_shared_state();
        connectivity_mock.spawn();
        Self {
            comms_provider: RpcCommsBackend::new(peer_manager, connectivity),
            connectivity_mock_state,
        }
    }

    pub fn request_with_context<T>(&self, node_id: NodeId, msg: T) -> Request<T> {
        let context = RequestContext::new(node_id, Box::new(self.comms_provider.clone()));
        Request::with_context(context, 0.into(), msg)
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
/// impl RpcMock for MyServiceMock{};
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
        let (mut tx, rx) = mpsc::channel(resp.len());
        let mut resp = stream::iter(resp.into_iter().map(Ok).map(Ok));
        tx.send_all(&mut resp).await.unwrap();
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

#[derive(Debug, Clone)]
pub struct MockCommsProvider;

#[async_trait]
impl RpcCommsProvider for MockCommsProvider {
    async fn fetch_peer(&self, _: &NodeId) -> Result<Peer, RpcError> {
        unimplemented!()
    }

    async fn dial_peer(&mut self, _: &NodeId) -> Result<PeerConnection, RpcError> {
        unimplemented!()
    }

    async fn select_connections(&mut self, _: ConnectivitySelection) -> Result<Vec<PeerConnection>, RpcError> {
        unimplemented!()
    }
}

pub struct MockRpcServer<TSvc, TSubstream> {
    inner: Option<PeerRpcServer<TSvc, TSubstream, MockCommsProvider>>,
    protocol_tx: ProtocolNotificationTx<TSubstream>,
    our_node: Arc<NodeIdentity>,
    request_tx: mpsc::Sender<RpcServerRequest>,
}

impl<TSvc> MockRpcServer<TSvc, Substream>
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

    /// Create a PeerConnection that can open a substream to this mock server.
    pub async fn create_connection(&self, peer: Peer, protocol_id: ProtocolId) -> PeerConnection {
        let peer_node_id = peer.node_id.clone();
        let (_, our_conn_mock, peer_conn, _) = create_peer_connection_mock_pair(1, peer, self.our_node.to_peer()).await;

        let mut protocol_tx = self.protocol_tx.clone();
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

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
    message::MessageExt,
    protocol::{
        rpc::{
            body::{Body, ClientStreaming},
            client::RpcClient,
            context::RpcCommsContext,
            message::RpcMethod,
            server::NamedProtocolService,
            Request,
            Response,
            RpcError,
            RpcStatus,
        },
        ProtocolId,
    },
    test_utils::{
        mocks::{create_connectivity_mock, ConnectivityManagerMockState},
        test_node::build_peer_manager,
    },
};
use bytes::Bytes;
use futures::future;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
        RwLock,
    },
    task::{Context, Poll},
};
use tower::Service;

#[derive(Clone, Default)]
pub struct MockRpcService {
    state: MockRpcServiceState,
}

impl NamedProtocolService for MockRpcService {
    const PROTOCOL_NAME: &'static [u8] = b"rpc-mock";
}

impl MockRpcService {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn shared_state(&self) -> MockRpcServiceState {
        self.state.clone()
    }
}

impl Service<ProtocolId> for MockRpcService {
    type Error = RpcError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    type Response = impl Service<
        Request<Bytes>,
        Response = Response<Body>,
        Error = RpcStatus,
        Future = future::Ready<Result<Response<Body>, RpcStatus>>,
    >;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ProtocolId) -> Self::Future {
        let state = self.state.clone();
        let my_service = tower::service_fn(move |_: Request<Bytes>| {
            state.inc_call_count();
            future::ready(state.get_response())
        });

        future::ready(Ok(my_service))
    }
}

#[derive(Debug, Clone)]
pub struct MockRpcServiceState {
    call_count: Arc<AtomicUsize>,
    response: Arc<RwLock<Result<Response<Bytes>, RpcStatus>>>,
}

impl Default for MockRpcServiceState {
    fn default() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            response: Arc::new(RwLock::new(Err(RpcStatus::not_implemented(
                "Mock service not implemented",
            )))),
        }
    }
}

impl MockRpcServiceState {
    pub fn new() -> Self {
        Default::default()
    }

    fn inc_call_count(&self) -> usize {
        self.call_count.fetch_add(1, Ordering::SeqCst)
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn get_response(&self) -> Result<Response<Body>, RpcStatus> {
        let lock = &*self.response.read().unwrap();
        lock.as_ref()
            .map(|r| r.clone().map(Body::single))
            .map_err(|err| err.clone())
    }

    pub fn set_response(&self, response: Result<Response<Bytes>, RpcStatus>) {
        *self.response.write().unwrap() = response;
    }

    pub fn set_response_ok<T: prost::Message>(&self, response: T) {
        self.set_response(Ok(Response::new(response.to_encoded_bytes().into())));
    }

    pub fn set_response_err(&self, err: RpcStatus) {
        self.set_response(Err(err));
    }
}

pub struct MockRpcClient {
    inner: RpcClient,
}

impl NamedProtocolService for MockRpcClient {
    const PROTOCOL_NAME: &'static [u8] = b"rpc-mock";
}

impl MockRpcClient {
    pub async fn request_response<T: prost::Message, R: prost::Message + Default>(
        &mut self,
        request: T,
        method: RpcMethod,
    ) -> Result<R, RpcError>
    {
        self.inner.request_response(request, method).await
    }

    pub async fn server_streaming<T: prost::Message, R: prost::Message + Default>(
        &mut self,
        request: T,
        method: RpcMethod,
    ) -> Result<ClientStreaming<R>, RpcError>
    {
        self.inner.server_streaming(request, method).await
    }
}

impl From<RpcClient> for MockRpcClient {
    fn from(inner: RpcClient) -> Self {
        Self { inner }
    }
}

pub(super) fn create_mocked_rpc_context() -> (RpcCommsContext, ConnectivityManagerMockState) {
    let (connectivity, mock) = create_connectivity_mock();
    let mock_state = mock.get_shared_state();
    mock.spawn();
    let peer_manager = build_peer_manager();

    (RpcCommsContext::new(peer_manager, connectivity), mock_state)
}

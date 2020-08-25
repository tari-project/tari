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

use futures::{channel::mpsc, SinkExt, StreamExt};
use prost::Message;
use std::{collections::HashMap, ops::AddAssign, sync::Arc};
use tari_comms::{
    framing,
    memsocket::MemorySocket,
    message::MessageExt,
    protocol::{
        rpc,
        rpc::{NamedProtocolService, Request, Response, RpcStatus, RpcStatusCode, Streaming},
    },
};
use tari_comms_rpc_macros::tari_rpc;
use tari_test_utils::unpack_enum;
use tokio::{sync::RwLock, task};
use tower_service::Service;

#[tari_rpc(protocol_name = b"/test/protocol/123", server_struct = TestServer, client_struct = TestClient)]
pub trait Test: Sync + Send + 'static {
    #[rpc(method = 1)]
    async fn request_response(&self, request: Request<u32>) -> Result<Response<u32>, RpcStatus>;
    #[rpc(method = 2)]
    async fn server_streaming(&self, request: Request<CustomMessage>) -> Result<Streaming<u32>, RpcStatus>;
    /// Some docs for unit
    #[rpc(method = 3)]
    async fn unit(&self, request: Request<()>) -> Result<Response<()>, RpcStatus>;

    // Although not typically needed, there is no reason why other non-rpc methods can't be included in the resulting
    // trait
    fn some_non_rpc_method(&self);
}

#[derive(prost::Message)]
pub struct CustomMessage;

#[derive(Default)]
pub struct TestService {
    state: Arc<RwLock<HashMap<&'static str, usize>>>,
}

impl TestService {
    pub async fn add_call(&self, call: &'static str) {
        self.state
            .write()
            .await
            .entry(call)
            .and_modify(|v| AddAssign::add_assign(v, 1))
            .or_insert(1);
    }
}

#[tari_comms::async_trait]
impl Test for TestService {
    async fn request_response(&self, request: Request<u32>) -> Result<Response<u32>, RpcStatus> {
        self.add_call("request_response").await;
        Ok(Response::new(request.message() + 1))
    }

    async fn server_streaming(&self, _: Request<CustomMessage>) -> Result<Streaming<u32>, RpcStatus> {
        self.add_call("server_streaming").await;
        let (mut tx, rx) = mpsc::channel(1);
        tx.send(Ok(1)).await.unwrap();
        Ok(Streaming::new(rx))
    }

    async fn unit(&self, _: Request<()>) -> Result<Response<()>, RpcStatus> {
        self.add_call("unit").await;
        Ok(Response::new(()))
    }

    fn some_non_rpc_method(&self) {
        unimplemented!()
    }
}

#[test]
fn it_sets_the_protocol_name() {
    assert_eq!(TestServer::<TestService>::PROTOCOL_NAME, b"/test/protocol/123");
    assert_eq!(TestClient::PROTOCOL_NAME, b"/test/protocol/123");
}

#[tokio_macros::test]
async fn it_returns_the_correct_type() {
    let mut server = TestServer::new(TestService::default());
    let resp = server
        .call(Request::new(1.into(), 11u32.to_encoded_bytes().into()))
        .await
        .unwrap();
    let v = resp.into_message().next().await.unwrap().unwrap();
    assert_eq!(u32::decode(v).unwrap(), 12);
}

#[tokio_macros::test]
async fn it_correctly_maps_the_method_nums() {
    let service = TestService::default();
    let spy = service.state.clone();
    let mut server = TestServer::new(service);
    server
        .call(Request::new(1.into(), 11u32.to_encoded_bytes().into()))
        .await
        .unwrap();
    assert_eq!(*spy.read().await.get("request_response").unwrap(), 1);
    server
        .call(Request::new(2.into(), CustomMessage.to_encoded_bytes().into()))
        .await
        .unwrap();
    assert_eq!(*spy.read().await.get("server_streaming").unwrap(), 1);

    server
        .call(Request::new(3.into(), ().to_encoded_bytes().into()))
        .await
        .unwrap();
    assert_eq!(*spy.read().await.get("unit").unwrap(), 1);
}

#[tokio_macros::test]
async fn it_returns_an_error_for_invalid_method_nums() {
    let service = TestService::default();
    let mut server = TestServer::new(service);
    let err = server
        .call(Request::new(10.into(), ().to_encoded_bytes().into()))
        .await
        .unwrap_err();

    unpack_enum!(RpcStatusCode::UnsupportedMethod = err.status_code());
}

#[tokio_macros::test]
async fn it_generates_client_calls() {
    let (sock_client, sock_server) = MemorySocket::new_pair();
    let client = task::spawn(TestClient::connect(framing::canonical(sock_client, 1024)));
    let mut sock_server = framing::canonical(sock_server, 1024);
    let mut handshake = rpc::Handshake::new(&mut sock_server);
    handshake.perform_server_handshake().await.unwrap();
    // Wait for client to connect
    let mut client = client.await.unwrap().unwrap();

    // This is a test that the correct client functions are generated - if this test compiles then it has already passed
    task::spawn(async move {
        let _ = client.request_response(111).await;
        let mut streaming_resp = client.server_streaming(CustomMessage).await.unwrap();
        streaming_resp.next().await;
        let _ = client.unit().await;
    });
}

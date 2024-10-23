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

use core::iter;
use std::{
    cmp,
    convert::TryFrom,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tari_utilities::hex::Hex;
use tokio::{
    sync::{mpsc, RwLock},
    task,
    time,
};

use crate::{
    async_trait,
    protocol::{
        rpc::{NamedProtocolService, Request, Response, RpcError, RpcServerError, RpcStatus, Streaming},
        ProtocolId,
    },
    utils,
    Substream,
};

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
    // #[rpc(method = 8)]
    async fn slow_stream(&self, request: Request<SlowStreamRequest>) -> Result<Streaming<Vec<u8>>, RpcStatus>;
}

#[derive(Clone)]
pub struct GreetingService {
    greetings: Vec<String>,
    call_count: Arc<AtomicUsize>,
}

impl GreetingService {
    pub const DEFAULT_GREETINGS: &'static [&'static str] =
        &["Sawubona", "Jambo", "Bonjour", "Hello", "Molo", "Olá", "سلام", "你好"];

    pub fn new(greetings: &[&str]) -> Self {
        Self {
            greetings: greetings.iter().map(ToString::to_string).collect(),
            call_count: Default::default(),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }
}

impl Default for GreetingService {
    fn default() -> Self {
        Self::new(Self::DEFAULT_GREETINGS)
    }
}

#[async_trait]
impl GreetingRpc for GreetingService {
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus> {
        self.inc_call_count();
        let msg = request.message();
        let greeting = self
            .greetings
            .get(msg.language as usize)
            .ok_or_else(|| RpcStatus::bad_request(&format!("{} is not a valid language identifier", msg.language)))?;

        let greeting = format!("{} {}", greeting, msg.name);
        Ok(Response::new(SayHelloResponse { greeting }))
    }

    async fn return_error(&self, _: Request<()>) -> Result<Response<()>, RpcStatus> {
        self.inc_call_count();
        Err(RpcStatus::not_implemented("I haven't gotten to this yet :("))
    }

    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
        self.inc_call_count();
        let (tx, rx) = mpsc::channel(1);
        let num = *request.message();
        let greetings = self.greetings[..cmp::min(num as usize, self.greetings.len())].to_vec();
        task::spawn(async move {
            let _result = utils::mpsc::send_all(&tx, greetings.into_iter().map(Ok)).await;
        });

        Ok(Streaming::new(rx))
    }

    async fn streaming_error(&self, request: Request<String>) -> Result<Streaming<String>, RpcStatus> {
        self.inc_call_count();
        Err(RpcStatus::bad_request(&format!(
            "What does '{}' mean?",
            request.message()
        )))
    }

    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus> {
        self.inc_call_count();
        let (tx, rx) = mpsc::channel(2);
        tx.send(Ok("This is ok".to_string())).await.unwrap();
        tx.send(Err(RpcStatus::bad_request("This is a problem"))).await.unwrap();

        Ok(Streaming::new(rx))
    }

    async fn get_public_key_hex(&self, req: Request<()>) -> Result<String, RpcStatus> {
        self.inc_call_count();
        let context = req.context();
        let peer = context.fetch_peer().await?;
        Ok(peer.public_key.to_hex())
    }

    async fn reply_with_msg_of_size(&self, request: Request<u64>) -> Result<Vec<u8>, RpcStatus> {
        self.inc_call_count();
        let size = usize::try_from(request.into_message()).unwrap();
        Ok(iter::repeat(0).take(size).collect())
    }

    async fn slow_stream(&self, request: Request<SlowStreamRequest>) -> Result<Streaming<Vec<u8>>, RpcStatus> {
        self.inc_call_count();
        let SlowStreamRequest {
            num_items,
            item_size,
            delay_ms,
        } = request.into_message();

        let (tx, rx) = mpsc::channel(1);
        let item = iter::repeat(0u8).take(item_size as usize).collect::<Vec<_>>();
        tokio::spawn(async move {
            for _ in 0..num_items {
                time::sleep(Duration::from_millis(delay_ms)).await;
                if tx.send(Ok(item.clone())).await.is_err() {
                    log::info!("stream was interrupted");
                    break;
                }
            }
        });

        Ok(Streaming::new(rx))
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
        time::sleep(delay).await;
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

    async fn slow_stream(&self, _: Request<SlowStreamRequest>) -> Result<Streaming<Vec<u8>>, RpcStatus> {
        unimplemented!()
    }
}
#[derive(prost::Message)]
pub struct SlowStreamRequest {
    #[prost(uint32, tag = "1")]
    pub num_items: u32,
    #[prost(uint32, tag = "2")]
    pub item_size: u32,
    #[prost(uint64, tag = "3")]
    pub delay_ms: u64,
}

#[derive(prost::Message)]
pub struct SayHelloRequest {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(uint32, tag = "2")]
    pub language: u32,
}

#[derive(prost::Message)]
pub struct SayHelloResponse {
    #[prost(string, tag = "1")]
    pub greeting: String,
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
            // slow_stream
            8 => {
                let fut = async move {
                    let resp = inner.slow_stream(req.decode()?).await?;
                    Ok(Response::new(resp.into_body()))
                };
                Box::pin(fut)
            },

            id => Box::pin(__rpc_deps::future::ready(Err(RpcStatus::unsupported_method(&format!(
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
    type Error = RpcServerError;
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
    pub async fn connect(framed: __rpc_deps::CanonicalFraming<Substream>) -> Result<Self, RpcError> {
        let inner = __rpc_deps::RpcClient::connect(
            Default::default(),
            Default::default(),
            framed,
            Self::PROTOCOL_NAME.into(),
            Default::default(),
        )
        .await?;
        Ok(Self { inner })
    }

    pub fn builder() -> __rpc_deps::RpcClientBuilder<Self> {
        __rpc_deps::RpcClientBuilder::new().with_protocol_id(Self::PROTOCOL_NAME.into())
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

    pub async fn slow_stream(
        &mut self,
        request: SlowStreamRequest,
    ) -> Result<__rpc_deps::ClientStreaming<Vec<u8>>, RpcError> {
        self.inner.server_streaming(request, 8).await
    }

    pub fn get_last_request_latency(&mut self) -> Option<Duration> {
        self.inner.get_last_request_latency()
    }

    pub async fn ping(&mut self) -> Result<Duration, RpcError> {
        self.inner.ping().await
    }

    pub async fn close(&mut self) {
        self.inner.close().await;
    }
}

impl From<__rpc_deps::RpcClient> for GreetingClient {
    fn from(inner: __rpc_deps::RpcClient) -> Self {
        Self { inner }
    }
}

impl __rpc_deps::RpcPoolClient for GreetingClient {
    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }
}

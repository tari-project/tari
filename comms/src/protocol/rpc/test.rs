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

use super::{body::Streaming, message::Request, Response, RpcError, RpcServer, RpcStatus, RpcStatusCode};
use crate::{
    framing,
    memsocket::MemorySocket,
    protocol::{ProtocolEvent, ProtocolId, ProtocolNotification},
    runtime,
    runtime::task,
    test_utils::node_identity::build_node_identity,
};
use async_trait::async_trait;
use futures::{channel::mpsc, stream, SinkExt, StreamExt};
use std::{io, sync::Arc, time::Duration};
use tari_shutdown::Shutdown;
use tari_test_utils::unpack_enum;
use tokio::{sync::RwLock, time};

#[async_trait]
// #[tari_rpc(module = generated, protocol_name = "/tari/greeting/1.0")]
pub trait GreetingRpc: Send + Sync + 'static {
    // #[rpc(method = 1)]
    async fn say_hello(&self, request: Request<SayHelloRequest>) -> Result<Response<SayHelloResponse>, RpcStatus>;
    // #[rpc(method = 2)]
    async fn return_error(&self, request: Request<()>) -> Result<Response<()>, RpcStatus>;
    // #[rpc(streaming, method = 3)]
    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus>;
    // #[rpc(streaming, method = 4)]
    async fn streaming_error(&self, request: Request<String>) -> Result<Streaming<String>, RpcStatus>;
    // #[rpc(streaming, method = 5)]
    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus>;
}

async fn setup_service<T: GreetingRpc>(
    service: T,
) -> (
    mpsc::Sender<ProtocolNotification<MemorySocket>>,
    task::JoinHandle<Result<(), RpcError>>,
    Shutdown,
) {
    let (notif_tx, notif_rx) = mpsc::channel(1);
    let shutdown = Shutdown::new();
    let server_hnd = task::spawn(
        RpcServer::new()
            .with_minimum_client_deadline(Duration::from_secs(0))
            .with_shutdown_signal(shutdown.to_signal())
            .add_service(generated::server::GreetingService::new(service))
            .serve(notif_rx),
    );
    (notif_tx, server_hnd, shutdown)
}

async fn setup<T: GreetingRpc>(service: T) -> (MemorySocket, task::JoinHandle<Result<(), RpcError>>, Shutdown) {
    let (mut notif_tx, server_hnd, shutdown) = setup_service(service).await;
    let (inbound, outbound) = MemorySocket::new_pair();
    let node_identity = build_node_identity(Default::default());

    // Notify that a peer wants to speak the greeting RPC protocol
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"/test/greeting/1.0"),
            ProtocolEvent::NewInboundSubstream(Box::new(node_identity.node_id().clone()), inbound),
        ))
        .await
        .unwrap();

    (outbound, server_hnd, shutdown)
}

#[runtime::test_basic]
async fn request_reponse_errors_and_streaming() // a.k.a  smoke test
{
    let greetings = &["Sawubona", "Jambo", "Bonjour", "Hello", "Molo", "Olá"];
    let (socket, server_hnd, mut shutdown) = setup(GreetingService::new(greetings)).await;

    let framed = framing::canonical(socket, 1024);
    let mut client = generated::client::GreetingServiceClient::builder(framed)
        .with_deadline(Duration::from_secs(5))
        .connect()
        .await
        .unwrap();

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
async fn server_shutdown_after_connect() {
    let (socket, _, mut shutdown) = setup(GreetingService::new(&[])).await;
    let framed = framing::canonical(socket, 1024);
    let mut client = generated::client::GreetingServiceClient::connect(framed).await.unwrap();
    shutdown.trigger().unwrap();

    let err = client.say_hello(Default::default()).await.unwrap_err();
    unpack_enum!(RpcError::RequestCancelled = err);
}

#[runtime::test_basic]
async fn server_shutdown_before_connect() {
    let (socket, _, mut shutdown) = setup(GreetingService::new(&[])).await;
    let framed = framing::canonical(socket, 1024);
    shutdown.trigger().unwrap();

    let err = generated::client::GreetingServiceClient::connect(framed)
        .await
        .unwrap_err();
    unpack_enum!(RpcError::Io(_err) = err);
}

#[runtime::test_basic]
async fn timeout() {
    let delay = Arc::new(RwLock::new(Duration::from_secs(10)));
    let (socket, _, _shutdown) = setup(SlowGreetingService::new(delay.clone())).await;
    let framed = framing::canonical(socket, 1024);
    let mut client = generated::client::GreetingServiceClient::builder(framed)
        .with_deadline(Duration::from_millis(50))
        .with_deadline_grace_period(Duration::from_secs(0))
        .connect()
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
    let (mut notif_tx, _, _shutdown) = setup_service(GreetingService::new(&[])).await;

    let (inbound, socket) = MemorySocket::new_pair();
    let node_identity = build_node_identity(Default::default());

    // This case should never happen because protocols are preregistered with the connection manager and so a
    // protocol notification should never be sent out if it is unrecognised. However it is still not a bad
    // idea to test the behaviour.
    notif_tx
        .send(ProtocolNotification::new(
            ProtocolId::from_static(b"this-is-junk"),
            ProtocolEvent::NewInboundSubstream(Box::new(node_identity.node_id().clone()), inbound),
        ))
        .await
        .unwrap();

    let framed = framing::canonical(socket, 1024);
    let err = generated::client::GreetingServiceClient::connect(framed)
        .await
        .unwrap_err();
    unpack_enum!(RpcError::Io(err) = err);
    // i.e the server just closed the stream immediately
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
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
        let greeting = self.greetings.get(request.message.language as usize).ok_or_else(|| {
            RpcStatus::bad_request(format!(
                "{} is not a valid language identifier",
                request.message.language
            ))
        })?;

        let greeting = format!("{} {}", greeting, request.message.name);
        Ok(Response::new(SayHelloResponse { greeting }))
    }

    async fn return_error(&self, _: Request<()>) -> Result<Response<()>, RpcStatus> {
        Err(RpcStatus::not_implemented("I haven't gotten to this yet :("))
    }

    async fn get_greetings(&self, request: Request<u32>) -> Result<Streaming<String>, RpcStatus> {
        let (mut tx, rx) = mpsc::channel(1);
        let greetings = self.greetings[..request.message as usize].to_vec();
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
        Err(RpcStatus::bad_request(format!("What does '{}' mean?", request.message)))
    }

    async fn streaming_error2(&self, _: Request<()>) -> Result<Streaming<String>, RpcStatus> {
        let (mut tx, rx) = mpsc::channel(2);
        tx.send(Ok("This is ok".to_string())).await.unwrap();
        tx.send(Err(RpcStatus::bad_request("This is a problem"))).await.unwrap();

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

// TODO: This will be generated by a proc macro
mod generated {
    pub mod server {
        #![allow(dead_code)]

        use crate::{
            protocol::{
                rpc::{
                    body::{Body, IntoBody},
                    message::{Request, Response},
                    server::NamedProtocolService,
                    test::GreetingRpc,
                    RpcError,
                    RpcStatus,
                },
                ProtocolId,
            },
            Bytes,
        };
        use futures::{future, future::BoxFuture};
        use std::{
            future::Future,
            sync::Arc,
            task::{Context, Poll},
        };
        use tower::Service;

        pub struct GreetingService<T> {
            inner: Arc<T>,
        }

        impl<T: GreetingRpc> GreetingService<T> {
            pub fn new(service: T) -> Self {
                Self {
                    inner: Arc::new(service),
                }
            }
        }

        impl<T: GreetingRpc> Service<Request<Bytes>> for GreetingService<T> {
            type Error = RpcStatus;
            type Future = BoxFuture<'static, Result<Response<Body>, RpcStatus>>;
            type Response = Response<Body>;

            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, mut req: Request<Bytes>) -> Self::Future {
                let inner = self.inner.clone();
                match req.method() {
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
                    id => Box::pin(future::ready(Err(RpcStatus::unsupported_method(format!(
                        "Method identifier `{}` is not recognised or supported",
                        id
                    ))))),
                }
            }
        }

        impl<T> Clone for GreetingService<T> {
            fn clone(&self) -> Self {
                Self {
                    inner: self.inner.clone(),
                }
            }
        }

        impl<T> NamedProtocolService for GreetingService<T> {
            const PROTOCOL_NAME: &'static [u8] = b"/test/greeting/1.0";
        }

        /// A service maker for GreetingService
        impl<T> Service<ProtocolId> for GreetingService<T>
        where T: GreetingRpc
        {
            type Error = RpcError;
            type Response = Self;

            type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _: ProtocolId) -> Self::Future {
                future::ready(Ok(self.clone()))
            }
        }
    }

    pub mod client {
        #![allow(dead_code)]

        use crate::{
            framing::CanonicalFraming,
            protocol::rpc::{
                body::ClientStreaming,
                client::{RpcClient, RpcClientBuilder},
                test::{SayHelloRequest, SayHelloResponse},
                RpcError,
            },
        };
        use futures::{AsyncRead, AsyncWrite};

        #[derive(Debug, Clone)]
        pub struct GreetingServiceClient {
            inner: RpcClient,
        }

        impl GreetingServiceClient {
            pub async fn connect<TSubstream>(framed: CanonicalFraming<TSubstream>) -> Result<Self, RpcError>
            where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static {
                let inner = RpcClient::connect(Default::default(), framed).await?;
                Ok(Self { inner })
            }

            pub fn builder<TSubstream>(framed: CanonicalFraming<TSubstream>) -> RpcClientBuilder<Self, TSubstream>
            where TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static {
                RpcClientBuilder::<Self, _>::new(framed)
            }

            pub async fn say_hello(&mut self, request: SayHelloRequest) -> Result<SayHelloResponse, RpcError> {
                self.inner.request_response(request, 1).await
            }

            pub async fn return_error(&mut self) -> Result<(), RpcError> {
                self.inner.request_response((), 2).await
            }

            pub async fn get_greetings(&mut self, request: u32) -> Result<ClientStreaming<String>, RpcError> {
                self.inner.server_streaming(request, 3).await
            }

            pub async fn streaming_error(&mut self, request: String) -> Result<ClientStreaming<String>, RpcError> {
                self.inner.server_streaming(request, 4).await
            }

            pub async fn streaming_error2(&mut self) -> Result<ClientStreaming<String>, RpcError> {
                self.inner.server_streaming((), 5).await
            }

            pub fn close(&mut self) {
                self.inner.close();
            }
        }

        impl From<RpcClient> for GreetingServiceClient {
            fn from(inner: RpcClient) -> Self {
                Self { inner }
            }
        }
    }
}

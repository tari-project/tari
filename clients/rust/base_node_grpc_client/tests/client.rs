//  Copyright 2022. The Tari Project
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
    convert::Infallible,
    future::{Ready, ready},
    time::{Duration, Instant},
};

use minotari_node_grpc_client::{BaseNodeGrpcClient, GrpcClientConfig, connect_channel, grpc};
use tonic::{
    Code,
    Request,
    Response,
    Status,
    body::Body,
    codec::ProstCodec,
    codegen::{BoxFuture, Context, Poll, Service, http},
    server::{Grpc, NamedService, UnaryService},
};

#[tokio::test]
async fn it_works() {
    match BaseNodeGrpcClient::connect("http://127.0.0.1:18142", &GrpcClientConfig::default()).await {
        Ok(mut client) => {
            let _tip_info = client.get_tip_info(grpc::Empty {}).await;
            #[cfg(debug_assertions)]
            eprintln!("Tip info: {_tip_info:?}");
        },
        Err(_err) => {
            #[cfg(debug_assertions)]
            eprintln!("Could not connect: {_err:?}");
        },
    }
}

/// Answers every BaseNode call with a version string of `reply_len` bytes (fits `GetVersion`), or never answers.
#[derive(Clone)]
struct StubBaseNode {
    reply_len: usize,
    hang: bool,
}

impl NamedService for StubBaseNode {
    const NAME: &'static str = "tari.rpc.BaseNode";
}

impl Service<http::Request<Body>> for StubBaseNode {
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    type Response = http::Response<Body>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let stub = self.clone();
        Box::pin(async move {
            if stub.hang {
                std::future::pending::<()>().await;
            }
            let mut grpc = Grpc::new(ProstCodec::<grpc::BaseNodeGetVersionResponse, grpc::Empty>::default())
                .max_decoding_message_size(usize::MAX)
                .max_encoding_message_size(usize::MAX);
            Ok(grpc.unary(stub, req).await)
        })
    }
}

impl UnaryService<grpc::Empty> for StubBaseNode {
    type Future = Ready<Result<Response<grpc::BaseNodeGetVersionResponse>, Status>>;
    type Response = grpc::BaseNodeGetVersionResponse;

    fn call(&mut self, _request: Request<grpc::Empty>) -> Self::Future {
        ready(Ok(Response::new(grpc::BaseNodeGetVersionResponse {
            version: "x".repeat(self.reply_len),
            network: 0,
        })))
    }
}

async fn serve(stub: StubBaseNode) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(stub)
            .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
    );
    format!("http://{addr}")
}

const LIMIT: usize = 1024;

fn small_limit() -> GrpcClientConfig {
    GrpcClientConfig {
        max_message_size: LIMIT,
        ..Default::default()
    }
}

#[tokio::test]
async fn response_over_max_message_size_is_rejected() {
    let cfg = small_limit();
    let addr = serve(StubBaseNode {
        reply_len: LIMIT + 1,
        hang: false,
    })
    .await;
    let mut client = BaseNodeGrpcClient::connect(&addr, &cfg).await.unwrap();
    let status = client.get_version(grpc::Empty {}).await.unwrap_err();
    assert_eq!(status.code(), Code::OutOfRange, "{status:?}");

    let addr = serve(StubBaseNode {
        reply_len: LIMIT - 16,
        hang: false,
    })
    .await;
    let mut client = BaseNodeGrpcClient::connect(&addr, &cfg).await.unwrap();
    client.get_version(grpc::Empty {}).await.unwrap();
}

#[tokio::test]
async fn request_over_max_message_size_is_rejected() {
    let cfg = small_limit();
    let addr = serve(StubBaseNode {
        reply_len: 0,
        hang: false,
    })
    .await;
    let channel = connect_channel(&addr, &cfg).await.unwrap();
    let mut client = BaseNodeGrpcClient::from_channel(channel, &cfg).unwrap();
    // The stub decodes without a limit, so only the client's encoding limit can fail the call. tonic resets the
    // stream rather than returning OutOfRange for a request body it refuses to encode.
    let header_by_hash = |len| grpc::GetHeaderByHashRequest { hash: vec![0u8; len] };
    client.get_header_by_hash(header_by_hash(LIMIT - 16)).await.unwrap();
    client.get_header_by_hash(header_by_hash(LIMIT + 1)).await.unwrap_err();
}

#[tokio::test]
async fn unanswered_request_times_out() {
    let request_timeout = Duration::from_millis(500);
    let cfg = GrpcClientConfig {
        request_timeout: Some(request_timeout),
        ..Default::default()
    };
    let addr = serve(StubBaseNode {
        reply_len: 0,
        hang: true,
    })
    .await;
    let mut client = BaseNodeGrpcClient::connect(&addr, &cfg).await.unwrap();
    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(10), client.get_version(grpc::Empty {}))
        .await
        .expect("request_timeout did not end the call");
    assert!(result.is_err());
    assert!(start.elapsed() < request_timeout + Duration::from_secs(1));
}

//  Copyright 2026. The Tari Project
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
    net::SocketAddr,
    task::{Context, Poll},
};

use minotari_app_grpc::tls::certs::{default_sans, generate_self_signed_certs};
use minotari_wallet_grpc_client::{
    Certificate,
    ClientTlsConfig,
    Endpoint,
    GrpcAuthentication,
    WalletClientError,
    WalletGrpcClient,
    grpc::{GetVersionRequest, GetVersionResponse},
};
use tari_utilities::SafePassword;
use tokio::net::TcpListener;
use tonic::{
    Request,
    Response,
    Status,
    body::Body,
    codec::ProstCodec,
    codegen::{BoxFuture, Service, http},
    server::{Grpc, NamedService, UnaryService},
    transport::{Identity, Server, ServerTlsConfig, server::TcpIncoming},
};

/// Minimal `tari.rpc.Wallet` service that answers every call as `GetVersion`, returning the request's
/// `authorization` header as the version. The generated `Wallet` trait would need all ~50 methods stubbed.
#[derive(Clone)]
struct EchoAuthWallet;

impl NamedService for EchoAuthWallet {
    const NAME: &'static str = "tari.rpc.Wallet";
}

impl Service<http::Request<Body>> for EchoAuthWallet {
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    type Response = http::Response<Body>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        Box::pin(async move {
            let mut grpc = Grpc::new(ProstCodec::<GetVersionResponse, GetVersionRequest>::default());
            Ok(grpc.unary(EchoAuthHeader, req).await)
        })
    }
}

struct EchoAuthHeader;

impl UnaryService<GetVersionRequest> for EchoAuthHeader {
    type Future = Ready<Result<Response<GetVersionResponse>, Status>>;
    type Response = GetVersionResponse;

    fn call(&mut self, request: Request<GetVersionRequest>) -> Self::Future {
        let version = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        ready(Ok(Response::new(GetVersionResponse { version })))
    }
}

async fn spawn_server(identity: Option<Identity>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut builder = Server::builder();
    if let Some(identity) = identity {
        builder = builder.tls_config(ServerTlsConfig::new().identity(identity)).unwrap();
    }
    tokio::spawn(
        builder
            .add_service(EchoAuthWallet)
            .serve_with_incoming(TcpIncoming::from(listener)),
    );
    addr
}

async fn spawn_tls_server() -> (SocketAddr, String) {
    let (ca_pem, _, server_cert_pem, server_key_pem) = generate_self_signed_certs(default_sans()).unwrap();
    let addr = spawn_server(Some(Identity::from_pem(server_cert_pem, server_key_pem.as_bytes()))).await;
    (addr, ca_pem)
}

fn tls_config(ca_pem: &str) -> ClientTlsConfig {
    ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .domain_name("127.0.0.1")
}

#[tokio::test]
async fn tls_round_trip_with_pinned_ca() {
    let (addr, ca_pem) = spawn_tls_server().await;
    let mut client = WalletGrpcClient::builder(&format!("https://127.0.0.1:{}", addr.port()))
        .tls(tls_config(&ca_pem))
        .connect()
        .await
        .unwrap();
    client.get_version(GetVersionRequest {}).await.unwrap();
}

#[tokio::test]
async fn tls_with_unrelated_ca_fails() {
    let (addr, _) = spawn_tls_server().await;
    let (unrelated_ca_pem, _, _, _) = generate_self_signed_certs(default_sans()).unwrap();
    let err = WalletGrpcClient::builder(&format!("https://127.0.0.1:{}", addr.port()))
        .tls(tls_config(&unrelated_ca_pem))
        .connect()
        .await
        .unwrap_err();
    assert!(matches!(err, WalletClientError::TransportError(_)), "{err:?}");
}

#[tokio::test]
async fn https_without_tls_config_fails() {
    let (addr, _) = spawn_tls_server().await;
    let result = WalletGrpcClient::builder(&format!("https://127.0.0.1:{}", addr.port()))
        .connect()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn from_channel_attaches_authorization_header() {
    let addr = spawn_server(None).await;
    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let auth = GrpcAuthentication::Basic {
        username: "user".to_string(),
        password: SafePassword::from("pass"),
    };
    let mut client = WalletGrpcClient::from_channel(channel, &auth).unwrap();
    let version = client
        .get_version(GetVersionRequest {})
        .await
        .unwrap()
        .into_inner()
        .version;
    // base64("user:pass")
    assert_eq!(version, "Basic dXNlcjpwYXNz");
}

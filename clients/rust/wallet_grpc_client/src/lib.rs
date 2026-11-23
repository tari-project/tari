// Copyright 2022, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    net::IpAddr,
    ops::{Deref, DerefMut},
    time::Duration,
};

use tari_common::MAX_GRPC_MESSAGE_SIZE;
use tonic::codegen::{InterceptedService, http::uri::InvalidUri};

pub type Client<TTransport> = grpc::wallet_client::WalletClient<TTransport>;

// Re-export so that users don't have to depend on minotari_app_grpc
pub use minotari_app_grpc::{
    authentication::{BasicAuthError, ClientAuthenticationInterceptor},
    tari_rpc as grpc,
};
pub use tari_common_types::grpc_authentication::GrpcAuthentication;
// Re-export so that users can set up TLS without depending on tonic
pub use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

#[derive(Debug, Clone)]
pub struct WalletGrpcClient<TTransport> {
    client: Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>,
}

impl WalletGrpcClient<Channel> {
    pub fn builder(addr: &str) -> WalletGrpcClientBuilder {
        WalletGrpcClientBuilder {
            addr: addr.to_string(),
            tls: None,
            auth: GrpcAuthentication::None,
            connect_timeout: Duration::from_secs(10),
            timeout: None,
            http2_keepalive_interval: Duration::from_secs(30),
            http2_keepalive_timeout: Duration::from_secs(20),
            tcp_keepalive: Some(Duration::from_secs(60)),
            allow_insecure_credentials: false,
        }
    }

    /// Wraps an already connected channel. The insecure-credential check is not applied: the caller chose the
    /// transport.
    pub fn from_channel(channel: Channel, auth: &GrpcAuthentication) -> Result<Self, WalletClientError> {
        Ok(Self {
            client: Client::with_interceptor(channel, ClientAuthenticationInterceptor::create(auth)?)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE),
        })
    }

    pub async fn connect(addr: &str) -> Result<Self, WalletClientError> {
        Self::builder(addr).connect().await
    }

    pub async fn connect_with_auth(addr: &str, auth: &GrpcAuthentication) -> Result<Self, WalletClientError> {
        Self::builder(addr).auth(auth).connect().await
    }
}

/// Builds a [`WalletGrpcClient`]. An `https://` address requires [`WalletGrpcClientBuilder::tls`] with the
/// wallet's CA; without it the connection fails.
pub struct WalletGrpcClientBuilder {
    addr: String,
    tls: Option<ClientTlsConfig>,
    auth: GrpcAuthentication,
    connect_timeout: Duration,
    timeout: Option<Duration>,
    http2_keepalive_interval: Duration,
    http2_keepalive_timeout: Duration,
    tcp_keepalive: Option<Duration>,
    allow_insecure_credentials: bool,
}

impl WalletGrpcClientBuilder {
    pub fn tls(mut self, cfg: ClientTlsConfig) -> Self {
        self.tls = Some(cfg);
        self
    }

    pub fn auth(mut self, auth: &GrpcAuthentication) -> Self {
        self.auth = auth.clone();
        self
    }

    /// Default 10s.
    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.connect_timeout = d;
        self
    }

    /// Per-request timeout. None by default, since it would also cut server-streaming RPCs.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    /// Default 30s interval, 20s timeout.
    pub fn http2_keepalive(mut self, interval: Duration, timeout: Duration) -> Self {
        self.http2_keepalive_interval = interval;
        self.http2_keepalive_timeout = timeout;
        self
    }

    /// Default `Some(60s)`.
    pub fn tcp_keepalive(mut self, d: Option<Duration>) -> Self {
        self.tcp_keepalive = d;
        self
    }

    /// Allow Basic credentials over plaintext `http://` to a non-loopback host. Default false.
    pub fn allow_insecure_credentials(mut self, allow: bool) -> Self {
        self.allow_insecure_credentials = allow;
        self
    }

    pub async fn connect(self) -> Result<WalletGrpcClient<Channel>, WalletClientError> {
        let is_unix_socket = self.addr.starts_with("unix:");
        let mut endpoint = Endpoint::from_shared(self.addr)?
            .connect_timeout(self.connect_timeout)
            .http2_keep_alive_interval(self.http2_keepalive_interval)
            .keep_alive_timeout(self.http2_keepalive_timeout)
            .keep_alive_while_idle(true)
            .tcp_keepalive(self.tcp_keepalive);
        // Only an `https` URI is encrypted by tonic, whether or not a TLS config is set.
        if matches!(self.auth, GrpcAuthentication::Basic { .. }) &&
            !is_unix_socket &&
            endpoint.uri().scheme_str() != Some("https") &&
            !self.allow_insecure_credentials
        {
            let host = endpoint.uri().host().unwrap_or_default();
            if !is_loopback_host(host) {
                return Err(WalletClientError::InsecureCredentialTransport { host: host.to_string() });
            }
        }
        if let Some(timeout) = self.timeout {
            endpoint = endpoint.timeout(timeout);
        }
        if let Some(tls) = self.tls {
            endpoint = endpoint.tls_config(tls)?;
        }
        WalletGrpcClient::from_channel(endpoint.connect().await?, &self.auth)
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") ||
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

impl<TTransport> Deref for WalletGrpcClient<TTransport> {
    type Target = Client<InterceptedService<TTransport, ClientAuthenticationInterceptor>>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl<TTransport> DerefMut for WalletGrpcClient<TTransport> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WalletClientError {
    #[error(transparent)]
    InvalidUri(#[from] InvalidUri),
    #[error(transparent)]
    TransportError(#[from] tonic::transport::Error),
    #[error(transparent)]
    AuthenticationError(#[from] BasicAuthError),
    /// Basic credentials would be sent over plaintext http to a non-loopback host.
    #[error(
        "refusing to send Basic credentials over plaintext http to non-loopback host '{host}'; use https with TLS or \
         allow_insecure_credentials(true)"
    )]
    InsecureCredentialTransport { host: String },
}

#[cfg(test)]
mod test {
    use tari_utilities::SafePassword;

    use super::*;

    fn basic() -> GrpcAuthentication {
        GrpcAuthentication::Basic {
            username: "user".to_string(),
            password: SafePassword::from("pass"),
        }
    }

    #[test]
    fn loopback_hosts() {
        for host in ["127.0.0.1", "127.5.5.5", "[::1]", "localhost"] {
            assert!(is_loopback_host(host), "{host}");
        }
        for host in ["10.0.0.1", "example.com", "[::ffff:10.0.0.1]"] {
            assert!(!is_loopback_host(host), "{host}");
        }
    }

    #[tokio::test]
    async fn refuses_basic_over_remote_http() {
        let err = WalletGrpcClient::builder("http://10.0.0.1:18143")
            .auth(&basic())
            .connect()
            .await
            .unwrap_err();
        assert!(
            matches!(&err, WalletClientError::InsecureCredentialTransport { host } if host == "10.0.0.1"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn allow_insecure_credentials_proceeds_to_transport() {
        let err = WalletGrpcClient::builder("http://10.0.0.1:18143")
            .auth(&basic())
            .allow_insecure_credentials(true)
            .connect_timeout(Duration::from_millis(200))
            .connect()
            .await
            .unwrap_err();
        assert!(matches!(err, WalletClientError::TransportError(_)), "{err:?}");
    }

    #[tokio::test]
    async fn no_auth_over_remote_http_is_not_refused() {
        let err = WalletGrpcClient::builder("http://10.0.0.1:18143")
            .connect_timeout(Duration::from_millis(200))
            .connect()
            .await
            .unwrap_err();
        assert!(matches!(err, WalletClientError::TransportError(_)), "{err:?}");
    }
}

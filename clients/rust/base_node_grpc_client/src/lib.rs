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

//! Base node gRPC client with auth, optional TLS, timeouts, keepalive and message size limits applied from one
//! [`GrpcClientConfig`].

use std::{
    net::IpAddr,
    ops::{Deref, DerefMut},
    time::Duration,
};

use log::warn;
use minotari_app_grpc::authentication::BasicAuthError;
pub use minotari_app_grpc::{authentication::ClientAuthenticationInterceptor, tari_rpc as grpc};
use tari_common::MAX_GRPC_MESSAGE_SIZE;
pub use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{
    codegen::InterceptedService,
    transport::{Certificate, Channel, ClientTlsConfig, Endpoint},
};

const LOG_TARGET: &str = "minotari::grpc_client";

/// Kept for compatibility with external users of the old generic alias.
pub type RawBaseNodeGrpcClient<TChannel> = grpc::base_node_client::BaseNodeClient<TChannel>;
pub type ShaP2PoolGrpcClient =
    grpc::sha_p2_pool_client::ShaP2PoolClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;
type InnerClient = RawBaseNodeGrpcClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

#[derive(Clone, Debug)]
pub struct GrpcTlsConfig {
    pub ca_cert_pem: Vec<u8>,
    pub domain_name: String,
}

#[derive(Clone, Debug)]
pub struct GrpcClientConfig {
    pub auth: GrpcAuthentication,
    /// Verify the node against this CA. Without it an `https://` address is verified against the system roots.
    pub tls: Option<GrpcTlsConfig>,
    pub connect_timeout: Duration,
    /// Bounds the time until the server starts responding; `None` disables it.
    pub request_timeout: Option<Duration>,
    pub http2_keep_alive_interval: Duration,
    pub http2_keep_alive_timeout: Duration,
    pub keep_alive_while_idle: bool,
    pub tcp_keepalive: Option<Duration>,
    /// Applied to both encoding and decoding.
    pub max_message_size: usize,
}

impl Default for GrpcClientConfig {
    fn default() -> Self {
        Self {
            auth: GrpcAuthentication::None,
            tls: None,
            connect_timeout: Duration::from_secs(5),
            request_timeout: Some(Duration::from_secs(30)),
            http2_keep_alive_interval: Duration::from_secs(30),
            http2_keep_alive_timeout: Duration::from_secs(20),
            keep_alive_while_idle: true,
            tcp_keepalive: Some(Duration::from_secs(60)),
            max_message_size: MAX_GRPC_MESSAGE_SIZE,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcClientError {
    #[error("Invalid gRPC address: {0}")]
    InvalidUri(#[source] tonic::transport::Error),
    #[error("gRPC TLS error: {0}")]
    Tls(String),
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("Invalid gRPC credentials: {0}")]
    Auth(#[from] BasicAuthError),
}

#[derive(Clone, Debug)]
pub struct BaseNodeGrpcClient {
    inner: InnerClient,
}

impl BaseNodeGrpcClient {
    pub async fn connect(addr: &str, cfg: &GrpcClientConfig) -> Result<Self, GrpcClientError> {
        Self::from_channel(connect_channel(addr, cfg).await?, cfg)
    }

    /// Applies the auth and message size settings of `cfg`; the transport settings belong to the channel.
    pub fn from_channel(channel: Channel, cfg: &GrpcClientConfig) -> Result<Self, GrpcClientError> {
        let inner = grpc::base_node_client::BaseNodeClient::with_interceptor(
            channel,
            ClientAuthenticationInterceptor::create(&cfg.auth)?,
        )
        .max_encoding_message_size(cfg.max_message_size)
        .max_decoding_message_size(cfg.max_message_size);
        Ok(Self { inner })
    }
}

impl Deref for BaseNodeGrpcClient {
    type Target = InnerClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for BaseNodeGrpcClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Shared channel builder so the p2pool client (and future clients) get the same defaults.
pub async fn connect_channel(addr: &str, cfg: &GrpcClientConfig) -> Result<Channel, GrpcClientError> {
    Ok(endpoint(addr, cfg)?.connect().await?)
}

/// The unconnected endpoint [`connect_channel`] dials, for callers that manage connections themselves. Logs the
/// plaintext warning.
pub fn endpoint(addr: &str, cfg: &GrpcClientConfig) -> Result<Endpoint, GrpcClientError> {
    let mut endpoint = Endpoint::from_shared(addr.to_string())
        .map_err(GrpcClientError::InvalidUri)?
        .connect_timeout(cfg.connect_timeout)
        .http2_keep_alive_interval(cfg.http2_keep_alive_interval)
        .keep_alive_timeout(cfg.http2_keep_alive_timeout)
        .keep_alive_while_idle(cfg.keep_alive_while_idle)
        .tcp_keepalive(cfg.tcp_keepalive);
    if let Some(timeout) = cfg.request_timeout {
        endpoint = endpoint.timeout(timeout);
    }
    let is_https = endpoint.uri().scheme_str() == Some("https");
    let tls_config = match &cfg.tls {
        // tonic only uses TLS for an https URI and would otherwise connect in plaintext without telling anyone.
        Some(_) if !is_https => {
            return Err(GrpcClientError::Tls(
                "TLS is configured but the address uses http://; use https://".to_string(),
            ));
        },
        Some(tls) => Some(
            ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(&tls.ca_cert_pem))
                .domain_name(&tls.domain_name),
        ),
        // Public-CA nodes; this crate enables tonic's `tls-native-roots`, so the root store is never empty.
        None if is_https => Some(ClientTlsConfig::new().with_enabled_roots()),
        None => None,
    };
    if let Some(tls_config) = tls_config {
        endpoint = endpoint
            .tls_config(tls_config)
            .map_err(|e| GrpcClientError::Tls(e.to_string()))?;
    }
    // A unix socket's URI is an `http://tonic` placeholder.
    if !addr.starts_with("unix:") {
        warn_if_plaintext_remote(endpoint.uri());
    }
    Ok(endpoint)
}

pub fn p2pool_client(channel: Channel, cfg: &GrpcClientConfig) -> Result<ShaP2PoolGrpcClient, GrpcClientError> {
    Ok(grpc::sha_p2_pool_client::ShaP2PoolClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&cfg.auth)?,
    )
    .max_encoding_message_size(cfg.max_message_size)
    .max_decoding_message_size(cfg.max_message_size))
}

/// Plaintext to a remote node is allowed (the node is the operator's own), but the operator is told what it exposes.
fn warn_if_plaintext_remote(uri: &tonic::codegen::http::Uri) {
    let host = uri.host().unwrap_or_default();
    if uri.scheme_str() == Some("http") && !is_loopback_host(host) {
        warn!(
            target: LOG_TARGET,
            "Connecting to {host} over plaintext gRPC. Templates and blocks can be altered and any Basic credentials \
             read by anyone on the network path. Configure TLS (CA certificate and domain name) and use an https:// \
             address."
        );
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") ||
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;

    use log::{Level, LevelFilter, Log, Metadata, Record};

    use super::*;

    #[test]
    fn default_config() {
        let cfg = GrpcClientConfig::default();
        assert!(matches!(cfg.auth, GrpcAuthentication::None));
        assert!(cfg.tls.is_none());
        assert_eq!(cfg.connect_timeout, Duration::from_secs(5));
        assert_eq!(cfg.request_timeout, Some(Duration::from_secs(30)));
        assert_eq!(cfg.http2_keep_alive_interval, Duration::from_secs(30));
        assert_eq!(cfg.http2_keep_alive_timeout, Duration::from_secs(20));
        assert!(cfg.keep_alive_while_idle);
        assert_eq!(cfg.tcp_keepalive, Some(Duration::from_secs(60)));
        assert_eq!(cfg.max_message_size, 8 * 1024 * 1024);
    }

    #[test]
    fn loopback_hosts() {
        for host in ["127.0.0.1", "127.5.5.5", "[::1]", "localhost"] {
            assert!(is_loopback_host(host), "{host}");
        }
        for host in ["10.0.0.1", "node.example", "[::ffff:127.0.0.1]"] {
            assert!(!is_loopback_host(host), "{host}");
        }
    }

    #[test]
    fn https_without_ca_uses_system_roots() {
        let endpoint = endpoint("https://10.0.0.1:18142", &GrpcClientConfig::default()).unwrap();
        assert_eq!(endpoint.uri().scheme_str(), Some("https"));
    }

    #[test]
    fn tls_with_http_address_is_refused() {
        let cfg = GrpcClientConfig {
            tls: Some(GrpcTlsConfig {
                ca_cert_pem: Vec::new(),
                domain_name: "localhost".to_string(),
            }),
            ..Default::default()
        };
        for addr in ["http://10.0.0.1:18142", "http://127.0.0.1:18142"] {
            let err = endpoint(addr, &cfg).unwrap_err();
            assert!(matches!(err, GrpcClientError::Tls(_)), "{addr}: {err:?}");
        }
    }

    struct ThreadLocalLogger;

    thread_local! {
        static CAPTURED: RefCell<Vec<(Level, String)>> = const { RefCell::new(Vec::new()) };
    }

    impl Log for ThreadLocalLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            if record.target() == LOG_TARGET {
                CAPTURED.with(|logs| logs.borrow_mut().push((record.level(), record.args().to_string())));
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: ThreadLocalLogger = ThreadLocalLogger;

    fn warnings_for(addr: &str) -> Vec<(Level, String)> {
        let _unused = log::set_logger(&LOGGER);
        log::set_max_level(LevelFilter::Debug);
        CAPTURED.with(|logs| logs.borrow_mut().clear());
        endpoint(addr, &GrpcClientConfig::default()).unwrap();
        CAPTURED.with(|logs| logs.borrow().clone())
    }

    #[test]
    fn plaintext_warning_only_for_remote_hosts() {
        let logs = warnings_for("http://10.0.0.1:18142");
        let [(level, message)] = logs.as_slice() else {
            panic!("expected one warning: {logs:?}");
        };
        assert_eq!(*level, Level::Warn);
        assert!(message.contains("10.0.0.1"), "{message}");

        for addr in ["http://127.0.0.1:18142", "http://[::1]:18142", "unix:///tmp/node.sock"] {
            assert!(warnings_for(addr).is_empty(), "{addr}");
        }
    }
}

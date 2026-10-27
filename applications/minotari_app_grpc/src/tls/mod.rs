// Copyright 2023. The Tari Project
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

use std::net::{IpAddr, SocketAddr};

use log::warn;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::codegen::http::{Uri, uri::InvalidUri};

pub mod certs;
pub mod error;
pub mod identity;

const LOG_TARGET: &str = "tari::application::grpc::tls";

pub fn protocol_string(tls_enabled: bool) -> String {
    format!("http{}://", if tls_enabled { "s" } else { "" })
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcTlsValidationError {
    #[error("Invalid gRPC endpoint URI: {0}")]
    InvalidUri(#[from] InvalidUri),
    #[error(
        "Refusing to start gRPC server with Basic authentication and TLS disabled on non-loopback address \
         '{bind_address}': Basic authentication transmits credentials in cleartext without TLS. Either enable TLS \
         ('grpc_tls_enabled = true'), bind to a loopback address ('127.0.0.1' or '::1'), or set \
         'grpc_allow_insecure_auth = true' to accept cleartext credentials."
    )]
    InsecureServerBasicAuthWithoutTls { bind_address: String },
    #[error(
        "gRPC TLS domain name is configured, but the endpoint address uses scheme '{actual_scheme}' instead of \
         '{expected_scheme}'; change the address scheme to '{expected_scheme}'"
    )]
    TlsDomainRequiresHttpsScheme {
        actual_scheme: String,
        expected_scheme: String,
    },
    #[error(
        "Refusing to send gRPC Basic authentication credentials over plaintext '{scheme}' transport to a non-loopback \
         endpoint: either use an '{https_scheme}' endpoint with TLS configured, connect to a loopback host \
         ('127.0.0.1', '::1', or 'localhost'), or set 'base_node_grpc_allow_insecure_auth = true' to allow cleartext \
         Basic authentication"
    )]
    InsecureClientBasicAuthOverHttp { scheme: String, https_scheme: String },
}

/// Returns `true` when `bind_address` (a multiaddr such as `/ip4/127.0.0.1/tcp/18142` or `/ip6/::1/tcp/18142`, a
/// `SocketAddr`, or an `IpAddr`) resolves to a loopback IPv4 (`127.0.0.0/8`) or IPv6 (`::1`) address.
pub fn is_loopback_bind_address(bind_address: &str) -> bool {
    let mut parts = bind_address.split('/').filter(|s| !s.is_empty());
    match (parts.next(), parts.next()) {
        (Some("ip4" | "ip6"), Some(host)) => host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()),
        _ => bind_address
            .parse::<SocketAddr>()
            .map(|sa| sa.ip().is_loopback())
            .or_else(|_| bind_address.parse::<IpAddr>().map(|ip| ip.is_loopback()))
            .unwrap_or(false),
    }
}

/// Returns `true` when `uri`'s host is `localhost` (case-insensitive) or resolves to a loopback IPv4 (`127.0.0.0/8`)
/// or IPv6 (`::1`) address literal.
pub fn is_loopback_uri_host(uri: &Uri) -> bool {
    let Some(host) = uri.host() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let unbracketed = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    unbracketed.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Validates server-side gRPC authentication and TLS transport settings at startup.
///
/// If `auth` is `GrpcAuthentication::Basic`, `tls_enabled` is `false`, and `bind_address` is not loopback, returns an
/// error unless `allow_insecure_auth` is `true` (in which case a loud warning is logged at startup).
pub fn validate_server_grpc_tls_and_auth(
    bind_address: &str,
    auth: &GrpcAuthentication,
    tls_enabled: bool,
    allow_insecure_auth: bool,
) -> Result<(), GrpcTlsValidationError> {
    if !matches!(auth, GrpcAuthentication::Basic { .. }) || tls_enabled || is_loopback_bind_address(bind_address) {
        return Ok(());
    }
    if allow_insecure_auth {
        warn!(
            target: LOG_TARGET,
            "WARNING: gRPC Basic authentication is enabled without TLS on non-loopback address '{bind_address}'; \
             credentials will be transmitted in cleartext! Enable TLS ('grpc_tls_enabled = true'), bind to a \
             loopback address ('127.0.0.1' or '::1'), or set 'grpc_allow_insecure_auth = false' to refuse cleartext \
             Basic authentication."
        );
        return Ok(());
    }
    Err(GrpcTlsValidationError::InsecureServerBasicAuthWithoutTls {
        bind_address: bind_address.to_string(),
    })
}

/// Validates client-side gRPC endpoint URI scheme, TLS domain configuration, and Basic authentication settings.
///
/// 1. If `tls_domain_name` is `Some`, the URI scheme must be `https` (`protocol_string(true)`).
/// 2. If `auth` is `GrpcAuthentication::Basic`, the URI scheme is not `https`, and the host is not loopback
///    (`127.0.0.0/8`, `::1`, or `localhost`), fails unless `allow_insecure_auth` is `true` (in which case a warning is
///    logged).
pub fn validate_client_grpc_tls_and_auth(
    addr: &str,
    tls_domain_name: Option<&str>,
    auth: &GrpcAuthentication,
    allow_insecure_auth: bool,
) -> Result<(), GrpcTlsValidationError> {
    let uri: Uri = addr.parse()?;
    let https_scheme = protocol_string(true);
    let http_scheme = protocol_string(false);
    let is_https = uri.scheme_str() == Some("https");
    let actual_scheme = uri
        .scheme_str()
        .map(|s| format!("{s}://"))
        .unwrap_or_else(|| "<missing scheme>".to_string());

    if tls_domain_name.is_some() && !is_https {
        return Err(GrpcTlsValidationError::TlsDomainRequiresHttpsScheme {
            actual_scheme,
            expected_scheme: https_scheme,
        });
    }

    if matches!(auth, GrpcAuthentication::Basic { .. }) && !is_https && !is_loopback_uri_host(&uri) {
        if allow_insecure_auth {
            warn!(
                target: LOG_TARGET,
                "gRPC Basic authentication is enabled over plaintext '{http_scheme}' transport to non-loopback \
                 endpoint '{addr}' because 'base_node_grpc_allow_insecure_auth = true'; credentials will be \
                 transmitted in cleartext"
            );
            return Ok(());
        }
        return Err(GrpcTlsValidationError::InsecureClientBasicAuthOverHttp {
            scheme: actual_scheme,
            https_scheme,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tari_utilities::SafePassword;

    use super::*;

    fn sample_basic_auth() -> GrpcAuthentication {
        GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret_password"),
        }
    }

    #[test]
    fn server_refuses_basic_auth_without_tls_on_non_loopback_and_names_override() {
        let err = validate_server_grpc_tls_and_auth("/ip4/0.0.0.0/tcp/18142", &sample_basic_auth(), false, false)
            .expect_err("non-loopback Basic auth without TLS must fail");
        let msg = err.to_string();
        assert!(msg.contains("grpc_tls_enabled = true"), "unexpected error: {msg}");
        assert!(msg.contains("127.0.0.1"), "unexpected error: {msg}");
        assert!(msg.contains("::1"), "unexpected error: {msg}");
        assert!(
            msg.contains("grpc_allow_insecure_auth = true"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn server_allows_basic_auth_without_tls_on_non_loopback_when_override_set() {
        validate_server_grpc_tls_and_auth("/ip4/0.0.0.0/tcp/18142", &sample_basic_auth(), false, true)
            .expect("override should permit non-loopback Basic auth without TLS");
    }

    #[test]
    fn server_allows_basic_auth_without_tls_on_ipv4_and_ipv6_loopback() {
        validate_server_grpc_tls_and_auth("/ip4/127.0.0.1/tcp/18142", &sample_basic_auth(), false, false)
            .expect("IPv4 loopback should be allowed without TLS");
        validate_server_grpc_tls_and_auth("/ip6/::1/tcp/18142", &sample_basic_auth(), false, false)
            .expect("IPv6 loopback should be allowed without TLS");
    }

    #[test]
    fn server_allows_basic_auth_with_tls_on_non_loopback() {
        validate_server_grpc_tls_and_auth("/ip4/0.0.0.0/tcp/18142", &sample_basic_auth(), true, false)
            .expect("TLS enabled should allow non-loopback Basic auth");
    }

    #[test]
    fn client_refuses_tls_domain_with_http_scheme_and_names_scheme() {
        let err = validate_client_grpc_tls_and_auth(
            "http://127.0.0.1:18142",
            Some("localhost"),
            &GrpcAuthentication::None,
            false,
        )
        .expect_err("TLS domain with http:// scheme must fail");
        let msg = err.to_string();
        assert!(msg.contains("http://"), "unexpected error: {msg}");
        assert!(msg.contains("https://"), "unexpected error: {msg}");
    }

    #[test]
    fn client_allows_tls_domain_with_https_scheme() {
        validate_client_grpc_tls_and_auth(
            "https://127.0.0.1:18142",
            Some("localhost"),
            &GrpcAuthentication::None,
            false,
        )
        .expect("TLS domain with https:// scheme should succeed");
    }

    #[test]
    fn client_allows_basic_auth_on_http_loopback_hosts() {
        for addr in [
            "http://127.0.0.1:18142",
            "http://127.0.0.2:18142",
            "http://[::1]:18142",
            "http://localhost:18142",
        ] {
            validate_client_grpc_tls_and_auth(addr, None, &sample_basic_auth(), false)
                .unwrap_or_else(|e| panic!("loopback address {addr} should be allowed over http://: {e}"));
        }
    }

    #[test]
    fn client_refuses_basic_auth_on_non_loopback_http_without_override() {
        for addr in ["http://192.168.1.10:18142", "http://my_base_node:18142"] {
            let err = validate_client_grpc_tls_and_auth(addr, None, &sample_basic_auth(), false)
                .expect_err("Basic auth over non-loopback http:// without override must fail");
            let msg = err.to_string();
            assert!(msg.contains("http://"), "unexpected error: {msg}");
            assert!(msg.contains("https://"), "unexpected error: {msg}");
            assert!(
                msg.contains("base_node_grpc_allow_insecure_auth = true"),
                "unexpected error: {msg}"
            );
        }
    }

    #[test]
    fn client_allows_basic_auth_on_non_loopback_http_when_override_set() {
        validate_client_grpc_tls_and_auth("http://192.168.1.10:18142", None, &sample_basic_auth(), true)
            .expect("Basic auth over non-loopback http:// with override should succeed");
    }

    #[test]
    fn client_allows_basic_auth_on_https_with_tls_domain() {
        validate_client_grpc_tls_and_auth(
            "https://192.168.1.10:18142",
            Some("localhost"),
            &sample_basic_auth(),
            false,
        )
        .expect("Basic auth over https:// with TLS domain should succeed");
    }
}

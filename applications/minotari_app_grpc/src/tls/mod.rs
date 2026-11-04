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

pub mod certs;
pub mod error;
pub mod identity;

use log::warn;
use tari_common_types::grpc_authentication::GrpcAuthentication;

pub fn is_host_loopback(host: &str) -> bool {
    let clean = host.trim_matches(|c| c == '[' || c == ']');
    if clean.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = clean.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    false
}

pub fn validate_client_transport(
    address: &str,
    tls_domain_name: Option<&str>,
    auth: &GrpcAuthentication,
    allow_insecure_auth: bool,
) -> Result<(), String> {
    let uri: tonic::codegen::http::Uri = address
        .parse()
        .map_err(|e| format!("invalid gRPC address scheme: {e}"))?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| "gRPC address is missing a scheme; use http:// or https://".to_string())?;
    match scheme {
        "https" | "http" => {},
        _ => {
            return Err(format!(
                "unsupported gRPC address scheme '{scheme}', use http:// or https://"
            ));
        },
    }
    if tls_domain_name.is_some() && scheme != "https" {
        warn!(
            "gRPC TLS domain is configured for '{address}' but the address scheme is http://; TLS will not be used \
             unless https:// is specified"
        );
    }
    if matches!(auth, GrpcAuthentication::Basic { .. }) && scheme == "http" && !allow_insecure_auth {
        let is_loopback = uri.host().is_some_and(is_host_loopback);
        if !is_loopback {
            return Err(
                "Basic gRPC authentication over http:// is disabled; use https://, remove authentication, or set \
                 allow_insecure_auth=true"
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tari_common_types::grpc_authentication::GrpcAuthentication;

    use super::*;

    fn basic() -> GrpcAuthentication {
        GrpcAuthentication::Basic {
            username: "user".to_string(),
            password: "password".into(),
        }
    }

    #[test]
    fn https_with_tls_domain_succeeds() {
        assert!(validate_client_transport("https://node:18142", Some("node"), &basic(), false).is_ok());
    }

    #[test]
    fn tls_domain_warns_on_http() {
        assert!(validate_client_transport("http://node:18142", Some("node"), &GrpcAuthentication::None, false).is_ok());
    }

    #[test]
    fn basic_http_requires_override_for_remote() {
        assert!(
            validate_client_transport("http://node:18142", None, &basic(), false)
                .unwrap_err()
                .contains("allow_insecure_auth")
        );
    }

    #[test]
    fn basic_http_exempts_loopback() {
        assert!(validate_client_transport("http://127.0.0.1:18142", None, &basic(), false).is_ok());
        assert!(validate_client_transport("http://127.0.0.2:18142", None, &basic(), false).is_ok());
        assert!(validate_client_transport("http://[::1]:18142", None, &basic(), false).is_ok());
        assert!(validate_client_transport("http://localhost:18142", None, &basic(), false).is_ok());
    }

    #[test]
    fn basic_http_override_and_no_auth_succeed() {
        assert!(validate_client_transport("http://node:18142", None, &basic(), true).is_ok());
        assert!(validate_client_transport("http://node:18142", None, &GrpcAuthentication::None, false).is_ok());
    }

    #[test]
    fn malformed_schemes_are_rejected() {
        assert!(
            validate_client_transport("node:18142", None, &GrpcAuthentication::None, false)
                .unwrap_err()
                .contains("missing a scheme")
        );
        assert!(
            validate_client_transport("ftp://node:18142", None, &GrpcAuthentication::None, false)
                .unwrap_err()
                .contains("unsupported")
        );
    }
}

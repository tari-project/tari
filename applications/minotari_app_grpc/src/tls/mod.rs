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

#[cfg(feature = "base_node")]
use tari_common_types::grpc_authentication::GrpcAuthentication;
#[cfg(feature = "base_node")]
use tari_comms::multiaddr::{Multiaddr, Protocol};

pub fn protocol_string(tls_enabled: bool) -> String {
    format!("http{}://", if tls_enabled { "s" } else { "" })
}

#[cfg(feature = "base_node")]
pub fn is_grpc_loopback(address: &Multiaddr) -> bool {
    address.iter().next().is_some_and(|protocol| match protocol {
        Protocol::Ip4(ip) => ip.is_loopback(),
        Protocol::Ip6(ip) => ip.is_loopback(),
        Protocol::Dns(name) | Protocol::Dns4(name) | Protocol::Dns6(name) => name.eq_ignore_ascii_case("localhost"),
        _ => false,
    })
}

#[cfg(feature = "base_node")]
pub fn validate_grpc_auth_transport(
    address: &Multiaddr,
    tls_enabled: bool,
    authentication: &GrpcAuthentication,
    require_secure_auth: bool,
) -> Result<Option<String>, String> {
    if tls_enabled || !matches!(authentication, GrpcAuthentication::Basic { .. }) || is_grpc_loopback(address) {
        return Ok(None);
    }
    let msg = "Basic gRPC authentication on a non-loopback address without TLS exposes credentials in cleartext; \
               enable TLS, bind to loopback (127.0.0.1, ::1, localhost), or set grpc_require_secure_auth=false";
    if require_secure_auth {
        Err(msg.to_string())
    } else {
        Ok(Some(msg.to_string()))
    }
}

#[cfg(all(test, feature = "base_node"))]
mod tests {
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tari_comms::multiaddr::Multiaddr;

    use super::*;

    fn basic() -> GrpcAuthentication {
        GrpcAuthentication::Basic {
            username: "user".to_string(),
            password: "password".into(),
        }
    }

    #[test]
    fn server_auth_transport_validates_loopback_tls_and_strict_mode() {
        let remote: Multiaddr = "/ip4/10.0.0.1/tcp/18142".parse().unwrap();
        let err = validate_grpc_auth_transport(&remote, false, &basic(), true).unwrap_err();
        assert!(err.contains("enable TLS"));
        assert!(err.contains("loopback"));
        assert!(err.contains("grpc_require_secure_auth=false"));

        let warn = validate_grpc_auth_transport(&remote, false, &basic(), false)
            .unwrap()
            .expect("expected warning");
        assert!(warn.contains("cleartext"));

        let local_addrs = [
            "/ip4/127.0.0.1/tcp/18142",
            "/ip6/::1/tcp/18142",
            "/dns4/localhost/tcp/18142",
        ];
        for local in local_addrs {
            let addr: Multiaddr = local.parse().unwrap();
            assert_eq!(validate_grpc_auth_transport(&addr, false, &basic(), true), Ok(None));
        }
        assert_eq!(validate_grpc_auth_transport(&remote, true, &basic(), true), Ok(None));
        let crafted: Multiaddr = "/ip4/10.0.0.1/tcp/18142/ip4/127.0.0.1".parse().unwrap();
        assert!(!is_grpc_loopback(&crafted));
    }
}

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

use std::{sync::Arc, time::Instant};

use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{Request, Status, codegen::http::header::AUTHORIZATION, service::Interceptor};

use crate::authentication::server_auth::{PresentedAuthorization, ServerAuthCore};

/// Tonic Interceptor that delegates authentication to `ServerAuthCore`.
#[derive(Debug, Clone)]
pub struct ServerAuthenticationInterceptor {
    core: Arc<ServerAuthCore>,
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        let core = ServerAuthCore::new(auth)?;
        Some(Self { core })
    }

    pub fn failure_count(&self) -> u64 {
        self.core.failure_count()
    }

    pub fn core(&self) -> &Arc<ServerAuthCore> {
        &self.core
    }

    #[cfg(test)]
    pub fn tracked_peer_count(&self) -> usize {
        self.core.tracked_peer_count()
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let auth_header = match request.metadata().get(AUTHORIZATION.as_str()) {
            Some(t) => match t.to_str() {
                Ok(s) => PresentedAuthorization::Value(s),
                Err(_) => PresentedAuthorization::InvalidAscii,
            },
            None => PresentedAuthorization::Missing,
        };

        let peer_addr = request.remote_addr();

        match self.core.verify(auth_header, peer_addr, None, Instant::now()) {
            Ok(Some(user)) => {
                request.extensions_mut().insert(user);
                Ok(request)
            },
            Ok(None) => Ok(request),
            Err(status) => Err(status),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::{Mutex, OnceLock},
        time::Duration,
    };

    use tari_utilities::SafePassword;
    use tonic::{metadata::MetadataValue, transport::server::TcpConnectInfo};

    use super::*;
    use crate::authentication::{
        BasicAuthCredentials,
        server_auth::{
            AuthenticatedUser,
            INITIAL_BACKOFF,
            MAX_ECHOED_SCHEME_LEN,
            MAX_FAILURES_BEFORE_BACKOFF,
            MAX_TRACKED_PEERS,
        },
    };

    const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

    thread_local! {
        static CAPTURE_THREAD: Cell<bool> = const { Cell::new(false) };
    }

    struct CapturedLogger {
        lines: Mutex<Vec<String>>,
    }

    impl log::Log for CapturedLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn && metadata.target() == LOG_TARGET
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) && CAPTURE_THREAD.with(Cell::get) {
                self.lines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(record.args().to_string());
            }
        }

        fn flush(&self) {}
    }

    static TEST_LOGGER: OnceLock<CapturedLogger> = OnceLock::new();
    static LOGGER_LOCK: Mutex<()> = Mutex::new(());

    fn captured_logger() -> &'static CapturedLogger {
        let logger = TEST_LOGGER.get_or_init(|| CapturedLogger {
            lines: Mutex::new(Vec::new()),
        });
        let _ = log::set_logger(logger);
        log::set_max_level(log::LevelFilter::Warn);
        logger
    }

    fn request_with_auth_and_addr(username: &str, password: &[u8], addr: SocketAddr) -> Request<()> {
        let mut req = Request::new(());
        req.extensions_mut().insert(TcpConnectInfo {
            local_addr: None,
            remote_addr: Some(addr),
        });
        let header = BasicAuthCredentials::generate_header(username, password).unwrap();
        req.metadata_mut().insert(AUTHORIZATION.as_str(), header);
        req
    }

    #[test]
    fn failed_auth_completes_in_microseconds() {
        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("correct-horse-battery-staple"),
        })
        .unwrap();

        let header = BasicAuthCredentials::generate_header("admin", b"wrong-password").unwrap();
        let iterations = 1_000u32;
        let start = Instant::now();
        for _ in 0..iterations {
            let mut req = Request::new(());
            req.metadata_mut().insert(AUTHORIZATION.as_str(), header.clone());
            assert!(interceptor.call(req).is_err());
        }
        let elapsed = start.elapsed();
        let avg_micros = elapsed.as_micros().checked_div(u128::from(iterations)).unwrap_or(0);
        assert!(
            avg_micros < 500,
            "expected microsecond-scale verification, got {avg_micros} us per attempt"
        );
    }

    #[test]
    fn consecutive_failures_from_one_address_trigger_backoff_and_expire() {
        let interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();
        let addr: SocketAddr = "198.51.100.7:43210".parse().unwrap();
        let t0 = Instant::now();

        let header_str = BasicAuthCredentials::generate_header("admin", b"wrong").unwrap();
        let header_val = header_str.to_str().unwrap();

        for _ in 0..MAX_FAILURES_BEFORE_BACKOFF {
            let err = interceptor
                .core
                .verify(PresentedAuthorization::Value(header_val), Some(addr), None, t0)
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        // Within backoff window, even valid credentials rejected
        let good_header = BasicAuthCredentials::generate_header("admin", b"secret").unwrap();
        let good_val = good_header.to_str().unwrap();
        let during_backoff = t0.checked_add(Duration::from_millis(25)).unwrap();
        let err = interceptor
            .core
            .verify(
                PresentedAuthorization::Value(good_val),
                Some(addr),
                None,
                during_backoff,
            )
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        // After expiry, valid request accepted
        let after_expiry = t0
            .checked_add(INITIAL_BACKOFF)
            .and_then(|t| t.checked_add(Duration::from_millis(1)))
            .unwrap();
        let ok = interceptor
            .core
            .verify(PresentedAuthorization::Value(good_val), Some(addr), None, after_expiry)
            .unwrap();
        assert_eq!(ok, Some(AuthenticatedUser("admin".to_string())));
    }

    #[test]
    fn loopback_clients_are_exempt_from_backoff_when_sibling_client_fails() {
        let interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();
        let t0 = Instant::now();

        let loopback_addrs: [SocketAddr; 3] = [
            "127.0.0.1:41000".parse().unwrap(),
            "[::1]:41001".parse().unwrap(),
            "[::ffff:127.0.0.1]:41002".parse().unwrap(),
        ];

        let bad_header = BasicAuthCredentials::generate_header("admin", b"stale-password").unwrap();
        let bad_val = bad_header.to_str().unwrap();
        let good_header = BasicAuthCredentials::generate_header("admin", b"secret").unwrap();
        let good_val = good_header.to_str().unwrap();

        for addr in loopback_addrs {
            for _ in 0..MAX_FAILURES_BEFORE_BACKOFF.saturating_mul(3) {
                let err = interceptor
                    .core
                    .verify(PresentedAuthorization::Value(bad_val), Some(addr), None, t0)
                    .unwrap_err();
                assert_eq!(err.code(), tonic::Code::Unauthenticated);
            }

            let ok = interceptor
                .core
                .verify(PresentedAuthorization::Value(good_val), Some(addr), None, t0)
                .unwrap();
            assert_eq!(ok, Some(AuthenticatedUser("admin".to_string())));
        }

        assert_eq!(interceptor.tracked_peer_count(), 0);
    }

    #[test]
    fn backoff_map_is_bounded_under_many_distinct_addresses() {
        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let total = MAX_TRACKED_PEERS.saturating_mul(2);
        for i in 0..total {
            let ip = Ipv4Addr::from(u32::try_from(i.saturating_add(1)).unwrap());
            let addr = SocketAddr::new(IpAddr::V4(ip), 10_000);
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            assert!(interceptor.call(req).is_err());
        }

        assert_eq!(interceptor.tracked_peer_count(), MAX_TRACKED_PEERS);
    }

    #[test]
    fn failure_log_line_contains_peer_address_and_is_rate_limited() {
        let _guard = LOGGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let logger = captured_logger();
        logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
        CAPTURE_THREAD.with(|c| c.set(true));

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let addr: SocketAddr = "203.0.113.42:55555".parse().unwrap();
        for _ in 0..20 {
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            assert!(interceptor.call(req).is_err());
        }

        CAPTURE_THREAD.with(|c| c.set(false));
        let lines = logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let [line] = lines.as_slice() else {
            panic!("expected 1 rate-limited log line, got {}", lines.len());
        };
        assert!(
            line.contains("203.0.113.42:55555"),
            "expected peer address in log line, got: {line}"
        );
    }

    #[test]
    fn truncates_overlong_scheme_and_throttles_auth_failure_warnings() {
        let _guard = LOGGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let logger = captured_logger();
        logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
        CAPTURE_THREAD.with(|c| c.set(true));

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let token = "A".repeat(8 * 1024);
        let header_val = MetadataValue::try_from(format!("{token} x")).unwrap();

        for _ in 0..1000 {
            let mut req = Request::new(());
            req.metadata_mut().insert(AUTHORIZATION.as_str(), header_val.clone());
            let err = interceptor.call(req).unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        CAPTURE_THREAD.with(|c| c.set(false));
        let lines = logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let [line] = lines.as_slice() else {
            panic!("expected 1 log line, got {}", lines.len());
        };
        let echoed_expected = "A".repeat(MAX_ECHOED_SCHEME_LEN);
        let echoed_too_long = "A".repeat(MAX_ECHOED_SCHEME_LEN + 1);
        assert!(line.contains(&echoed_expected));
        assert!(!line.contains(&echoed_too_long));
    }

    #[test]
    fn username_containing_colon_keeps_starting_logs_error_and_rejects_requests() {
        let _guard = LOGGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let logger = captured_logger();
        logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
        CAPTURE_THREAD.with(|c| c.set(true));

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "bad:user".to_string(),
            password: SafePassword::from("secret"),
        })
        .expect("server interceptor should still construct when username contains ':'");

        let startup_lines = logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(
            startup_lines
                .iter()
                .any(|l| l.contains("gRPC username must not contain ':'; all gRPC requests will be rejected")),
            "expected startup error log, got: {startup_lines:?}"
        );

        let req = request_with_auth_and_addr("bad:user", b"secret", "127.0.0.1:45000".parse().unwrap());
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        CAPTURE_THREAD.with(|c| c.set(false));
    }
}

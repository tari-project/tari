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
    collections::VecDeque,
    fmt,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use log::*;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{Request, Status, codegen::http::header::AUTHORIZATION, service::Interceptor};

use crate::authentication::BasicAuthCredentials;

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

pub const MAX_TRACKED_PEERS: usize = 1024;
pub const MAX_FAILURES_BEFORE_BACKOFF: u32 = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);
pub const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct PeerFailureState {
    consecutive_failures: u32,
    last_failure_at: Instant,
    blocked_until: Option<Instant>,
}

/// Bounded LRU tracker for per-source-IP failed authentication backoff.
#[derive(Debug, Default)]
struct AuthBackoffTracker {
    entries: VecDeque<(IpAddr, PeerFailureState)>,
}

impl AuthBackoffTracker {
    fn is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool {
        let Some(pos) = self.entries.iter().position(|(addr, _)| *addr == ip) else {
            return false;
        };
        let (_, state) = &mut self.entries[pos];
        if let Some(until) = state.blocked_until {
            if now < until {
                return true;
            }
            state.blocked_until = None;
        }
        if now.saturating_duration_since(state.last_failure_at) > FAILURE_WINDOW {
            state.consecutive_failures = 0;
        }
        false
    }

    fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        let mut state = if let Some(pos) = self.entries.iter().position(|(addr, _)| *addr == ip) {
            let (_, existing) = self.entries.remove(pos).expect("pos is valid");
            existing
        } else {
            if self.entries.len() >= MAX_TRACKED_PEERS {
                self.entries.pop_front();
            }
            PeerFailureState {
                consecutive_failures: 0,
                last_failure_at: now,
                blocked_until: None,
            }
        };

        if now.saturating_duration_since(state.last_failure_at) > FAILURE_WINDOW {
            state.consecutive_failures = 0;
        }
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_failure_at = now;

        if state.consecutive_failures >= MAX_FAILURES_BEFORE_BACKOFF {
            let exponent = state
                .consecutive_failures
                .saturating_sub(MAX_FAILURES_BEFORE_BACKOFF)
                .min(10);
            let factor = 1u32 << exponent;
            let delay = INITIAL_BACKOFF.saturating_mul(factor).min(MAX_BACKOFF);
            state.blocked_until = Some(now + delay);
        }

        self.entries.push_back((ip, state));
    }

    fn record_success(&mut self, ip: IpAddr) {
        if let Some(pos) = self.entries.iter().position(|(addr, _)| *addr == ip) {
            self.entries.remove(pos);
        }
    }
}

fn verify_request_header(req: &Request<()>, valid_credentials: &BasicAuthCredentials) -> Result<(), String> {
    let Some(t) = req.metadata().get(AUTHORIZATION.as_str()) else {
        return Err("Missing authorization header".to_string());
    };
    let header = t.to_str().map_err(|e| e.to_string())?;
    let (header_username, header_password) = BasicAuthCredentials::parse_header(header).map_err(|e| e.to_string())?;
    valid_credentials
        .constant_time_validate(&header_username, &header_password)
        .map_err(|e| e.to_string())
}

#[derive(Debug)]
pub struct ServerAuthenticationInterceptor {
    credentials: Option<Arc<BasicAuthCredentials>>,
    backoff: Arc<Mutex<AuthBackoffTracker>>,
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        let credentials = match auth {
            GrpcAuthentication::None => None,
            GrpcAuthentication::Basic { username, password } => {
                Some(Arc::new(BasicAuthCredentials::new(username, password).ok()?))
            },
        };

        Some(Self {
            credentials,
            backoff: Arc::default(),
        })
    }

    #[allow(clippy::result_large_err)]
    fn handle_basic_auth(
        &self,
        req: Request<()>,
        valid_credentials: &BasicAuthCredentials,
    ) -> Result<Request<()>, Status> {
        self.handle_basic_auth_at(req, valid_credentials, Instant::now())
    }

    #[allow(clippy::result_large_err)]
    fn handle_basic_auth_at(
        &self,
        req: Request<()>,
        valid_credentials: &BasicAuthCredentials,
        now: Instant,
    ) -> Result<Request<()>, Status> {
        let peer_addr = req.remote_addr();
        if let Some(addr) = peer_addr {
            let mut tracker = match self.backoff.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if tracker.is_blocked(addr.ip(), now) {
                return Err(unauthenticated(
                    peer_addr,
                    "Too many failed authentication attempts; backing off",
                ));
            }
        }

        match verify_request_header(&req, valid_credentials) {
            Ok(()) => {
                if let Some(addr) = peer_addr {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_success(addr.ip());
                }
                Ok(req)
            },
            Err(err) => {
                if let Some(addr) = peer_addr {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_failure(addr.ip(), now);
                }
                Err(unauthenticated(peer_addr, err))
            },
        }
    }

    #[cfg(test)]
    fn tracked_peer_count(&self) -> usize {
        self.backoff.lock().unwrap_or_else(|e| e.into_inner()).entries.len()
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        match &self.credentials {
            None => Ok(request),
            Some(credentials) => self.handle_basic_auth(request, credentials),
        }
    }
}

impl Clone for ServerAuthenticationInterceptor {
    fn clone(&self) -> Self {
        Self {
            credentials: self.credentials.clone(),
            backoff: self.backoff.clone(),
        }
    }
}

/// Standard unauthenticated response
fn unauthenticated<E: fmt::Display>(peer_addr: Option<SocketAddr>, err: E) -> Status {
    let peer = peer_addr
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    warn!(
        target: LOG_TARGET,
        "GRPC authentication failed for peer {peer}: {err}"
    );
    Status::unauthenticated("Auth failed")
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, net::Ipv4Addr, sync::OnceLock};

    use tari_utilities::SafePassword;
    use tonic::transport::server::TcpConnectInfo;

    use super::*;

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
        let avg_micros = elapsed.as_micros() / u128::from(iterations);
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
        let creds = interceptor.credentials.as_ref().unwrap().clone();
        let addr: SocketAddr = "198.51.100.7:43210".parse().unwrap();
        let t0 = Instant::now();

        for _ in 0..MAX_FAILURES_BEFORE_BACKOFF {
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            let err = interceptor.handle_basic_auth_at(req, &creds, t0).unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        // Within the backoff window, even a request carrying valid credentials is rejected without comparison.
        let valid_during_backoff = request_with_auth_and_addr("admin", b"secret", addr);
        let err = interceptor
            .handle_basic_auth_at(valid_during_backoff, &creds, t0 + INITIAL_BACKOFF / 2)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        // Once the backoff window expires, the valid request is accepted again.
        let valid_after_expiry = request_with_auth_and_addr("admin", b"secret", addr);
        assert!(
            interceptor
                .handle_basic_auth_at(
                    valid_after_expiry,
                    &creds,
                    t0 + INITIAL_BACKOFF + Duration::from_millis(1)
                )
                .is_ok()
        );
    }

    #[test]
    fn backoff_map_is_bounded_under_many_distinct_addresses() {
        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let total = MAX_TRACKED_PEERS * 2 + 50;
        for i in 0..total {
            let ip = Ipv4Addr::from(u32::try_from(i + 1).unwrap());
            let addr = SocketAddr::new(IpAddr::V4(ip), 10_000);
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            assert!(interceptor.call(req).is_err());
        }

        assert_eq!(interceptor.tracked_peer_count(), MAX_TRACKED_PEERS);
    }

    #[test]
    fn failure_log_line_contains_peer_address() {
        let logger = captured_logger();
        logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
        CAPTURE_THREAD.with(|c| c.set(true));

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let addr: SocketAddr = "203.0.113.42:55555".parse().unwrap();
        let req = request_with_auth_and_addr("admin", b"wrong", addr);
        assert!(interceptor.call(req).is_err());

        CAPTURE_THREAD.with(|c| c.set(false));
        let lines = logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("203.0.113.42:55555"),
            "expected peer address in log line, got: {}",
            lines[0]
        );
    }
}

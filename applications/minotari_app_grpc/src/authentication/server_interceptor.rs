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
    collections::{HashMap, VecDeque},
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
const AUTH_FAILURE_WARN_INTERVAL: Duration = Duration::from_secs(60);

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
    }
}

#[derive(Debug, Default)]
struct WarnLimiter {
    state: Mutex<(Option<Instant>, u64)>,
}

impl WarnLimiter {
    fn should_warn(&self, now: Instant) -> Option<u64> {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (last_warning, num_suppressed) = &mut *state;
        let may_warn =
            last_warning.is_none_or(|last| now.saturating_duration_since(last) >= AUTH_FAILURE_WARN_INTERVAL);
        if may_warn {
            *last_warning = Some(now);
            let suppressed = *num_suppressed;
            *num_suppressed = 0;
            Some(suppressed)
        } else {
            *num_suppressed = num_suppressed.saturating_add(1);
            None
        }
    }
}

#[derive(Debug, Clone)]
struct PeerFailureState {
    consecutive_failures: u32,
    last_failure_at: Instant,
    blocked_until: Option<Instant>,
}

/// Bounded tracker for per-source-IP failed authentication backoff using a `HashMap` + recency queue.
#[derive(Debug, Default)]
struct AuthBackoffTracker {
    states: HashMap<IpAddr, PeerFailureState>,
    order: VecDeque<IpAddr>,
}

impl AuthBackoffTracker {
    fn is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool {
        let Some(state) = self.states.get_mut(&ip) else {
            return false;
        };
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
        let mut state = if let Some(existing) = self.states.remove(&ip) {
            self.order.retain(|addr| *addr != ip);
            existing
        } else {
            while self.states.len() >= MAX_TRACKED_PEERS {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                self.states.remove(&oldest);
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
            state.blocked_until = now.checked_add(delay);
        }

        self.order.push_back(ip);
        self.states.insert(ip, state);
    }

    fn record_success(&mut self, ip: IpAddr) {
        if self.states.remove(&ip).is_some() {
            self.order.retain(|addr| *addr != ip);
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
    warn_limiter: Arc<WarnLimiter>,
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
            warn_limiter: Arc::default(),
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
        let tracked_ip = peer_addr.map(|addr| addr.ip()).filter(|ip| !is_loopback_ip(*ip));

        if let Some(ip) = tracked_ip {
            let mut tracker = match self.backoff.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if tracker.is_blocked(ip, now) {
                return Err(self.unauthenticated_at(
                    peer_addr,
                    "Too many failed authentication attempts; backing off",
                    now,
                ));
            }
        }

        match verify_request_header(&req, valid_credentials) {
            Ok(()) => {
                if let Some(ip) = tracked_ip {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_success(ip);
                }
                Ok(req)
            },
            Err(err) => {
                if let Some(ip) = tracked_ip {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_failure(ip, now);
                }
                Err(self.unauthenticated_at(peer_addr, err, now))
            },
        }
    }

    /// Standard unauthenticated response with rate-limited warning
    fn unauthenticated_at<E: fmt::Display>(&self, peer_addr: Option<SocketAddr>, err: E, now: Instant) -> Status {
        if let Some(suppressed) = self.warn_limiter.should_warn(now) {
            let peer = peer_addr
                .map(|addr| addr.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            if suppressed == 0 {
                warn!(
                    target: LOG_TARGET,
                    "GRPC authentication failed for peer {peer}: {err}"
                );
            } else {
                warn!(
                    target: LOG_TARGET,
                    "GRPC authentication failed for peer {peer}: {err} ({suppressed} similar warnings suppressed)"
                );
            }
        }
        Status::unauthenticated("Auth failed")
    }

    #[cfg(test)]
    fn tracked_peer_count(&self) -> usize {
        self.backoff.lock().unwrap_or_else(|e| e.into_inner()).states.len()
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
            warn_limiter: self.warn_limiter.clone(),
        }
    }
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
        let during_backoff = t0.checked_add(Duration::from_millis(25)).unwrap();
        let err = interceptor
            .handle_basic_auth_at(valid_during_backoff, &creds, during_backoff)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        // Once the backoff window expires, the valid request is accepted again.
        let valid_after_expiry = request_with_auth_and_addr("admin", b"secret", addr);
        let after_expiry = t0
            .checked_add(INITIAL_BACKOFF)
            .and_then(|t| t.checked_add(Duration::from_millis(1)))
            .unwrap();
        assert!(
            interceptor
                .handle_basic_auth_at(valid_after_expiry, &creds, after_expiry)
                .is_ok()
        );
    }

    #[test]
    fn loopback_clients_are_exempt_from_backoff_when_sibling_client_fails() {
        let interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();
        let creds = interceptor.credentials.as_ref().unwrap().clone();
        let t0 = Instant::now();

        let loopback_addrs: [SocketAddr; 3] = [
            "127.0.0.1:41000".parse().unwrap(),
            "[::1]:41001".parse().unwrap(),
            "[::ffff:127.0.0.1]:41002".parse().unwrap(),
        ];

        for addr in loopback_addrs {
            // Misconfigured local client fails repeatedly well beyond MAX_FAILURES_BEFORE_BACKOFF.
            for _ in 0..MAX_FAILURES_BEFORE_BACKOFF.saturating_mul(3) {
                let bad_req = request_with_auth_and_addr("admin", b"stale-password", addr);
                let err = interceptor.handle_basic_auth_at(bad_req, &creds, t0).unwrap_err();
                assert_eq!(err.code(), tonic::Code::Unauthenticated);
            }

            // Correctly configured local client on the same loopback IP still authenticates immediately.
            let good_req = request_with_auth_and_addr("admin", b"secret", addr);
            assert!(interceptor.handle_basic_auth_at(good_req, &creds, t0).is_ok());
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

        let total = MAX_TRACKED_PEERS.saturating_mul(2).saturating_add(50);
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
}

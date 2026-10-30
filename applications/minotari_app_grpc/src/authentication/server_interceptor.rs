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
    ops::BitOr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use log::*;
use subtle::Choice;
use tari_common_types::grpc_authentication::{GrpcAuthentication, GrpcCredential, GrpcCredentials};
use tari_utilities::SafePassword;
use tonic::{Request, Status, codegen::http::header::AUTHORIZATION, service::Interceptor};

use crate::authentication::{
    BasicAuthCredentials,
    BasicAuthError,
    PlaintextBasicAuthCredentials,
    basic_auth::MAX_USERNAME_LEN,
};

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

/// Authenticated caller identity attached to request extensions after successful Basic authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    username: String,
}

impl AuthenticatedUser {
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
        }
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

/// Resolves the effective server-side [`GrpcCredentials`] from `grpc_credentials` and the deprecated
/// `grpc_authentication` field.
///
/// When `deprecated_grpc_authentication` is configured as `Basic`, emits a startup deprecation warning naming
/// `<config_section>.grpc_credentials` and loads it as a one-entry credential list if `grpc_credentials` is empty.
pub fn resolve_server_grpc_credentials(
    config_section: &str,
    grpc_credentials: &GrpcCredentials,
    deprecated_grpc_authentication: &GrpcAuthentication,
) -> GrpcCredentials {
    if let GrpcAuthentication::Basic { username, password } = deprecated_grpc_authentication {
        warn!(
            target: LOG_TARGET,
            "`{config_section}.grpc_authentication` is deprecated; use `{config_section}.grpc_credentials`"
        );
        if grpc_credentials.is_empty() {
            return GrpcCredentials::from(vec![GrpcCredential::new(username.clone(), password.clone())]);
        }
    }
    grpc_credentials.clone()
}

#[derive(Debug, Clone)]
pub struct ServerAuthenticationInterceptor {
    /// Configured plaintext credentials prepared for constant-time comparison (empty when authentication is disabled).
    credentials: Arc<[PlaintextBasicAuthCredentials]>,
    reject_all: bool,
    backoff: Arc<Mutex<AuthBackoffTracker>>,
    warn_limiter: Arc<WarnLimiter>,
    #[cfg(test)]
    last_visited_entries: Arc<std::sync::atomic::AtomicUsize>,
}

impl ServerAuthenticationInterceptor {
    /// Constructs a server authentication verifier from the `grpc_credentials` list of plaintext `{ username, password
    /// }` entries.
    pub fn from_credentials(credentials: GrpcCredentials) -> Option<Self> {
        let mut prepared = Vec::with_capacity(credentials.len());
        let mut reject_all = false;
        for entry in credentials {
            match PlaintextBasicAuthCredentials::from_plaintext(entry.username.clone(), &entry.password) {
                Ok(creds) => prepared.push(creds),
                Err(BasicAuthError::InvalidUsername)
                    if entry.username.contains(':') &&
                        entry.username.len() <= MAX_USERNAME_LEN &&
                        std::str::from_utf8(entry.password.reveal()).is_ok() =>
                {
                    error!(
                        target: LOG_TARGET,
                        "gRPC username must not contain ':'; all gRPC requests will be rejected"
                    );
                    reject_all = true;
                },
                Err(_) => return None,
            }
        }
        Some(Self {
            credentials: Arc::from(prepared),
            reject_all,
            backoff: Arc::default(),
            warn_limiter: Arc::default(),
            #[cfg(test)]
            last_visited_entries: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    #[cfg(test)]
    fn last_visited_entries(&self) -> usize {
        self.last_visited_entries.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    fn tracked_peer_count(&self) -> usize {
        self.backoff.lock().unwrap_or_else(|e| e.into_inner()).states.len()
    }

    /// Resolves `grpc_credentials` with fallback to deprecated `grpc_authentication` (logging a deprecation warning
    /// once at startup when the deprecated key is present) and constructs the server verifier.
    pub fn from_config(
        config_section: &str,
        grpc_credentials: &GrpcCredentials,
        deprecated_grpc_authentication: &GrpcAuthentication,
    ) -> Option<Self> {
        let effective =
            resolve_server_grpc_credentials(config_section, grpc_credentials, deprecated_grpc_authentication);
        Self::from_credentials(effective)
    }

    /// Constructs a server authentication verifier from the legacy single-credential [`GrpcAuthentication`] enum,
    /// emitting a deprecation warning when `Basic` authentication is used.
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        if matches!(auth, GrpcAuthentication::Basic { .. }) {
            warn!(
                target: LOG_TARGET,
                "`grpc_authentication` is deprecated; use `grpc_credentials`"
            );
        }
        Self::from_credentials(GrpcCredentials::from(auth))
    }

    /// Verifies `candidate_username` and `candidate_password` against all configured credential entries in constant
    /// time, without short-circuiting, and returns the matched [`AuthenticatedUser`] on success.
    #[allow(clippy::result_large_err)]
    pub fn verify_credentials(
        &self,
        candidate_username: &str,
        candidate_password: &SafePassword,
    ) -> Result<Option<AuthenticatedUser>, Status> {
        self.verify_credentials_for_peer_at(candidate_username, candidate_password, None, Instant::now())
    }

    #[allow(clippy::result_large_err)]
    fn verify_credentials_for_peer_at(
        &self,
        candidate_username: &str,
        candidate_password: &SafePassword,
        peer_addr: Option<SocketAddr>,
        now: Instant,
    ) -> Result<Option<AuthenticatedUser>, Status> {
        if self.reject_all {
            return Err(self.unauthenticated_at(
                peer_addr,
                "gRPC username must not contain ':'; all gRPC requests will be rejected",
                now,
            ));
        }
        if self.credentials.is_empty() {
            return Ok(None);
        }

        let mut any_match = Choice::from(0u8);
        let mut matched_index: usize = 0;
        #[cfg(test)]
        let mut visited_entries = 0usize;

        for (idx, entry) in self.credentials.iter().enumerate() {
            let is_match = entry.constant_time_matches(candidate_username, candidate_password);
            #[cfg(test)]
            {
                visited_entries = visited_entries.saturating_add(1);
            }
            // Branchless conditional selection of `idx` when `is_match` is 1:
            let mask = usize::wrapping_sub(0, usize::from(is_match.unwrap_u8()));
            matched_index = (matched_index & !mask) | (idx & mask);
            any_match = any_match.bitor(is_match);
        }

        #[cfg(test)]
        self.last_visited_entries
            .store(visited_entries, std::sync::atomic::Ordering::SeqCst);

        if bool::from(any_match) {
            let matched_username = self
                .credentials
                .get(matched_index)
                .map(|entry| entry.username().to_string())
                .ok_or_else(|| self.unauthenticated_at(peer_addr, "Invalid username or password", now))?;
            Ok(Some(AuthenticatedUser::new(matched_username)))
        } else {
            Err(self.unauthenticated_at(peer_addr, "Invalid username or password", now))
        }
    }

    #[allow(clippy::result_large_err)]
    fn handle_request_at(&self, mut request: Request<()>, now: Instant) -> Result<Request<()>, Status> {
        let peer_addr = request.remote_addr();
        let tracked_ip = peer_addr.map(|addr| addr.ip()).filter(|ip| !is_loopback_ip(*ip));

        if self.reject_all {
            if let Some(ip) = tracked_ip {
                let mut tracker = match self.backoff.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                tracker.record_failure(ip, now);
            }
            return Err(self.unauthenticated_at(
                peer_addr,
                "gRPC username must not contain ':'; all gRPC requests will be rejected",
                now,
            ));
        }

        if self.credentials.is_empty() {
            return Ok(request);
        }

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

        let fail_peer = |reason: String| -> Status {
            if let Some(ip) = tracked_ip {
                let mut tracker = match self.backoff.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                tracker.record_failure(ip, now);
            }
            self.unauthenticated_at(peer_addr, reason, now)
        };

        let Some(raw_header) = request.metadata().get(AUTHORIZATION.as_str()) else {
            return Err(fail_peer("Missing authorization header".to_string()));
        };
        let header = match raw_header.to_str() {
            Ok(s) => s,
            Err(e) => return Err(fail_peer(e.to_string())),
        };
        let (header_username, header_password) = match BasicAuthCredentials::parse_header(header) {
            Ok(pair) => pair,
            Err(e) => return Err(fail_peer(e.to_string())),
        };

        match self.verify_credentials_for_peer_at(&header_username, &header_password, peer_addr, now) {
            Ok(user_opt) => {
                if let Some(ip) = tracked_ip {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_success(ip);
                }
                if let Some(user) = user_opt {
                    request.extensions_mut().insert(user);
                }
                Ok(request)
            },
            Err(status) => {
                if let Some(ip) = tracked_ip {
                    let mut tracker = match self.backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_failure(ip, now);
                }
                Err(status)
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
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        self.handle_request_at(request, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::arithmetic_side_effects)]
    use std::{
        net::Ipv4Addr,
        sync::{Mutex, OnceLock},
    };

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use tonic::{metadata::MetadataValue, transport::server::TcpConnectInfo};

    use super::*;
    use crate::authentication::basic_auth::MAX_ECHOED_SCHEME_LEN;

    struct CapturingLogger;

    fn captured_logs() -> &'static Mutex<Vec<(Level, String)>> {
        static LOGS: OnceLock<Mutex<Vec<(Level, String)>>> = OnceLock::new();
        LOGS.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn logger_serial_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    impl Log for CapturingLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            if record.target() == LOG_TARGET &&
                let Ok(mut guard) = captured_logs().lock()
            {
                guard.push((record.level(), format!("{}", record.args())));
            }
        }

        fn flush(&self) {}
    }

    fn init_logger() {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            drop(log::set_logger(&CapturingLogger));
            log::set_max_level(LevelFilter::Trace);
        });
    }

    fn request_with_basic_auth(username: &str, password: &str) -> Request<()> {
        let header: MetadataValue<_> = BasicAuthCredentials::generate_header(username, password.as_bytes()).unwrap();
        let mut req = Request::new(());
        req.metadata_mut().insert(AUTHORIZATION.as_str(), header);
        req
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
    fn two_credentials_accept_either_pair_reject_crossed_pairs_and_report_matched_username() {
        let creds = GrpcCredentials::from(vec![
            GrpcCredential::new("miner", SafePassword::from("miner_secret")),
            GrpcCredential::new("monitor", SafePassword::from("monitor_secret")),
        ]);
        let mut interceptor = ServerAuthenticationInterceptor::from_credentials(creds).unwrap();

        // First credential succeeds and attaches "miner"
        let req1 = interceptor
            .call(request_with_basic_auth("miner", "miner_secret"))
            .expect("first credential should succeed");
        assert_eq!(
            req1.extensions().get::<AuthenticatedUser>().map(|u| u.username()),
            Some("miner")
        );

        // Second credential succeeds and attaches "monitor"
        let req2 = interceptor
            .call(request_with_basic_auth("monitor", "monitor_secret"))
            .expect("second credential should succeed");
        assert_eq!(
            req2.extensions().get::<AuthenticatedUser>().map(|u| u.username()),
            Some("monitor")
        );

        // Crossed username/password combinations are rejected
        assert!(
            interceptor
                .call(request_with_basic_auth("miner", "monitor_secret"))
                .is_err()
        );
        assert!(
            interceptor
                .call(request_with_basic_auth("monitor", "miner_secret"))
                .is_err()
        );

        // Unknown username or wrong password is rejected
        assert!(
            interceptor
                .call(request_with_basic_auth("unknown", "miner_secret"))
                .is_err()
        );
        assert!(interceptor.call(request_with_basic_auth("miner", "wrong")).is_err());
    }

    #[test]
    fn deprecated_grpc_authentication_authenticates_and_logs_exactly_one_warning_at_startup() {
        let _guard = logger_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = captured_logs().lock() {
            logs.clear();
        }

        let deprecated = GrpcAuthentication::Basic {
            username: "legacy_client".to_string(),
            password: SafePassword::from("legacy_pass"),
        };
        let mut interceptor =
            ServerAuthenticationInterceptor::from_config("base_node", &GrpcCredentials::new(), &deprecated).unwrap();

        let deprecation_warnings: Vec<_> = captured_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, msg)| {
                *level == Level::Warn &&
                    msg.contains("base_node.grpc_authentication") &&
                    msg.contains("base_node.grpc_credentials")
            })
            .cloned()
            .collect();
        assert_eq!(
            deprecation_warnings.len(),
            1,
            "expected exactly one startup deprecation warning, got: {deprecation_warnings:?}"
        );

        let authed_req = interceptor
            .call(request_with_basic_auth("legacy_client", "legacy_pass"))
            .expect("deprecated grpc_authentication should still authenticate");
        assert_eq!(
            authed_req.extensions().get::<AuthenticatedUser>().map(|u| u.username()),
            Some("legacy_client")
        );
    }

    #[test]
    fn verification_visits_all_entries_for_wrong_username_and_wrong_password() {
        let creds = GrpcCredentials::from(vec![
            GrpcCredential::new("client_a", SafePassword::from("pass_a")),
            GrpcCredential::new("client_b", SafePassword::from("pass_b")),
            GrpcCredential::new("client_c", SafePassword::from("pass_c")),
        ]);
        let interceptor = ServerAuthenticationInterceptor::from_credentials(creds).unwrap();

        // Matching the first entry must still evaluate all 3 configured credentials without short-circuiting.
        let matched_first = interceptor
            .verify_credentials("client_a", &SafePassword::from("pass_a"))
            .unwrap()
            .unwrap();
        assert_eq!(matched_first.username(), "client_a");
        assert_eq!(interceptor.last_visited_entries(), 3);

        // Matching the middle entry must evaluate all 3 configured credentials.
        let matched_middle = interceptor
            .verify_credentials("client_b", &SafePassword::from("pass_b"))
            .unwrap()
            .unwrap();
        assert_eq!(matched_middle.username(), "client_b");
        assert_eq!(interceptor.last_visited_entries(), 3);

        // Matching the last entry must evaluate all 3 configured credentials.
        let matched_last = interceptor
            .verify_credentials("client_c", &SafePassword::from("pass_c"))
            .unwrap()
            .unwrap();
        assert_eq!(matched_last.username(), "client_c");
        assert_eq!(interceptor.last_visited_entries(), 3);

        // Failing on unknown username or wrong password must evaluate all 3 configured credentials.
        assert!(
            interceptor
                .verify_credentials("nonexistent_user", &SafePassword::from("pass_a"))
                .is_err()
        );
        assert_eq!(interceptor.last_visited_entries(), 3);

        assert!(
            interceptor
                .verify_credentials("client_a", &SafePassword::from("wrong_password"))
                .is_err()
        );
        assert_eq!(interceptor.last_visited_entries(), 3);
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

        for _ in 0..MAX_FAILURES_BEFORE_BACKOFF {
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            let err = interceptor.handle_request_at(req, t0).unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        let valid_during_backoff = request_with_auth_and_addr("admin", b"secret", addr);
        let during_backoff = t0.checked_add(Duration::from_millis(25)).unwrap();
        let err = interceptor
            .handle_request_at(valid_during_backoff, during_backoff)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        let valid_after_expiry = request_with_auth_and_addr("admin", b"secret", addr);
        let after_expiry = t0
            .checked_add(INITIAL_BACKOFF)
            .and_then(|t| t.checked_add(Duration::from_millis(1)))
            .unwrap();
        assert!(interceptor.handle_request_at(valid_after_expiry, after_expiry).is_ok());
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

        for addr in loopback_addrs {
            for _ in 0..MAX_FAILURES_BEFORE_BACKOFF.saturating_mul(3) {
                let bad_req = request_with_auth_and_addr("admin", b"stale-password", addr);
                let err = interceptor.handle_request_at(bad_req, t0).unwrap_err();
                assert_eq!(err.code(), tonic::Code::Unauthenticated);
            }

            let good_req = request_with_auth_and_addr("admin", b"secret", addr);
            assert!(interceptor.handle_request_at(good_req, t0).is_ok());
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
        let _guard = logger_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = captured_logs().lock() {
            logs.clear();
        }

        let mut interceptor =
            ServerAuthenticationInterceptor::from_credentials(GrpcCredentials::from(vec![GrpcCredential::new(
                "admin",
                SafePassword::from("secret"),
            )]))
            .unwrap();

        let addr: SocketAddr = "203.0.113.42:55555".parse().unwrap();
        for _ in 0..20 {
            let req = request_with_auth_and_addr("admin", b"wrong", addr);
            assert!(interceptor.call(req).is_err());
        }

        let warn_lines: Vec<String> = captured_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, msg)| *level == Level::Warn && msg.contains("203.0.113.42:55555"))
            .map(|(_, msg)| msg.clone())
            .collect();
        assert_eq!(
            warn_lines.len(),
            1,
            "expected 1 rate-limited log line, got: {warn_lines:?}"
        );
    }

    #[test]
    fn truncates_overlong_scheme_and_throttles_auth_failure_warnings() {
        let _guard = logger_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = captured_logs().lock() {
            logs.clear();
        }

        let mut interceptor =
            ServerAuthenticationInterceptor::from_credentials(GrpcCredentials::from(vec![GrpcCredential::new(
                "admin",
                SafePassword::from("secret"),
            )]))
            .unwrap();

        let token = "A".repeat(8 * 1024);
        let header_val = MetadataValue::try_from(format!("{token} x")).unwrap();

        for _ in 0..1000 {
            let mut req = Request::new(());
            req.metadata_mut().insert(AUTHORIZATION.as_str(), header_val.clone());
            let err = interceptor.call(req).unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        let warn_lines: Vec<String> = captured_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| *level == Level::Warn)
            .map(|(_, msg)| msg.clone())
            .collect();
        let [line] = warn_lines.as_slice() else {
            panic!("expected 1 log line, got {}", warn_lines.len());
        };
        let echoed_expected = "A".repeat(MAX_ECHOED_SCHEME_LEN);
        let echoed_too_long = "A".repeat(MAX_ECHOED_SCHEME_LEN + 1);
        assert!(line.contains(&echoed_expected));
        assert!(!line.contains(&echoed_too_long));
    }

    #[test]
    fn username_containing_colon_keeps_starting_logs_error_and_rejects_requests() {
        let _guard = logger_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = captured_logs().lock() {
            logs.clear();
        }

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "bad:user".to_string(),
            password: SafePassword::from("secret"),
        })
        .expect("server interceptor should still construct when username contains ':'");

        let startup_lines: Vec<String> = captured_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| *level == Level::Error)
            .map(|(_, msg)| msg.clone())
            .collect();
        assert!(
            startup_lines
                .iter()
                .any(|l| l.contains("gRPC username must not contain ':'; all gRPC requests will be rejected")),
            "expected startup error log, got: {startup_lines:?}"
        );

        let req = request_with_auth_and_addr("bad:user", b"secret", "127.0.0.1:45000".parse().unwrap());
        let err = interceptor.call(req).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}

//  Copyright 2024 The Tari Project
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
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use log::*;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::Status;

use crate::authentication::{BasicAuthCredentials, BasicAuthError, basic_auth::MAX_USERNAME_LEN};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

pub const MAX_TRACKED_PEERS: usize = 1024;
pub const MAX_FAILURES_BEFORE_BACKOFF: u32 = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);
pub const MAX_BACKOFF: Duration = Duration::from_secs(60);
pub const AUTH_FAILURE_WARN_INTERVAL: Duration = Duration::from_secs(60);
pub const MAX_LOGGED_PATH_LEN: usize = 256;
pub use crate::authentication::basic_auth::MAX_ECHOED_SCHEME_LEN;

/// Strongly-typed extension representing the authenticated gRPC username.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser(pub String);

pub fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
    }
}

/// Rate-limiter for logging warnings about authentication failures.
#[derive(Debug, Default)]
pub struct WarnLimiter {
    state: Mutex<(Option<Instant>, u64)>,
}

impl WarnLimiter {
    pub fn should_warn(&self, now: Instant) -> Option<u64> {
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
pub struct PeerFailureState {
    pub consecutive_failures: u32,
    pub last_failure_at: Instant,
    pub blocked_until: Option<Instant>,
}

/// Bounded tracker for per-source-IP failed authentication backoff using a `HashMap` + recency queue.
#[derive(Debug, Default)]
pub struct AuthBackoffTracker {
    pub states: HashMap<IpAddr, PeerFailureState>,
    pub order: VecDeque<IpAddr>,
}

impl AuthBackoffTracker {
    pub fn is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool {
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

    pub fn record_failure(&mut self, ip: IpAddr, now: Instant) {
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

    pub fn record_success(&mut self, ip: IpAddr) {
        if self.states.remove(&ip).is_some() {
            self.order.retain(|addr| *addr != ip);
        }
    }
}

/// Authorization header representation for abstraction across Tower (http) and Interceptor (tonic).
pub enum PresentedAuthorization<'a> {
    Missing,
    InvalidAscii,
    Value(&'a str),
}

/// Shared authentication engine containing parsed credentials, rate limiting, and backoff state.
#[derive(Debug)]
pub struct ServerAuthCore {
    credentials: Option<Arc<BasicAuthCredentials>>,
    reject_all: bool,
    backoff: Mutex<AuthBackoffTracker>,
    warn_limiter: WarnLimiter,
    failure_counter: AtomicU64,
}

impl ServerAuthCore {
    pub fn new(auth: GrpcAuthentication) -> Option<Arc<Self>> {
        let (credentials, reject_all) = match auth {
            GrpcAuthentication::None => (None, false),
            GrpcAuthentication::Basic { username, password } => {
                match BasicAuthCredentials::new(username.clone(), password.clone()) {
                    Ok(creds) => (Some(Arc::new(creds)), false),
                    Err(BasicAuthError::InvalidUsername)
                        if username.contains(':') &&
                            username.len() <= MAX_USERNAME_LEN &&
                            std::str::from_utf8(password.reveal()).is_ok() =>
                    {
                        error!(
                            target: LOG_TARGET,
                            "gRPC username must not contain ':'; all gRPC requests will be rejected"
                        );
                        (None, true)
                    },
                    Err(_) => return None,
                }
            },
        };

        Some(Arc::new(Self {
            credentials,
            reject_all,
            backoff: Mutex::new(AuthBackoffTracker::default()),
            warn_limiter: WarnLimiter::default(),
            failure_counter: AtomicU64::new(0),
        }))
    }

    pub fn failure_count(&self) -> u64 {
        self.failure_counter.load(Ordering::Relaxed)
    }

    #[allow(clippy::result_large_err)]
    pub fn verify(
        &self,
        auth_header: PresentedAuthorization<'_>,
        peer_addr: Option<SocketAddr>,
        path: Option<&str>,
        now: Instant,
    ) -> Result<Option<AuthenticatedUser>, Status> {
        let tracked_ip = peer_addr.map(|addr| addr.ip()).filter(|ip| !is_loopback_ip(*ip));

        if self.reject_all {
            self.record_failure_and_reject(
                tracked_ip,
                peer_addr,
                path,
                "gRPC username must not contain ':'; all gRPC requests will be rejected",
                now,
            )
        } else if let Some(credentials) = &self.credentials {
            if let Some(ip) = tracked_ip {
                let mut tracker = match self.backoff.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if tracker.is_blocked(ip, now) {
                    drop(tracker);
                    return self.record_failure_and_reject(
                        None,
                        peer_addr,
                        path,
                        "Too many failed authentication attempts; backing off",
                        now,
                    );
                }
            }

            match auth_header {
                PresentedAuthorization::Missing => {
                    self.record_failure_and_reject(tracked_ip, peer_addr, path, "Missing authorization header", now)
                },
                PresentedAuthorization::InvalidAscii => self.record_failure_and_reject(
                    tracked_ip,
                    peer_addr,
                    path,
                    "Invalid characters in authorization header",
                    now,
                ),
                PresentedAuthorization::Value(header_str) => {
                    let parse_res = BasicAuthCredentials::parse_header(header_str);
                    match parse_res {
                        Ok((header_username, header_password)) => {
                            match credentials.constant_time_validate(&header_username, &header_password) {
                                Ok(()) => {
                                    if let Some(ip) = tracked_ip {
                                        let mut tracker = match self.backoff.lock() {
                                            Ok(guard) => guard,
                                            Err(poisoned) => poisoned.into_inner(),
                                        };
                                        tracker.record_success(ip);
                                    }
                                    debug!(target: LOG_TARGET, "gRPC request authenticated for user '{header_username}'");
                                    Ok(Some(AuthenticatedUser(header_username)))
                                },
                                Err(err) => self.record_failure_and_reject(tracked_ip, peer_addr, path, err, now),
                            }
                        },
                        Err(err) => self.record_failure_and_reject(tracked_ip, peer_addr, path, err, now),
                    }
                },
            }
        } else {
            Ok(None)
        }
    }

    #[allow(clippy::result_large_err)]
    fn record_failure_and_reject<E: fmt::Display>(
        &self,
        tracked_ip: Option<IpAddr>,
        peer_addr: Option<SocketAddr>,
        path: Option<&str>,
        err: E,
        now: Instant,
    ) -> Result<Option<AuthenticatedUser>, Status> {
        self.failure_counter.fetch_add(1, Ordering::Relaxed);
        if let Some(ip) = tracked_ip {
            let mut tracker = match self.backoff.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            tracker.record_failure(ip, now);
        }

        if let Some(suppressed) = self.warn_limiter.should_warn(now) {
            let peer = peer_addr
                .map(|addr| addr.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let path_info = match path {
                Some(p) => {
                    let truncated = if p.len() > MAX_LOGGED_PATH_LEN {
                        let mut end = MAX_LOGGED_PATH_LEN;
                        while !p.is_char_boundary(end) && end > 0 {
                            end = end.saturating_sub(1);
                        }
                        &p[..end]
                    } else {
                        p
                    };
                    format!(" on path '{truncated}'")
                },
                None => String::new(),
            };
            if suppressed == 0 {
                warn!(
                    target: LOG_TARGET,
                    "GRPC authentication failed for peer {peer}{path_info}: {err}"
                );
            } else {
                warn!(
                    target: LOG_TARGET,
                    "GRPC authentication failed for peer {peer}{path_info}: {err} ({suppressed} similar warnings suppressed)"
                );
            }
        }

        Err(Status::unauthenticated("Auth failed"))
    }

    #[cfg(test)]
    pub fn tracked_peer_count(&self) -> usize {
        self.backoff.lock().unwrap_or_else(|e| e.into_inner()).states.len()
    }
}

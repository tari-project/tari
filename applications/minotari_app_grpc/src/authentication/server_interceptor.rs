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
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        LazyLock,
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use log::*;
use tari_common_types::grpc_authentication::GrpcAuthentication;
#[cfg(feature = "metrics")]
use tari_metrics::IntCounter;
use tonic::{
    Request,
    Status,
    codegen::{
        BoxFuture,
        Service,
        http::{Extensions, HeaderMap, Request as HttpRequest, Response as HttpResponse, header::AUTHORIZATION},
    },
    server::NamedService,
    service::Interceptor,
    transport::server::{TcpConnectInfo, TlsConnectInfo},
};
use tower_layer::Layer;

use crate::authentication::{BasicAuthCredentials, BasicAuthError, basic_auth::MAX_USERNAME_LEN};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";
#[cfg(feature = "metrics")]
const AUTH_FAILURE_METRIC_NAME: &str = "grpc_authentication_failures_total";
#[cfg(feature = "metrics")]
const AUTH_FAILURE_METRIC_HELP: &str = "Total number of failed gRPC authentication attempts";
const FAILURE_LOG_WINDOW: Duration = Duration::from_secs(1);
const MAX_FAILURE_WARN_LOGS_PER_WINDOW: usize = 10;

pub const MAX_TRACKED_PEERS: usize = 1024;
pub const MAX_FAILURES_BEFORE_BACKOFF: u32 = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(50);
pub const MAX_BACKOFF: Duration = Duration::from_secs(60);

static AUTH_FAILURE_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "metrics")]
static AUTH_FAILURE_COUNTER: LazyLock<IntCounter> = LazyLock::new(|| {
    tari_metrics::register_int_counter(AUTH_FAILURE_METRIC_NAME, AUTH_FAILURE_METRIC_HELP)
        .expect("valid grpc_authentication_failures_total counter")
});

/// Returns the total number of failed gRPC authentication attempts recorded by this process.
/// When the `metrics` feature is enabled, this counter is also exported as `grpc_authentication_failures_total`
/// in the default [`tari_metrics::Registry`].
pub fn grpc_authentication_failures_total() -> u64 {
    AUTH_FAILURE_COUNT.load(Ordering::Relaxed)
}

fn is_exempt_health_check_path(method_path: &str) -> bool {
    matches!(
        method_path,
        "/grpc.health.v1.Health/Check" | "/grpc.health.v1.Health/Watch"
    )
}

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
    }
}

struct FailureLogRateLimiter {
    window_start: Instant,
    emitted_in_window: usize,
    suppressed_in_window: usize,
}

impl FailureLogRateLimiter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            emitted_in_window: 0,
            suppressed_in_window: 0,
        }
    }

    /// Returns `Some(suppressed_from_prior_window)` if this failure should be logged, or `None` if rate-limited.
    fn check(&mut self, now: Instant) -> Option<usize> {
        let mut prior_suppressed = 0;
        if now.saturating_duration_since(self.window_start) >= FAILURE_LOG_WINDOW {
            prior_suppressed = self.suppressed_in_window;
            self.window_start = now;
            self.emitted_in_window = 0;
            self.suppressed_in_window = 0;
        }
        if self.emitted_in_window < MAX_FAILURE_WARN_LOGS_PER_WINDOW {
            self.emitted_in_window = self.emitted_in_window.saturating_add(1);
            Some(prior_suppressed)
        } else {
            self.suppressed_in_window = self.suppressed_in_window.saturating_add(1);
            None
        }
    }
}

static FAILURE_LOG_LIMITER: LazyLock<Mutex<FailureLogRateLimiter>> =
    LazyLock::new(|| Mutex::new(FailureLogRateLimiter::new()));

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

/// Authenticated caller identity inserted into request extensions when Basic authentication succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser(pub String);

impl AuthenticatedUser {
    pub fn new(username: impl Into<String>) -> Self {
        Self(username.into())
    }

    pub fn username(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
enum PreparedServerAuth {
    None,
    RejectAll {
        backoff: Arc<Mutex<AuthBackoffTracker>>,
    },
    Basic {
        username: String,
        credentials: Arc<BasicAuthCredentials>,
        backoff: Arc<Mutex<AuthBackoffTracker>>,
    },
}

impl PreparedServerAuth {
    fn from_config(auth: GrpcAuthentication) -> Option<Self> {
        #[cfg(feature = "metrics")]
        LazyLock::force(&AUTH_FAILURE_COUNTER);
        match auth {
            GrpcAuthentication::None => Some(Self::None),
            GrpcAuthentication::Basic { username, password } => {
                match BasicAuthCredentials::new(username.clone(), password.clone()) {
                    Ok(credentials) => Some(Self::Basic {
                        username,
                        credentials: Arc::new(credentials),
                        backoff: Arc::default(),
                    }),
                    Err(BasicAuthError::InvalidUsername)
                        if username.contains(':') &&
                            username.len() <= MAX_USERNAME_LEN &&
                            std::str::from_utf8(password.reveal()).is_ok() =>
                    {
                        error!(
                            target: LOG_TARGET,
                            "gRPC username must not contain ':'; all gRPC requests will be rejected"
                        );
                        Some(Self::RejectAll {
                            backoff: Arc::default(),
                        })
                    },
                    Err(_) => None,
                }
            },
        }
    }

    #[allow(clippy::result_large_err)]
    fn verify_headers_at(
        &self,
        headers: &HeaderMap,
        method_path: &str,
        peer_addr: Option<SocketAddr>,
        now: Instant,
    ) -> Result<Option<AuthenticatedUser>, Status> {
        let peer_display = peer_addr
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        if is_exempt_health_check_path(method_path) {
            debug!(
                target: LOG_TARGET,
                "gRPC auth decision: method={} username=none peer={} outcome=allow (health probe exempt)",
                method_path,
                peer_display,
            );
            return Ok(None);
        }

        match self {
            Self::None => {
                debug!(
                    target: LOG_TARGET,
                    "gRPC auth decision: method={} username=none peer={} outcome=allow",
                    method_path,
                    peer_display,
                );
                Ok(None)
            },
            Self::RejectAll { backoff } => {
                if let Some(ip) = peer_addr.map(|addr| addr.ip()).filter(|ip| !is_loopback_ip(*ip)) {
                    let mut tracker = match backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_failure(ip, now);
                }
                record_auth_failure(
                    method_path,
                    "none",
                    &peer_display,
                    "gRPC username must not contain ':'; all gRPC requests will be rejected",
                    now,
                );
                Err(Status::unauthenticated("Auth failed"))
            },
            Self::Basic {
                username,
                credentials,
                backoff,
            } => {
                let tracked_ip = peer_addr.map(|addr| addr.ip()).filter(|ip| !is_loopback_ip(*ip));
                if let Some(ip) = tracked_ip {
                    let mut tracker = match backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if tracker.is_blocked(ip, now) {
                        record_auth_failure(
                            method_path,
                            "none",
                            &peer_display,
                            "Too many failed authentication attempts; backing off",
                            now,
                        );
                        return Err(Status::unauthenticated("Auth failed"));
                    }
                }

                let fail_peer = |attempted_user: &str, reason: &str| -> Status {
                    if let Some(ip) = tracked_ip {
                        let mut tracker = match backoff.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        tracker.record_failure(ip, now);
                    }
                    record_auth_failure(method_path, attempted_user, &peer_display, reason, now);
                    Status::unauthenticated("Auth failed")
                };

                let Some(raw_header) = headers.get(AUTHORIZATION) else {
                    return Err(fail_peer("none", "Missing authorization header"));
                };
                let Ok(header_str) = raw_header.to_str() else {
                    return Err(fail_peer("none", "Invalid authorization header encoding"));
                };
                let (header_username, header_password) = match BasicAuthCredentials::parse_header(header_str) {
                    Ok(pair) => pair,
                    Err(err) => {
                        return Err(fail_peer("none", &err.to_string()));
                    },
                };
                let attempted_user = sanitize_log_token(&header_username);
                if let Err(err) = credentials.constant_time_validate(&header_username, &header_password) {
                    return Err(fail_peer(&attempted_user, &err.to_string()));
                }

                if let Some(ip) = tracked_ip {
                    let mut tracker = match backoff.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    tracker.record_success(ip);
                }

                debug!(
                    target: LOG_TARGET,
                    "gRPC auth decision: method={} username={} peer={} outcome=allow",
                    method_path,
                    sanitize_log_token(username),
                    peer_display,
                );
                Ok(Some(AuthenticatedUser::new(username.clone())))
            },
        }
    }

    #[cfg(test)]
    fn tracked_peer_count(&self) -> usize {
        match self {
            Self::None => 0,
            Self::RejectAll { backoff } | Self::Basic { backoff, .. } => {
                backoff.lock().unwrap_or_else(|e| e.into_inner()).states.len()
            },
        }
    }
}

fn sanitize_log_token(value: &str) -> String {
    if value.is_empty() {
        return "none".to_string();
    }
    value
        .chars()
        .take(64)
        .map(|c| if c.is_ascii_graphic() && c != '=' { c } else { '_' })
        .collect()
}

fn peer_addr_from_extensions(extensions: &Extensions) -> Option<SocketAddr> {
    extensions
        .get::<TcpConnectInfo>()
        .and_then(|info| info.remote_addr())
        .or_else(|| {
            extensions
                .get::<TlsConnectInfo<TcpConnectInfo>>()
                .and_then(|info| info.get_ref().remote_addr())
        })
}

fn record_auth_failure(method_path: &str, username: &str, peer_display: &str, reason: &str, now: Instant) {
    AUTH_FAILURE_COUNT.fetch_add(1, Ordering::Relaxed);
    #[cfg(feature = "metrics")]
    AUTH_FAILURE_COUNTER.inc();
    let should_log = FAILURE_LOG_LIMITER
        .lock()
        .ok()
        .and_then(|mut limiter| limiter.check(now));
    if let Some(suppressed) = should_log {
        if suppressed > 0 {
            warn!(
                target: LOG_TARGET,
                "gRPC auth decision: method={} username={} peer={} outcome=deny reason={} (suppressed={} prior failures)",
                method_path,
                username,
                peer_display,
                reason,
                suppressed,
            );
        } else {
            warn!(
                target: LOG_TARGET,
                "gRPC auth decision: method={} username={} peer={} outcome=deny reason={}",
                method_path,
                username,
                peer_display,
                reason,
            );
        }
    }
}

/// Server-wide `tower::Layer` that enforces gRPC authentication across all registered services (including health
/// reporting), attaches [`AuthenticatedUser`] to request extensions on success, and emits an audit trail and metrics.
#[derive(Debug, Clone)]
pub struct ServerAuthenticationLayer {
    auth: PreparedServerAuth,
}

impl ServerAuthenticationLayer {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        Some(Self {
            auth: PreparedServerAuth::from_config(auth)?,
        })
    }
}

impl<S> Layer<S> for ServerAuthenticationLayer {
    type Service = ServerAuthenticationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ServerAuthenticationService {
            inner,
            auth: self.auth.clone(),
        }
    }
}

/// `tower::Service` produced by [`ServerAuthenticationLayer`].
#[derive(Debug, Clone)]
pub struct ServerAuthenticationService<S> {
    inner: S,
    auth: PreparedServerAuth,
}

impl<S: NamedService> NamedService for ServerAuthenticationService<S> {
    const NAME: &'static str = S::NAME;
}

impl<S, ReqBody, ResBody> Service<HttpRequest<ReqBody>> for ServerAuthenticationService<S>
where
    S: Service<HttpRequest<ReqBody>, Response = HttpResponse<ResBody>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Error = S::Error;
    type Future = BoxFuture<Self::Response, Self::Error>;
    type Response = HttpResponse<ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: HttpRequest<ReqBody>) -> Self::Future {
        let method_path = req.uri().path().to_string();
        let peer_addr = peer_addr_from_extensions(req.extensions());
        match self
            .auth
            .verify_headers_at(req.headers(), &method_path, peer_addr, Instant::now())
        {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user);
                Box::pin(self.inner.call(req))
            },
            Ok(None) => Box::pin(self.inner.call(req)),
            Err(status) => Box::pin(std::future::ready(Ok(status.into_http()))),
        }
    }
}

/// Compatibility `tonic::service::Interceptor` shim backed by the same verification logic as
/// [`ServerAuthenticationLayer`].
#[derive(Debug, Clone)]
pub struct ServerAuthenticationInterceptor {
    auth: PreparedServerAuth,
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        Some(Self {
            auth: PreparedServerAuth::from_config(auth)?,
        })
    }

    #[cfg(test)]
    #[allow(clippy::result_large_err)]
    fn handle_request_at(&self, mut request: Request<()>, now: Instant) -> Result<Request<()>, Status> {
        let method_path = request
            .extensions()
            .get::<tonic::GrpcMethod<'_>>()
            .map(|m| format!("/{}/{}", m.service(), m.method()))
            .unwrap_or_else(|| "unknown".to_string());
        let peer_addr = request.remote_addr();
        let headers = request.metadata().clone().into_headers();
        if let Some(user) = self.auth.verify_headers_at(&headers, &method_path, peer_addr, now)? {
            request.extensions_mut().insert(user);
        }
        Ok(request)
    }

    #[cfg(test)]
    fn tracked_peer_count(&self) -> usize {
        self.auth.tracked_peer_count()
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let method_path = request
            .extensions()
            .get::<tonic::GrpcMethod<'_>>()
            .map(|m| format!("/{}/{}", m.service(), m.method()))
            .unwrap_or_else(|| "unknown".to_string());
        let peer_addr = request.remote_addr();
        let headers = request.metadata().clone().into_headers();
        if let Some(user) = self
            .auth
            .verify_headers_at(&headers, &method_path, peer_addr, Instant::now())?
        {
            request.extensions_mut().insert(user);
        }
        Ok(request)
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::arithmetic_side_effects)]
    use std::{
        convert::Infallible,
        net::{IpAddr, Ipv4Addr},
        sync::{Mutex, OnceLock},
    };

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use tari_utilities::SafePassword;
    use tonic::metadata::MetadataValue;

    use super::*;
    use crate::authentication::basic_auth::MAX_ECHOED_SCHEME_LEN;

    struct TestLogSink;

    fn test_logs() -> &'static Mutex<Vec<(Level, String)>> {
        static LOGS: OnceLock<Mutex<Vec<(Level, String)>>> = OnceLock::new();
        LOGS.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn test_serial_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    impl Log for TestLogSink {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &Record<'_>) {
            if record.target() == LOG_TARGET &&
                let Ok(mut guard) = test_logs().lock()
            {
                guard.push((record.level(), format!("{}", record.args())));
            }
        }

        fn flush(&self) {}
    }

    fn init_logger() {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            drop(log::set_boxed_logger(Box::new(TestLogSink)));
            log::set_max_level(LevelFilter::Debug);
        });
    }

    #[derive(Clone)]
    struct EchoUserService;

    impl Service<HttpRequest<()>> for EchoUserService {
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        type Response = HttpResponse<()>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: HttpRequest<()>) -> Self::Future {
            let mut resp = HttpResponse::new(());
            if let Some(user) = req.extensions().get::<AuthenticatedUser>() {
                resp.extensions_mut().insert(user.clone());
            }
            Box::pin(std::future::ready(Ok(resp)))
        }
    }

    fn basic_header_value(username: &str, password: &str) -> String {
        let value: MetadataValue<_> = BasicAuthCredentials::generate_header(username, password.as_bytes()).unwrap();
        value.to_str().unwrap().to_string()
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

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn layer_inserts_authenticated_username_and_logs_decision() {
        let _serial_guard = test_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        let auth = GrpcAuthentication::Basic {
            username: "miner_1".to_string(),
            password: SafePassword::from("s3cret_pass"),
        };
        let layer = ServerAuthenticationLayer::new(auth).unwrap();
        let mut svc = layer.layer(EchoUserService);

        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54321);
        let mut req = HttpRequest::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .header(AUTHORIZATION, basic_header_value("miner_1", "s3cret_pass"))
            .body(())
            .unwrap();
        req.extensions_mut().insert(TcpConnectInfo {
            local_addr: None,
            remote_addr: Some(peer),
        });

        let resp = svc.call(req).await.unwrap();
        let user = resp
            .extensions()
            .get::<AuthenticatedUser>()
            .expect("authenticated username must be inserted into request extensions");
        assert_eq!(user.username(), "miner_1");

        let logs = test_logs().lock().unwrap();
        assert!(
            logs.iter().any(|(level, msg)| {
                *level == Level::Debug &&
                    msg.contains("method=/tari.rpc.BaseNode/GetVersion") &&
                    msg.contains("username=miner_1") &&
                    msg.contains("peer=127.0.0.1:54321") &&
                    msg.contains("outcome=allow")
            }),
            "expected debug audit log with method, username, peer, and outcome=allow, got: {logs:?}"
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn layer_rejects_unauthenticated_request_exports_metric_and_rate_limits_warn_logs() {
        let _serial_guard = test_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("correct_horse"),
        };
        let layer = ServerAuthenticationLayer::new(auth).unwrap();
        let mut svc = layer.layer(EchoUserService);

        if let Ok(mut logs) = test_logs().lock() {
            logs.clear();
        }
        if let Ok(mut limiter) = FAILURE_LOG_LIMITER.lock() {
            *limiter = FailureLogRateLimiter::new();
        }
        let before_count = grpc_authentication_failures_total();

        // Flood 50 unauthenticated requests against a protected RPC endpoint
        for _ in 0..50 {
            let req = HttpRequest::builder()
                .uri("/tari.rpc.BaseNode/GetVersion")
                .body(())
                .unwrap();
            let resp = svc.call(req).await.unwrap();
            assert_eq!(
                resp.headers().get("grpc-status").and_then(|v| v.to_str().ok()),
                Some("16"),
                "expected UNAUTHENTICATED (grpc-status 16)"
            );
            assert!(resp.extensions().get::<AuthenticatedUser>().is_none());
        }

        let after_count = grpc_authentication_failures_total();
        assert!(
            after_count >= before_count + 50,
            "expected failure counter to increase by at least 50 (before={before_count}, after={after_count})"
        );

        #[cfg(feature = "metrics")]
        {
            let gathered = tari_metrics::get_default_registry().gather();
            assert!(
                gathered.iter().any(|mf| mf.name() == AUTH_FAILURE_METRIC_NAME),
                "expected {AUTH_FAILURE_METRIC_NAME} in default metrics registry"
            );
        }

        let warn_logs: Vec<_> = test_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, msg)| *level == Level::Warn && msg.contains("method=/tari.rpc.BaseNode/GetVersion"))
            .cloned()
            .collect();
        assert!(
            !warn_logs.is_empty() && warn_logs.len() <= MAX_FAILURE_WARN_LOGS_PER_WINDOW,
            "expected between 1 and {MAX_FAILURE_WARN_LOGS_PER_WINDOW} warn logs under flood, got {}",
            warn_logs.len()
        );
    }

    #[tokio::test]
    async fn layer_exempts_grpc_health_check_and_watch_from_basic_auth() {
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("correct_horse"),
        };
        let layer = ServerAuthenticationLayer::new(auth).unwrap();
        let mut svc = layer.layer(EchoUserService);

        for path in ["/grpc.health.v1.Health/Check", "/grpc.health.v1.Health/Watch"] {
            let req = HttpRequest::builder().uri(path).body(()).unwrap();
            let resp = svc.call(req).await.unwrap();
            assert!(
                resp.headers().get("grpc-status").is_none(),
                "expected unauthenticated health probe {path} to be allowed"
            );
            assert!(resp.extensions().get::<AuthenticatedUser>().is_none());
        }
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

        // Within the backoff window, even a request carrying valid credentials is rejected without comparison.
        let valid_during_backoff = request_with_auth_and_addr("admin", b"secret", addr);
        let during_backoff = t0.checked_add(Duration::from_millis(25)).unwrap();
        let err = interceptor
            .handle_request_at(valid_during_backoff, during_backoff)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        // Once the backoff window expires, the valid request is accepted again.
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
        let _serial_guard = test_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = test_logs().lock() {
            logs.clear();
        }
        if let Ok(mut limiter) = FAILURE_LOG_LIMITER.lock() {
            *limiter = FailureLogRateLimiter::new();
        }

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

        let warn_logs: Vec<_> = test_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| *level == Level::Warn)
            .map(|(_, msg)| msg.clone())
            .collect();
        assert!(
            !warn_logs.is_empty() && warn_logs.len() <= MAX_FAILURE_WARN_LOGS_PER_WINDOW,
            "expected between 1 and {MAX_FAILURE_WARN_LOGS_PER_WINDOW} rate-limited log lines, got {}",
            warn_logs.len()
        );
        assert!(
            warn_logs.iter().all(|line| line.contains("203.0.113.42:55555")),
            "expected peer address in log lines, got: {warn_logs:?}"
        );
    }

    #[test]
    fn truncates_overlong_scheme_and_throttles_auth_failure_warnings() {
        let _serial_guard = test_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = test_logs().lock() {
            logs.clear();
        }
        if let Ok(mut limiter) = FAILURE_LOG_LIMITER.lock() {
            *limiter = FailureLogRateLimiter::new();
        }

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

        let warn_logs: Vec<_> = test_logs()
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| *level == Level::Warn)
            .map(|(_, msg)| msg.clone())
            .collect();
        assert!(
            !warn_logs.is_empty() && warn_logs.len() <= MAX_FAILURE_WARN_LOGS_PER_WINDOW,
            "expected between 1 and {MAX_FAILURE_WARN_LOGS_PER_WINDOW} log lines, got {}",
            warn_logs.len()
        );
        let echoed_expected = "A".repeat(MAX_ECHOED_SCHEME_LEN);
        let echoed_too_long = "A".repeat(MAX_ECHOED_SCHEME_LEN + 1);
        assert!(warn_logs.iter().all(|line| line.contains(&echoed_expected)));
        assert!(warn_logs.iter().all(|line| !line.contains(&echoed_too_long)));
    }

    #[test]
    fn username_containing_colon_keeps_starting_logs_error_and_rejects_requests() {
        let _serial_guard = test_serial_lock().lock().unwrap_or_else(|e| e.into_inner());
        init_logger();
        if let Ok(mut logs) = test_logs().lock() {
            logs.clear();
        }
        if let Ok(mut limiter) = FAILURE_LOG_LIMITER.lock() {
            *limiter = FailureLogRateLimiter::new();
        }

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "bad:user".to_string(),
            password: SafePassword::from("secret"),
        })
        .expect("server interceptor should still construct when username contains ':'");

        let startup_lines: Vec<_> = test_logs()
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

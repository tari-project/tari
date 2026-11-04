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
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use http::{Request, Response, header::AUTHORIZATION};
use log::{debug, warn};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{Status, body::Body, transport::server::TcpConnectInfo};
use tower_layer::Layer;
use tower_service::Service;

use super::{basic_auth::BasicAuthCredentials, salted_password::create_salted_hashed_password};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";

static AUTH_FAILURE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Returns the total number of gRPC authentication failures recorded by the server auth layer.
pub fn grpc_auth_failures() -> u64 {
    AUTH_FAILURE_COUNTER.load(Ordering::Relaxed)
}

/// Reset the failure counter to zero (useful for tests).
pub fn reset_grpc_auth_failures() {
    AUTH_FAILURE_COUNTER.store(0, Ordering::Relaxed);
}

/// Increment the total authentication failures counter.
pub fn record_grpc_auth_failure() {
    AUTH_FAILURE_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Extension containing the authenticated username
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser(pub String);

/// Rate limiter for auth failure logs under flood attacks
#[derive(Debug)]
struct LogRateLimiter {
    state: Mutex<(Instant, u32)>,
}

impl LogRateLimiter {
    fn new() -> Self {
        Self {
            state: Mutex::new((Instant::now(), 0)),
        }
    }

    fn should_log(&self) -> bool {
        if let Ok(mut guard) = self.state.lock() {
            let (last_time, count) = &mut *guard;
            let now = Instant::now();
            if now.duration_since(*last_time) >= Duration::from_secs(1) {
                *last_time = now;
                *count = 1;
                true
            } else if *count < 10 {
                *count += 1;
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

static LOG_LIMITER: OnceLock<LogRateLimiter> = OnceLock::new();

fn log_limiter() -> &'static LogRateLimiter {
    LOG_LIMITER.get_or_init(LogRateLimiter::new)
}

/// Server-wide tower Layer that enforces gRPC authentication across all services added to a tonic Server router.
#[derive(Debug, Clone)]
pub struct GrpcAuthLayer {
    auth: GrpcAuthentication,
}

impl GrpcAuthLayer {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        let processed_auth = match auth {
            GrpcAuthentication::None => auth,
            GrpcAuthentication::Basic { username, password } => GrpcAuthentication::Basic {
                username,
                password: create_salted_hashed_password(password.reveal()).ok()?.as_str().into(),
            },
        };

        Some(Self { auth: processed_auth })
    }

    pub fn auth(&self) -> &GrpcAuthentication {
        &self.auth
    }
}

impl<S> Layer<S> for GrpcAuthLayer {
    type Service = GrpcAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcAuthService {
            inner,
            auth: self.auth.clone(),
        }
    }
}

/// Tower Service that intercepts gRPC requests at the HTTP/router level before dispatching to any handler.
#[derive(Debug, Clone)]
pub struct GrpcAuthService<S> {
    inner: S,
    auth: GrpcAuthentication,
}

impl<S, ReqBody> Service<Request<ReqBody>> for GrpcAuthService<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<Body>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
        let path = req.uri().path().to_string();
        let peer_addr = req
            .extensions()
            .get::<TcpConnectInfo>()
            .and_then(|info| info.remote_addr());

        match &self.auth {
            GrpcAuthentication::None => {
                debug!(
                    target: LOG_TARGET,
                    "gRPC request authorized (no auth configured): method='{}', peer='{:?}'",
                    path,
                    peer_addr
                );
                let fut = self.inner.call(req);
                Box::pin(fut)
            },
            GrpcAuthentication::Basic {
                username: valid_username,
                password: valid_phc_password,
            } => {
                let auth_result = match req.headers().get(AUTHORIZATION.as_str()) {
                    Some(header_val) => match header_val.to_str() {
                        Ok(header_str) => match BasicAuthCredentials::parse_header(header_str) {
                            Ok((header_username, header_password)) => {
                                match BasicAuthCredentials::new(valid_username.clone(), valid_phc_password.clone()) {
                                    Ok(valid_creds) => {
                                        match valid_creds.constant_time_validate(&header_username, &header_password) {
                                            Ok(()) => Ok(header_username),
                                            Err(e) => Err(format!("Invalid credentials: {e}")),
                                        }
                                    },
                                    Err(e) => Err(format!("Internal credential setup error: {e}")),
                                }
                            },
                            Err(e) => Err(format!("Malformed authorization header: {e}")),
                        },
                        Err(_) => Err("Invalid ASCII in authorization header".to_string()),
                    },
                    None => Err("Missing authorization header".to_string()),
                };

                match auth_result {
                    Ok(user) => {
                        debug!(
                            target: LOG_TARGET,
                            "gRPC request authorized: method='{}', user='{}', peer='{:?}'",
                            path,
                            user,
                            peer_addr
                        );
                        req.extensions_mut().insert(AuthenticatedUser(user.clone()));
                        req.extensions_mut().insert(user);
                        let fut = self.inner.call(req);
                        Box::pin(fut)
                    },
                    Err(reason) => {
                        AUTH_FAILURE_COUNTER.fetch_add(1, Ordering::Relaxed);
                        if log_limiter().should_log() {
                            warn!(
                                target: LOG_TARGET,
                                "gRPC request authentication failed: method='{}', peer='{:?}', reason='{}'",
                                path,
                                peer_addr,
                                reason
                            );
                        }
                        let resp = Status::unauthenticated("Auth failed").into_http::<Body>();
                        Box::pin(async move { Ok(resp) })
                    },
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use http::{Request, Response, StatusCode};
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tonic::body::Body;
    use tower_layer::Layer;
    use tower_service::Service;

    use super::*;

    #[derive(Clone)]
    struct EchoService;

    impl Service<Request<()>> for EchoService {
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
        type Response = Response<Body>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request<()>) -> Self::Future {
            let mut builder = Response::builder().status(StatusCode::OK);
            if let Some(AuthenticatedUser(u)) = req.extensions().get::<AuthenticatedUser>() {
                builder = builder.header("x-authenticated-user", u.as_str());
            }
            let resp = builder.body(Body::empty()).unwrap();
            Box::pin(async move { Ok(resp) })
        }
    }

    #[tokio::test]
    async fn test_layer_none_passes() {
        let layer = GrpcAuthLayer::new(GrpcAuthentication::None).unwrap();
        let mut svc = layer.layer(EchoService);

        let req = Request::builder().uri("/grpc.health.v1.Health/Check").body(()).unwrap();

        let resp = svc.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_layer_basic_auth_rejects_missing_header() {
        reset_grpc_auth_failures();
        let layer = GrpcAuthLayer::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: "password123".into(),
        })
        .unwrap();
        let mut svc = layer.layer(EchoService);

        let req = Request::builder().uri("/grpc.health.v1.Health/Check").body(()).unwrap();

        let resp = svc.call(req).await.unwrap();
        // tonic status unauthenticated is returned as HTTP 200 with grpc-status 16
        assert_eq!(resp.headers().get("grpc-status").unwrap(), "16");
        assert_eq!(grpc_auth_failures(), 1);
    }

    #[tokio::test]
    async fn test_layer_basic_auth_accepts_valid_header_and_injects_user() {
        let layer = GrpcAuthLayer::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: "password123".into(),
        })
        .unwrap();
        let mut svc = layer.layer(EchoService);

        use base64::Engine;
        let creds = base64::engine::general_purpose::STANDARD.encode("admin:password123");
        let req = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .header("authorization", format!("Basic {creds}"))
            .body(())
            .unwrap();

        let resp = svc.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("grpc-status").is_none());
        assert_eq!(resp.headers().get("x-authenticated-user").unwrap(), "admin");
    }

    #[tokio::test]
    async fn test_layer_rate_limiting_under_flood() {
        let initial_failures = grpc_auth_failures();
        let layer = GrpcAuthLayer::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: "password123".into(),
        })
        .unwrap();
        let mut svc = layer.layer(EchoService);

        for _ in 0..50 {
            let req = Request::builder().uri("/grpc.health.v1.Health/Check").body(()).unwrap();
            let resp = svc.call(req).await.unwrap();
            assert_eq!(resp.headers().get("grpc-status").unwrap(), "16");
        }

        assert!(grpc_auth_failures() >= initial_failures + 50);
    }
}

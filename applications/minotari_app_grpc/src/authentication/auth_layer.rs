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
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use http::{HeaderMap, Request, Response, header::AUTHORIZATION};
use http_body::{Body as HttpBody, Frame, SizeHint};
use log::{Level, log_enabled};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{body::Body, server::NamedService, transport::server::TcpConnectInfo};
use tower_layer::Layer;
use tower_service::Service;

use super::server_auth::{
    AuthenticatedUser,
    HEALTH_PREFIX,
    LOG_TARGET,
    PresentedAuthorization,
    ServerAuthCore,
    log_rpc_audit_outcome,
};

/// Tower Layer enforcing gRPC authentication server-wide.
#[derive(Clone, Debug)]
pub struct GrpcAuthLayer {
    core: Arc<ServerAuthCore>,
    allow_anonymous_health: bool,
}

impl GrpcAuthLayer {
    /// Creates a server-wide gRPC authentication layer for a server that hosts `grpc.health.v1.Health`
    /// (such as the base node), where `health_requires_auth = false` allows anonymous health probes.
    pub fn new(auth: GrpcAuthentication, health_requires_auth: bool) -> Option<Self> {
        let core = ServerAuthCore::new(auth)?;
        Some(Self {
            core,
            allow_anonymous_health: !health_requires_auth,
        })
    }

    /// Creates a server-wide gRPC authentication layer for a server that does not host a health service
    /// (such as the console wallet), without any special-case exemption for `/grpc.health.v1.Health/`.
    pub fn new_without_health(auth: GrpcAuthentication) -> Option<Self> {
        let core = ServerAuthCore::new(auth)?;
        Some(Self {
            core,
            allow_anonymous_health: false,
        })
    }

    pub fn wrap<S>(&self, inner: S) -> GrpcAuthService<S> {
        GrpcAuthService {
            inner,
            core: self.core.clone(),
            allow_anonymous_health: self.allow_anonymous_health,
        }
    }

    pub fn failure_count(&self) -> u64 {
        self.core.failure_count()
    }
}

impl<S> Layer<S> for GrpcAuthLayer {
    type Service = GrpcAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcAuthService {
            inner,
            core: self.core.clone(),
            allow_anonymous_health: self.allow_anonymous_health,
        }
    }
}

struct AuditBody {
    inner: Body,
    user: Option<AuthenticatedUser>,
    peer_addr: Option<SocketAddr>,
    path: String,
    logged: bool,
}

impl AuditBody {
    fn log_once(&mut self, status: &str) {
        if !self.logged {
            self.logged = true;
            log_rpc_audit_outcome(self.user.as_ref(), self.peer_addr, &self.path, status, None);
        }
    }
}

impl HttpBody for AuditBody {
    type Data = <Body as HttpBody>::Data;
    type Error = <Body as HttpBody>::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(trailers) = frame.trailers_ref() {
                    let status = extract_grpc_status(trailers).unwrap_or("missing_status");
                    this.log_once(status);
                }
                Poll::Ready(Some(Ok(frame)))
            },
            Poll::Ready(Some(Err(err))) => {
                this.log_once("stream_error");
                Poll::Ready(Some(Err(err)))
            },
            Poll::Ready(None) => {
                this.log_once("missing_status");
                Poll::Ready(None)
            },
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for AuditBody {
    fn drop(&mut self) {
        if !self.logged {
            self.log_once("cancelled");
        }
    }
}

fn extract_grpc_status(headers: &HeaderMap) -> Option<&str> {
    headers.get("grpc-status").and_then(|v| v.to_str().ok())
}

fn wrap_audit_response(
    res: Response<Body>,
    user: Option<AuthenticatedUser>,
    peer_addr: Option<SocketAddr>,
    path: String,
) -> Response<Body> {
    if !log_enabled!(target: LOG_TARGET, Level::Debug) {
        return res;
    }
    let http_status = Some(res.status().as_u16());
    if let Some(status) = extract_grpc_status(res.headers()) {
        log_rpc_audit_outcome(user.as_ref(), peer_addr, &path, status, http_status);
        return res;
    }
    if res.body().is_end_stream() {
        let grpc_status = if res.status().is_success() {
            "0"
        } else {
            "missing_status"
        };
        log_rpc_audit_outcome(user.as_ref(), peer_addr, &path, grpc_status, http_status);
        return res;
    }
    let (parts, body) = res.into_parts();
    let audited = Body::new(AuditBody {
        inner: body,
        user,
        peer_addr,
        path,
        logged: false,
    });
    Response::from_parts(parts, audited)
}

/// Tower Service wrapper that verifies authentication headers.
#[derive(Clone, Debug)]
pub struct GrpcAuthService<S> {
    inner: S,
    core: Arc<ServerAuthCore>,
    allow_anonymous_health: bool,
}

impl<S: NamedService> NamedService for GrpcAuthService<S> {
    const NAME: &'static str = S::NAME;
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
        if !self.core.requires_auth() && !log_enabled!(target: LOG_TARGET, Level::Debug) {
            return Box::pin(self.inner.call(req));
        }

        // Health check exemption: keep /grpc.health.v1.Health/* reachable without credentials
        // when allow_anonymous_health is enabled.
        if self.allow_anonymous_health && req.uri().path().starts_with(HEALTH_PREFIX) {
            return Box::pin(self.inner.call(req));
        }

        let path = req.uri().path().to_string();

        let auth_header = match req.headers().get(AUTHORIZATION) {
            Some(val) => match val.to_str() {
                Ok(s) => PresentedAuthorization::Value(s),
                Err(_) => PresentedAuthorization::InvalidAscii,
            },
            None => PresentedAuthorization::Missing,
        };

        let peer_addr = req
            .extensions()
            .get::<TcpConnectInfo>()
            .and_then(|info| info.remote_addr());

        match self.core.verify(auth_header, peer_addr, Some(&path), Instant::now()) {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user.clone());
                let fut = self.inner.call(req);
                Box::pin(async move {
                    let res = fut.await?;
                    Ok(wrap_audit_response(res, Some(user), peer_addr, path))
                })
            },
            Ok(None) => {
                let fut = self.inner.call(req);
                Box::pin(async move {
                    let res = fut.await?;
                    Ok(wrap_audit_response(res, None, peer_addr, path))
                })
            },
            Err(status) => {
                let response = status.into_http::<Body>();
                Box::pin(async move { Ok(response) })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, net::SocketAddr, time::Duration};

    use http::{HeaderValue, StatusCode};
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tari_utilities::SafePassword;
    use tonic::body::Body;
    use tower_layer::Layer;
    use tower_service::Service;

    use super::*;
    use crate::authentication::{
        basic_auth::BasicAuthCredentials,
        server_auth::{AUTH_FAILURE_WARN_INTERVAL, AuthenticatedUser, MAX_FAILURES_BEFORE_BACKOFF},
    };

    #[derive(Clone)]
    struct DummyService;

    impl Service<Request<Body>> for DummyService {
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
        type Response = Response<Body>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request<Body>) -> Self::Future {
            let user = req.extensions().get::<AuthenticatedUser>().cloned();
            let mut res = Response::new(Body::empty());
            if let Some(user) = user {
                res.headers_mut()
                    .insert("x-authenticated-user", HeaderValue::from_str(user.as_str()).unwrap());
            }
            Box::pin(async move { Ok(res) })
        }
    }

    #[tokio::test]
    async fn health_check_unauthenticated_when_health_requires_auth_is_false() {
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("password"),
        };
        let layer = GrpcAuthLayer::new(auth, false).unwrap();
        let mut service = layer.layer(DummyService);

        let req = Request::builder()
            .uri("/grpc.health.v1.Health/Check")
            .body(Body::empty())
            .unwrap();

        let res = service.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(layer.failure_count(), 0);
    }

    #[tokio::test]
    async fn health_check_requires_auth_when_opted_in_without_triggering_ip_backoff_on_missing_header() {
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("password"),
        };
        let layer = GrpcAuthLayer::new(auth, true).unwrap();
        let mut service = layer.layer(DummyService);
        let remote_peer: SocketAddr = "198.51.100.25:50000".parse().unwrap();

        // Repeated unauthenticated health probes (no Authorization header) are rejected with UNAUTHENTICATED,
        // but must not lock out the IP.
        for _ in 0..MAX_FAILURES_BEFORE_BACKOFF.saturating_mul(2) {
            let mut req = Request::builder()
                .uri("/grpc.health.v1.Health/Check")
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(TcpConnectInfo {
                local_addr: None,
                remote_addr: Some(remote_peer),
            });

            let res = service.call(req).await.unwrap();
            let grpc_status = res.headers().get("grpc-status").and_then(|v| v.to_str().ok());
            assert_eq!(grpc_status, Some("16"));
        }
        assert_eq!(layer.core.tracked_peer_count(), 0);

        // Authenticated RPC from the same IP still succeeds immediately.
        let auth_header = BasicAuthCredentials::generate_header("admin", b"password").unwrap();
        let auth_val = auth_header.to_str().unwrap();
        let mut req = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .header(AUTHORIZATION, auth_val)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(TcpConnectInfo {
            local_addr: None,
            remote_addr: Some(remote_peer),
        });

        let res = service.call(req).await.unwrap();
        assert_eq!(res.headers().get("x-authenticated-user").unwrap(), "admin");

        // Wrong credentials presented on /grpc.health.v1.Health/Check from a non-loopback IP DO count toward backoff
        // and block subsequent requests from that IP, including health checks.
        let wrong_header = BasicAuthCredentials::generate_header("admin", b"wrong").unwrap();
        let wrong_val = wrong_header.to_str().unwrap();
        for _ in 0..MAX_FAILURES_BEFORE_BACKOFF {
            let mut req = Request::builder()
                .uri("/grpc.health.v1.Health/Check")
                .header(AUTHORIZATION, wrong_val)
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(TcpConnectInfo {
                local_addr: None,
                remote_addr: Some(remote_peer),
            });
            let res = service.call(req).await.unwrap();
            let grpc_status = res.headers().get("grpc-status").and_then(|v| v.to_str().ok());
            assert_eq!(grpc_status, Some("16"));
        }
        assert_eq!(layer.core.tracked_peer_count(), 1);

        // While blocked, even valid credentials on the health route are rejected.
        let mut blocked_health_req = Request::builder()
            .uri("/grpc.health.v1.Health/Check")
            .header(AUTHORIZATION, auth_val)
            .body(Body::empty())
            .unwrap();
        blocked_health_req.extensions_mut().insert(TcpConnectInfo {
            local_addr: None,
            remote_addr: Some(remote_peer),
        });
        let blocked_res = service.call(blocked_health_req).await.unwrap();
        let blocked_status = blocked_res.headers().get("grpc-status").and_then(|v| v.to_str().ok());
        assert_eq!(blocked_status, Some("16"));
    }

    #[tokio::test]
    async fn non_health_requests_require_authentication() {
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("password"),
        };
        let layer = GrpcAuthLayer::new(auth, false).unwrap();
        let mut service = layer.layer(DummyService);

        let req = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .body(Body::empty())
            .unwrap();

        let res = service.call(req).await.unwrap();
        let grpc_status = res.headers().get("grpc-status").and_then(|v| v.to_str().ok());
        assert_eq!(grpc_status, Some("16"));
        assert_eq!(layer.failure_count(), 1);

        let auth_header = BasicAuthCredentials::generate_header("admin", b"password").unwrap();
        let auth_val = auth_header.to_str().unwrap();
        let req = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .header(AUTHORIZATION, auth_val)
            .body(Body::empty())
            .unwrap();

        let res = service.call(req).await.unwrap();
        assert_eq!(res.headers().get("x-authenticated-user").unwrap(), "admin");
    }

    #[tokio::test]
    async fn parallel_layers_have_independent_failure_counters() {
        let auth1 = GrpcAuthentication::Basic {
            username: "user1".to_string(),
            password: SafePassword::from("password1"),
        };
        let auth2 = GrpcAuthentication::Basic {
            username: "user2".to_string(),
            password: SafePassword::from("password2"),
        };
        let layer1 = GrpcAuthLayer::new(auth1, false).unwrap();
        let layer2 = GrpcAuthLayer::new(auth2, false).unwrap();

        let mut service1 = layer1.layer(DummyService);
        let mut service2 = layer2.layer(DummyService);

        let req1 = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .body(Body::empty())
            .unwrap();
        drop(service1.call(req1).await.unwrap());

        assert_eq!(layer1.failure_count(), 1);
        assert_eq!(layer2.failure_count(), 0);

        let req2 = Request::builder()
            .uri("/tari.rpc.BaseNode/GetVersion")
            .body(Body::empty())
            .unwrap();
        drop(service2.call(req2).await.unwrap());

        assert_eq!(layer1.failure_count(), 1);
        assert_eq!(layer2.failure_count(), 1);
    }

    #[test]
    fn warn_limiter_rate_limits_and_resets() {
        use crate::authentication::server_auth::WarnLimiter;

        let limiter = WarnLimiter::default();
        let now = Instant::now();

        assert_eq!(limiter.should_warn(now), Some(0));

        for _ in 0..10 {
            assert_eq!(limiter.should_warn(now), None);
        }

        let later = now
            .checked_add(AUTH_FAILURE_WARN_INTERVAL)
            .and_then(|t| t.checked_add(Duration::from_secs(1)))
            .unwrap();
        assert_eq!(limiter.should_warn(later), Some(10));
        assert_eq!(limiter.should_warn(later), None);
    }
}

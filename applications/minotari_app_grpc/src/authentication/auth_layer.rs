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
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use http::{Request, Response, header::AUTHORIZATION};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tonic::{body::Body, transport::server::TcpConnectInfo};
use tower_layer::Layer;
use tower_service::Service;

use super::server_auth::{PresentedAuthorization, ServerAuthCore};

pub const HEALTH_PREFIX: &str = "/grpc.health.v1.Health/";

/// Tower Layer enforcing gRPC authentication server-wide.
#[derive(Clone, Debug)]
pub struct GrpcAuthLayer {
    core: Arc<ServerAuthCore>,
    health_requires_auth: bool,
}

impl GrpcAuthLayer {
    pub fn new(auth: GrpcAuthentication, health_requires_auth: bool) -> Option<Self> {
        let core = ServerAuthCore::new(auth)?;
        Some(Self {
            core,
            health_requires_auth,
        })
    }

    pub fn failure_count(&self) -> u64 {
        self.core.failure_count()
    }

    pub fn core(&self) -> &Arc<ServerAuthCore> {
        &self.core
    }
}

impl<S> Layer<S> for GrpcAuthLayer {
    type Service = GrpcAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcAuthService {
            inner,
            core: self.core.clone(),
            health_requires_auth: self.health_requires_auth,
        }
    }
}

/// Tower Service wrapper that verifies authentication headers.
#[derive(Clone, Debug)]
pub struct GrpcAuthService<S> {
    inner: S,
    core: Arc<ServerAuthCore>,
    health_requires_auth: bool,
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
        let path = req.uri().path();

        // Health check exemption: keep /grpc.health.v1.Health/* reachable without credentials
        // unless health_requires_auth is explicitly enabled. Probes are exempted from failure counting.
        if !self.health_requires_auth && path.starts_with(HEALTH_PREFIX) {
            return Box::pin(self.inner.call(req));
        }

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

        match self.core.verify(auth_header, peer_addr, Some(path), Instant::now()) {
            Ok(Some(user)) => {
                req.extensions_mut().insert(user);
                Box::pin(self.inner.call(req))
            },
            Ok(None) => Box::pin(self.inner.call(req)),
            Err(status) => {
                let response = status.into_http::<Body>();
                Box::pin(async move { Ok(response) })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, time::Duration};

    use http::{HeaderValue, StatusCode};
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tari_utilities::SafePassword;
    use tonic::body::Body;
    use tower_layer::Layer;
    use tower_service::Service;

    use super::*;
    use crate::authentication::{
        basic_auth::BasicAuthCredentials,
        server_auth::{AUTH_FAILURE_WARN_INTERVAL, AuthenticatedUser},
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
                    .insert("x-authenticated-user", HeaderValue::from_str(&user.0).unwrap());
            }
            Box::pin(async move { Ok(res) })
        }
    }

    #[tokio::test]
    async fn health_check_unauthenticated_by_default() {
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
    async fn health_check_requires_auth_when_opted_in() {
        let auth = GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("password"),
        };
        let layer = GrpcAuthLayer::new(auth, true).unwrap();
        let mut service = layer.layer(DummyService);

        // Without header: rejected
        let req = Request::builder()
            .uri("/grpc.health.v1.Health/Check")
            .body(Body::empty())
            .unwrap();

        let res = service.call(req).await.unwrap();
        let grpc_status = res.headers().get("grpc-status").and_then(|v| v.to_str().ok());
        assert_eq!(grpc_status, Some("16")); // UNAUTHENTICATED
        assert_eq!(layer.failure_count(), 1);

        // With valid header: accepted
        let auth_header = BasicAuthCredentials::generate_header("admin", b"password").unwrap();
        let auth_val = auth_header.to_str().unwrap();
        let req = Request::builder()
            .uri("/grpc.health.v1.Health/Check")
            .header(AUTHORIZATION, auth_val)
            .body(Body::empty())
            .unwrap();

        let res = service.call(req).await.unwrap();
        assert_eq!(res.headers().get("x-authenticated-user").unwrap(), "admin");
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

        // Valid credentials succeed and inject AuthenticatedUser
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

        // First call should warn (0 suppressed)
        assert_eq!(limiter.should_warn(now), Some(0));

        // Rapid successive calls in the same window should be suppressed (return None)
        for _ in 0..10 {
            assert_eq!(limiter.should_warn(now), None);
        }

        // After window expires, should warn again and return the number of suppressed logs (10)
        let later = now + AUTH_FAILURE_WARN_INTERVAL + Duration::from_secs(1);
        assert_eq!(limiter.should_warn(later), Some(10));

        // Immediately after, suppressed again
        assert_eq!(limiter.should_warn(later), None);
    }
}

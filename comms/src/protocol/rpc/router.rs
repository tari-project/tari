//  Copyright 2020, The Tari Project
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

use super::{
    body::Body,
    either::Either,
    message::{Request, Response},
    not_found::ProtocolServiceNotFound,
    server::NamedProtocolService,
    RpcError,
    RpcServer,
    RpcStatus,
};
use crate::{
    protocol::{
        rpc::context::{RpcCommsBackend, RpcCommsProvider},
        ProtocolExtension,
        ProtocolExtensionContext,
        ProtocolExtensionError,
        ProtocolId,
        ProtocolNotificationRx,
    },
    runtime::task,
    Bytes,
};
use futures::{
    channel::mpsc,
    task::{Context, Poll},
    AsyncRead,
    AsyncWrite,
    Future,
    FutureExt,
};
use std::sync::Arc;
use tower::Service;
use tower_make::MakeService;

/// Allows service factories of different types to be composed into a single service that resolves a given `ProtocolId`
pub struct Router<A, B> {
    server: RpcServer,
    protocol_names: Vec<ProtocolId>,
    routes: Or<A, B>,
}

impl<A> Router<A, ProtocolServiceNotFound>
where A: NamedProtocolService
{
    /// Create a new Router
    pub fn new(server: RpcServer, service: A) -> Self {
        let expected_protocol = ProtocolId::from_static(<A as NamedProtocolService>::PROTOCOL_NAME);
        let protocols = vec![expected_protocol.clone()];
        let predicate = move |protocol: &ProtocolId| expected_protocol == protocol;
        Self {
            protocol_names: protocols,
            server,
            routes: Or::new(predicate, service, ProtocolServiceNotFound),
        }
    }
}

impl<A, B> Router<A, B> {
    /// Consume this router and return a new router composed of the given service and any previously added services
    pub fn add_service<T>(mut self, service: T) -> Router<T, Or<A, B>>
    where T: NamedProtocolService {
        let expected_protocol = ProtocolId::from_static(<T as NamedProtocolService>::PROTOCOL_NAME);
        self.protocol_names.push(expected_protocol.clone());
        let predicate = move |protocol: &ProtocolId| expected_protocol == protocol;
        Router {
            protocol_names: self.protocol_names,
            server: self.server,
            routes: Or::new(predicate, service, self.routes),
        }
    }

    /// Sets the maximum number of sessions this node will allow before rejecting the request to connect
    pub fn with_maximum_concurrent_sessions(mut self, limit: usize) -> Self {
        self.server = self.server.with_maximum_concurrent_sessions(limit);
        self
    }

    /// Allows unlimited (memory-bound) sessions. This should probably only be used for scalability testing.
    pub fn with_unlimited_concurrent_sessions(mut self) -> Self {
        self.server = self.server.with_unlimited_concurrent_sessions();
        self
    }

    pub fn into_boxed(self) -> Box<Self>
    where Self: 'static {
        Box::new(self)
    }

    pub(crate) fn all_protocols(&mut self) -> &[ProtocolId] {
        &self.protocol_names
    }
}

impl<A, B> Router<A, B>
where
    A: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>
        + Send
        + 'static,
    A::Service: Send + 'static,
    A::Future: Send + 'static,
    <A::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    B: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>
        + Send
        + 'static,
    B::Service: Send + 'static,
    B::Future: Send + 'static,
    <B::Service as Service<Request<Bytes>>>::Future: Send + 'static,
{
    /// Start all services
    pub(crate) async fn serve<TSubstream, TCommsProvider>(
        self,
        protocol_notifications: ProtocolNotificationRx<TSubstream>,
        comms_provider: TCommsProvider,
    ) -> Result<(), RpcError>
    where
        TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        TCommsProvider: RpcCommsProvider + Clone + Send + 'static,
    {
        self.server
            .serve(self.routes, protocol_notifications, comms_provider)
            .await
    }
}

impl<A, B> Service<ProtocolId> for Router<A, B>
where
    A: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>,
    B: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>,
{
    type Error = A::MakeError;
    type Response = Either<A::Service, B::Service>;

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Service::poll_ready(&mut self.routes, cx)
    }

    fn call(&mut self, protocol: ProtocolId) -> Self::Future {
        Service::call(&mut self.routes, protocol)
    }
}

impl<A, B> ProtocolExtension for Router<A, B>
where
    A: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>
        + Send
        + Sync
        + 'static,
    A::Service: Send + 'static,
    A::Future: Send + 'static,
    <A::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    B: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>
        + Send
        + Sync
        + 'static,
    B::Service: Send + 'static,
    B::Future: Send + 'static,
    <B::Service as Service<Request<Bytes>>>::Future: Send + 'static,
{
    fn install(self: Box<Self>, context: &mut ProtocolExtensionContext) -> Result<(), ProtocolExtensionError> {
        let (proto_notif_tx, proto_notif_rx) = mpsc::channel(10);
        context.add_protocol(&self.protocol_names, proto_notif_tx);
        let rpc_context = RpcCommsBackend::new(context.peer_manager(), context.connectivity());
        task::spawn(self.serve(proto_notif_rx, rpc_context));
        Ok(())
    }
}

pub struct Or<A, B> {
    predicate: Arc<dyn Fn(&ProtocolId) -> bool + Send + Sync + 'static>,
    a: A,
    b: B,
}

impl<A, B> Or<A, B> {
    pub fn new<P>(predicate: P, a: A, b: B) -> Self
    where P: Fn(&ProtocolId) -> bool + Send + Sync + 'static {
        Self {
            predicate: Arc::new(predicate),
            a,
            b,
        }
    }
}

impl<A, B> Service<ProtocolId> for Or<A, B>
where
    A: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>,
    B: MakeService<ProtocolId, Request<Bytes>, Response = Response<Body>, Error = RpcStatus, MakeError = RpcError>,
{
    type Error = A::MakeError;
    type Response = Either<A::Service, B::Service>;

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, protocol: ProtocolId) -> Self::Future {
        if (self.predicate)(&protocol) {
            Either::A(self.a.make_service(protocol).map(|r| r.map(Either::A)))
        } else {
            Either::B(self.b.make_service(protocol).map(|r| r.map(Either::B)))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::runtime;
    use futures::{future, StreamExt};
    use prost::Message;
    use tari_test_utils::unpack_enum;

    #[derive(Clone)]
    struct HelloService;
    impl NamedProtocolService for HelloService {
        const PROTOCOL_NAME: &'static [u8] = b"hello";
    }
    impl Service<ProtocolId> for HelloService {
        type Error = RpcError;

        type Future = impl Future<Output = Result<Self::Response, Self::Error>>;
        type Response = impl Service<Request<Bytes>, Response = Response<Body>, Error = RpcStatus>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: ProtocolId) -> Self::Future {
            let my_service = tower::service_fn(|req: Request<Bytes>| {
                let msg = req.into_message();
                let str = String::from_utf8_lossy(&msg);
                future::ready(Ok(Response::from_message(format!("Hello {}", str))))
            });

            future::ready(Ok(my_service))
        }
    }

    #[derive(Clone)]
    struct GoodbyeService;
    impl NamedProtocolService for GoodbyeService {
        const PROTOCOL_NAME: &'static [u8] = b"goodbye";
    }
    impl Service<ProtocolId> for GoodbyeService {
        type Error = RpcError;

        type Future = impl Future<Output = Result<Self::Response, Self::Error>>;
        type Response = impl Service<Request<Bytes>, Response = Response<Body>, Error = RpcStatus>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: ProtocolId) -> Self::Future {
            let my_service = tower::service_fn(|req: Request<Bytes>| {
                let msg = req.into_message();
                let str = String::from_utf8_lossy(&msg);
                future::ready(Ok(Response::from_message(format!("Goodbye {}", str))))
            });

            future::ready(Ok(my_service))
        }
    }

    #[runtime::test_basic]
    async fn find_route() {
        let server = RpcServer::new();
        let mut router = Router::new(server, HelloService).add_service(GoodbyeService);
        assert_eq!(router.all_protocols(), &[
            HelloService::PROTOCOL_NAME,
            GoodbyeService::PROTOCOL_NAME
        ]);

        let mut hello_svc = router.call(HelloService::PROTOCOL_NAME.into()).await.unwrap();
        let req = Request::new(1.into(), b"Kerbal".to_vec().into());

        let resp = hello_svc.call(req).await.unwrap();
        let resp = resp.into_message().next().await.unwrap().unwrap().into_bytes_mut();
        let s = String::decode(resp).unwrap();
        assert_eq!(s, "Hello Kerbal");

        let mut bye_svc = router.call(GoodbyeService::PROTOCOL_NAME.into()).await.unwrap();
        let req = Request::new(1.into(), b"Xel'naga".to_vec().into());
        let resp = bye_svc.call(req).await.unwrap();
        let resp = resp.into_message().next().await.unwrap().unwrap().into_bytes_mut();
        let s = String::decode(resp).unwrap();
        assert_eq!(s, "Goodbye Xel'naga");

        let result = router.call(ProtocolId::from_static(b"/totally/real/protocol")).await;
        let err = match result {
            Ok(_) => panic!("Unexpected success for non-existent route"),
            Err(err) => err,
        };
        unpack_enum!(RpcError::ProtocolServiceNotFound(proto_str) = err);
        assert_eq!(proto_str, "/totally/real/protocol");
    }
}

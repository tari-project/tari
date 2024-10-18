//  Copyright 2021, The Tari Project
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

use std::sync::Arc;

use bytes::Bytes;
use futures::{
    future::BoxFuture,
    task::{Context, Poll},
    FutureExt,
};
use libp2p::StreamProtocol;
use tower::{make::MakeService, Service};

use super::RpcServerError;
use crate::{
    body::Body,
    either::Either,
    message::{Request, Response},
    not_found::ProtocolServiceNotFound,
    notify::ProtocolNotificationRx,
    server::{NamedProtocolService, RpcServerHandle},
    RpcError,
    RpcServer,
    RpcStatus,
    Substream,
};

/// Allows service factories of different types to be composed into a single service that resolves a given
/// `StreamProtocol`
pub struct Router<A, B> {
    server: RpcServer,
    protocol_names: Vec<StreamProtocol>,
    routes: Or<A, B>,
}

impl<A> Router<A, ProtocolServiceNotFound>
where A: NamedProtocolService
{
    /// Create a new Router
    pub fn new(server: RpcServer, service: A) -> Self {
        let expected_protocol = StreamProtocol::new(<A as NamedProtocolService>::PROTOCOL_NAME);
        let protocols = vec![expected_protocol.clone()];
        let predicate = move |protocol: &StreamProtocol| expected_protocol == *protocol;
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
        let expected_protocol = StreamProtocol::new(<T as NamedProtocolService>::PROTOCOL_NAME);
        self.protocol_names.push(expected_protocol.clone());
        let predicate = move |protocol: &StreamProtocol| expected_protocol == *protocol;
        Router {
            protocol_names: self.protocol_names,
            server: self.server,
            routes: Or::new(predicate, service, self.routes),
        }
    }

    pub fn get_handle(&self) -> RpcServerHandle {
        self.server.get_handle()
    }

    pub fn into_boxed(self) -> Box<Self>
    where Self: 'static {
        Box::new(self)
    }

    pub fn all_protocols(&self) -> &[StreamProtocol] {
        &self.protocol_names
    }
}

impl<A, B> Router<A, B>
where
    A: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send
        + 'static,
    A::Service: Send + 'static,
    A::Future: Send + 'static,
    <A::Service as Service<Request<Bytes>>>::Future: Send + 'static,
    B: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send
        + 'static,
    B::Service: Send + 'static,
    B::Future: Send + 'static,
    <B::Service as Service<Request<Bytes>>>::Future: Send + 'static,
{
    /// Start all services
    pub async fn serve(self, protocol_notifications: ProtocolNotificationRx<Substream>) -> Result<(), RpcError> {
        self.server
            .serve(self.routes, protocol_notifications)
            .await
            .map_err(Into::into)
    }
}

impl<A, B> Service<StreamProtocol> for Router<A, B>
where
    A: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send,
    B: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send,
    A::Future: Send + 'static,
    B::Future: Send + 'static,
{
    type Error = <Or<A, B> as Service<StreamProtocol>>::Error;
    type Future = <Or<A, B> as Service<StreamProtocol>>::Future;
    type Response = <Or<A, B> as Service<StreamProtocol>>::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Service::poll_ready(&mut self.routes, cx)
    }

    fn call(&mut self, protocol: StreamProtocol) -> Self::Future {
        Service::call(&mut self.routes, protocol)
    }
}

pub struct Or<A, B> {
    predicate: Arc<dyn Fn(&StreamProtocol) -> bool + Send + Sync + 'static>,
    a: A,
    b: B,
}

impl<A, B> Or<A, B> {
    pub fn new<P>(predicate: P, a: A, b: B) -> Self
    where P: Fn(&StreamProtocol) -> bool + Send + Sync + 'static {
        Self {
            predicate: Arc::new(predicate),
            a,
            b,
        }
    }
}

impl<A, B> Service<StreamProtocol> for Or<A, B>
where
    A: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send,
    B: MakeService<
            StreamProtocol,
            Request<Bytes>,
            Response = Response<Body>,
            Error = RpcStatus,
            MakeError = RpcServerError,
        > + Send,
    A::Future: Send + 'static,
    B::Future: Send + 'static,
{
    type Error = A::MakeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = Either<A::Service, B::Service>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, protocol: StreamProtocol) -> Self::Future {
        let fut = if (self.predicate)(&protocol) {
            Either::A(self.a.make_service(protocol).map(|r| r.map(Either::A)))
        } else {
            Either::B(self.b.make_service(protocol).map(|r| r.map(Either::B)))
        };

        Box::pin(fut)
    }
}

// TODO: Tests
// #[cfg(test)]
// mod test {
//     use futures::{future, StreamExt};
//     use prost::Message;
//     use tari_test_utils::unpack_enum;
//     use tower::util::BoxService;
//
//     use super::*;
//
//     #[derive(Clone)]
//     struct HelloService;
//     impl NamedProtocolService for HelloService {
//         const PROTOCOL_NAME: &'static [u8] = b"hello";
//     }
//     impl Service<StreamProtocol> for HelloService {
//         type Error = RpcServerError;
//         type Future = future::Ready<Result<Self::Response, Self::Error>>;
//         type Response = BoxService<Request<Bytes>, Response<Body>, RpcStatus>;
//
//         fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//             Poll::Ready(Ok(()))
//         }
//
//         fn call(&mut self, _: StreamProtocol) -> Self::Future {
//             let my_service = tower::service_fn(|req: Request<Bytes>| {
//                 let msg = req.into_message();
//                 let str = String::from_utf8_lossy(&msg);
//                 future::ready(Ok(Response::from_message(format!("Hello {}", str))))
//             });
//
//             future::ready(Ok(BoxService::new(my_service)))
//         }
//     }
//
//     #[derive(Clone)]
//     struct GoodbyeService;
//     impl NamedProtocolService for GoodbyeService {
//         const PROTOCOL_NAME: &'static [u8] = b"goodbye";
//     }
//     impl Service<StreamProtocol> for GoodbyeService {
//         type Error = RpcServerError;
//         type Future = future::Ready<Result<Self::Response, Self::Error>>;
//         type Response = BoxService<Request<Bytes>, Response<Body>, RpcStatus>;
//
//         fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//             Poll::Ready(Ok(()))
//         }
//
//         fn call(&mut self, _: StreamProtocol) -> Self::Future {
//             let my_service = tower::service_fn(|req: Request<Bytes>| {
//                 let msg = req.into_message();
//                 let str = String::from_utf8_lossy(&msg);
//                 future::ready(Ok(Response::from_message(format!("Goodbye {}", str))))
//             });
//
//             future::ready(Ok(BoxService::new(my_service)))
//         }
//     }
//
//     #[tokio::test]
//     async fn find_route() {
//         let server = RpcServer::new();
//         let mut router = Router::new(server, HelloService).add_service(GoodbyeService);
//         assert_eq!(router.all_protocols(), &[
//             HelloService::PROTOCOL_NAME,
//             GoodbyeService::PROTOCOL_NAME
//         ]);
//
//         let mut hello_svc = router.call(HelloService::PROTOCOL_NAME.into()).await.unwrap();
//         let req = Request::new(1.into(), b"Kerbal".to_vec().into());
//
//         let resp = hello_svc.call(req).await.unwrap();
//         let resp = resp.into_message().next().await.unwrap().unwrap().into_bytes_mut();
//         let s = String::decode(resp).unwrap();
//         assert_eq!(s, "Hello Kerbal");
//
//         let mut bye_svc = router.call(GoodbyeService::PROTOCOL_NAME.into()).await.unwrap();
//         let req = Request::new(1.into(), b"Xel'naga".to_vec().into());
//         let resp = bye_svc.call(req).await.unwrap();
//         let resp = resp.into_message().next().await.unwrap().unwrap().into_bytes_mut();
//         let s = String::decode(resp).unwrap();
//         assert_eq!(s, "Goodbye Xel'naga");
//
//         let result = router
//             .call(StreamProtocol::from_static(b"/totally/real/protocol"))
//             .await;
//         let err = match result {
//             Ok(_) => panic!("Unexpected success for non-existent route"),
//             Err(err) => err,
//         };
//         unpack_enum!(RpcServerError::ProtocolServiceNotFound(proto_str) = err);
//         assert_eq!(proto_str, "/totally/real/protocol");
//     }
// }

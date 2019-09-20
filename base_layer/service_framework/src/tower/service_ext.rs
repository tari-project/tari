// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use futures::{ready, task::Context, Future, FutureExt, Poll};
use std::pin::Pin;
use tower_service::Service;

impl<T: ?Sized, TRequest> ServiceExt<TRequest> for T where T: Service<TRequest> {}

pub trait ServiceExt<TRequest>: Service<TRequest> {
    /// The service combinator combines calling `poll_ready` and `call` into a single call.
    /// It returns a [ServiceCallReady](./struct.ServiceCallReady.html) future that
    /// calls `poll_ready` on the given service, once the service is ready to
    /// receive a request, `call` is called and the resulting future is polled.
    fn call_ready(&mut self, req: TRequest) -> ServiceCallReady<'_, Self, TRequest>
    where Self::Future: Unpin {
        ServiceCallReady::new(self, req)
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ServiceCallReady<'a, S, TRequest>
where S: Service<TRequest> + ?Sized
{
    service: &'a mut S,
    request: Option<TRequest>,
    pending: Option<S::Future>,
}

impl<S: ?Sized + Service<TRequest> + Unpin, TRequest> Unpin for ServiceCallReady<'_, S, TRequest> {}

impl<'a, S, TRequest> ServiceCallReady<'a, S, TRequest>
where
    S: Service<TRequest> + ?Sized,
    S::Future: Unpin,
{
    fn new(service: &'a mut S, request: TRequest) -> Self {
        Self {
            service,
            request: Some(request),
            pending: None,
        }
    }
}

impl<S, TRequest> Future for ServiceCallReady<'_, S, TRequest>
where
    S: Service<TRequest> + ?Sized + Unpin,
    S::Future: Unpin,
{
    type Output = Result<S::Response, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        loop {
            match this.pending {
                Some(ref mut fut) => return fut.poll_unpin(cx),
                None => {
                    // Poll the service to check if it's ready. If so, make the call
                    ready!(this.service.poll_ready(cx))?;
                    let req = this.request.take().expect("the request cannot be made twice");
                    this.pending = Some(this.service.call(req));
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::tower::service_fn;
    use futures::{future, FutureExt};
    use futures_test::task::panic_context;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn service_ready() {
        let mut double_service = service_fn(|req: u32| future::ok::<_, ()>(req + req));

        let mut cx = panic_context();

        match ServiceCallReady::new(&mut double_service, 157).poll_unpin(&mut cx) {
            Poll::Ready(Ok(v)) => assert_eq!(v, 314),
            _ => panic!("Expected future to be ready"),
        }
    }

    #[test]
    fn service_ready_later() {
        struct ReadyLater {
            call_count: u32,
            flag: Arc<AtomicBool>,
        }

        impl Service<u32> for ReadyLater {
            type Error = ();
            type Response = u32;

            type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                if self.flag.load(Ordering::Acquire) {
                    Ok(()).into()
                } else {
                    Poll::Pending
                }
            }

            fn call(&mut self, req: u32) -> Self::Future {
                self.call_count += 1;
                future::ok(req + req)
            }
        }

        let mut cx = panic_context();
        let ready_flag = Arc::new(AtomicBool::new(false));
        let mut service = ReadyLater {
            flag: ready_flag.clone(),
            call_count: 0,
        };

        let mut fut = ServiceCallReady::new(&mut service, 157);

        match fut.poll_unpin(&mut cx) {
            Poll::Pending => {},
            _ => panic!("Expected future to be pending"),
        }

        ready_flag.store(true, Ordering::Release);

        match fut.poll_unpin(&mut cx) {
            Poll::Ready(Ok(v)) => assert_eq!(v, 314),
            _ => panic!("Expected future to be ready"),
        }

        assert_eq!(service.call_count, 1);
    }
}

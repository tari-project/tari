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

use futures::{ready, task::Context, Future, FutureExt};
use std::task::Poll;
use tower_service::Service;

/// LazyService state
enum State<S> {
    Pending,
    Ready(S),
}

/// LazyService
///
/// Implements the `tower_service::Service` trait. The `poll_ready` function will poll
/// the given future. Once that future is ready, the resulting value is passed into the
/// given `service_fn` function which must return a service. Subsequent calls to
/// `poll_ready` and `call` are delegated to that service.
///
/// This is instantiated by the `lazy_service` combinator in `ServiceHandlesFuture`.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LazyService<TFn, F, S> {
    future: F,
    service_fn: Option<TFn>,
    state: State<S>,
}

impl<TFn, F, S> LazyService<TFn, F, S> {
    /// Create a new LazyService
    pub fn new(future: F, service_fn: TFn) -> Self {
        Self {
            future,
            service_fn: Some(service_fn),
            state: State::Pending,
        }
    }
}

impl<TFn, F, S, TReq> Service<TReq> for LazyService<TFn, F, S>
where
    F: Future + Unpin,
    TFn: FnOnce(F::Output) -> S,
    S: Service<TReq>,
{
    type Error = S::Error;
    type Future = S::Future;
    type Response = S::Response;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            match self.state {
                State::Pending => {
                    let item = ready!(self.future.poll_unpin(cx));
                    let service_fn = self
                        .service_fn
                        .take()
                        .expect("service_fn cannot be None in Pending state");
                    self.state = State::Ready((service_fn)(item));
                },
                State::Ready(ref mut service) => {
                    return service.poll_ready(cx);
                },
            }
        }
    }

    fn call(&mut self, req: TReq) -> Self::Future {
        match self.state {
            State::Pending => panic!("`Service::call` called before `Service::poll_ready` was ready"),
            State::Ready(ref mut service) => service.call(req),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::tower::service_fn;
    use futures::future::{self, poll_fn};
    use futures_test::task::panic_context;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        task::Poll,
    };

    fn mock_fut(flag: Arc<AtomicBool>) -> impl Future<Output = ()> {
        poll_fn::<_, _>(move |_: &mut Context<'_>| {
            if flag.load(Ordering::SeqCst) {
                ().into()
            } else {
                Poll::Pending
            }
        })
    }

    #[test]
    fn ready_after_handles() {
        let flag = Arc::new(AtomicBool::new(false));
        let fut = mock_fut(flag.clone());

        let mut cx = panic_context();

        let mut service = LazyService::new(fut, |_: ()| service_fn(|num: u8| future::ok::<_, ()>(num)));

        assert!(service.poll_ready(&mut cx).is_pending());

        flag.store(true, Ordering::SeqCst);

        match service.poll_ready(&mut cx) {
            Poll::Ready(Ok(_)) => {},
            _ => panic!("Unexpected poll result"),
        }
    }

    #[test]
    fn call_after_ready() {
        let flag = Arc::new(AtomicBool::new(true));
        let fut = mock_fut(flag.clone());
        let mut service = LazyService::new(fut, |_: ()| service_fn(|num: u8| future::ok::<_, ()>(num)));

        let mut cx = panic_context();

        assert!(service.poll_ready(&mut cx).is_ready());
        let mut fut = service.call(123);
        assert!(fut.poll_unpin(&mut cx).is_ready());
    }

    #[test]
    #[should_panic]
    fn call_before_ready_should_panic() {
        let flag = Arc::new(AtomicBool::new(false));
        let fut = mock_fut(flag.clone());
        let mut service = LazyService::new(fut, |_: ()| service_fn(|num: u8| future::ok::<_, ()>(num)));

        let mut cx = panic_context();

        assert!(service.poll_ready(&mut cx).is_pending());
        let _ = service.call(123);
    }
}

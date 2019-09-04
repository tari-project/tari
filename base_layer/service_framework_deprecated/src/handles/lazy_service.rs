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

use futures::{Future, Poll};
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
/// given function which must return a service. Subsequent calls to `poll_ready` and `call`
/// are delegated to that service.
///
/// This is used by the `lazy_service` combinator in `ServiceHandlesFuture`.
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

impl<TFn, F, S, T, TReq> Service<TReq> for LazyService<TFn, F, S>
where
    F: Future<Item = T, Error = S::Error>,
    TFn: FnOnce(F::Item) -> S,
    S: Service<TReq>,
{
    type Error = S::Error;
    type Future = S::Future;
    type Response = S::Response;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            match self.state {
                State::Pending => {
                    let item = try_ready!(self.future.poll());
                    let service_fn = self
                        .service_fn
                        .take()
                        .expect("service_fn cannot be None in Pending state");
                    self.state = State::Ready((service_fn)(item));
                },
                State::Ready(ref mut service) => {
                    return service.poll_ready();
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
    use futures::{
        future::{self, poll_fn},
        Async,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tower_util::service_fn;

    fn mock_fut(flag: Arc<AtomicBool>) -> impl Future<Item = (), Error = ()> {
        poll_fn::<_, (), _>(move || {
            if flag.load(Ordering::SeqCst) {
                Ok(().into())
            } else {
                Ok(Async::NotReady)
            }
        })
    }

    #[test]
    fn ready_after_handles() {
        let flag = Arc::new(AtomicBool::new(false));
        let fut = mock_fut(flag.clone());

        let mut service = LazyService::new(fut, |_: ()| service_fn(|_: ()| future::ok::<_, ()>(())));

        assert!(service.poll_ready().unwrap().is_not_ready());

        flag.store(true, Ordering::SeqCst);

        assert!(service.poll_ready().unwrap().is_ready());
    }

    #[test]
    fn call_after_ready() {
        let flag = Arc::new(AtomicBool::new(true));
        let fut = mock_fut(flag.clone());
        let mut service = LazyService::new(fut, |_: ()| service_fn(|_: ()| future::ok::<_, ()>(())));

        assert!(service.poll_ready().unwrap().is_ready());
        let mut fut = service.call(());
        assert!(fut.poll().unwrap().is_ready());
    }

    #[test]
    #[should_panic]
    fn call_before_ready() {
        let flag = Arc::new(AtomicBool::new(false));
        let fut = mock_fut(flag.clone());
        let mut service = LazyService::new(fut, |_: ()| service_fn(|_: ()| future::ok::<_, ()>(())));
        assert!(service.poll_ready().unwrap().is_not_ready());
        let _ = service.call(());
    }
}

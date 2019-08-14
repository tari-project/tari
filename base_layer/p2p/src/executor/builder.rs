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

use crate::executor::handles::ServiceHandles;
use futures::{future, Future, IntoFuture};
use std::{any::Any, hash::Hash, sync::Arc};

/// Builder trait for creating a service/handle pair.
/// The `StackBuilder` builds impls of this trait.
pub trait MakeServicePair<N> {
    type Future: Future<Item = (), Error = ()> + Send;
    type Handle: Any + Send + Sync;

    fn make_pair(self, handles: Arc<ServiceHandles<N>>) -> (Self::Handle, Self::Future);
}

/// Implementation of MakeServicePair for any function taking a ServiceHandle and returning a (Handle, Future) pair.
impl<TFunc, F, H, N> MakeServicePair<N> for TFunc
where
    TFunc: FnOnce(Arc<ServiceHandles<N>>) -> (H, F),
    F: Future<Item = (), Error = ()> + Send,
    H: Any,
    H: Send + Sync,
    N: Eq + Hash,
{
    type Future = F;
    type Handle = H;

    fn make_pair(self, handles: Arc<ServiceHandles<N>>) -> (Self::Handle, Self::Future) {
        (self)(handles)
    }
}

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// This can be converted into a future which resolves once all contained service futures are complete
/// by using the `IntoFuture` implementation.
pub struct StackBuilder<N> {
    handles: Arc<ServiceHandles<N>>,
    futures: Vec<Box<dyn Future<Item = (), Error = ()> + Send>>,
}

impl<N> StackBuilder<N>
where
    N: Eq + Hash,
    N: Send + Sync + 'static,
{
    pub fn new() -> Self {
        let handles = Arc::new(ServiceHandles::new());
        Self {
            handles,
            futures: Vec::new(),
        }
    }

    pub fn add_service(mut self, name: N, maker: impl MakeServicePair<N> + Send + 'static) -> Self {
        let (handle, fut) = maker.make_pair(self.handles.clone());
        self.handles.insert(name, handle);
        self.futures.push(Box::new(fut));
        self
    }
}

impl<N> IntoFuture for StackBuilder<N> {
    type Error = ();
    type Item = ();

    existential type Future: Future<Item = (), Error = ()> + Send;

    fn into_future(self) -> Self::Future {
        future::join_all(self.futures).map(|_| ())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{future::poll_fn, Async};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn service_stack_new() {
        let state = Arc::new(AtomicBool::new(false));
        let state_inner = Arc::clone(&state.clone());
        let service_pair_factory = |handles: Arc<ServiceHandles<&'static str>>| {
            let fut = poll_fn(move || {
                // Test that this futures own handle is available
                let fake_handle = handles.get_handle::<&str>(&"test-service").unwrap();
                assert_eq!(fake_handle, "Fake Handle");
                let not_found = handles.get_handle::<&str>(&"not-found");
                assert!(not_found.is_none());

                // Any panics above won't fail the test so a marker bool is set
                // if there are no panics.
                // catch_unwind could be used but then the UnwindSafe trait bound
                // needs to be added. TODO: handle panics in service poll functions
                state_inner.store(true, Ordering::Release);
                Ok(Async::Ready(()))
            });

            ("Fake Handle", fut)
        };

        tokio::run(
            StackBuilder::new()
                .add_service("test-service", service_pair_factory)
                .into_future(),
        );

        assert!(state.load(Ordering::Acquire))
    }
}

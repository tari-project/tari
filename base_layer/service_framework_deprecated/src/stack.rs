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

use crate::handles::{ServiceHandles, ServiceHandlesFuture};
use derive_error::Error;
use futures::{
    future::{self, Either},
    Future,
};
use std::{hash::Hash, sync::Arc};

#[derive(Debug, Error)]
pub enum ServiceInitializationError {
    #[error(msg_embedded, non_std, no_from)]
    InvariantError(String),
}

/// Builder trait for creating a service/handle pair.
/// The `StackBuilder` builds impls of this trait.
pub trait ServiceInitializer<N> {
    fn initialize(self: Box<Self>, handles: ServiceHandlesFuture<N>) -> Result<(), ServiceInitializationError>;
}

/// Implementation of MakeServicePair for any function taking a ServiceHandle and returning a (Handle, Future) pair.
impl<TFunc, N> ServiceInitializer<N> for TFunc
where
    N: Eq + Hash,
    TFunc: FnOnce(ServiceHandlesFuture<N>) -> Result<(), ServiceInitializationError>,
{
    fn initialize(self: Box<Self>, handles: ServiceHandlesFuture<N>) -> Result<(), ServiceInitializationError> {
        (self)(handles)
    }
}

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// This can be converted into a future which resolves once all contained service futures are complete
/// by using the `IntoFuture` implementation.
pub struct StackBuilder<N> {
    initializers: Vec<Box<dyn ServiceInitializer<N> + Send>>,
}

impl<N> StackBuilder<N>
where N: Eq + Hash
{
    pub fn new() -> Self {
        Self {
            initializers: Vec::new(),
        }
    }

    pub fn add_initializer(mut self, initializer: impl ServiceInitializer<N> + Send + 'static) -> Self {
        self.initializers.push(Box::new(initializer));
        self
    }

    pub fn finish(self) -> impl Future<Item = Arc<ServiceHandles<N>>, Error = ServiceInitializationError> {
        future::lazy(move || {
            let handles = ServiceHandlesFuture::new();

            for init in self.initializers.into_iter() {
                if let Err(err) = init.initialize(handles.clone()) {
                    return Either::B(future::err(err));
                }
            }

            handles.notify_ready();

            Either::A(handles.map_err(|_| {
                ServiceInitializationError::InvariantError("ServiceHandlesFuture cannot fail".to_string())
            }))
        })
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
        let service_initializer = |handles: ServiceHandlesFuture<&'static str>| {
            handles.insert("test-service", "Fake Handle");

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

            tokio::spawn(fut);
            Ok(())
        };

        tokio::run(
            StackBuilder::new()
                .add_initializer(service_initializer)
                .finish()
                .map(|_| ())
                .or_else(|err| {
                    panic!("{:?}", err);
                    #[allow(unreachable_code)]
                    future::err(())
                }),
        );

        assert!(state.load(Ordering::Acquire))
    }
}

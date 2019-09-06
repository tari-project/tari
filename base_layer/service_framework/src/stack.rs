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

use crate::{
    handles::{ServiceHandles, ServiceHandlesFuture},
    initializer::{BoxedServiceInitializer, ServiceInitializationError, ServiceInitializer},
};
use futures::future::join_all;
use std::sync::Arc;

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// `finish` is an async function which resolves once all the services are initialized, or returns
/// an error if any one of the services fails to initialize.
pub struct StackBuilder<'a, TExec> {
    initializers: Vec<BoxedServiceInitializer<TExec>>,
    executor: &'a mut TExec,
}

impl<'a, TExec> StackBuilder<'a, TExec> {
    pub fn new(executor: &'a mut TExec) -> Self {
        Self {
            initializers: Vec::new(),
            executor,
        }
    }
}

impl<'a, TExec> StackBuilder<'a, TExec> {
    /// Add an impl of ServiceInitializer to the stack
    pub fn add_initializer<I>(self, initializer: I) -> Self
    where
        I: ServiceInitializer<TExec> + Send + 'static,
        I::Future: Send + 'static,
    {
        self.add_initializer_boxed(initializer.boxed())
    }

    /// Add a ServiceInitializer which has been boxed using `ServiceInitializer::boxed`
    pub fn add_initializer_boxed(mut self, initializer: BoxedServiceInitializer<TExec>) -> Self {
        self.initializers.push(initializer);
        self
    }

    /// Concurrently initialize the services. Once all service have been initialized, `notify_ready`
    /// is called, which completes initialization for those services which  . The resulting service handles are
    /// returned. If ANY of the services fail to initialize, an error is returned.
    pub async fn finish(self) -> Result<Arc<ServiceHandles>, ServiceInitializationError> {
        let handles_fut = ServiceHandlesFuture::new();

        let executor = self.executor;

        // Collect all the initialization futures
        let init_futures = self
            .initializers
            .into_iter()
            .map(|mut init| ServiceInitializer::initialize(&mut init, executor, handles_fut.clone()));

        // Run all the initializers concurrently and check each Result returning an error
        // on the first one that failed.
        for result in join_all(init_futures).await {
            result?;
        }

        handles_fut.notify_ready();

        Ok(handles_fut.into_inner())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{handles::ServiceHandlesFuture, initializer::ServiceInitializer, tower::service_fn};
    use futures::{executor::block_on, future, task::SpawnExt, Future};
    use futures_test::task::NoopSpawner;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn service_defn_simple() {
        // This is less of a test and more of a demo of using the short-hand implementation of ServiceInitializer
        let simple_initializer = |executor: &mut NoopSpawner, _: ServiceHandlesFuture| {
            executor.spawn(future::ready(())).unwrap();
            future::ok(())
        };

        let mut executor = NoopSpawner::new();
        let handles = block_on(
            StackBuilder::new(&mut executor)
                .add_initializer(simple_initializer)
                .finish(),
        );

        assert!(handles.is_ok());
    }

    #[derive(Clone)]
    struct DummyServiceHandle(usize);
    struct DummyInitializer {
        state: Arc<AtomicUsize>,
    }

    impl DummyInitializer {
        fn new(state: Arc<AtomicUsize>) -> Self {
            Self { state }
        }
    }

    impl<TExec> ServiceInitializer<TExec> for DummyInitializer
    where TExec: SpawnExt
    {
        type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

        fn initialize(&mut self, executor: &mut TExec, handles_fut: ServiceHandlesFuture) -> Self::Future {
            // Spawn some task on the given executor (in this case, NoopSpawner)
            executor.spawn(future::ready(())).unwrap();
            // Add a handle
            handles_fut.register(DummyServiceHandle(123));

            // This demonstrates the chicken and egg problem with services and handles. Specifically,
            // that we have a service which requires the handles of other services to be able to
            // create it's own handle. Here we wait for the handles_fut to resolve before continuing
            // to initialize the service.
            //
            // Critically, you should never wait for handles in the initialize method because
            // handles are only resolved after all initialization methods have completed.
            let spawn_result = executor
                .spawn(async move {
                    let final_handles = handles_fut.await;

                    let handle = final_handles.get_handle::<DummyServiceHandle>().unwrap();
                    assert_eq!(handle.0, 123);
                    // Something which uses the handle
                    service_fn(|_: ()| future::ok::<_, ()>(handle.0));
                })
                .map_err(Into::into);

            self.state.fetch_add(1, Ordering::AcqRel);
            future::ready(spawn_result)
        }
    }

    #[test]
    fn service_stack_new() {
        let shared_state = Arc::new(AtomicUsize::new(0));

        let initializer = DummyInitializer::new(Arc::clone(&shared_state));

        let mut executor = NoopSpawner::new();
        let handles = block_on(StackBuilder::new(&mut executor).add_initializer(initializer).finish()).unwrap();

        handles.get_handle::<DummyServiceHandle>().unwrap();

        assert_eq!(shared_state.load(Ordering::SeqCst), 1);
    }
}

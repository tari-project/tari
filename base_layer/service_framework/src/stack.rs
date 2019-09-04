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
use std::{hash::Hash, sync::Arc};

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// `finish` is an async function which resolves once all the services are initialized, or returns
/// an error if any one of the services fails to initialize.
pub struct StackBuilder<'a, TName, TExec> {
    initializers: Vec<BoxedServiceInitializer<TName, TExec>>,
    executor: &'a mut TExec,
}

impl<'a, TName, TExec> StackBuilder<'a, TName, TExec> {
    pub fn new(executor: &'a mut TExec) -> Self {
        Self {
            initializers: Vec::new(),
            executor,
        }
    }
}

impl<'a, TName, TExec> StackBuilder<'a, TName, TExec>
where TName: Eq + Hash
{
    /// Add an impl of ServiceInitializer to the stack
    pub fn add_initializer<I>(self, initializer: I) -> Self
    where
        I: ServiceInitializer<TName, TExec> + Send + 'static,
        I::Future: Send + 'static,
    {
        self.add_initializer_boxed(initializer.boxed())
    }

    /// Add a ServiceInitializer which has been boxed using `ServiceInitializer::boxed`
    pub fn add_initializer_boxed(mut self, initializer: BoxedServiceInitializer<TName, TExec>) -> Self {
        self.initializers.push(initializer);
        self
    }

    /// Concurrently initialize the services. Once all service have been initialized, `notify_ready`
    /// is called, which completes initialization for those services which  . The resulting service handles are
    /// returned. If ANY of the services fail to initialize, an error is returned.
    pub async fn finish(self) -> Result<Arc<ServiceHandles<TName>>, ServiceInitializationError> {
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
    use tower_service::Service;

    #[test]
    fn service_defn_simple() {
        // This is less of a test and more of a demo of using the short-hand implementation of ServiceInitializer
        let simple_initializer = |executor: &mut NoopSpawner, _: ServiceHandlesFuture<()>| {
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

    struct DummyInitializer {
        n: usize,
        state: Arc<AtomicUsize>,
    }

    impl DummyInitializer {
        fn new(n: usize, state: Arc<AtomicUsize>) -> Self {
            Self { n, state }
        }

        fn get_name(&self) -> String {
            format!("dummy-{}", self.n)
        }

        // This takes a service so that we can pretend it's to create the handle
        fn get_handle(&self, _: impl Service<()>) -> String {
            format!("handle-{}", self.n)
        }
    }

    impl<TExec> ServiceInitializer<String, TExec> for DummyInitializer
    where TExec: SpawnExt
    {
        type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

        fn initialize(&mut self, executor: &mut TExec, handles_fut: ServiceHandlesFuture<String>) -> Self::Future {
            // Spawn some task on the given executor (in this case, NoopSpawner)
            executor.spawn(future::ready(())).unwrap();

            // This demonstrates the chicken and egg problem with services and handles. Specifically,
            // that we have a service which requires the handles of other services to be able to
            // create it's own handle. The lazy_service combinator is used to defer the
            // initialization of the service until all handles are ready, while still
            // registering a handle in the initialize function.
            let service = handles_fut.lazy_service(|_handles| {
                // All handles are available here - continue to initialize our service
                service_fn(|_: ()| future::ok::<_, ()>(()))
            });

            // Add a handle
            handles_fut.insert(self.get_name(), self.get_handle(service));

            self.state.fetch_add(1, Ordering::AcqRel);
            future::ok(())
        }
    }

    #[test]
    fn service_stack_new() {
        let shared_state = Arc::new(AtomicUsize::new(0));

        let initializer_1 = DummyInitializer::new(0, Arc::clone(&shared_state));
        let initializer_2 = DummyInitializer::new(1, Arc::clone(&shared_state));

        let mut executor = NoopSpawner::new();
        let handles = block_on(
            StackBuilder::new(&mut executor)
                .add_initializer(initializer_1)
                .add_initializer(initializer_2)
                .finish(),
        )
        .unwrap();

        for i in 0..=1 {
            assert_eq!(
                handles.get_handle::<String>(format!("dummy-{}", i)).unwrap(),
                format!("handle-{}", i)
            );
        }

        assert_eq!(shared_state.load(Ordering::SeqCst), 2);
    }
}

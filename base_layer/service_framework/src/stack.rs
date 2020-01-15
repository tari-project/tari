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
    handles::{handle_notifier_pair, ServiceHandles},
    initializer::{BoxedServiceInitializer, ServiceInitializationError, ServiceInitializer},
};
use futures::future::join_all;
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// `finish` is an async function which resolves once all the services are initialized, or returns
/// an error if any one of the services fails to initialize.
pub struct StackBuilder {
    initializers: Vec<BoxedServiceInitializer>,
    executor: runtime::Handle,
    shutdown_signal: ShutdownSignal,
}

impl StackBuilder {
    pub fn new(executor: runtime::Handle, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            initializers: Vec::new(),
            executor,
            shutdown_signal,
        }
    }
}

impl StackBuilder {
    /// Add an impl of ServiceInitializer to the stack
    pub fn add_initializer<I>(self, initializer: I) -> Self
    where
        I: ServiceInitializer + Send + 'static,
        I::Future: Send + 'static,
    {
        self.add_initializer_boxed(initializer.boxed())
    }

    /// Add a ServiceInitializer which has been boxed using `ServiceInitializer::boxed`
    pub fn add_initializer_boxed(mut self, initializer: BoxedServiceInitializer) -> Self {
        self.initializers.push(initializer);
        self
    }

    /// Concurrently initialize the services. Once all service have been initialized, `notify_ready`
    /// is called, which completes initialization for those services. The resulting service handles are
    /// returned. If ANY of the services fail to initialize, an error is returned.
    pub async fn finish(self) -> Result<Arc<ServiceHandles>, ServiceInitializationError> {
        let (notifier, handles_fut) = handle_notifier_pair();

        let StackBuilder {
            executor,
            shutdown_signal,
            initializers,
        } = self;

        // Collect all the initialization futures
        let init_futures = initializers.into_iter().map(|mut init| {
            ServiceInitializer::initialize(
                &mut init,
                executor.clone(),
                handles_fut.clone(),
                shutdown_signal.clone(),
            )
        });

        // Run all the initializers concurrently and check each Result returning an error
        // on the first one that failed.
        for result in join_all(init_futures).await {
            result?;
        }

        notifier.notify();

        Ok(handles_fut.into_inner())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{handles::ServiceHandlesFuture, initializer::ServiceInitializer, tower::service_fn};
    use futures::{executor::block_on, future, Future};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tari_shutdown::Shutdown;
    use tokio::runtime::Runtime;

    #[test]
    fn service_defn_simple() {
        let rt = Runtime::new().unwrap();
        // This is less of a test and more of a demo of using the short-hand implementation of ServiceInitializer
        let simple_initializer = |executor: runtime::Handle, _: ServiceHandlesFuture, _: ShutdownSignal| {
            executor.spawn(future::ready(()));
            future::ok(())
        };

        let shutdown = Shutdown::new();

        let handles = block_on(
            StackBuilder::new(rt.handle().clone(), shutdown.to_signal())
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

    impl ServiceInitializer for DummyInitializer {
        type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

        fn initialize(
            &mut self,
            executor: runtime::Handle,
            handles_fut: ServiceHandlesFuture,
            _shutdown: ShutdownSignal,
        ) -> Self::Future
        {
            // Spawn some task on the given runtime::Handle
            executor.spawn(future::ready(()));
            // Add a handle
            handles_fut.register(DummyServiceHandle(123));

            // This demonstrates the chicken and egg problem with services and handles. Specifically,
            // that we have a service which requires the handles of other services to be able to
            // create it's own handle. Here we wait for the handles_fut to resolve before continuing
            // to initialize the service.
            //
            // Critically, you should never wait for handles in the initialize method because
            // handles are only resolved after all initialization methods have completed.
            executor.spawn(async move {
                let final_handles = handles_fut.await;

                let handle = final_handles.get_handle::<DummyServiceHandle>().unwrap();
                assert_eq!(handle.0, 123);
                // Something which uses the handle
                service_fn(|_: ()| future::ok::<_, ()>(handle.0));
            });

            self.state.fetch_add(1, Ordering::AcqRel);
            future::ready(Ok(()))
        }
    }

    #[test]
    fn service_stack_new() {
        let rt = Runtime::new().unwrap();
        let shared_state = Arc::new(AtomicUsize::new(0));

        let shutdown = Shutdown::new();
        let initializer = DummyInitializer::new(Arc::clone(&shared_state));

        let handles = block_on(
            StackBuilder::new(rt.handle().clone(), shutdown.to_signal())
                .add_initializer(initializer)
                .finish(),
        )
        .unwrap();

        handles.get_handle::<DummyServiceHandle>().unwrap();

        assert_eq!(shared_state.load(Ordering::SeqCst), 1);
    }
}

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
    context::{create_context_notifier_pair, ServiceHandles},
    initializer::{BoxedServiceInitializer, InitializerFn, ServiceInitializationError, ServiceInitializer},
    ServiceInitializerContext,
};
use futures::future;
use std::future::Future;
use tari_shutdown::ShutdownSignal;

/// Responsible for building and collecting handles and (usually long-running) service futures.
/// `finish` is an async function which resolves once all the services are initialized, or returns
/// an error if any one of the services fails to initialize.
pub struct StackBuilder {
    initializers: Vec<BoxedServiceInitializer>,
    shutdown_signal: ShutdownSignal,
}

impl StackBuilder {
    pub fn new(shutdown_signal: ShutdownSignal) -> Self {
        Self {
            initializers: Vec::new(),
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

    /// Add an impl of ServiceInitializer to the stack
    pub fn add_initializer_fn<TFunc, TFut>(self, initializer: TFunc) -> Self
    where
        TFunc: FnOnce(ServiceInitializerContext) -> TFut + Send + 'static,
        TFut: Future<Output = Result<(), ServiceInitializationError>> + Send + 'static,
    {
        self.add_initializer_boxed(InitializerFn::new(initializer).boxed())
    }

    /// Add a ServiceInitializer which has been boxed using `ServiceInitializer::boxed`
    pub fn add_initializer_boxed(mut self, initializer: BoxedServiceInitializer) -> Self {
        self.initializers.push(initializer);
        self
    }

    /// Concurrently initialize the services. Once all service have been initialized, `notify_ready`
    /// is called, which completes initialization for those services. The resulting service handles are
    /// returned. If ANY of the services fail to initialize, an error is returned.
    pub async fn build(self) -> Result<ServiceHandles, ServiceInitializationError> {
        let StackBuilder {
            shutdown_signal,
            mut initializers,
        } = self;

        let (mut notifier, context) = create_context_notifier_pair(shutdown_signal);

        // Collect all the initialization futures
        let init_futures = initializers
            .iter_mut()
            .map(|init| ServiceInitializer::initialize(init, context.clone()));

        // Run all the initializers concurrently and check each Result returning an error
        // on the first one that failed.
        for result in future::join_all(init_futures).await {
            result?;
        }

        let _ = notifier.trigger();

        Ok(context.into_inner())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{initializer::ServiceInitializer, ServiceInitializerContext};
    use futures::{executor::block_on, future, Future};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tari_shutdown::Shutdown;
    use tower::service_fn;

    #[tokio_macros::test]
    async fn service_defn_simple() {
        // This is less of a test and more of a demo of using the short-hand implementation of ServiceInitializer
        let simple_initializer = |_: ServiceInitializerContext| future::ok(());

        let shutdown = Shutdown::new();

        let handles = StackBuilder::new(shutdown.to_signal())
            .add_initializer(simple_initializer)
            .build()
            .await;

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

        fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
            // Add a handle
            context.register_handle(DummyServiceHandle(123));

            // This demonstrates the chicken and egg problem with services and handles. Specifically,
            // that we have a service which requires the handles of other services to be able to
            // create it's own handle. Here we wait for the handles_fut to resolve before continuing
            // to initialize the service.
            //
            // Critically, you should never wait for handles in the initialize method because
            // handles are only resolved after all initialization methods have completed.
            context.spawn_when_ready(|handles| async move {
                let handle = handles.get_handle::<DummyServiceHandle>().unwrap();
                assert_eq!(handle.0, 123);
                // Something which uses the handle
                service_fn(|_: ()| future::ok::<_, ()>(handle.0));
            });

            self.state.fetch_add(1, Ordering::AcqRel);
            future::ready(Ok(()))
        }
    }

    #[tokio_macros::test]
    async fn service_stack_new() {
        let shared_state = Arc::new(AtomicUsize::new(0));

        let shutdown = Shutdown::new();
        let initializer = DummyInitializer::new(Arc::clone(&shared_state));

        let handles = block_on(
            StackBuilder::new(shutdown.to_signal())
                .add_initializer(initializer)
                .build(),
        )
        .unwrap();

        handles.get_handle::<DummyServiceHandle>().unwrap();

        assert_eq!(shared_state.load(Ordering::SeqCst), 1);
    }
}

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

use std::{
    any,
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, Mutex},
};

use futures::{future, future::Either, Future, FutureExt};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::task;

use crate::context::LazyService;

/// Create a Notifier, ServiceInitializerContext pair.
///
/// The `Notifier::notify` method will notify all cloned `ServiceHandlesFuture`s
/// and which will resolve with the collected `ServiceHandles`.
pub(crate) fn create_context_notifier_pair(shutdown_signal: ShutdownSignal) -> (Shutdown, ServiceInitializerContext) {
    let trigger = Shutdown::new();
    let trigger_signal = trigger.to_signal();
    (trigger, ServiceInitializerContext::new(shutdown_signal, trigger_signal))
}

/// Contains context for service initialization.
///
/// `ServiceInitializerContext` also implements `Future` and resolves to `ServiceHandles` once
/// it is signaled to do so.
#[derive(Clone)]
pub struct ServiceInitializerContext {
    inner: ServiceHandles,
    ready_signal: ShutdownSignal,
}

impl ServiceInitializerContext {
    /// Create a new ServiceInitializerContext.
    ///
    /// ## Parameters
    /// `shutdown_signal` - signal that is provided to services. If this signal is triggered, services should terminate.
    /// `ready_signal` - indicates that all services are ready. This should be triggered by the `StackBuilder` once all
    ///                  initializers have run.
    pub(crate) fn new(shutdown_signal: ShutdownSignal, ready_signal: ShutdownSignal) -> Self {
        Self {
            inner: ServiceHandles::new(shutdown_signal),
            ready_signal,
        }
    }

    /// Insert a service handle with the given name
    pub fn register_handle<H>(&self, handle: H)
    where H: Any + Send {
        self.inner.register(handle);
    }

    /// Call the given function with the final handles once this future is ready (`notify_ready` is called).
    pub fn lazy_service<F, S>(&self, service_fn: F) -> LazyService<F, Self, S>
    where F: FnOnce(ServiceHandles) -> S {
        LazyService::new(self.clone(), service_fn)
    }

    /// Spawn a task once handles are ready. The resolved handles are passed into this closure.
    pub fn spawn_when_ready<F, Fut>(self, f: F) -> task::JoinHandle<Fut::Output>
    where
        F: FnOnce(ServiceHandles) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send,
    {
        task::spawn(self.wait_ready().then(f))
    }

    /// Spawn a task once handles are ready. The resolved handles are passed into this closure.
    /// The future returned from the closure is polled on a new task until the shutdown signal is triggered.
    pub fn spawn_until_shutdown<F, Fut>(self, f: F) -> task::JoinHandle<Option<Fut::Output>>
    where
        F: FnOnce(ServiceHandles) -> Fut + Send + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        task::spawn(async move {
            let shutdown_signal = self.get_shutdown_signal();
            self.ready_signal.await;
            let fut = f(self.inner);
            futures::pin_mut!(fut);
            let either = future::select(shutdown_signal, fut).await;
            match either {
                Either::Left((_, _)) => None,
                Either::Right((res, _)) => Some(res),
            }
        })
    }

    /// Wait until the service handle are ready and return them when they are.
    pub async fn wait_ready(self) -> ServiceHandles {
        self.ready_signal.await;
        self.inner
    }

    /// Returns the shutdown signal for this stack
    pub fn get_shutdown_signal(&self) -> ShutdownSignal {
        self.inner.get_shutdown_signal()
    }

    pub fn into_inner(self) -> ServiceHandles {
        self.inner
    }
}

/// This macro unlocks a Mutex or RwLock. If the lock is
/// poisoned (i.e. panic while unlocked) the last value
/// before the panic is used.
macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        match $e.$m() {
            Ok(lock) => lock,
            Err(poisoned) => {
                log::warn!(target: "service_framework", "Lock has been POISONED and will be silently recovered");
                poisoned.into_inner()
            },
        }
    };
    ($e:expr) => {
        acquire_lock!($e, lock)
    };
}

/// Simple collection for named handles
#[derive(Clone)]
pub struct ServiceHandles {
    handles: Arc<Mutex<HashMap<TypeId, Box<dyn Any + Send>>>>,
    shutdown_signal: ShutdownSignal,
}

impl ServiceHandles {
    /// Create a new ServiceHandles
    pub(crate) fn new(shutdown_signal: ShutdownSignal) -> Self {
        Self {
            handles: Default::default(),
            shutdown_signal,
        }
    }

    /// Register a handle
    pub fn register<H>(&self, handle: H)
    where H: Any + Send {
        acquire_lock!(self.handles).insert(TypeId::of::<H>(), Box::new(handle));
    }

    /// Get a handle from the given type (`TypeId`) and downcast it to a type `H`.
    /// If the item does not exist or the downcast fails, `None` is returned.
    pub fn get_handle<H>(&self) -> Option<H>
    where H: Clone + 'static {
        self.get_handle_by_type_id(TypeId::of::<H>())
    }

    /// Take ownership of a handle
    pub fn take_handle<H: 'static>(&mut self) -> Option<H> {
        acquire_lock!(self.handles)
            .remove(&TypeId::of::<H>())
            .and_then(|handle| handle.downcast::<H>().ok().map(|h| *h))
    }

    /// Get a handle from the given type (`TypeId`) and downcast it to a type `H`.
    /// If the item does not exist or the downcast fails, a panic occurs
    pub fn expect_handle<H>(&self) -> H
    where H: Clone + 'static {
        match self.get_handle_by_type_id(TypeId::of::<H>()) {
            Some(h) => h,
            None => panic!("Service handle `{}` is not registered", any::type_name::<H>()),
        }
    }

    /// Get a ServiceHandle by name and downcast it to a type `H`. If the item
    /// does not exist or the downcast fails, `None` is returned.
    fn get_handle_by_type_id<H>(&self, type_id: TypeId) -> Option<H>
    where H: Clone + 'static {
        acquire_lock!(self.handles)
            .get(&type_id)
            .and_then(|b| b.downcast_ref::<H>())
            .cloned()
    }

    /// Returns the shutdown signal for this stack
    pub fn get_shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown_signal.clone()
    }
}

#[cfg(test)]
mod test {
    use tari_shutdown::Shutdown;

    use super::*;

    #[test]
    fn service_handles_insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let handles = ServiceHandles::new(Shutdown::new().to_signal());
        handles.register(TestHandle);
        handles.get_handle::<TestHandle>().unwrap();
        assert!(handles.get_handle::<()>().is_none());
        assert!(handles.get_handle::<usize>().is_none());
    }

    #[test]
    fn insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let trigger = Shutdown::new();
        let context = ServiceInitializerContext::new(trigger.to_signal(), trigger.to_signal());
        context.register_handle(TestHandle);
        context.inner.expect_handle::<TestHandle>();
        assert!(context.inner.get_handle::<()>().is_none());
    }
}

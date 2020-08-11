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

use crate::handles::ServiceHandlesFuture;
use futures::{Future, FutureExt};
use std::pin::Pin;
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::runtime;

#[derive(Debug, Error)]
pub enum ServiceInitializationError {
    #[error("General error for failed initialization: `{0}`")]
    Failed(String),
    // Specialized errors should be added and used if appropriate.
}

/// Implementors of this trait will initialize a service
/// The `StackBuilder` builds impls of this trait.
pub trait ServiceInitializer {
    /// The future returned from the initialize function
    type Future: Future<Output = Result<(), ServiceInitializationError>>;

    /// Async initialization code for a service
    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future;

    /// Create a boxed version of this ServiceInitializer.
    fn boxed(self) -> BoxedServiceInitializer
    where
        Self: Sized + Send + 'static,
        Self::Future: Send + 'static,
    {
        BoxedServiceInitializer::new(self)
    }
}

/// Implementation of ServiceInitializer for any function matching the signature of `ServiceInitializer::initialize`
/// This allows the following "short-hand" syntax to be used:
///
/// ```edition2018
/// # use tari_service_framework::handles::ServiceHandlesFuture;
/// # use tokio::runtime;
/// let my_initializer = |executor: runtime::Handle, handles_fut: ServiceHandlesFuture| {
///     // initialization code
///     futures::future::ready(Result::<_, ()>::Ok(()))
/// };
/// ```
impl<TFunc, TFut> ServiceInitializer for TFunc
where
    TFunc: FnMut(runtime::Handle, ServiceHandlesFuture, ShutdownSignal) -> TFut,
    TFut: Future<Output = Result<(), ServiceInitializationError>>,
{
    type Future = TFut;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        (self)(executor, handles, shutdown)
    }
}

//---------------------------------- Boxed Service Initializer --------------------------------------------//
// The following code is essentially a substitute for async trait functions. Any initializer can
// converted to the boxed form by using ServiceInitializer::boxed(). This is done for you when
// using `StackBuilder::add_initializer`.

/// A pinned, boxed form of the future resulting from a boxed ServiceInitializer
type ServiceInitializationFuture = Pin<Box<dyn Future<Output = Result<(), ServiceInitializationError>> + Send>>;

/// This trait mirrors the ServiceInitializer trait, with the exception
/// of always returning a boxed future (aliased ServiceInitializationFuture type),
/// therefore it does not need the `Future` associated type. This makes it
/// possible to store a boxed dyn `AbstractServiceInitializer<TName, TExec>`.
pub trait AbstractServiceInitializer {
    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> ServiceInitializationFuture;
}

/// AbstractServiceInitializer impl for every T: ServiceInitializer.
impl<T> AbstractServiceInitializer for T
where
    T: ServiceInitializer,
    T::Future: Send + 'static,
{
    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> ServiceInitializationFuture
    {
        let initialization = self.initialize(executor, handles, shutdown);
        initialization.boxed() as ServiceInitializationFuture
    }
}

/// A concrete boxed version of a ServiceInitializer. This makes it possible
/// to have a collection of ServiceInitializers which return various boxed future types.
/// This type is used in StackBuilder's internal vec.
pub struct BoxedServiceInitializer {
    inner: Box<dyn AbstractServiceInitializer + Send + 'static>,
}

impl BoxedServiceInitializer {
    pub(super) fn new<T>(initializer: T) -> Self
    where
        T: ServiceInitializer + Send + 'static,
        T::Future: Send + 'static,
    {
        Self {
            inner: Box::new(initializer),
        }
    }
}

impl ServiceInitializer for BoxedServiceInitializer {
    type Future = ServiceInitializationFuture;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        self.inner.initialize(executor, handles_fut, shutdown)
    }
}

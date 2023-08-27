// Copyright 2019 The Taiji Project
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

use async_trait::async_trait;

use crate::context::ServiceInitializerContext;

pub type ServiceInitializationError = anyhow::Error;

type Output = Result<(), ServiceInitializationError>;

/// Implementors of this trait will initialize a service
/// The `StackBuilder` builds impls of this trait.
#[async_trait]
pub trait ServiceInitializer {
    /// Async initialization code for a service
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Output;
}

/// Implementation of ServiceInitializer for any function matching the signature of `ServiceInitializer::initialize`
/// This allows the following "short-hand" syntax to be used:
///
/// ```edition2018
/// # use taiji_service_framework::ServiceInitializerContext;
/// # use tokio::runtime;
/// let my_initializer = |context: ServiceInitializerContext| {
///     // initialization code
///     futures::future::ready(Result::<_, ()>::Ok(()))
/// };
/// ```
#[async_trait]
impl<TFunc> ServiceInitializer for TFunc
where TFunc: FnMut(ServiceInitializerContext) -> Output + Send
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Output {
        (self)(context)
    }
}

/// An implementation of `ServiceInitializer` that wraps an `impl FnOnce`
pub struct InitializerFn<TFunc>(Option<TFunc>);

impl<TFunc> InitializerFn<TFunc> {
    pub fn new(f: TFunc) -> Self {
        Self(Some(f))
    }
}

#[async_trait]
impl<TFunc> ServiceInitializer for InitializerFn<TFunc>
where TFunc: FnOnce(ServiceInitializerContext) -> Output + Send
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Output {
        let f = self.0.take().expect("initializer called more than once");
        (f)(context)
    }
}

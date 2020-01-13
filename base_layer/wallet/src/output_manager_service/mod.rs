// Copyright 2019. The Tari Project
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

use crate::output_manager_service::{handle::OutputManagerHandle, service::OutputManagerService};

use crate::output_manager_service::storage::database::{OutputManagerBackend, OutputManagerDatabase};
use futures::{future, Future};
use log::*;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};
use tari_shutdown::ShutdownSignal;
use tari_transactions::types::CryptoFactories;
use tokio::runtime;

pub mod error;
pub mod handle;
pub mod service;
pub mod storage;

const LOG_TARGET: &'static str = "wallet::output_manager_service::initializer";

pub type TxId = u64;

pub struct OutputManagerServiceInitializer<T>
where T: OutputManagerBackend
{
    backend: Option<T>,
    factories: CryptoFactories,
}

impl<T> OutputManagerServiceInitializer<T>
where T: OutputManagerBackend
{
    pub fn new(backend: T, factories: CryptoFactories) -> Self {
        Self {
            backend: Some(backend),
            factories,
        }
    }
}

impl<T> ServiceInitializer for OutputManagerServiceInitializer<T>
where T: OutputManagerBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: runtime::Handle,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let (sender, receiver) = reply_channel::unbounded();

        let oms_handle = OutputManagerHandle::new(sender);

        // Register handle before waiting for handles to be ready
        handles_fut.register(oms_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Output Manager Service without setting a storage backend");
        let factories = self.factories.clone();
        executor.spawn(async move {
            let service = OutputManagerService::new(receiver, OutputManagerDatabase::new(backend), factories)
                .expect("Could not initialize Output Manager Service")
                .start();

            futures::pin_mut!(service);
            future::select(service, shutdown).await;
            info!(target: LOG_TARGET, "Output manager service shutdown");
        });
        future::ready(Ok(()))
    }
}

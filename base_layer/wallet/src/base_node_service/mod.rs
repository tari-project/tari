// Copyright 2020. The Tari Project
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

pub mod config;
pub mod error;
pub mod handle;
pub mod mock_base_node_service;
pub mod service;

mod monitor;

use crate::{
    base_node_service::{config::BaseNodeServiceConfig, handle::BaseNodeServiceHandle, service::BaseNodeService},
    storage::database::{WalletBackend, WalletDatabase},
};
use futures::{future, Future};
use log::*;
use tari_comms::connectivity::ConnectivityRequester;
use tari_service_framework::{
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::broadcast;

const LOG_TARGET: &str = "wallet::base_node_service";

pub struct BaseNodeServiceInitializer<T>
where T: WalletBackend + 'static
{
    config: BaseNodeServiceConfig,
    db: WalletDatabase<T>,
}

impl<T> BaseNodeServiceInitializer<T>
where T: WalletBackend + 'static
{
    pub fn new(config: BaseNodeServiceConfig, db: WalletDatabase<T>) -> Self {
        Self { config, db }
    }
}

impl<T> ServiceInitializer for BaseNodeServiceInitializer<T>
where T: WalletBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        info!(target: LOG_TARGET, "Wallet base node service initializing.");

        let (sender, request_stream) = reply_channel::unbounded();

        let (event_publisher, _) = broadcast::channel(200);

        let basenode_service_handle = BaseNodeServiceHandle::new(sender, event_publisher.clone());

        // Register handle before waiting for handles to be ready
        context.register_handle(basenode_service_handle);

        let config = self.config.clone();
        let db = self.db.clone();

        context.spawn_when_ready(move |handles| async move {
            let connectivity_manager = handles.expect_handle::<ConnectivityRequester>();

            let service = BaseNodeService::new(
                config,
                request_stream,
                connectivity_manager,
                event_publisher,
                handles.get_shutdown_signal(),
                db,
            )
            .start();
            futures::pin_mut!(service);
            let _ = service.await;
            info!(target: LOG_TARGET, "Wallet Base Node Service shutdown");
        });

        future::ready(Ok(()))
    }
}

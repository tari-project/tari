// Copyright 2021. The Tari Project
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
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::utxo_scanning::{UtxoScannerMode, UtxoScannerService},
};
use futures::{future, Future};
use log::*;
use std::{sync::Arc, time::Duration};
use tari_comms::{connectivity::ConnectivityRequester, NodeIdentity};
use tari_core::transactions::types::CryptoFactories;
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};

pub mod utxo_scanning;

const LOG_TARGET: &str = "wallet::utxo_scanner_service::initializer";

pub struct UtxoScannerServiceInitializer<T>
where T: WalletBackend + 'static
{
    interval: Duration,
    backend: Option<WalletDatabase<T>>,
    factories: CryptoFactories,
    node_identity: Arc<NodeIdentity>,
}

impl<T> UtxoScannerServiceInitializer<T>
where T: WalletBackend + 'static
{
    pub fn new(
        interval: Duration,
        backend: WalletDatabase<T>,
        factories: CryptoFactories,
        node_identity: Arc<NodeIdentity>,
    ) -> Self
    {
        Self {
            interval,
            backend: Some(backend),
            factories,
            node_identity,
        }
    }
}

impl<T> ServiceInitializer for UtxoScannerServiceInitializer<T>
where T: WalletBackend + 'static
{
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, context: ServiceInitializerContext) -> Self::Future {
        trace!(target: LOG_TARGET, "Utxo scanner initialization");

        let backend = self
            .backend
            .take()
            .expect("Cannot start Utxo scanner service without setting a storage backend");
        let factories = self.factories.clone();
        let interval = self.interval;
        let node_identity = self.node_identity.clone();

        context.spawn_when_ready(move |handles| async move {
            let transaction_service = handles.expect_handle::<TransactionServiceHandle>();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let connectivity_manager = handles.expect_handle::<ConnectivityRequester>();
            let peer_seeds = Vec::new();

            let scanning_service = UtxoScannerService::<T>::builder()
                .with_peer_seeds(peer_seeds)
                .with_retry_limit(10)
                .with_scanning_interval(interval)
                .with_mode(UtxoScannerMode::Scanning)
                .build_with_resources(
                    backend,
                    connectivity_manager,
                    output_manager_service,
                    transaction_service,
                    node_identity,
                    factories,
                    handles.get_shutdown_signal(),
                )
                .run();

            futures::pin_mut!(scanning_service);
            info!(target: LOG_TARGET, "Utxo scanner service shutdown");
        });
        future::ready(Ok(()))
    }
}

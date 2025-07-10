// Copyright 2022. The Tari Project
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

use std::marker::PhantomData;

use futures::future;
use log::*;
use tari_common::configuration::Network;
use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
use tari_core::transactions::transaction_key_manager::TransactionKeyManagerInterface;
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::sync::broadcast;
use url::Url;

use crate::{
    client::http_client_factory::{DefaultHttpClientFactory, HttpClientFactory},
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::handle::TransactionServiceHandle,
    util::watch::Watch,
    utxo_scanner_service::{
        handle::UtxoScannerHandle,
        service::UtxoScannerService,
        uxto_scanner_service_builder::UtxoScannerMode,
    },
};

const LOG_TARGET: &str = "wallet::utxo_scanner_service::initializer";

pub struct UtxoScannerServiceInitializer<T, TKeyManagerInterface> {
    backend: Option<WalletDatabase<T>>,
    network: Network,
    birthday_offset: u16,
    http_node_url: Url,
    http_fallback_url: Url,
    scanning_interval: u64,
    phantom: PhantomData<TKeyManagerInterface>,
}

impl<T, TKeyManagerInterface> UtxoScannerServiceInitializer<T, TKeyManagerInterface>
where T: WalletBackend + 'static
{
    pub fn new(
        backend: WalletDatabase<T>,
        network: Network,
        birthday_offset: u16,
        http_node_url: Url,
        http_fallback_url: Url,
        scanning_interval: u64,
    ) -> Self {
        Self {
            backend: Some(backend),
            network,
            phantom: PhantomData,
            birthday_offset,
            http_node_url,
            http_fallback_url,
            scanning_interval,
        }
    }
}

#[async_trait]
impl<T, TKeyManagerInterface> ServiceInitializer for UtxoScannerServiceInitializer<T, TKeyManagerInterface>
where
    T: WalletBackend + 'static,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        trace!(target: LOG_TARGET, "Utxo scanner initialization");

        let (event_sender, _) = broadcast::channel(200);

        let recovery_message_watch = Watch::new("Output found on blockchain during Wallet Recovery".to_string());
        let one_sided_message_watch = Watch::new("Detected one-sided payment on blockchain".to_string());

        // Register handle before waiting for handles to be ready
        let utxo_scanner_handle =
            UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);
        context.register_handle(utxo_scanner_handle);

        let backend = self
            .backend
            .take()
            .expect("Cannot start Utxo scanner service without setting a storage backend");
        let network = self.network;
        let birthday_offset = self.birthday_offset;
        let node_url = self.http_node_url.clone();
        let fallback_http_server_url = self.http_fallback_url.clone();
        let scanning_interval = self.scanning_interval;

        context.spawn_when_ready(move |handles| async move {
            let transaction_service = handles.expect_handle::<TransactionServiceHandle>();
            let output_manager_service = handles.expect_handle::<OutputManagerHandle>();
            let key_manager = handles.expect_handle::<TKeyManagerInterface>();

            let view_key = key_manager
                .get_view_key()
                .await
                .expect("Could not initialize UTXO scanner Service");
            let spend_key = key_manager
                .get_spend_key()
                .await
                .expect("Could not initialize UTXO scanner Service");
            let one_sided_tari_address = TariAddress::new_dual_address(
                view_key.pub_key,
                spend_key.pub_key,
                network,
                TariAddressFeatures::create_one_sided_only(),
                None,
            )
            .expect("Could not create one-sided Tari address");

            let scanning_service = UtxoScannerService::<T, TKeyManagerInterface, _>::builder()
                .with_client_factory(DefaultHttpClientFactory::new(node_url, fallback_http_server_url))
                .with_retry_limit(2)
                .with_mode(UtxoScannerMode::Scanning)
                .with_scanning_interval(scanning_interval)
                .build_with_resources::<T, TKeyManagerInterface>(
                    backend,
                    output_manager_service,
                    transaction_service,
                    one_sided_tari_address,
                    handles.get_shutdown_signal(),
                    event_sender,
                    birthday_offset,
                    key_manager,
                )
                .await
                .expect("Failed to build UTXO scanner service")
                .run();

            futures::pin_mut!(scanning_service);
            future::select(scanning_service, handles.get_shutdown_signal()).await;
            info!(target: LOG_TARGET, "Utxo scanner service shutdown");
        });
        Ok(())
    }
}

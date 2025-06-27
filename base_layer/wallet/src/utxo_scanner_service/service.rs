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

use std::sync::Arc;

use chrono::NaiveDateTime;
use futures::FutureExt;
use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{tari_address::TariAddress, types::HashOutput};
use tari_core::transactions::transaction_key_manager::TransactionKeyManagerInterface;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    sync::{broadcast, watch},
    task,
};
use url::Url;

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    client::http_client_factory::HttpClientFactory,
    error::WalletError,
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        handle::UtxoScannerEvent,
        utxo_scanner_task::UtxoScannerTask,
        uxto_scanner_service_builder::{UtxoScannerMode, UtxoScannerServiceBuilder},
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

// Cache 1 days worth of headers.
pub const SCANNED_BLOCK_CACHE_SIZE: u64 = 720;

pub struct UtxoScannerService<
    TBackend,
    TKeyManagerInterface,
    TWalletClientFactory: HttpClientFactory + Clone + Send + Sync + 'static,
> {
    pub(crate) resources: UtxoScannerResources<TBackend, TWalletClientFactory>,
    pub(crate) retry_limit: usize,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) base_node_service: BaseNodeServiceHandle,
    one_sided_message_watch: watch::Receiver<String>,
    recovery_message_watch: watch::Receiver<String>,
    pub(crate) key_manager: TKeyManagerInterface,
}

impl<TBackend, TKeyManagerInterface: Clone, TWalletClientFactory>
    UtxoScannerService<TBackend, TKeyManagerInterface, TWalletClientFactory>
where
    TBackend: WalletBackend + 'static,
    TKeyManagerInterface: TransactionKeyManagerInterface + Clone + Send + Sync + 'static,
    TWalletClientFactory: HttpClientFactory + Clone + Send + Sync + 'static,
{
    pub fn new(
        retry_limit: usize,
        mode: UtxoScannerMode,
        resources: UtxoScannerResources<TBackend, TWalletClientFactory>,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        base_node_service: BaseNodeServiceHandle,
        one_sided_message_watch: watch::Receiver<String>,
        recovery_message_watch: watch::Receiver<String>,
        key_manager: TKeyManagerInterface,
    ) -> Self {
        Self {
            resources,
            retry_limit,
            mode,
            shutdown_signal,
            event_sender,
            base_node_service,
            one_sided_message_watch,
            recovery_message_watch,
            key_manager,
        }
    }

    fn create_task(
        &self,
        shutdown_signal: ShutdownSignal,
    ) -> UtxoScannerTask<TBackend, TKeyManagerInterface, TWalletClientFactory> {
        UtxoScannerTask {
            resources: self.resources.clone(),
            event_sender: self.event_sender.clone(),
            retry_limit: self.retry_limit,
            num_retries: 1,
            mode: self.mode.clone(),
            shutdown_signal,
            birthday_offset: self.resources.birthday_offset,
            key_manager: self.key_manager.clone(),
        }
    }

    pub fn builder() -> UtxoScannerServiceBuilder<TWalletClientFactory> {
        UtxoScannerServiceBuilder::<TWalletClientFactory>::default()
    }

    pub fn get_event_receiver(&mut self) -> broadcast::Receiver<UtxoScannerEvent> {
        self.event_sender.subscribe()
    }

    pub async fn run(mut self) -> Result<(), WalletError> {
        info!(target: LOG_TARGET, "UTXO scanning service starting");

        if self.mode == UtxoScannerMode::Recovery {
            let task = self.create_task(self.shutdown_signal.clone());
            task::spawn(async move {
                if let Err(err) = task.run().await {
                    error!(target: LOG_TARGET, "Error scanning UTXOs: {}", err);
                }
            });
            return Ok(());
        }

        let mut main_shutdown = self.shutdown_signal.clone();
        let mut scanning_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        scanning_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
            _ = scanning_interval.tick() => {
                let local_shutdown = main_shutdown.clone();
                let task = self.create_task(local_shutdown);
                let task_join_handle = task::spawn(async move {
                    if let Err(err) = task.run().await {
                        error!(target: LOG_TARGET, "Error scanning UTXOs: {}", err);
                    }
                });

                info!(target: LOG_TARGET, "UTXO scanning round started");
                // Wait for the task to complete or shutdown signal
                match task_join_handle.await {
                    Ok(_) => {
                        debug!(target: LOG_TARGET, "UTXO scanning round completed successfully");
                    },
                    Err(e) => {
                        error!(target: LOG_TARGET, "UTXO scanning round failed: {}", e);
                    },
                }
            }
            _ = main_shutdown.wait() => {
                // this will stop the task if its running, and let that thread exit gracefully
                info!(target: LOG_TARGET, "UTXO scanning service shutting down because it received the shutdown signal");
                return Ok(());
            }
                         }
        }
    }
}

#[derive(Clone)]
pub struct UtxoScannerResources<TBackend, THttpClientFactory>
where THttpClientFactory: HttpClientFactory + Clone + Send + Sync + 'static
{
    pub(crate) db: WalletDatabase<TBackend>,
    pub(crate) output_manager_service: OutputManagerHandle,
    pub(crate) transaction_service: TransactionServiceHandle,
    pub(crate) one_sided_tari_address: TariAddress,
    pub(crate) recovery_message: String,
    pub(crate) one_sided_payment_message: String,
    pub(crate) birthday_offset: u16,
    pub(crate) client_factory: THttpClientFactory,
}

#[derive(Debug, Clone)]
pub struct ScannedBlock {
    pub header_hash: HashOutput,
    pub height: u64,
    pub timestamp: NaiveDateTime,
}

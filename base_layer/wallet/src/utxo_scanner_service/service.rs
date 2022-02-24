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
use tari_common_types::types::HashOutput;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::Peer, types::CommsPublicKey, NodeIdentity};
use tari_core::transactions::{tari_amount::MicroTari, CryptoFactories};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    sync::{broadcast, watch},
    task,
};

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
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
// TODO Determine a better strategy for maintaining a cache. Logarithmic sampling has been suggested but the problem
// with it is that as you move on to the next block you need to resample say a 100 headers where a simple window like
// this only samples 1 header per new block. A ticket has been added to the backlog to think about this #LOGGED
pub const SCANNED_BLOCK_CACHE_SIZE: u64 = 720;

pub struct UtxoScannerService<TBackend>
where TBackend: WalletBackend + 'static
{
    pub(crate) resources: UtxoScannerResources<TBackend>,
    pub(crate) retry_limit: usize,
    pub(crate) peer_seeds: Vec<CommsPublicKey>,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) base_node_service: BaseNodeServiceHandle,
}

impl<TBackend> UtxoScannerService<TBackend>
where TBackend: WalletBackend + 'static
{
    pub fn new(
        peer_seeds: Vec<CommsPublicKey>,
        retry_limit: usize,
        mode: UtxoScannerMode,
        resources: UtxoScannerResources<TBackend>,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        base_node_service: BaseNodeServiceHandle,
    ) -> Self {
        Self {
            resources,
            peer_seeds,
            retry_limit,
            mode,
            shutdown_signal,
            event_sender,
            base_node_service,
        }
    }

    fn create_task(&self, shutdown_signal: ShutdownSignal) -> UtxoScannerTask<TBackend> {
        UtxoScannerTask {
            resources: self.resources.clone(),
            peer_seeds: self.peer_seeds.clone(),
            event_sender: self.event_sender.clone(),
            retry_limit: self.retry_limit,
            peer_index: 0,
            num_retries: 1,
            mode: self.mode.clone(),
            shutdown_signal,
        }
    }

    pub fn builder() -> UtxoScannerServiceBuilder {
        UtxoScannerServiceBuilder::default()
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
        let mut base_node_service_event_stream = self.base_node_service.get_event_stream();

        loop {
            let mut local_shutdown = Shutdown::new();
            let task = self.create_task(local_shutdown.to_signal());
            let mut task_join_handle = task::spawn(async move {
                if let Err(err) = task.run().await {
                    error!(target: LOG_TARGET, "Error scanning UTXOs: {}", err);
                }
            })
            .fuse();

            loop {
                tokio::select! {
                    event = base_node_service_event_stream.recv() => {
                        match event {
                            Ok(e) => {
                                if let BaseNodeEvent::NewBlockDetected(h) = (*e).clone() {
                                        debug!(target: LOG_TARGET, "New block event received: {}", h);
                                        if local_shutdown.is_triggered() {
                                            debug!(target: LOG_TARGET, "Starting new round of UTXO scanning");
                                            break;
                                        }
                                }
                            },
                            Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {}", e),
                        };
                    },
                    _ = &mut task_join_handle => {
                        debug!(target: LOG_TARGET, "UTXO scanning round completed");
                        local_shutdown.trigger();
                    }
                    _ = self.resources.current_base_node_watcher.changed() => {
                        debug!(target: LOG_TARGET, "Base node change detected.");
                        let peer =  self.resources.current_base_node_watcher.borrow().as_ref().cloned();
                        if let Some(peer) = peer {
                            self.peer_seeds = vec![peer.public_key];
                        }
                        local_shutdown.trigger();
                    },
                    _ = main_shutdown.wait() => {
                        // this will stop the task if its running, and let that thread exit gracefully
                        local_shutdown.trigger();
                        info!(target: LOG_TARGET, "UTXO scanning service shutting down because it received the shutdown signal");
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct UtxoScannerResources<TBackend> {
    pub db: WalletDatabase<TBackend>,
    pub comms_connectivity: ConnectivityRequester,
    pub current_base_node_watcher: watch::Receiver<Option<Peer>>,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub node_identity: Arc<NodeIdentity>,
    pub factories: CryptoFactories,
}

#[derive(Debug, Clone)]
pub struct ScannedBlock {
    pub header_hash: HashOutput,
    pub height: u64,
    pub num_outputs: Option<u64>,
    pub amount: Option<MicroTari>,
    pub timestamp: NaiveDateTime,
}

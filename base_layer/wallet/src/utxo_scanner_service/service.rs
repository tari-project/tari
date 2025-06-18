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

use chrono::NaiveDateTime;
use futures::FutureExt;
use log::*;
use tari_common_types::{
    tari_address::TariAddress,
    types::{BlockHash, HashOutput},
};
use tari_comms::{connectivity::ConnectivityRequester, types::CommsPublicKey};
use tari_core::transactions::{tari_amount::MicroMinotari, CryptoFactories};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    sync::{broadcast, watch},
    task,
};

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::{BaseNodePeerManager, WalletConnectivityInterface},
    error::WalletError,
    output_manager_service::handle::OutputManagerHandle,
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        error::UtxoScannerError,
        handle::UtxoScannerEvent,
        utxo_scanner_task::UtxoScannerTask,
        uxto_scanner_service_builder::{UtxoScannerMode, UtxoScannerServiceBuilder},
        RECOVERY_KEY,
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

// Cache 1 days worth of headers.
pub const SCANNED_BLOCK_CACHE_SIZE: u64 = 720;

pub struct UtxoScannerService<TBackend, TWalletConnectivity> {
    pub(crate) resources: UtxoScannerResources<TBackend, TWalletConnectivity>,
    pub(crate) retry_limit: usize,
    pub(crate) peer_seeds: Vec<CommsPublicKey>,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) base_node_service: BaseNodeServiceHandle,
    block_tip_to_scan_to: Option<BlockHash>,
    last_block_tip_scanned: Option<BlockHash>,
    one_sided_message_watch: watch::Receiver<String>,
    recovery_message_watch: watch::Receiver<String>,
}

impl<TBackend, TWalletConnectivity> UtxoScannerService<TBackend, TWalletConnectivity>
where
    TBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        peer_seeds: Vec<CommsPublicKey>,
        retry_limit: usize,
        mode: UtxoScannerMode,
        resources: UtxoScannerResources<TBackend, TWalletConnectivity>,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        base_node_service: BaseNodeServiceHandle,
        one_sided_message_watch: watch::Receiver<String>,
        recovery_message_watch: watch::Receiver<String>,
    ) -> Self {
        debug!(target: LOG_TARGET, "{:?}: New scanning service created", mode);
        Self {
            resources,
            peer_seeds,
            retry_limit,
            mode,
            shutdown_signal,
            event_sender,
            base_node_service,
            block_tip_to_scan_to: None,
            last_block_tip_scanned: None,
            one_sided_message_watch,
            recovery_message_watch,
        }
    }

    fn create_task(&self, shutdown_signal: ShutdownSignal) -> UtxoScannerTask<TBackend, TWalletConnectivity> {
        UtxoScannerTask {
            resources: self.resources.clone(),
            peer_seeds: self.peer_seeds.clone(),
            event_sender: self.event_sender.clone(),
            retry_limit: self.retry_limit,
            peer_index: 0,
            num_retries: 1,
            mode: self.mode.clone(),
            shutdown_signal,
            birthday_offset: self.resources.birthday_offset,
        }
    }

    pub fn builder() -> UtxoScannerServiceBuilder {
        UtxoScannerServiceBuilder::default()
    }

    pub fn get_event_receiver(&mut self) -> broadcast::Receiver<UtxoScannerEvent> {
        self.event_sender.subscribe()
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run(mut self) -> Result<(), WalletError> {
        info!(target: LOG_TARGET, "{:?}: UTXO scanning service starting", self.mode);

        if self.mode == UtxoScannerMode::Recovery {
            let task = self.create_task(self.shutdown_signal.clone());
            task::spawn(async move {
                trace!(target: LOG_TARGET, "{:?}: Spawning new UTXO recovery task", self.mode);
                if let Err(err) = task.run().await {
                    error!(target: LOG_TARGET, "{:?}: Error scanning UTXOs: {}", self.mode, err);
                }
            });
            return Ok(());
        }

        let mut main_shutdown = self.shutdown_signal.clone();
        let mut base_node_service_event_stream = self.base_node_service.get_event_stream();

        loop {
            let mut local_shutdown = Shutdown::new();

            match self.resources.comms_connectivity.get_connectivity_status().await {
                Ok(status) if status.is_offline() => {
                    debug!(target: LOG_TARGET,
                     "{:?}: Comms connectivity is offline - waiting for connectivity.",
                     self.mode);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET,
                        "{:?}: Failed to query connectivity status: {} – retrying in 5 s",
                        self.mode, e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                },
                _ => {},
            }

            // If we have a base node connection, we can spawn a task to scanning UTXOs
            let task = self.create_task(local_shutdown.to_signal());
            let mode = self.mode.clone();
            let mut task_join_handle = task::spawn(async move {
                trace!(target: LOG_TARGET, "{:?}: Spawning new UTXO scanning task", mode);
                if let Err(err) = task.run().await {
                    error!(target: LOG_TARGET, "{:?}: Error scanning UTXOs: {}", mode, err);
                }
            })
            .fuse();

            // These events will change the scanning behaviour:
            // - Base node state changes will trigger a new scanning round if the block tip has changed and not yet
            //   scanned.
            // - A successfully completed scanning round will update the last scanned block tip.
            // - A newly established base node connection will reset the last and current scanned block tip states to
            //   force a new scanning round when the next base node state change is received.
            // - One-sided payment message changes will update the message used in one-sided payments.
            // - Recovery message changes will update the message used in recovery.
            // - Shutdown signal will stop the task if it is running, and let that thread exit gracefully.
            loop {
                tokio::select! {
                    event = base_node_service_event_stream.recv() => {
                        if self.check_recovery_mode()? {
                            debug!(
                                target: LOG_TARGET,
                                "{:?}: Ignoring base node events while recovery is in progress",
                                self.mode,
                            );
                            local_shutdown.trigger();
                            self.last_block_tip_scanned = None;
                            self.block_tip_to_scan_to = None;
                            continue;
                        }
                        match event {
                            Ok(e) => {
                                if let Some((hash, height)) = match *e {
                                    BaseNodeEvent::BaseNodeStateChanged(ref state) => state
                                        .chain_metadata
                                        .as_ref()
                                        .map(|metadata| (*metadata.best_block_hash(), metadata.best_block_height())),
                                    BaseNodeEvent::NewBlockDetected(hash, height) => Some((hash, height)),
                                } {
                                    debug!(
                                        target: LOG_TARGET,
                                        "{:?}: New base node data received: height: {}, hash: {}",
                                        self.mode, height, hash
                                    );
                                    if self.should_scan(hash) {
                                        self.block_tip_to_scan_to = Some(hash);
                                        local_shutdown.trigger();
                                        debug!(
                                            target: LOG_TARGET,
                                            "{:?}: Base node state changed, starting new round of UTXO scanning to \
                                            height: {}, hash: {}",
                                            self.mode, height, hash
                                        );
                                        // Trigger the scanning task to start a new round
                                        break;
                                    }
                                }
                            },
                            Err(e) => debug!(
                                target: LOG_TARGET,
                                "{:?}: Lagging read on base node event broadcast channel: {}",
                                self.mode, e
                            ),
                        };
                    },
                    _ = &mut task_join_handle => {
                        debug!(target: LOG_TARGET, "{:?}: UTXO scanning round completed", self.mode);
                        self.last_block_tip_scanned = self.block_tip_to_scan_to;
                        local_shutdown.trigger();
                    }
                    _ = self.resources.current_base_node_watcher.changed() => {
                        self.last_block_tip_scanned = None;
                        self.block_tip_to_scan_to = None;
                        debug!(target: LOG_TARGET, "{:?}: Base node change detected.", self.mode);
                        let selected_peer =  self.resources.current_base_node_watcher.borrow().as_ref().cloned();
                        if let Some(peer) = selected_peer {
                            self.peer_seeds = vec![peer.get_current_peer().public_key];
                        }
                        local_shutdown.trigger();
                    },
                    _ = main_shutdown.wait() => {
                        // This will stop the task if its running, and let that thread exit gracefully
                        local_shutdown.trigger();
                        info!(
                            target: LOG_TARGET,
                            "{:?}: UTXO scanning service shutting down because it received the shutdown signal",
                            self.mode
                        );
                        return Ok(());
                    }
                    Ok(_) = self.one_sided_message_watch.changed() => {
                            self.resources.one_sided_payment_message = (*self.one_sided_message_watch.borrow()).clone();
                    },
                    Ok(_) = self.recovery_message_watch.changed() => {
                            self.resources.recovery_message = (*self.recovery_message_watch.borrow()).clone();
                    },
                }
            }
        }
    }

    pub fn check_recovery_mode(&self) -> Result<bool, UtxoScannerError> {
        self.resources
            .db
            .get_client_key_from_str::<String>(RECOVERY_KEY.to_owned())
            .map(|x| x.is_some())
            .map_err(UtxoScannerError::from) // in case if `get_client_key_from_str` returns not exactly that type
    }

    fn should_scan(&self, new_hash: BlockHash) -> bool {
        let mut should_trigger_scanning = false;
        if let Some(last_block_tip_scanned) = self.last_block_tip_scanned {
            if let Some(block_tip_to_scan_to) = self.block_tip_to_scan_to {
                if last_block_tip_scanned != new_hash && block_tip_to_scan_to != new_hash {
                    should_trigger_scanning = true;
                }
            }
        } else if self.block_tip_to_scan_to.is_none() || self.block_tip_to_scan_to != Some(new_hash) {
            should_trigger_scanning = true;
        } else {
            // Nothing here
        }
        should_trigger_scanning
    }
}

#[derive(Clone)]
pub struct UtxoScannerResources<TBackend, TWalletConnectivity> {
    pub db: WalletDatabase<TBackend>,
    pub comms_connectivity: ConnectivityRequester,
    pub wallet_connectivity: TWalletConnectivity,
    pub current_base_node_watcher: watch::Receiver<Option<BaseNodePeerManager>>,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub one_sided_tari_address: TariAddress,
    pub factories: CryptoFactories,
    pub recovery_message: String,
    pub one_sided_payment_message: String,
    pub birthday_offset: u16,
}

#[derive(Debug, Clone)]
pub struct ScannedBlock {
    pub header_hash: HashOutput,
    pub height: u64,
    pub num_outputs: Option<u64>,
    pub amount: Option<MicroMinotari>,
    pub timestamp: NaiveDateTime,
}

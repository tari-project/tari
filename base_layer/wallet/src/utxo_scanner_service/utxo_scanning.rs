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

use std::{
    convert::TryFrom,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use chrono::Utc;
use futures::StreamExt;
use log::*;
use serde::{Deserialize, Serialize};
use tokio::{sync::broadcast, task, time};

use tari_common_types::{transaction::TxId, types::HashOutput};
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
    types::CommsPublicKey,
    NodeIdentity,
    PeerConnection,
};
use tari_core::{
    base_node::sync::rpc::BaseNodeSyncRpcClient,
    blocks::BlockHeader,
    crypto::tari_utilities::hex::Hex,
    proto,
    proto::base_node::{FindChainSplitRequest, SyncUtxosRequest},
    tari_utilities::Hashable,
    transactions::{
        tari_amount::MicroTari,
        transaction::{TransactionOutput, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_shutdown::ShutdownSignal;
use tracing::instrument;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    error::WalletError,
    output_manager_service::handle::OutputManagerHandle,
    storage::{
        database::{WalletBackend, WalletDatabase},
        sqlite_db::WalletSqliteDatabase,
    },
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{error::UtxoScannerError, handle::UtxoScannerEvent},
    WalletSqlite,
};
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::Peer};
use tokio::{sync::watch, time::MissedTickBehavior};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub const RECOVERY_KEY: &str = "recovery_data";
const SCANNING_KEY: &str = "scanning_data";

#[derive(Debug, Clone, PartialEq)]
pub enum UtxoScannerMode {
    Recovery,
    Scanning,
}

impl Default for UtxoScannerMode {
    fn default() -> UtxoScannerMode {
        UtxoScannerMode::Recovery
    }
}

#[derive(Debug, Default, Clone)]
pub struct UtxoScannerServiceBuilder {
    retry_limit: usize,
    peers: Vec<CommsPublicKey>,
    mode: Option<UtxoScannerMode>,
    scanning_interval: Option<Duration>,
}

#[derive(Clone)]
struct UtxoScannerResources<TBackend> {
    pub db: WalletDatabase<TBackend>,
    pub comms_connectivity: ConnectivityRequester,
    pub current_base_node_watcher: watch::Receiver<Option<Peer>>,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub node_identity: Arc<NodeIdentity>,
    pub factories: CryptoFactories,
}

impl UtxoScannerServiceBuilder {
    /// Set the maximum number of times we retry recovery. A failed recovery is counted as _all_ peers have failed.
    /// i.e. worst-case number of recovery attempts = number of sync peers * retry limit
    pub fn with_retry_limit(&mut self, limit: usize) -> &mut Self {
        self.retry_limit = limit;
        self
    }

    pub fn with_scanning_interval(&mut self, interval: Duration) -> &mut Self {
        self.scanning_interval = Some(interval);
        self
    }

    pub fn with_peers(&mut self, peer_public_keys: Vec<CommsPublicKey>) -> &mut Self {
        self.peers = peer_public_keys;
        self
    }

    pub fn with_mode(&mut self, mode: UtxoScannerMode) -> &mut Self {
        self.mode = Some(mode);
        self
    }

    pub fn build_with_wallet(
        &mut self,
        wallet: &WalletSqlite,
        shutdown_signal: ShutdownSignal,
    ) -> UtxoScannerService<WalletSqliteDatabase> {
        let resources = UtxoScannerResources {
            db: wallet.db.clone(),
            comms_connectivity: wallet.comms.connectivity(),
            current_base_node_watcher: wallet.wallet_connectivity.get_current_base_node_watcher(),
            output_manager_service: wallet.output_manager_service.clone(),
            transaction_service: wallet.transaction_service.clone(),
            node_identity: wallet.comms.node_identity(),
            factories: wallet.factories.clone(),
        };

        let (event_sender, _) = broadcast::channel(200);

        let interval = self
            .scanning_interval
            .unwrap_or_else(|| Duration::from_secs(60 * 60 * 12));
        UtxoScannerService::new(
            self.peers.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            interval,
            shutdown_signal,
            event_sender,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_with_resources<TBackend: WalletBackend + 'static>(
        &mut self,
        db: WalletDatabase<TBackend>,
        comms_connectivity: ConnectivityRequester,
        base_node_watcher: watch::Receiver<Option<Peer>>,
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
    ) -> UtxoScannerService<TBackend> {
        let resources = UtxoScannerResources {
            db,
            comms_connectivity,
            current_base_node_watcher: base_node_watcher,
            output_manager_service,
            transaction_service,
            node_identity,
            factories,
        };
        let interval = self
            .scanning_interval
            .unwrap_or_else(|| Duration::from_secs(60 * 60 * 12));
        UtxoScannerService::new(
            self.peers.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            interval,
            shutdown_signal,
            event_sender,
        )
    }
}

struct UtxoScannerTask<TBackend>
where TBackend: WalletBackend + 'static
{
    resources: UtxoScannerResources<TBackend>,
    event_sender: broadcast::Sender<UtxoScannerEvent>,
    retry_limit: usize,
    num_retries: usize,
    peer_seeds: Vec<CommsPublicKey>,
    peer_index: usize,
    mode: UtxoScannerMode,
    run_flag: Arc<AtomicBool>,
}
impl<TBackend> UtxoScannerTask<TBackend>
where TBackend: WalletBackend + 'static
{
    async fn finalize(
        &self,
        total_scanned: u64,
        final_utxo_pos: u64,
        elapsed: Duration,
    ) -> Result<(), UtxoScannerError> {
        let metadata = self.get_metadata().await?.unwrap_or_default();
        self.publish_event(UtxoScannerEvent::Progress {
            current_block: final_utxo_pos,
            current_chain_height: final_utxo_pos,
        });
        self.publish_event(UtxoScannerEvent::Completed {
            number_scanned: total_scanned,
            number_received: metadata.number_of_utxos,
            value_received: metadata.total_amount,
            time_taken: elapsed,
        });

        // Presence of scanning keys are used to determine if a wallet is busy with recovery or not.
        if self.mode == UtxoScannerMode::Recovery {
            self.clear_db().await?;
        }
        Ok(())
    }

    #[instrument(name = "utxo_scanner_task::connect_to_peer", skip(self, peer))]
    async fn connect_to_peer(&mut self, peer: NodeId) -> Result<PeerConnection, UtxoScannerError> {
        self.publish_event(UtxoScannerEvent::ConnectingToBaseNode(peer.clone()));
        debug!(
            target: LOG_TARGET,
            "Attempting UTXO sync with seed peer {} ({})", self.peer_index, peer,
        );
        match self.resources.comms_connectivity.dial_peer(peer.clone()).await {
            Ok(conn) => Ok(conn),
            Err(e) => {
                self.publish_event(UtxoScannerEvent::ConnectionFailedToBaseNode {
                    peer,
                    num_retries: self.num_retries,
                    retry_limit: self.retry_limit,
                    error: e.to_string(),
                });

                Err(e.into())
            },
        }
    }

    #[instrument(name = "utxo_scanner_task::attempt_sync", skip(self, peer))]
    async fn attempt_sync(&mut self, peer: NodeId) -> Result<(u64, u64, Duration), UtxoScannerError> {
        let mut connection = self.connect_to_peer(peer.clone()).await?;

        let mut client = connection
            .connect_rpc_using_builder(BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;

        let latency = client.get_last_request_latency().await?;
        self.publish_event(UtxoScannerEvent::ConnectedToBaseNode(
            peer.clone(),
            latency.unwrap_or_default(),
        ));

        let timer = Instant::now();
        let mut total_scanned = 0u64;
        loop {
            let start_index = self.get_start_utxo_mmr_pos(&mut client).await?;
            let tip_header = self.get_chain_tip_header(&mut client).await?;
            let output_mmr_size = tip_header.output_mmr_size;
            if !self.run_flag.load(Ordering::Relaxed) {
                // if running is set to false, we know its been canceled upstream so lets exit the loop
                return Ok((total_scanned, start_index, timer.elapsed()));
            }
            debug!(
                target: LOG_TARGET,
                "Scanning UTXO's (start_index = {}, output_mmr_size = {}, height = {}, tip_hash = {})",
                start_index,
                output_mmr_size,
                tip_header.height,
                tip_header.hash().to_hex()
            );
            // start_index could be greater than output_mmr_size if we switch to a new peer that is behind the original
            // peer. In the common case, we wait for start index.
            if start_index >= output_mmr_size - 1 {
                debug!(
                    target: LOG_TARGET,
                    "Scanning complete UTXO #{} in {:.2?}",
                    start_index,
                    timer.elapsed()
                );
                return Ok((total_scanned, start_index, timer.elapsed()));
            }

            let num_scanned = self.scan_utxos(&mut client, start_index, tip_header).await?;
            if num_scanned == 0 {
                return Err(UtxoScannerError::UtxoScanningError(
                    "Peer returned 0 UTXOs to scan".to_string(),
                ));
            }
            debug!(
                target: LOG_TARGET,
                "Scanning round completed UTXO #{} in {:.2?} ({} scanned)",
                output_mmr_size,
                timer.elapsed(),
                num_scanned
            );

            // let num_scanned = 0;
            total_scanned += num_scanned;
            // return Ok((total_scanned, start_index, timer.elapsed()));
        }
    }

    #[instrument(name = "utxo_scanner_task::get_chain_tip_header", skip(self, client))]
    async fn get_chain_tip_header(&self, client: &mut BaseNodeSyncRpcClient) -> Result<BlockHeader, UtxoScannerError> {
        let chain_metadata = client.get_chain_metadata().await?;
        let chain_height = chain_metadata.height_of_longest_chain();
        let end_header = client.get_header_by_height(chain_height).await?;
        let end_header = BlockHeader::try_from(end_header).map_err(|_| UtxoScannerError::ConversionError)?;

        Ok(end_header)
    }

    #[instrument(name = "utxo_scanner_task::get_start_utxo_mmr_pos", skip(self, client))]
    async fn get_start_utxo_mmr_pos(&self, client: &mut BaseNodeSyncRpcClient) -> Result<u64, UtxoScannerError> {
        let metadata = self.get_metadata().await?.unwrap_or_default();
        if metadata.height_hash.is_empty() {
            // Set a value in here so that if the recovery fails on the genesis block the client will know a
            // recover was started. Important on Console wallet that otherwise makes this decision based on the
            // presence of the data file
            self.set_metadata(metadata).await?;
            return Ok(0);
        }
        // if it's none, we return 0 above.
        let request = FindChainSplitRequest {
            block_hashes: vec![metadata.height_hash],
            header_count: 1,
        };
        // this returns the index of the vec of hashes we sent it, that is the last hash it knows of.
        match client.find_chain_split(request).await {
            Ok(_) => Ok(metadata.utxo_index + 1),
            Err(RpcError::RequestFailed(err)) if err.status_code().is_not_found() => {
                warn!(target: LOG_TARGET, "Reorg detected: {}", err);
                // The node does not know of the last hash we scanned, thus we had a chain split.
                // We now start at 0 again.
                Ok(0)
            },
            Err(err) => Err(err.into()),
        }
    }

    #[instrument(
        name = "utxo_scanner_task::scan_utxos",
        skip(self, client, start_mmr_leaf_index, end_header)
    )]
    async fn scan_utxos(
        &mut self,
        client: &mut BaseNodeSyncRpcClient,
        start_mmr_leaf_index: u64,
        end_header: BlockHeader,
    ) -> Result<u64, UtxoScannerError> {
        debug!(
            target: LOG_TARGET,
            "Scanning UTXO's from #{} to #{} (height {})",
            start_mmr_leaf_index,
            end_header.output_mmr_size,
            end_header.height
        );

        let end_header_hash = end_header.hash();
        let end_header_size = end_header.output_mmr_size;
        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let mut total_scanned = 0;

        self.publish_event(UtxoScannerEvent::Progress {
            current_block: start_mmr_leaf_index,
            current_chain_height: (end_header_size - 1),
        });
        let request = SyncUtxosRequest {
            start: start_mmr_leaf_index,
            end_header_hash: end_header_hash.clone(),
            include_pruned_utxos: false,
            include_deleted_bitmaps: false,
        };

        let utxo_stream = client.sync_utxos(request).await?;
        // We download in chunks just because rewind_outputs works with multiple outputs (and could parallelized
        // rewinding)
        let mut utxo_stream = utxo_stream.chunks(10);
        let mut last_utxo_index = 0u64;
        let mut iteration_count = 0u64;
        while let Some(response) = utxo_stream.next().await {
            if !self.run_flag.load(Ordering::Relaxed) {
                // if running is set to false, we know its been canceled upstream so lets exit the loop
                return Ok(total_scanned as u64);
            }
            let (outputs, utxo_index) = convert_response_to_transaction_outputs(response, last_utxo_index)?;
            last_utxo_index = utxo_index;
            total_scanned += outputs.len();
            iteration_count += 1;
            let found_outputs = self.scan_for_outputs(outputs).await?;

            // Reduce the number of db hits by only persisting progress every N iterations
            const COMMIT_EVERY_N: u64 = 100;
            if iteration_count % COMMIT_EVERY_N == 0 || last_utxo_index >= end_header_size - 1 {
                self.publish_event(UtxoScannerEvent::Progress {
                    current_block: last_utxo_index,
                    current_chain_height: (end_header_size - 1),
                });
                self.update_scanning_progress_in_db(
                    last_utxo_index,
                    total_amount,
                    num_recovered,
                    end_header_hash.clone(),
                )
                .await?;
            }
            let (count, amount) = self.import_utxos_to_transaction_service(found_outputs).await?;
            num_recovered = num_recovered.saturating_add(count);
            total_amount += amount;
        }
        self.update_scanning_progress_in_db(last_utxo_index, total_amount, num_recovered, end_header_hash)
            .await?;
        self.publish_event(UtxoScannerEvent::Progress {
            current_block: (end_header_size - 1),
            current_chain_height: (end_header_size - 1),
        });
        Ok(total_scanned as u64)
    }

    #[instrument(
        name = "utxo_scanner_task::update_scanning_progress_in_db",
        skip(self, last_utxo_index, total_amount, num_recovered, end_header_hash)
    )]
    async fn update_scanning_progress_in_db(
        &self,
        last_utxo_index: u64,
        total_amount: MicroTari,
        num_recovered: u64,
        end_header_hash: Vec<u8>,
    ) -> Result<(), UtxoScannerError> {
        let mut meta_data = self.get_metadata().await?.unwrap_or_default();
        meta_data.height_hash = end_header_hash;
        meta_data.number_of_utxos += num_recovered;
        meta_data.utxo_index = last_utxo_index;
        meta_data.total_amount += total_amount;

        self.set_metadata(meta_data).await?;
        Ok(())
    }

    #[instrument(name = "utxo_scanner_task::scan_for_outputs", skip(self, outputs))]
    async fn scan_for_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<(UnblindedOutput, String)>, UtxoScannerError> {
        let mut found_outputs: Vec<(UnblindedOutput, String)> = Vec::new();
        if self.mode == UtxoScannerMode::Recovery {
            found_outputs.append(
                &mut self
                    .resources
                    .output_manager_service
                    .scan_for_recoverable_outputs(outputs.clone())
                    .await?
                    .into_iter()
                    .map(|v| (v, format!("Recovered on {}.", Utc::now().naive_utc())))
                    .collect(),
            );
        };
        found_outputs.append(
            &mut self
                .resources
                .output_manager_service
                .scan_outputs_for_one_sided_payments(outputs.clone())
                .await?
                .into_iter()
                .map(|v| {
                    (
                        v,
                        format!("Detected one-sided transaction on {}.", Utc::now().naive_utc()),
                    )
                })
                .collect(),
        );
        Ok(found_outputs)
    }

    #[instrument(name = "utxo_scanner_task::import_utxos_to_transaction_service", skip(self, utxos))]
    async fn import_utxos_to_transaction_service(
        &mut self,
        utxos: Vec<(UnblindedOutput, String)>,
    ) -> Result<(u64, MicroTari), UtxoScannerError> {
        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let source_public_key = self.resources.node_identity.public_key().clone();

        for uo in utxos {
            match self
                .import_unblinded_utxo_to_transaction_service(uo.0.clone(), &source_public_key, uo.1)
                .await
            {
                Ok(_) => {
                    num_recovered = num_recovered.saturating_add(1);
                    total_amount += uo.0.value;
                },
                Err(e) => return Err(UtxoScannerError::UtxoImportError(e.to_string())),
            }
        }
        Ok((num_recovered, total_amount))
    }

    fn get_db_mode_key(&self) -> String {
        match self.mode {
            UtxoScannerMode::Recovery => RECOVERY_KEY.to_owned(),
            UtxoScannerMode::Scanning => SCANNING_KEY.to_owned(),
        }
    }

    #[instrument(name = "utxo_scanner_task::set_metadata", skip(self, data))]
    async fn set_metadata(&self, data: ScanningMetadata) -> Result<(), UtxoScannerError> {
        let total_key = self.get_db_mode_key();
        let db_value = serde_json::to_string(&data)?;
        self.resources.db.set_client_key_value(total_key, db_value).await?;
        Ok(())
    }

    #[instrument(name = "utxo_scanner_task::get_metadata", skip(self))]
    async fn get_metadata(&self) -> Result<Option<ScanningMetadata>, UtxoScannerError> {
        let total_key = self.get_db_mode_key();
        let value: Option<String> = self.resources.db.get_client_key_from_str(total_key).await?;
        match value {
            None => Ok(None),
            Some(v) => Ok(serde_json::from_str(&v)?),
        }
    }

    #[instrument(name = "utxo_scanner_task::clear_db", skip(self))]
    async fn clear_db(&self) -> Result<(), UtxoScannerError> {
        let total_key = self.get_db_mode_key();
        let _ = self.resources.db.clear_client_value(total_key).await?;
        Ok(())
    }

    fn publish_event(&self, event: UtxoScannerEvent) {
        let _ = self.event_sender.send(event);
    }

    /// A faux incoming transaction will be created to provide a record of the event of importing a UTXO. The TxId of
    /// the generated transaction is returned.
    #[instrument(
        name = "utxo_scanner_task::import_unblinded_utxo_to_transaction_service",
        skip(self, unblinded_output, source_public_key, message)
    )]
    pub async fn import_unblinded_utxo_to_transaction_service(
        &mut self,
        unblinded_output: UnblindedOutput,
        source_public_key: &CommsPublicKey,
        message: String,
    ) -> Result<TxId, WalletError> {
        let tx_id = self
            .resources
            .transaction_service
            .import_utxo(
                unblinded_output.value,
                source_public_key.clone(),
                message,
                Some(unblinded_output.features.maturity),
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet",
            unblinded_output
                .as_transaction_input(&self.resources.factories.commitment)?
                .commitment
                .to_hex()
        );

        Ok(tx_id)
    }

    async fn run(mut self) -> Result<(), UtxoScannerError> {
        self.run_flag.store(true, Ordering::Relaxed);
        loop {
            if !self.run_flag.load(Ordering::Relaxed) {
                // if running is set to false, we know its been canceled upstream so lets exit the loop
                return Ok(());
            }
            match self.get_next_peer() {
                Some(peer) => match self.attempt_sync(peer.clone()).await {
                    Ok((total_scanned, final_utxo_pos, elapsed)) => {
                        debug!(target: LOG_TARGET, "Scanned to UTXO #{}", final_utxo_pos);
                        self.finalize(total_scanned, final_utxo_pos, elapsed).await?;
                        return Ok(());
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to scan UTXO's from base node {}: {}", peer, e
                        );
                        self.publish_event(UtxoScannerEvent::ScanningRoundFailed {
                            num_retries: self.num_retries,
                            retry_limit: self.retry_limit,
                            error: e.to_string(),
                        });
                        continue;
                    },
                },
                None => {
                    self.publish_event(UtxoScannerEvent::ScanningRoundFailed {
                        num_retries: self.num_retries,
                        retry_limit: self.retry_limit,
                        error: "No new peers to try after this round".to_string(),
                    });

                    if self.num_retries >= self.retry_limit {
                        self.publish_event(UtxoScannerEvent::ScanningFailed);
                        return Err(UtxoScannerError::UtxoScanningError(format!(
                            "Failed to scan UTXO's after {} attempt(s) using all {} sync peer(s). Aborting...",
                            self.num_retries,
                            self.peer_seeds.len()
                        )));
                    }

                    self.num_retries += 1;
                    // Reset peer index to try connect to the first peer again
                    self.peer_index = 0;
                },
            }
        }
    }

    fn get_next_peer(&mut self) -> Option<NodeId> {
        let peer = self.peer_seeds.get(self.peer_index).map(NodeId::from_public_key);
        self.peer_index += 1;
        peer
    }
}

pub struct UtxoScannerService<TBackend>
where TBackend: WalletBackend + 'static
{
    resources: UtxoScannerResources<TBackend>,
    retry_limit: usize,
    peer_seeds: Vec<CommsPublicKey>,
    mode: UtxoScannerMode,
    is_running: Arc<AtomicBool>,
    scan_for_utxo_interval: Duration,
    shutdown_signal: ShutdownSignal,
    event_sender: broadcast::Sender<UtxoScannerEvent>,
}

impl<TBackend> UtxoScannerService<TBackend>
where TBackend: WalletBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    fn new(
        peer_seeds: Vec<CommsPublicKey>,
        retry_limit: usize,
        mode: UtxoScannerMode,
        resources: UtxoScannerResources<TBackend>,
        scan_for_utxo_interval: Duration,
        shutdown_signal: ShutdownSignal,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
    ) -> Self {
        Self {
            resources,
            peer_seeds,
            retry_limit,
            mode,
            is_running: Arc::new(AtomicBool::new(false)),
            scan_for_utxo_interval,
            shutdown_signal,
            event_sender,
        }
    }

    fn create_task(&self) -> UtxoScannerTask<TBackend> {
        UtxoScannerTask {
            resources: self.resources.clone(),
            peer_seeds: self.peer_seeds.clone(),
            event_sender: self.event_sender.clone(),
            retry_limit: self.retry_limit,
            peer_index: 0,
            num_retries: 1,
            mode: self.mode.clone(),
            run_flag: self.is_running.clone(),
        }
    }

    pub fn builder() -> UtxoScannerServiceBuilder {
        UtxoScannerServiceBuilder::default()
    }

    pub fn get_event_receiver(&mut self) -> broadcast::Receiver<UtxoScannerEvent> {
        self.event_sender.subscribe()
    }

    pub async fn run(mut self) -> Result<(), WalletError> {
        info!(
            target: LOG_TARGET,
            "UTXO scanning service starting (interval = {:.2?})", self.scan_for_utxo_interval
        );

        let mut shutdown = self.shutdown_signal.clone();
        let start_at = Instant::now() + Duration::from_secs(1);
        let mut work_interval = time::interval_at(start_at.into(), self.scan_for_utxo_interval);
        work_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = work_interval.tick() => {
                    let running_flag = self.is_running.clone();
                    if !running_flag.load(Ordering::SeqCst) {
                        let task = self.create_task();
                        debug!(target: LOG_TARGET, "UTXO scanning service starting scan for utxos");
                        task::spawn(async move {
                            if let Err(err) = task.run().await {
                                error!(target: LOG_TARGET, "Error scanning UTXOs: {}", err);
                            }
                            //we make sure the flag is set to false here
                            running_flag.store(false, Ordering::Relaxed);
                        });
                        if self.mode == UtxoScannerMode::Recovery {
                            return Ok(());
                        }
                    }
                },
                _ = self.resources.current_base_node_watcher.changed() => {
                    debug!(target: LOG_TARGET, "Base node change detected.");
                    let peer =  self.resources.current_base_node_watcher.borrow().as_ref().cloned();

                    // If we are recovering we will stick to the initially provided seeds
                    if self.mode != UtxoScannerMode::Recovery {
                        if let Some(peer) = peer {
                            self.peer_seeds = vec![peer.public_key];
                        }
                    }

                    self.is_running.store(false, Ordering::Relaxed);
                },
                _ = shutdown.wait() => {
                    // this will stop the task if its running, and let that thread exit gracefully
                    self.is_running.store(false, Ordering::Relaxed);
                    info!(target: LOG_TARGET, "UTXO scanning service shutting down because it received the shutdown signal");
                    return Ok(());
                }
            }
        }
    }
}

fn convert_response_to_transaction_outputs(
    response: Vec<Result<proto::base_node::SyncUtxosResponse, RpcStatus>>,
    last_utxo_index: u64,
) -> Result<(Vec<TransactionOutput>, u64), UtxoScannerError> {
    let response: Vec<proto::base_node::SyncUtxosResponse> = response
        .into_iter()
        .map(|v| v.map_err(|e| UtxoScannerError::RpcStatus(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    let current_utxo_index = response
    // Assumes correct ordering which is otherwise not required for this protocol
    .last()
    .ok_or_else(|| {
        UtxoScannerError::BaseNodeResponseError("Invalid response from base node: response was empty".to_string())
    })?
    .mmr_index;
    if current_utxo_index < last_utxo_index {
        return Err(UtxoScannerError::BaseNodeResponseError(
            "Invalid response from base node: mmr index must be non-decreasing".to_string(),
        ));
    }

    let outputs = response
        .into_iter()
        .filter_map(|utxo| {
            utxo.into_utxo()
                .and_then(|o| o.utxo)
                .and_then(|utxo| utxo.into_transaction_output())
                .map(|output| TransactionOutput::try_from(output).map_err(|_| UtxoScannerError::ConversionError))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((outputs, current_utxo_index))
}

#[derive(Default, Serialize, Deserialize)]
struct ScanningMetadata {
    pub total_amount: MicroTari,
    pub number_of_utxos: u64,
    pub utxo_index: u64,
    pub height_hash: HashOutput,
}

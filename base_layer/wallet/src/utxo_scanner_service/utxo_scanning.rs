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
    error::WalletError,
    output_manager_service::{handle::OutputManagerHandle, TxId},
    storage::{
        database::{WalletBackend, WalletDatabase},
        sqlite_db::WalletSqliteDatabase,
    },
    transaction_service::handle::TransactionServiceHandle,
    utxo_scanner_service::{
        error::UtxoScannerError,
        handle::{UtxoScannerEvent, UtxoScannerRequest, UtxoScannerResponse},
    },
    WalletSqlite,
};
use chrono::Utc;
use futures::{pin_mut, FutureExt, StreamExt};
use log::*;
use std::{
    convert::TryFrom,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
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
        types::CryptoFactories,
    },
};
use tari_crypto::tari_utilities::hex::*;
use tari_service_framework::{reply_channel, reply_channel::SenderService};
use tari_shutdown::ShutdownSignal;
use tokio::{sync::broadcast, time::delay_for};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub const RECOVERY_HEIGHT_KEY: &str = "recovery/height-progress";
const RECOVERY_NUM_UTXOS_KEY: &str = "recovery/num-utxos";
const RECOVERY_UTXO_INDEX_KEY: &str = "recovery/utxos-index";
const RECOVERY_TOTAL_AMOUNT_KEY: &str = "recovery/total-amount";
const SCANNING_HASH_KEY: &str = "scanning/hash";
const SCANNING_UTXO_INDEX_KEY: &str = "scanning/utxos-index";
const SCANNING_TOTAL_AMOUNT_KEY: &str = "scanning/total-amount";
const SCANNING_NUM_UTXOS_KEY: &str = "scanning/num-utxos";

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
    peer_seeds: Vec<CommsPublicKey>,
    mode: Option<UtxoScannerMode>,
    scanning_interval: Option<Duration>,
}

#[derive(Clone)]
struct UtxoScannerResources<TBackend>
where TBackend: WalletBackend + 'static
{
    pub db: WalletDatabase<TBackend>,
    pub connectivity: ConnectivityRequester,
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

    pub fn with_peer_seeds(&mut self, peer_seeds: Vec<CommsPublicKey>) -> &mut Self {
        self.peer_seeds = peer_seeds;
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
            connectivity: wallet.comms.connectivity(),
            output_manager_service: wallet.output_manager_service.clone(),
            transaction_service: wallet.transaction_service.clone(),
            node_identity: wallet.comms.node_identity(),
            factories: wallet.factories.clone(),
        };

        // When the Utxo Scanner is built using this method it is not going to run as a Service so we will pass in the
        // sender to be held by the service so that the receiver will not error when it is polled
        let (sender, receiver) = reply_channel::unbounded();
        let (event_sender, _) = broadcast::channel(200);

        let interval = self
            .scanning_interval
            .unwrap_or_else(|| Duration::from_secs(60 * 60 * 12));
        UtxoScannerService::new(
            self.peer_seeds.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            interval,
            shutdown_signal,
            receiver,
            event_sender,
            Some(sender),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_with_resources<TBackend: WalletBackend + 'static>(
        &mut self,
        db: WalletDatabase<TBackend>,
        connectivity: ConnectivityRequester,
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
        shutdown_signal: ShutdownSignal,
        request_stream: reply_channel::Receiver<UtxoScannerRequest, Result<UtxoScannerResponse, UtxoScannerError>>,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
    ) -> UtxoScannerService<TBackend> {
        let resources = UtxoScannerResources {
            db,
            connectivity,
            output_manager_service,
            transaction_service,
            node_identity,
            factories,
        };
        let interval = self
            .scanning_interval
            .unwrap_or_else(|| Duration::from_secs(60 * 60 * 12));
        UtxoScannerService::new(
            self.peer_seeds.drain(..).collect(),
            self.retry_limit,
            self.mode.clone().unwrap_or_default(),
            resources,
            interval,
            shutdown_signal,
            request_stream,
            event_sender,
            None,
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
        match self.mode {
            UtxoScannerMode::Recovery => {
                let num_recovered = self
                    .get_metadata(ScanningMetadataKey::RecoveryNumUtxos)
                    .await?
                    .unwrap_or(0);
                let total_amount = self
                    .resources
                    .db
                    .get_client_key_from_str(RECOVERY_TOTAL_AMOUNT_KEY.to_string())
                    .await?
                    .unwrap_or_else(|| 0.into());
                let height = self
                    .get_metadata(ScanningMetadataKey::RecoveryHeight)
                    .await?
                    .unwrap_or(0);
                self.set_metadata(ScanningMetadataKey::RecoveryHeight, height).await?;

                let _ = self
                    .resources
                    .db
                    .clear_client_value(RECOVERY_HEIGHT_KEY.to_string())
                    .await?;
                let _ = self
                    .resources
                    .db
                    .clear_client_value(RECOVERY_NUM_UTXOS_KEY.to_string())
                    .await?;
                let _ = self
                    .resources
                    .db
                    .clear_client_value(RECOVERY_TOTAL_AMOUNT_KEY.to_string())
                    .await?;
                self.publish_event(UtxoScannerEvent::Progress {
                    current_block: final_utxo_pos,
                    current_chain_height: final_utxo_pos,
                });
                self.publish_event(UtxoScannerEvent::Completed {
                    number_scanned: total_scanned,
                    number_received: num_recovered,
                    value_received: total_amount,
                    time_taken: elapsed,
                });
            },
            UtxoScannerMode::Scanning => {},
        }

        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: NodeId) -> Result<PeerConnection, UtxoScannerError> {
        self.publish_event(UtxoScannerEvent::ConnectingToBaseNode(peer.clone()));
        match self.resources.connectivity.dial_peer(peer.clone()).await {
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
            debug!(
                target: LOG_TARGET,
                "Scanning round completed UTXO #{} in {:.2?} ({} scanned)",
                output_mmr_size,
                timer.elapsed(),
                num_scanned
            );
            total_scanned += num_scanned;
        }
    }

    async fn get_chain_tip_header(&self, client: &mut BaseNodeSyncRpcClient) -> Result<BlockHeader, UtxoScannerError> {
        let chain_metadata = client.get_chain_metadata().await?;
        let chain_height = chain_metadata.height_of_longest_chain();
        let end_header = client.get_header_by_height(chain_height).await?;
        let end_header = BlockHeader::try_from(end_header).map_err(|_| UtxoScannerError::ConversionError)?;

        Ok(end_header)
    }

    async fn get_start_utxo_mmr_pos(&self, client: &mut BaseNodeSyncRpcClient) -> Result<u64, UtxoScannerError> {
        match self.mode {
            UtxoScannerMode::Recovery => {
                let previous_sync_height = self
                    .get_metadata::<u64>(ScanningMetadataKey::RecoveryHeight)
                    .await
                    .ok()
                    .flatten();
                let previous_utxo_index = self
                    .get_metadata::<u64>(ScanningMetadataKey::RecoveryUtxoIndex)
                    .await
                    .ok()
                    .flatten();

                if previous_sync_height.is_none() || previous_utxo_index.is_none() {
                    // Set a value in here so that if the recovery fails on the genesis block the client will know a
                    // recover was started. Important on Console wallet that otherwise makes this decision based on the
                    // presence of the data file
                    self.set_metadata(ScanningMetadataKey::RecoveryHeight, 0u64).await?;
                    self.set_metadata(ScanningMetadataKey::RecoveryUtxoIndex, 0u64).await?;
                }

                Ok(previous_utxo_index.unwrap_or(0u64))
            },
            UtxoScannerMode::Scanning => {
                let previous_scan_hash = self
                    .get_metadata::<String>(ScanningMetadataKey::ScanningHash)
                    .await
                    .ok()
                    .flatten();
                let previous_utxo_index = self
                    .get_metadata::<u64>(ScanningMetadataKey::ScanningUtxoIndex)
                    .await
                    .ok()
                    .flatten();

                if previous_utxo_index.is_none() || previous_scan_hash.is_none() {
                    // Set a value in here so that if the recovery fails on the genesis block the client will know a
                    // recover was started. Important on Console wallet that otherwise makes this decision based on the
                    // presence of the data file
                    self.set_metadata(ScanningMetadataKey::ScanningUtxoIndex, 0u64).await?;
                    let _ = self
                        .resources
                        .db
                        .clear_client_value(SCANNING_HASH_KEY.to_string())
                        .await?;
                    return Ok(0);
                }
                // if it's none, we return 0 above.
                let hash: Vec<u8> = from_hex(&previous_scan_hash.unwrap())?;
                let request = FindChainSplitRequest {
                    block_hashes: vec![hash],
                    header_count: 1,
                };
                let resp = client.find_chain_split(request).await?;
                if resp.fork_hash_index != 0 {
                    // we had a fork, lets calc a new sync height
                    return Ok(resp.headers[0]
                        .output_mmr_size
                        .saturating_sub(previous_utxo_index.unwrap()));
                }

                // If its none, we return 0 above
                Ok(previous_utxo_index.unwrap())
            },
        }
    }

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
            end_header_hash,
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
            last_utxo_index = current_utxo_index;

            let outputs = response
                .into_iter()
                .filter_map(|utxo| {
                    utxo.into_utxo()
                        .and_then(|o| o.utxo)
                        .and_then(|utxo| utxo.into_transaction_output())
                        .map(|output| {
                            TransactionOutput::try_from(output).map_err(|_| UtxoScannerError::ConversionError)
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;

            total_scanned += outputs.len();
            // Reduce the number of db hits by only persisting progress every N iterations
            const COMMIT_EVERY_N: u64 = 100;
            if iteration_count % COMMIT_EVERY_N == 0 || current_utxo_index >= end_header_size - 1 {
                self.publish_event(UtxoScannerEvent::Progress {
                    current_block: current_utxo_index,
                    current_chain_height: (end_header_size - 1),
                });
                match self.mode {
                    UtxoScannerMode::Recovery => {
                        self.set_metadata(ScanningMetadataKey::RecoveryUtxoIndex, current_utxo_index)
                            .await?
                    },
                    UtxoScannerMode::Scanning => {
                        self.set_metadata(ScanningMetadataKey::ScanningUtxoIndex, current_utxo_index)
                            .await?
                    },
                };
            }
            // ToDo fix this,m this should come from the syncing node.
            let height = 0;
            iteration_count += 1;
            let (standard_outputs, one_sided_outputs) = match self.mode {
                UtxoScannerMode::Recovery => {
                    let standard_outputs = self
                        .resources
                        .output_manager_service
                        .scan_for_recoverable_outputs(outputs.clone(), height)
                        .await?;
                    let one_sided_outputs = self
                        .resources
                        .output_manager_service
                        .scan_outputs_for_one_sided_payments(outputs.clone(), height)
                        .await?;

                    (standard_outputs, one_sided_outputs)
                },
                UtxoScannerMode::Scanning => (
                    vec![],
                    self.resources
                        .output_manager_service
                        .scan_outputs_for_one_sided_payments(outputs.clone(), height)
                        .await?,
                ),
            };
            if standard_outputs.is_empty() && one_sided_outputs.is_empty() {
                continue;
            }

            let source_public_key = self.resources.node_identity.public_key().clone();

            for uo in standard_outputs {
                match self
                    .import_unblinded_utxo_to_transaction_service(
                        uo.clone(),
                        &source_public_key,
                        format!("Recovered on {}.", Utc::now().naive_utc()),
                    )
                    .await
                {
                    Ok(_) => {
                        num_recovered = num_recovered.saturating_add(1);
                        total_amount += uo.value;
                    },
                    Err(e) => return Err(UtxoScannerError::UtxoImportError(e.to_string())),
                }
            }

            for uo in one_sided_outputs {
                match self
                    .import_unblinded_utxo_to_transaction_service(
                        uo.clone(),
                        &source_public_key,
                        format!("Detected one-sided transaction on {}.", Utc::now().naive_utc()),
                    )
                    .await
                {
                    Ok(_) => {
                        num_recovered = num_recovered.saturating_add(1);
                        total_amount += uo.value;
                    },
                    Err(e) => return Err(UtxoScannerError::UtxoImportError(e.to_string())),
                }
            }
        }

        match self.mode {
            UtxoScannerMode::Recovery => {
                self.set_metadata(ScanningMetadataKey::RecoveryHeight, end_header.height)
                    .await?;

                let current_num_utxos = self
                    .get_metadata(ScanningMetadataKey::RecoveryNumUtxos)
                    .await?
                    .unwrap_or(0u64);
                self.set_metadata(
                    ScanningMetadataKey::RecoveryNumUtxos,
                    (current_num_utxos + num_recovered).to_string(),
                )
                .await?;

                let current_total_amount = self
                    .get_metadata::<MicroTari>(ScanningMetadataKey::RecoveryTotalAmount)
                    .await?
                    .unwrap_or_else(|| 0.into());

                self.set_metadata(ScanningMetadataKey::RecoveryUtxoIndex, last_utxo_index)
                    .await?;
                self.set_metadata(
                    ScanningMetadataKey::RecoveryTotalAmount,
                    (current_total_amount + total_amount).as_u64().to_string(),
                )
                .await?;

                self.publish_event(UtxoScannerEvent::Progress {
                    current_block: (end_header_size - 1),
                    current_chain_height: (end_header_size - 1),
                });
            },
            UtxoScannerMode::Scanning => {
                self.set_metadata(ScanningMetadataKey::ScanningHash, end_header.hash().to_hex())
                    .await?;
                let current_num_utxos = self
                    .get_metadata(ScanningMetadataKey::ScanningNumUtxos)
                    .await?
                    .unwrap_or(0u64);
                self.set_metadata(
                    ScanningMetadataKey::ScanningNumUtxos,
                    (current_num_utxos + num_recovered).to_string(),
                )
                .await?;

                let current_total_amount = self
                    .get_metadata::<MicroTari>(ScanningMetadataKey::ScanningTotalAmount)
                    .await?
                    .unwrap_or_else(|| 0.into());

                self.set_metadata(ScanningMetadataKey::ScanningUtxoIndex, last_utxo_index)
                    .await?;
                self.set_metadata(
                    ScanningMetadataKey::ScanningTotalAmount,
                    (current_total_amount + total_amount).as_u64().to_string(),
                )
                .await?;
            },
        };

        Ok(total_scanned as u64)
    }

    async fn set_metadata<T: ToString>(&self, key: ScanningMetadataKey, value: T) -> Result<(), UtxoScannerError> {
        self.resources
            .db
            .set_client_key_value(key.as_key_str().to_string(), value.to_string())
            .await?;
        Ok(())
    }

    async fn get_metadata<T>(&self, key: ScanningMetadataKey) -> Result<Option<T>, UtxoScannerError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        let value = self
            .resources
            .db
            .get_client_key_from_str(key.as_key_str().to_string())
            .await?;
        Ok(value)
    }

    fn publish_event(&self, event: UtxoScannerEvent) {
        let _ = self.event_sender.send(event);
    }

    /// A faux incoming transaction will be created to provide a record of the event of importing a UTXO. The TxId of
    /// the generated transaction is returned.
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
                        debug!(target: LOG_TARGET, "Scanning to UTXO #{}", final_utxo_pos);
                        self.finalize(total_scanned, final_utxo_pos, elapsed).await?;
                        return Ok(());
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to scan UTXO's from base node {}: {}", peer, e
                        );

                        continue;
                    },
                },
                None => {
                    self.publish_event(UtxoScannerEvent::ScanningRoundFailed {
                        num_retries: self.num_retries,
                        retry_limit: self.retry_limit,
                    });

                    if self.num_retries >= self.retry_limit {
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
    request_stream: Option<reply_channel::Receiver<UtxoScannerRequest, Result<UtxoScannerResponse, UtxoScannerError>>>,
    event_sender: broadcast::Sender<UtxoScannerEvent>,
    _request_stream_sender_holder:
        Option<SenderService<UtxoScannerRequest, Result<UtxoScannerResponse, UtxoScannerError>>>,
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
        request_stream: reply_channel::Receiver<UtxoScannerRequest, Result<UtxoScannerResponse, UtxoScannerError>>,
        event_sender: broadcast::Sender<UtxoScannerEvent>,
        _request_stream_sender_holder: Option<
            SenderService<UtxoScannerRequest, Result<UtxoScannerResponse, UtxoScannerError>>,
        >,
    ) -> Self {
        Self {
            resources,
            peer_seeds,
            retry_limit,
            mode,
            is_running: Arc::new(AtomicBool::new(false)),
            scan_for_utxo_interval,
            shutdown_signal,
            request_stream: Some(request_stream),
            event_sender,
            _request_stream_sender_holder,
        }
    }

    fn create_task(&self) -> UtxoScannerTask<TBackend> {
        UtxoScannerTask {
            resources: self.resources.clone(),
            peer_seeds: self.peer_seeds.clone(),
            event_sender: self.event_sender.clone(),
            retry_limit: self.retry_limit,
            peer_index: 0,
            num_retries: 0,
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
        info!(target: LOG_TARGET, "UTXO scanning service starting");

        let request_stream = self
            .request_stream
            .take()
            .expect("UTXO Scanner Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown = self.shutdown_signal.clone();
        let mut delay_time = Duration::from_secs(1);
        loop {
            let mut work_interval = delay_for(delay_time).fuse();

            futures::select! {
            _ = work_interval => {
                debug!(target: LOG_TARGET, "UTXO scanning service starting scan for utxos");
                let task = self.create_task();
                let running_flag = self.is_running.clone();
                tokio::task::spawn(async move {
                    let _ = task.run().await;
                    //we make sure the flag is set to false here
                    running_flag.store(false, Ordering::Relaxed);
                });
                delay_time = self.scan_for_utxo_interval;
                },
            request_context = request_stream.select_next_some() => {
                trace!(target: LOG_TARGET, "Handling Service API Request");
                let (request, reply_tx) = request_context.split();
                let response = self.handle_request(request).await.map_err(|e| {
                    warn!(target: LOG_TARGET, "Error handling request: {:?}", e);
                    e
                });
                let _ = reply_tx.send(response).map_err(|e| {
                    warn!(target: LOG_TARGET, "Failed to send reply");
                    e
                });
            },
             _ = shutdown => {
                 // this will stop the task if its running, and let that thread exit gracefully
                 self.is_running.store(false, Ordering::Relaxed);
                info!(target: LOG_TARGET, "UTXO scanning service shutting down because it received the shutdown signal");
                return Ok(());
                }
            }
            if self.mode == UtxoScannerMode::Recovery {
                return Ok(());
            };
        }
    }

    async fn handle_request(&mut self, request: UtxoScannerRequest) -> Result<UtxoScannerResponse, UtxoScannerError> {
        trace!(target: LOG_TARGET, "Handling Service Request: {:?}", request);
        match request {
            UtxoScannerRequest::SetBaseNodePublicKey(pk) => {
                self.is_running.store(false, Ordering::Relaxed);
                self.peer_seeds = vec![pk];
                Ok(UtxoScannerResponse::BaseNodePublicKeySet)
            },
        }
    }
}

#[derive(Debug, Clone)]
enum ScanningMetadataKey {
    RecoveryTotalAmount,
    RecoveryNumUtxos,
    RecoveryUtxoIndex,
    RecoveryHeight,
    ScanningHash,
    ScanningUtxoIndex,
    ScanningNumUtxos,
    ScanningTotalAmount,
}

impl ScanningMetadataKey {
    pub fn as_key_str(&self) -> &'static str {
        use ScanningMetadataKey::*;
        match self {
            RecoveryTotalAmount => RECOVERY_TOTAL_AMOUNT_KEY,
            RecoveryNumUtxos => RECOVERY_NUM_UTXOS_KEY,
            RecoveryUtxoIndex => RECOVERY_UTXO_INDEX_KEY,
            RecoveryHeight => RECOVERY_HEIGHT_KEY,
            ScanningHash => SCANNING_HASH_KEY,
            ScanningUtxoIndex => SCANNING_UTXO_INDEX_KEY,
            ScanningNumUtxos => SCANNING_NUM_UTXOS_KEY,
            ScanningTotalAmount => SCANNING_TOTAL_AMOUNT_KEY,
        }
    }
}

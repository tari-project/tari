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
    time::{Duration, Instant},
};

use chrono::Utc;
use futures::StreamExt;
use log::*;
use tari_common_types::transaction::TxId;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer},
    protocol::rpc::{RpcError, RpcStatus},
    types::CommsPublicKey,
    PeerConnection,
};
use tari_core::{
    base_node::sync::rpc::BaseNodeSyncRpcClient,
    blocks::BlockHeader,
    proto,
    proto::base_node::{FindChainSplitRequest, SyncUtxosRequest},
    transactions::{
        tari_amount::MicroTari,
        transaction::{TransactionOutput, UnblindedOutput},
    },
};
use tari_shutdown::ShutdownSignal;
use tokio::{sync::broadcast, time};

use crate::{
    error::WalletError,
    storage::database::WalletBackend,
    utxo_scanner_service::{
        error::UtxoScannerError,
        handle::UtxoScannerEvent,
        service::{ScanningMetadata, UtxoScannerResources},
        uxto_scanner_service_builder::UtxoScannerMode,
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub const RECOVERY_KEY: &str = "recovery_data";
const SCANNING_KEY: &str = "scanning_data";

pub struct UtxoScannerTask<TBackend>
where TBackend: WalletBackend + 'static
{
    pub(crate) resources: UtxoScannerResources<TBackend>,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) retry_limit: usize,
    pub(crate) num_retries: usize,
    pub(crate) peer_seeds: Vec<CommsPublicKey>,
    pub(crate) peer_index: usize,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
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
            current_index: final_utxo_pos,
            total_index: final_utxo_pos,
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
                    peer: peer.clone(),
                    num_retries: self.num_retries,
                    retry_limit: self.retry_limit,
                    error: e.to_string(),
                });
                // No use re-dialing a peer that is not responsive for recovery mode
                if self.mode == UtxoScannerMode::Recovery {
                    if let Ok(Some(connection)) = self.resources.comms_connectivity.get_connection(peer.clone()).await {
                        if connection.clone().disconnect().await.is_ok() {
                            debug!(target: LOG_TARGET, "Disconnected base node peer {}", peer);
                        }
                    };
                    let _ = time::sleep(Duration::from_secs(30));
                }

                Err(e.into())
            },
        }
    }

    async fn attempt_sync(&mut self, peer: NodeId) -> Result<(u64, u64, Duration), UtxoScannerError> {
        let mut connection = self.connect_to_peer(peer.clone()).await?;

        let mut client = connection
            .connect_rpc_using_builder(BaseNodeSyncRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;

        let latency = client.get_last_request_latency();
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
            if self.shutdown_signal.is_triggered() {
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

    async fn get_chain_tip_header(&self, client: &mut BaseNodeSyncRpcClient) -> Result<BlockHeader, UtxoScannerError> {
        let chain_metadata = client.get_chain_metadata().await?;
        let chain_height = chain_metadata.height_of_longest_chain();
        let end_header = client.get_header_by_height(chain_height).await?;
        let end_header = BlockHeader::try_from(end_header).map_err(|_| UtxoScannerError::ConversionError)?;

        Ok(end_header)
    }

    async fn get_start_utxo_mmr_pos(&self, client: &mut BaseNodeSyncRpcClient) -> Result<u64, UtxoScannerError> {
        let metadata = match self.get_metadata().await? {
            None => {
                let birthday_metadata = self.get_birthday_metadata(client).await?;
                self.set_metadata(birthday_metadata.clone()).await?;
                return Ok(birthday_metadata.utxo_index);
            },
            Some(m) => m,
        };

        // if it's none, we return 0 above.
        let request = FindChainSplitRequest {
            block_hashes: vec![metadata.height_hash],
            header_count: 1,
        };
        // this returns the index of the vec of hashes we sent it, that is the last hash it knows of.
        match client.find_chain_split(request).await {
            Ok(_) => Ok(metadata.utxo_index + 1),
            Err(RpcError::RequestFailed(err)) if err.as_status_code().is_not_found() => {
                warn!(target: LOG_TARGET, "Reorg detected: {}", err);
                // The node does not know of the last hash we scanned, thus we had a chain split.
                // We now start at the wallet birthday again
                let birthday_metdadata = self.get_birthday_metadata(client).await?;
                Ok(birthday_metdadata.utxo_index)
            },
            Err(err) => Err(err.into()),
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
        let output_mmr_size = end_header.output_mmr_size;
        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let mut total_scanned = 0;

        self.publish_event(UtxoScannerEvent::Progress {
            current_index: start_mmr_leaf_index,
            total_index: (output_mmr_size - 1),
        });
        let request = SyncUtxosRequest {
            start: start_mmr_leaf_index,
            end_header_hash: end_header_hash.clone(),
            include_pruned_utxos: false,
            include_deleted_bitmaps: false,
        };

        let start = Instant::now();
        let utxo_stream = client.sync_utxos(request).await?;
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - UTXO stream request time {} ms",
            start.elapsed().as_millis(),
        );

        // We download in chunks for improved streaming efficiency
        const CHUNK_SIZE: usize = 125;
        let mut utxo_stream = utxo_stream.chunks(CHUNK_SIZE);
        const COMMIT_EVERY_N: u64 = (1000_i64 / CHUNK_SIZE as i64) as u64;
        let mut last_utxo_index = 0u64;
        let mut iteration_count = 0u64;
        let mut utxo_next_await_profiling = Vec::new();
        let mut scan_for_outputs_profiling = Vec::new();
        while let Some(response) = {
            let start = Instant::now();
            let utxo_stream_next = utxo_stream.next().await;
            utxo_next_await_profiling.push(start.elapsed());
            utxo_stream_next
        } {
            if self.shutdown_signal.is_triggered() {
                // if running is set to false, we know its been canceled upstream so lets exit the loop
                return Ok(total_scanned as u64);
            }
            let (outputs, utxo_index) = convert_response_to_transaction_outputs(response, last_utxo_index)?;
            last_utxo_index = utxo_index;
            total_scanned += outputs.len();
            iteration_count += 1;

            let start = Instant::now();
            let found_outputs = self.scan_for_outputs(outputs).await?;
            scan_for_outputs_profiling.push(start.elapsed());

            // Reduce the number of db hits by only persisting progress every N iterations
            if iteration_count % COMMIT_EVERY_N == 0 || last_utxo_index >= output_mmr_size - 1 {
                self.publish_event(UtxoScannerEvent::Progress {
                    current_index: last_utxo_index,
                    total_index: (output_mmr_size - 1),
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
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - streamed {} outputs in {} ms",
            total_scanned,
            utxo_next_await_profiling.iter().fold(0, |acc, &x| acc + x.as_millis()),
        );
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - scanned {} outputs in {} ms",
            total_scanned,
            scan_for_outputs_profiling.iter().fold(0, |acc, &x| acc + x.as_millis()),
        );
        self.update_scanning_progress_in_db(last_utxo_index, total_amount, num_recovered, end_header_hash)
            .await?;
        self.publish_event(UtxoScannerEvent::Progress {
            current_index: (output_mmr_size - 1),
            total_index: (output_mmr_size - 1),
        });
        Ok(total_scanned as u64)
    }

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

    async fn set_metadata(&self, data: ScanningMetadata) -> Result<(), UtxoScannerError> {
        let total_key = self.get_db_mode_key();
        let db_value = serde_json::to_string(&data)?;
        self.resources.db.set_client_key_value(total_key, db_value).await?;
        Ok(())
    }

    async fn get_metadata(&self) -> Result<Option<ScanningMetadata>, UtxoScannerError> {
        let total_key = self.get_db_mode_key();
        let value: Option<String> = self.resources.db.get_client_key_from_str(total_key).await?;
        match value {
            None => Ok(None),
            Some(v) => Ok(serde_json::from_str(&v)?),
        }
    }

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

    pub async fn run(mut self) -> Result<(), UtxoScannerError> {
        loop {
            if self.shutdown_signal.is_triggered() {
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

    async fn get_birthday_metadata(
        &self,
        client: &mut BaseNodeSyncRpcClient,
    ) -> Result<ScanningMetadata, UtxoScannerError> {
        let birthday = self.resources.db.get_wallet_birthday().await?;
        // Calculate the unix epoch time of two days before the wallet birthday. This is to avoid any weird time zone
        // issues
        let epoch_time = (birthday.saturating_sub(2) as u64) * 60 * 60 * 24;
        let block_height = match client.get_height_at_time(epoch_time).await {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Problem requesting `height_at_time` from Base Node: {}", e
                );
                0
            },
        };
        let header = client.get_header_by_height(block_height).await?;
        let header = BlockHeader::try_from(header).map_err(|_| UtxoScannerError::ConversionError)?;

        info!(
            target: LOG_TARGET,
            "Fresh wallet recovery starting at Block {}", block_height
        );
        Ok(ScanningMetadata {
            total_amount: Default::default(),
            number_of_utxos: 0,
            utxo_index: header.output_mmr_size,
            height_hash: header.hash(),
        })
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

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
use tari_common_types::{
    transaction::{ImportStatus, TxId},
    types::HashOutput,
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcClient,
    blocks::BlockHeader,
    proto::base_node::SyncUtxosByBlockRequest,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{TransactionOutput, UnblindedOutput},
    },
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, Hashable};
use tokio::sync::broadcast;

use crate::{
    error::WalletError,
    storage::database::WalletBackend,
    transaction_service::error::{TransactionServiceError, TransactionStorageError},
    utxo_scanner_service::{
        error::UtxoScannerError,
        handle::UtxoScannerEvent,
        service::{ScannedBlock, UtxoScannerResources, SCANNED_BLOCK_CACHE_SIZE},
        uxto_scanner_service_builder::UtxoScannerMode,
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub const RECOVERY_KEY: &str = "recovery_data";

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
    pub async fn run(mut self) -> Result<(), UtxoScannerError> {
        if self.mode == UtxoScannerMode::Recovery {
            self.set_recovery_mode().await?;
        } else if self.check_recovery_mode().await? {
            warn!(
                target: LOG_TARGET,
                "Scanning round aborted as a Recovery is in progress"
            );
            return Ok(());
        }

        loop {
            if self.shutdown_signal.is_triggered() {
                return Ok(());
            }
            match self.get_next_peer() {
                Some(peer) => match self.attempt_sync(peer.clone()).await {
                    Ok((num_outputs_recovered, final_height, final_amount, elapsed)) => {
                        debug!(target: LOG_TARGET, "Scanned to height #{}", final_height);
                        self.finalize(num_outputs_recovered, final_height, final_amount, elapsed)
                            .await?;
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

    async fn finalize(
        &self,
        num_outputs_recovered: u64,
        final_height: u64,
        total_value: MicroTari,
        elapsed: Duration,
    ) -> Result<(), UtxoScannerError> {
        self.publish_event(UtxoScannerEvent::Progress {
            current_height: final_height,
            tip_height: final_height,
        });
        self.publish_event(UtxoScannerEvent::Completed {
            final_height,
            num_recovered: num_outputs_recovered,
            value_recovered: total_value,
            time_taken: elapsed,
        });

        // Presence of scanning keys are used to determine if a wallet is busy with recovery or not.
        if self.mode == UtxoScannerMode::Recovery {
            self.clear_recovery_mode().await?;
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

                if let Ok(Some(connection)) = self.resources.comms_connectivity.get_connection(peer.clone()).await {
                    if connection.clone().disconnect().await.is_ok() {
                        debug!(target: LOG_TARGET, "Disconnected base node peer {}", peer);
                    }
                }

                Err(e.into())
            },
        }
    }

    async fn attempt_sync(&mut self, peer: NodeId) -> Result<(u64, u64, MicroTari, Duration), UtxoScannerError> {
        let mut connection = self.connect_to_peer(peer.clone()).await?;

        let mut client = connection
            .connect_rpc_using_builder(BaseNodeWalletRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;

        let latency = client.get_last_request_latency();
        self.publish_event(UtxoScannerEvent::ConnectedToBaseNode(
            peer.clone(),
            latency.unwrap_or_default(),
        ));

        let timer = Instant::now();

        loop {
            let tip_header = self.get_chain_tip_header(&mut client).await?;
            let tip_header_hash = tip_header.hash();
            let start_block = self.get_start_scanned_block(tip_header.height, &mut client).await?;

            if self.shutdown_signal.is_triggered() {
                return Ok((
                    start_block.num_outputs.unwrap_or(0),
                    start_block.height,
                    start_block.amount.unwrap_or_else(|| MicroTari::from(0)),
                    timer.elapsed(),
                ));
            }
            debug!(
                target: LOG_TARGET,
                "Scanning UTXO's from height = {} to current tip_height = {} (starting header_hash: {})",
                start_block.height,
                tip_header.height,
                start_block.header_hash.to_hex(),
            );

            // If we have scanned to the tip we are done
            if start_block.height >= tip_header.height || start_block.header_hash == tip_header_hash {
                debug!(
                    target: LOG_TARGET,
                    "Scanning complete to current tip (height: {}) in {:.2?}",
                    start_block.height,
                    timer.elapsed()
                );
                return Ok((
                    start_block.num_outputs.unwrap_or(0),
                    start_block.height,
                    start_block.amount.unwrap_or_else(|| MicroTari::from(0)),
                    timer.elapsed(),
                ));
            }

            let (num_recovered, num_scanned, amount) = self
                .scan_utxos(&mut client, start_block.header_hash, tip_header_hash, tip_header.height)
                .await?;
            if num_scanned == 0 {
                return Err(UtxoScannerError::UtxoScanningError(
                    "Peer returned 0 UTXOs to scan".to_string(),
                ));
            }
            debug!(
                target: LOG_TARGET,
                "Scanning round completed up to height {} in {:.2?} ({} outputs scanned, {} recovered with value {})",
                tip_header.height,
                timer.elapsed(),
                num_scanned,
                num_recovered,
                amount
            );
        }
    }

    async fn get_chain_tip_header(
        &self,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<BlockHeader, UtxoScannerError> {
        let tip_info = client.get_tip_info().await?;
        let chain_height = tip_info.metadata.map(|m| m.height_of_longest_chain()).unwrap_or(0);
        let end_header = client.get_header_by_height(chain_height).await?;
        let end_header = BlockHeader::try_from(end_header).map_err(UtxoScannerError::ConversionError)?;

        Ok(end_header)
    }

    async fn get_start_scanned_block(
        &self,
        current_tip_height: u64,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<ScannedBlock, UtxoScannerError> {
        // Check for reogs
        let scanned_blocks = self.resources.db.get_scanned_blocks().await?;

        if scanned_blocks.is_empty() {
            let birthday_height_hash = self.get_birthday_header_height_hash(client).await?;
            return Ok(ScannedBlock {
                header_hash: birthday_height_hash.header_hash,
                height: birthday_height_hash.height,
                num_outputs: None,
                amount: None,
                timestamp: Utc::now().naive_utc(),
            });
        }

        // Run through the cached blocks and check which are not found in the current chain anymore
        // Accumulate number of outputs and recovered Tari in the valid blocks
        // Assumption: The blocks are ordered and a reorg will occur to the most recent blocks. Once you have found a
        // valid block the blocks before it are also valid and don't need to be checked
        let mut missing_scanned_blocks = Vec::new();
        let mut found_scanned_block = None;
        let mut num_outputs = 0u64;
        let mut amount = MicroTari::from(0);
        for sb in scanned_blocks.into_iter() {
            if sb.height <= current_tip_height {
                if found_scanned_block.is_none() {
                    let header = BlockHeader::try_from(client.get_header_by_height(sb.height).await?)
                        .map_err(UtxoScannerError::ConversionError)?;
                    let header_hash = header.hash();
                    if header_hash != sb.header_hash {
                        missing_scanned_blocks.push(sb.clone());
                    } else {
                        found_scanned_block = Some(sb.clone());
                    }
                }
                if found_scanned_block.is_some() {
                    num_outputs = num_outputs.saturating_add(sb.num_outputs.unwrap_or(0));
                    amount = amount
                        .checked_add(sb.amount.unwrap_or_else(|| MicroTari::from(0)))
                        .ok_or(UtxoScannerError::OverflowError)?;
                }
            } else {
                missing_scanned_blocks.push(sb.clone());
            }
        }

        if let Some(sb) = found_scanned_block {
            let (height, next_header_hash) = if sb.height == current_tip_height {
                // If we are at the tip just return the tip height and hash
                (current_tip_height, sb.header_hash)
            } else {
                // If we are not at the tip scanning should resume from the next header in the chain
                let next_header = BlockHeader::try_from(client.get_header_by_height(sb.height + 1).await?)
                    .map_err(UtxoScannerError::ConversionError)?;
                let next_header_hash = next_header.hash();
                (sb.height + 1, next_header_hash)
            };

            if !missing_scanned_blocks.is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "Reorg detected on base node. Restarting scanning from height {} (Header Hash: {})",
                    height,
                    next_header_hash.to_hex()
                );
                self.resources
                    .db
                    .clear_scanned_blocks_from_and_higher(
                        missing_scanned_blocks
                            .last()
                            .expect("cannot fail, the vector is not empty")
                            .height,
                    )
                    .await?;
            }
            Ok(ScannedBlock {
                height,
                num_outputs: Some(num_outputs),
                amount: Some(amount),
                header_hash: next_header_hash,
                timestamp: Utc::now().naive_utc(),
            })
        } else {
            warn!(
                target: LOG_TARGET,
                "Reorg detected on base node. No previously scanned block headers found, resuming scan from wallet \
                 birthday"
            );
            // The node does not know of any of our cached headers so we will start the scan anew from the wallet
            // birthday
            self.resources.db.clear_scanned_blocks().await?;
            let birthday_height_hash = self.get_birthday_header_height_hash(client).await?;
            Ok(ScannedBlock {
                header_hash: birthday_height_hash.header_hash,
                height: birthday_height_hash.height,
                num_outputs: None,
                amount: None,
                timestamp: Utc::now().naive_utc(),
            })
        }
    }

    async fn scan_utxos(
        &mut self,
        client: &mut BaseNodeWalletRpcClient,
        start_header_hash: HashOutput,
        end_header_hash: HashOutput,
        tip_height: u64,
    ) -> Result<(u64, u64, MicroTari), UtxoScannerError> {
        // Setting how often the progress event and log should occur during scanning. Defined in blocks
        const PROGRESS_REPORT_INTERVAL: u64 = 100;

        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let mut total_scanned = 0;

        let request = SyncUtxosByBlockRequest {
            start_header_hash: start_header_hash.clone(),
            end_header_hash: end_header_hash.clone(),
        };

        let start = Instant::now();
        let mut utxo_stream = client.sync_utxos_by_block(request).await?;
        trace!(
            target: LOG_TARGET,
            "bulletproof rewind profile - UTXO stream request time {} ms",
            start.elapsed().as_millis(),
        );

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
                return Ok((num_recovered, total_scanned as u64, total_amount));
            }

            let response = response.map_err(|e| UtxoScannerError::RpcStatus(e.to_string()))?;
            let current_height = response.height;
            let current_header_hash = response.header_hash;
            let outputs = response
                .outputs
                .into_iter()
                .map(|utxo| TransactionOutput::try_from(utxo).map_err(UtxoScannerError::ConversionError))
                .collect::<Result<Vec<_>, _>>()?;

            total_scanned += outputs.len();

            let start = Instant::now();
            let (tx_id, found_outputs) = self.scan_for_outputs(outputs).await?;
            scan_for_outputs_profiling.push(start.elapsed());

            let (count, amount) = self
                .import_utxos_to_transaction_service(found_outputs, tx_id, current_height)
                .await?;

            self.resources
                .db
                .save_scanned_block(ScannedBlock {
                    header_hash: current_header_hash,
                    height: current_height,
                    num_outputs: Some(count),
                    amount: Some(amount),
                    timestamp: Utc::now().naive_utc(),
                })
                .await?;

            self.resources
                .db
                .clear_scanned_blocks_before_height(current_height.saturating_sub(SCANNED_BLOCK_CACHE_SIZE), true)
                .await?;

            if current_height % PROGRESS_REPORT_INTERVAL == 0 {
                debug!(
                    target: LOG_TARGET,
                    "Scanned up to block {} with a current tip_height of {}", current_height, tip_height
                );
                self.publish_event(UtxoScannerEvent::Progress {
                    current_height,
                    tip_height,
                });
            }

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

        Ok((num_recovered, total_scanned as u64, total_amount))
    }

    async fn scan_for_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<(TxId, Vec<(UnblindedOutput, String)>), UtxoScannerError> {
        let mut found_outputs: Vec<(UnblindedOutput, String)> = Vec::new();
        let tx_id = TxId::new_random();
        if self.mode == UtxoScannerMode::Recovery {
            found_outputs.append(
                &mut self
                    .resources
                    .output_manager_service
                    .scan_for_recoverable_outputs(outputs.clone(), tx_id)
                    .await?
                    .into_iter()
                    .map(|uo| (uo, format!("Recovered output on {}.", Utc::now().naive_utc())))
                    .collect(),
            );
        };
        found_outputs.append(
            &mut self
                .resources
                .output_manager_service
                .scan_outputs_for_one_sided_payments(outputs.clone(), tx_id)
                .await?
                .into_iter()
                .map(|uo| {
                    (
                        uo,
                        format!("Detected one-sided transaction output on {}.", Utc::now().naive_utc()),
                    )
                })
                .collect(),
        );
        Ok((tx_id, found_outputs))
    }

    async fn import_utxos_to_transaction_service(
        &mut self,
        utxos: Vec<(UnblindedOutput, String)>,
        tx_id: TxId,
        current_height: u64,
    ) -> Result<(u64, MicroTari), UtxoScannerError> {
        let mut num_recovered = 0u64;
        let mut total_amount = MicroTari::from(0);
        let source_public_key = self.resources.node_identity.public_key().clone();

        for (uo, message) in utxos {
            match self
                .import_unblinded_utxo_to_transaction_service(
                    uo.clone(),
                    &source_public_key,
                    message,
                    tx_id,
                    current_height,
                )
                .await
            {
                Ok(_) => {
                    num_recovered = num_recovered.saturating_add(1);
                    total_amount += uo.value;
                },
                Err(WalletError::TransactionServiceError(TransactionServiceError::TransactionStorageError(
                    TransactionStorageError::DuplicateOutput,
                ))) => {
                    info!(
                        target: LOG_TARGET,
                        "Recoverer attempted to add a duplicate output to the database for faux transaction ({}); \
                         ignoring it as this is not a real error",
                        tx_id
                    );
                },
                Err(e) => return Err(UtxoScannerError::UtxoImportError(e.to_string())),
            }
        }
        Ok((num_recovered, total_amount))
    }

    async fn set_recovery_mode(&self) -> Result<(), UtxoScannerError> {
        self.resources
            .db
            .set_client_key_value(RECOVERY_KEY.to_owned(), Utc::now().to_string())
            .await?;
        Ok(())
    }

    async fn check_recovery_mode(&self) -> Result<bool, UtxoScannerError> {
        let value: Option<String> = self
            .resources
            .db
            .get_client_key_from_str(RECOVERY_KEY.to_owned())
            .await?;
        match value {
            None => Ok(false),
            Some(_v) => Ok(true),
        }
    }

    async fn clear_recovery_mode(&self) -> Result<(), UtxoScannerError> {
        let _ = self.resources.db.clear_client_value(RECOVERY_KEY.to_owned()).await?;
        Ok(())
    }

    fn publish_event(&self, event: UtxoScannerEvent) {
        let _ = self.event_sender.send(event);
    }

    /// A faux incoming transaction will be created to provide a record of the event of importing a scanned UTXO. The
    /// TxId of the generated transaction is returned.
    pub async fn import_unblinded_utxo_to_transaction_service(
        &mut self,
        unblinded_output: UnblindedOutput,
        source_public_key: &CommsPublicKey,
        message: String,
        tx_id: TxId,
        current_height: u64,
    ) -> Result<TxId, WalletError> {
        let tx_id = self
            .resources
            .transaction_service
            .import_utxo_with_status(
                unblinded_output.value,
                source_public_key.clone(),
                message,
                Some(unblinded_output.features.maturity),
                ImportStatus::FauxUnconfirmed,
                Some(tx_id),
                Some(current_height),
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet as 'ImportStatus::FauxUnconfirmed'",
            unblinded_output
                .as_transaction_input(&self.resources.factories.commitment)?
                .commitment()
                .map_err(WalletError::TransactionError)?
                .to_hex(),
        );

        Ok(tx_id)
    }

    fn get_next_peer(&mut self) -> Option<NodeId> {
        let peer = self.peer_seeds.get(self.peer_index).map(NodeId::from_public_key);
        self.peer_index += 1;
        peer
    }

    async fn get_birthday_header_height_hash(
        &self,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<HeightHash, UtxoScannerError> {
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
        let header = BlockHeader::try_from(header).map_err(UtxoScannerError::ConversionError)?;
        let header_hash = header.hash();
        info!(
            target: LOG_TARGET,
            "Fresh wallet recovery starting at Block {} (Header Hash: {})",
            block_height,
            header_hash.to_hex(),
        );
        Ok(HeightHash {
            height: block_height,
            header_hash,
        })
    }
}

struct HeightHash {
    height: u64,
    header_hash: HashOutput,
}

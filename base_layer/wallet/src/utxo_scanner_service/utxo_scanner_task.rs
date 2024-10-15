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
    convert::{TryFrom, TryInto},
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, Utc};
use futures::StreamExt;
use log::*;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::{ImportStatus, TxId},
    types::HashOutput,
    wallet_types::WalletType,
};
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::RpcClientLease,
    traits::OrOptional,
    types::CommsPublicKey,
    Minimized,
    PeerConnection,
};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcClient,
    blocks::BlockHeader,
    proto::base_node::SyncUtxosByBlockRequest,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{encrypted_data::PaymentId, TransactionOutput, WalletOutput},
    },
};
use tari_key_manager::get_birthday_from_unix_epoch_in_seconds;
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;

use crate::{
    connectivity_service::WalletConnectivityInterface,
    error::WalletError,
    storage::database::WalletBackend,
    transaction_service::error::{TransactionServiceError, TransactionStorageError},
    utxo_scanner_service::{
        error::UtxoScannerError,
        handle::UtxoScannerEvent,
        service::{ScannedBlock, UtxoScannerResources, SCANNED_BLOCK_CACHE_SIZE},
        uxto_scanner_service_builder::UtxoScannerMode,
        RECOVERY_KEY,
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub struct UtxoScannerTask<TBackend, TWalletConnectivity> {
    pub(crate) resources: UtxoScannerResources<TBackend, TWalletConnectivity>,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) retry_limit: usize,
    pub(crate) num_retries: usize,
    pub(crate) peer_seeds: Vec<CommsPublicKey>,
    pub(crate) peer_index: usize,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
}
impl<TBackend, TWalletConnectivity> UtxoScannerTask<TBackend, TWalletConnectivity>
where
    TBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub async fn run(mut self) -> Result<(), UtxoScannerError> {
        if self.mode == UtxoScannerMode::Recovery {
            self.set_recovery_mode()?;
        } else {
            let in_progress = self.check_recovery_mode()?;
            if in_progress {
                warn!(
                    target: LOG_TARGET,
                    "Scanning round aborted as a Recovery is in progress"
                );
                return Ok(());
            }
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
                            "Failed to scan UTXO's after {} attempt(s) using sync peer(s). Aborting...",
                            self.num_retries,
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
        &mut self,
        num_outputs_recovered: u64,
        final_height: u64,
        total_value: MicroMinotari,
        elapsed: Duration,
    ) -> Result<(), UtxoScannerError> {
        if num_outputs_recovered > 0 {
            // this is a best effort, if this fails, its very likely that it's already busy with a validation.
            let _result = self.resources.output_manager_service.validate_txos().await;
            let _result = self.resources.transaction_service.validate_transactions().await;
        }
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
            self.clear_recovery_mode()?;
        }
        Ok(())
    }

    async fn connect_to_peer(&mut self, peer: NodeId) -> Result<PeerConnection, UtxoScannerError> {
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
                    if connection.clone().disconnect(Minimized::No).await.is_ok() {
                        debug!(target: LOG_TARGET, "Disconnected base node peer {}", peer);
                    }
                }

                Err(e.into())
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn attempt_sync(&mut self, peer: NodeId) -> Result<(u64, u64, MicroMinotari, Duration), UtxoScannerError> {
        self.publish_event(UtxoScannerEvent::ConnectingToBaseNode(peer.clone()));
        let selected_peer = self.resources.wallet_connectivity.get_current_base_node_peer_node_id();

        let mut client = if selected_peer.map(|p| p == peer).unwrap_or(false) {
            // Use the wallet connectivity service so that RPC pools are correctly managed
            self.resources
                .wallet_connectivity
                .obtain_base_node_wallet_rpc_client()
                .await
                .ok_or(UtxoScannerError::ConnectivityShutdown)?
        } else {
            self.establish_new_rpc_connection(&peer).await?
        };

        let latency = client.get_last_request_latency();
        self.publish_event(UtxoScannerEvent::ConnectedToBaseNode(
            peer.clone(),
            latency.unwrap_or_default(),
        ));

        let timer = Instant::now();
        loop {
            let tip_header = self.get_chain_tip_header(&mut client).await?;
            let tip_header_hash = tip_header.hash();
            let last_scanned_block = self.get_last_scanned_block(tip_header.height, &mut client).await?;

            let next_block_to_scan = if let Some(last_scanned_block) = last_scanned_block {
                // If we have scanned to the tip and are told to start beyond the tip we are done
                if last_scanned_block.height >= tip_header.height {
                    debug!(
                        target: LOG_TARGET,
                        "Scanning complete to current tip (height: {}) in {:.2?}",
                        last_scanned_block.height,
                        timer.elapsed()
                    );
                    return Ok((
                        last_scanned_block.num_outputs.unwrap_or(0),
                        last_scanned_block.height,
                        last_scanned_block.amount.unwrap_or_else(|| MicroMinotari::from(0)),
                        timer.elapsed(),
                    ));
                }

                let next_header =
                    BlockHeader::try_from(client.get_header_by_height(last_scanned_block.height + 1).await?)
                        .map_err(UtxoScannerError::ConversionError)?;
                let next_header_hash = next_header.hash();

                ScannedBlock {
                    height: next_header.height,
                    num_outputs: last_scanned_block.num_outputs,
                    amount: last_scanned_block.amount,
                    header_hash: next_header_hash,
                    timestamp: Utc::now().naive_utc(),
                }
            } else {
                // The node does not know of any of our cached headers so we will start the scan anew from the
                // wallet birthday
                self.resources.db.clear_scanned_blocks()?;
                let birthday_height_hash = match self.resources.db.get_wallet_type()? {
                    Some(WalletType::ProvidedKeys(_)) => {
                        let header_proto = client.get_header_by_height(0).await?;
                        let header = BlockHeader::try_from(header_proto).map_err(UtxoScannerError::ConversionError)?;
                        HeightHash {
                            height: 0,
                            header_hash: header.hash(),
                        }
                    },
                    _ => self.get_birthday_header_height_hash(&mut client).await?,
                };

                ScannedBlock {
                    height: birthday_height_hash.height,
                    num_outputs: None,
                    amount: None,
                    header_hash: birthday_height_hash.header_hash,
                    timestamp: Utc::now().naive_utc(),
                }
            };

            if self.shutdown_signal.is_triggered() {
                return Ok((
                    next_block_to_scan.num_outputs.unwrap_or(0),
                    next_block_to_scan.height,
                    next_block_to_scan.amount.unwrap_or_else(|| MicroMinotari::from(0)),
                    timer.elapsed(),
                ));
            }

            debug!(
                target: LOG_TARGET,
                "Scanning UTXO's from height = {} to current tip_height = {} (starting header_hash: {})",
                next_block_to_scan.height,
                tip_header.height,
                next_block_to_scan.header_hash.to_hex(),
            );

            let (num_recovered, num_scanned, amount) = self
                .scan_utxos(
                    &mut client,
                    next_block_to_scan.header_hash,
                    tip_header_hash,
                    tip_header.height,
                )
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

    async fn establish_new_rpc_connection(
        &mut self,
        peer: &NodeId,
    ) -> Result<RpcClientLease<BaseNodeWalletRpcClient>, UtxoScannerError> {
        let mut connection = self.connect_to_peer(peer.clone()).await?;
        let client = connection
            .connect_rpc_using_builder(BaseNodeWalletRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;
        Ok(RpcClientLease::new(client))
    }

    async fn get_chain_tip_header(
        &self,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<BlockHeader, UtxoScannerError> {
        let tip_info = client.get_tip_info().await?;
        let chain_height = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
        let end_header = client.get_header_by_height(chain_height).await?;
        let end_header = BlockHeader::try_from(end_header).map_err(UtxoScannerError::ConversionError)?;

        Ok(end_header)
    }

    async fn get_last_scanned_block(
        &self,
        current_tip_height: u64,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<Option<ScannedBlock>, UtxoScannerError> {
        let scanned_blocks = self.resources.db.get_scanned_blocks()?;
        debug!(
            target: LOG_TARGET,
            "Found {} cached previously scanned blocks",
            scanned_blocks.len()
        );

        if scanned_blocks.is_empty() {
            return Ok(None);
        }

        // Run through the cached blocks and check which are not found in the current chain anymore
        // Accumulate number of outputs and recovered Tari in the valid blocks
        // Assumption: The blocks are ordered and a reorg will occur to the most recent blocks. Once you have found a
        // valid block the blocks before it are also valid and don't need to be checked
        let mut last_missing_scanned_block = None;
        let mut found_scanned_block = None;
        let mut num_outputs = 0u64;
        let mut amount = MicroMinotari::from(0);
        for sb in scanned_blocks {
            // The scanned block has a higher height than the current tip, meaning the previously scanned block was
            // reorged out.
            if sb.height > current_tip_height {
                last_missing_scanned_block = Some(sb);
                continue;
            }

            if found_scanned_block.is_none() {
                let header = client.get_header_by_height(sb.height).await.or_optional()?;
                let header = header
                    .map(BlockHeader::try_from)
                    .transpose()
                    .map_err(UtxoScannerError::ConversionError)?;

                match header {
                    Some(header) => {
                        let header_hash = header.hash();
                        if header_hash == sb.header_hash {
                            found_scanned_block = Some(sb.clone());
                        } else {
                            last_missing_scanned_block = Some(sb.clone());
                        }
                    },
                    None => {
                        last_missing_scanned_block = Some(sb.clone());
                    },
                }
            }
            // Sum up the number of outputs recovered starting from the first found block
            if found_scanned_block.is_some() {
                num_outputs = num_outputs.saturating_add(sb.num_outputs.unwrap_or(0));
                amount = amount
                    .checked_add(sb.amount.unwrap_or_else(|| MicroMinotari::from(0)))
                    .ok_or(UtxoScannerError::OverflowError)?;
            }
        }

        if let Some(block) = last_missing_scanned_block {
            warn!(
                target: LOG_TARGET,
                "Reorg detected on base node. Removing scanned blocks from height {}", block.height
            );
            self.resources.db.clear_scanned_blocks_from_and_higher(block.height)?;
        }

        if let Some(sb) = found_scanned_block {
            debug!(
                target: LOG_TARGET,
                "Last scanned block found at height {} (Header Hash: {})",
                sb.height,
                sb.header_hash.to_hex()
            );
            Ok(Some(ScannedBlock {
                height: sb.height,
                num_outputs: Some(num_outputs),
                amount: Some(amount),
                header_hash: sb.header_hash,
                timestamp: Utc::now().naive_utc(),
            }))
        } else {
            warn!(
                target: LOG_TARGET,
                "Reorg detected on base node. No previously scanned block headers found, resuming scan from wallet \
                 birthday"
            );
            Ok(None)
        }
    }

    #[allow(clippy::too_many_lines)]
    // converting u64 to i64 is its only used for timestamps
    #[allow(clippy::cast_possible_wrap)]
    async fn scan_utxos(
        &mut self,
        client: &mut BaseNodeWalletRpcClient,
        start_header_hash: HashOutput,
        end_header_hash: HashOutput,
        tip_height: u64,
    ) -> Result<(u64, u64, MicroMinotari), UtxoScannerError> {
        // Setting how often the progress event and log should occur during scanning. Defined in blocks
        const PROGRESS_REPORT_INTERVAL: u64 = 100;

        let mut num_recovered = 0u64;
        let mut total_amount = MicroMinotari::from(0);
        let mut total_scanned = 0;

        let request = SyncUtxosByBlockRequest {
            start_header_hash: start_header_hash.to_vec(),
            end_header_hash: end_header_hash.to_vec(),
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
        let mut prev_scanned_block: Option<ScannedBlock> = None;
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
            let mined_timestamp =
                NaiveDateTime::from_timestamp_opt(response.mined_timestamp as i64, 0).unwrap_or(NaiveDateTime::MIN);
            let outputs = response
                .outputs
                .into_iter()
                .map(|utxo| TransactionOutput::try_from(utxo).map_err(UtxoScannerError::ConversionError))
                .collect::<Result<Vec<_>, _>>()?;
            total_scanned += outputs.len();

            let start = Instant::now();
            let found_outputs = self.scan_for_outputs(outputs, current_height).await?;
            scan_for_outputs_profiling.push(start.elapsed());

            let (mut count, mut amount) = self
                .import_utxos_to_transaction_service(found_outputs, current_height, mined_timestamp)
                .await?;
            let block_hash = current_header_hash.try_into()?;
            if let Some(scanned_block) = prev_scanned_block {
                if block_hash == scanned_block.header_hash {
                    count += scanned_block.num_outputs.unwrap_or(0);
                    amount += scanned_block.amount.unwrap_or_else(|| 0.into())
                } else {
                    self.resources.db.save_scanned_block(scanned_block)?;
                    self.resources.db.clear_scanned_blocks_before_height(
                        current_height.saturating_sub(SCANNED_BLOCK_CACHE_SIZE),
                        true,
                    )?;

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
            }
            prev_scanned_block = Some(ScannedBlock {
                header_hash: block_hash,
                height: current_height,
                num_outputs: Some(count),
                amount: Some(amount),
                timestamp: Utc::now().naive_utc(),
            });
        }
        // We need to update the last one
        if let Some(scanned_block) = prev_scanned_block {
            self.resources.db.clear_scanned_blocks_before_height(
                scanned_block.height.saturating_sub(SCANNED_BLOCK_CACHE_SIZE),
                true,
            )?;
            self.resources.db.save_scanned_block(scanned_block)?;
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
        height: u64,
    ) -> Result<Vec<(WalletOutput, String, ImportStatus, TxId, TransactionOutput)>, UtxoScannerError> {
        let mut found_outputs: Vec<(WalletOutput, String, ImportStatus, TxId, TransactionOutput)> = Vec::new();
        let start = Instant::now();
        found_outputs.append(
            &mut self
                .resources
                .output_manager_service
                .scan_for_recoverable_outputs(outputs.clone().into_iter().map(|o| (o, None)).collect())
                .await?
                .into_iter()
                .map(|ro| -> Result<_, UtxoScannerError> {
                    let (message, status) = if ro.output.features.is_coinbase() {
                        (
                            format!("Coinbase for height: {}", height),
                            ImportStatus::CoinbaseUnconfirmed,
                        )
                    } else {
                        (self.resources.recovery_message.clone(), ImportStatus::Imported)
                    };
                    let output = outputs.iter().find(|o| o.hash() == ro.hash).ok_or_else(|| {
                        UtxoScannerError::UtxoScanningError(format!("Output '{}' not found", ro.hash.to_hex()))
                    })?;
                    Ok((ro.output, message, status, ro.tx_id, output.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let scanned_time = start.elapsed();
        let start = Instant::now();

        found_outputs.append(
            &mut self
                .resources
                .output_manager_service
                .scan_outputs_for_one_sided_payments(outputs.clone().into_iter().map(|o| (o, None)).collect())
                .await?
                .into_iter()
                .map(|ro| -> Result<_, UtxoScannerError> {
                    let (message, status) = if ro.output.features.is_coinbase() {
                        (
                            format!("Coinbase for height: {}", height),
                            ImportStatus::CoinbaseUnconfirmed,
                        )
                    } else {
                        (
                            self.resources.recovery_message.clone(),
                            ImportStatus::OneSidedUnconfirmed,
                        )
                    };
                    let output = outputs.iter().find(|o| o.hash() == ro.hash).ok_or_else(|| {
                        UtxoScannerError::UtxoScanningError(format!("Output '{}' not found", ro.hash.to_hex()))
                    })?;
                    Ok((ro.output, message, status, ro.tx_id, output.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let one_sided_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "Scanned for outputs: outputs took {} ms , one-sided took {} ms",
            scanned_time.as_millis(),
            one_sided_time.as_millis(),
        );
        Ok(found_outputs)
    }

    async fn import_utxos_to_transaction_service(
        &mut self,
        utxos: Vec<(WalletOutput, String, ImportStatus, TxId, TransactionOutput)>,
        current_height: u64,
        mined_timestamp: NaiveDateTime,
    ) -> Result<(u64, MicroMinotari), UtxoScannerError> {
        let mut num_recovered = 0u64;
        let mut total_amount = MicroMinotari::from(0);
        for (wo, message, import_status, tx_id, to) in utxos {
            let source_address = if wo.features.is_coinbase() {
                // It's a coinbase, so we know we mined it (we do mining with cold wallets).
                self.resources.one_sided_tari_address.clone()
            } else {
                match &wo.payment_id {
                    PaymentId::AddressAndData(address, _) | PaymentId::Address(address) => address.clone(),
                    _ => TariAddress::default(),
                }
            };
            match self
                .import_key_manager_utxo_to_transaction_service(
                    wo.clone(),
                    source_address,
                    message,
                    import_status,
                    tx_id,
                    current_height,
                    mined_timestamp,
                    to.clone(),
                )
                .await
            {
                Ok(_) => {
                    num_recovered = num_recovered.saturating_add(1);
                    total_amount += wo.value;
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

    fn set_recovery_mode(&self) -> Result<(), UtxoScannerError> {
        self.resources
            .db
            .set_client_key_value(RECOVERY_KEY.to_owned(), Utc::now().to_string())?;
        Ok(())
    }

    fn check_recovery_mode(&self) -> Result<bool, UtxoScannerError> {
        self.resources
            .db
            .get_client_key_from_str::<String>(RECOVERY_KEY.to_owned())
            .map(|x| x.is_some())
            .map_err(UtxoScannerError::from) // in case if `get_client_key_from_str` returns not exactly that type
    }

    fn clear_recovery_mode(&self) -> Result<(), UtxoScannerError> {
        let _ = self.resources.db.clear_client_value(RECOVERY_KEY.to_owned())?;
        Ok(())
    }

    fn publish_event(&self, event: UtxoScannerEvent) {
        let _size = self.event_sender.send(event);
    }

    /// A faux incoming transaction will be created to provide a record of the event of importing a scanned UTXO. The
    /// TxId of the generated transaction is returned.
    pub async fn import_key_manager_utxo_to_transaction_service(
        &mut self,
        wallet_output: WalletOutput,
        source_address: TariAddress,
        message: String,
        import_status: ImportStatus,
        tx_id: TxId,
        current_height: u64,
        mined_timestamp: NaiveDateTime,
        scanned_output: TransactionOutput,
    ) -> Result<TxId, WalletError> {
        let tx_id = self
            .resources
            .transaction_service
            .import_utxo_with_status(
                wallet_output.value,
                source_address,
                message,
                import_status.clone(),
                Some(tx_id),
                Some(current_height),
                Some(mined_timestamp),
                scanned_output,
                wallet_output.payment_id,
            )
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO with value {},  imported into wallet as 'ImportStatus::{}'", wallet_output.value, import_status
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
        let birthday = self.resources.db.get_wallet_birthday()?;
        // Calculate the unix epoch time of two weeks (14 days), in seconds, before the
        // wallet birthday. The latter avoids any possible issues with reorgs.
        let epoch_time = get_birthday_from_unix_epoch_in_seconds(birthday, 14u16);

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
            "Fresh wallet recovery/scanning starting at Block {} (Header Hash: {})",
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

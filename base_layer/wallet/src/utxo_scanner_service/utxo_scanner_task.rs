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
    convert::TryInto,
    time::{Duration, Instant},
};
use tari_utilities::ByteArray;
use chrono::{DateTime, Utc};
use log::*;
use minotari_node_wallet_client::{http, BaseNodeWalletClient};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::{ImportStatus, TxId},
    types::HashOutput,
    wallet_types::WalletType,
};
use tari_comms::{
    peer_manager::NodeId,
    protocol::rpc::RpcClientLease,
    types::CommsPublicKey,
    Minimized,
    PeerConnection,
};
use tari_core::{
    base_node::rpc::{models::{BlockHeader, MinimalUtxoInfo}, BaseNodeWalletRpcClient},
    // blocks::BlockHeader,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{encrypted_data::PaymentId, EncryptedData, TransactionError, TransactionOutput, WalletOutput}, transaction_key_manager::TransactionKeyManagerInterface,
    },
};
use tari_crypto::compressed_commitment::CompressedCommitment;
use tari_key_manager::get_birthday_from_unix_epoch_in_seconds;
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::sync::broadcast;
use url::Url;

use crate::{
    connectivity_service::WalletConnectivityInterface, error::WalletError, schema::outputs::encrypted_data, storage::database::WalletBackend, transaction_service::error::{TransactionServiceError, TransactionStorageError}, utxo_scanner_service::{
        error::UtxoScannerError,
        handle::UtxoScannerEvent,
        service::{ScannedBlock, UtxoScannerResources, SCANNED_BLOCK_CACHE_SIZE},
        uxto_scanner_service_builder::UtxoScannerMode,
        RECOVERY_KEY,
    }
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

pub struct UtxoScannerTask<TBackend, TWalletConnectivity, TKeyManager> {
    pub(crate) resources: UtxoScannerResources<TBackend, TWalletConnectivity>,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) retry_limit: usize,
    pub(crate) num_retries: usize,
    pub(crate) peer_index: usize,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
    pub birthday_offset: u16,
    pub key_manager: TKeyManager,
}
impl<TBackend, TWalletConnectivity, TKeyManager> UtxoScannerTask<TBackend, TWalletConnectivity, TKeyManager>
where
    TBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManager: TransactionKeyManagerInterface
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
            match self.attempt_sync().await {
                    Ok((num_outputs_recovered, final_height, final_amount, elapsed)) => {
                        debug!(target: LOG_TARGET, "Scanned to height #{}", final_height);
                        self.finalize(num_outputs_recovered, final_height, final_amount, elapsed)
                            .await?;
                        return Ok(());
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to scan UTXO's from base node: {}", e
                        );
                        self.publish_event(UtxoScannerEvent::ScanningRoundFailed {
                            num_retries: self.num_retries,
                            retry_limit: self.retry_limit,
                            error: e.to_string(),
                        });
                        continue;
                    },
        };
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

    async fn new_connection_to_peer(&mut self, peer: NodeId) -> Result<PeerConnection, UtxoScannerError> {
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

    /// Try to instantiate a Base Node Wallet Service client.
    fn base_node_wallet_service_client(
        &self,
    ) -> Result<http::Client, UtxoScannerError> {
        // let address = rpc_client.get_wallet_query_http_service_address().await?;
        // if address.http_address.is_empty() {
            // Err(UtxoScannerError::BaseNodeWalletServiceUrlEmpty)
        // } else {
            Ok(http::Client::new(self.resources.http_client_url.clone()))
        // }
    }

    #[allow(clippy::too_many_lines)]
    async fn attempt_sync(&mut self) -> Result<(u64, u64, MicroMinotari, Duration), UtxoScannerError> {
        // let selected_peer = self.resources.wallet_connectivity.get_current_base_node_peer_node_id();

        // get RPC client
        // let mut client = if selected_peer.map(|p| p == peer).unwrap_or(false) {
        //     // Use the wallet connectivity service so that RPC pools are correctly managed
        //     self.resources
        //         .wallet_connectivity
        //         .obtain_base_node_wallet_rpc_client()
        //         .await
        //         .ok_or(UtxoScannerError::ConnectivityShutdown)?
        // } else {
        //     self.establish_new_rpc_connection(&peer).await?
        // };

        // let latency = client.get_last_request_latency();
        // self.publish_event(UtxoScannerEvent::ConnectedToBaseNode(
        //     peer.clone(),
        //     latency.unwrap_or_default(),
        // ));
info!(target: LOG_TARGET, "Starting UTXO scanning task");

        // get wallet service query client
        let wallet_service_client = self.base_node_wallet_service_client()?;

        let timer = Instant::now();
        loop {
            info!(target: LOG_TARGET, "here");
            let tip_header = self.get_chain_tip_header(&wallet_service_client).await?;
            let tip_header_hash = tip_header.hash;
            let last_scanned_block = self
                .get_last_scanned_block(&wallet_service_client, tip_header.height)
                .await?;

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

                let next_header = wallet_service_client
                    .get_header_by_height(last_scanned_block.height + 1)
                    .await?;
                let next_header_hash = next_header.hash;

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
                let scanning_start_height_hash = match self.resources.db.get_wallet_type()? {
                    Some(WalletType::ProvidedKeys(wallet)) => {
                        self.get_scanning_start_header_height_hash(&wallet_service_client, wallet.birthday)
                            .await?
                    },
                    _ => {
                        self.get_scanning_start_header_height_hash(&wallet_service_client, None)
                            .await?
                    },
                };

                ScannedBlock {
                    height: scanning_start_height_hash.height,
                    num_outputs: None,
                    amount: None,
                    header_hash: scanning_start_height_hash.header_hash,
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

            info!(target: LOG_TARGET, "here");
            let (num_recovered, num_scanned, amount) = self
                .scan_utxos(
                    &wallet_service_client,
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
        let mut connection = self.new_connection_to_peer(peer.clone()).await?;
        let client = connection
            .connect_rpc_using_builder(BaseNodeWalletRpcClient::builder().with_deadline(Duration::from_secs(60)))
            .await?;
        Ok(RpcClientLease::new(client))
    }

    async fn get_chain_tip_header(&self, client: &http::Client) -> Result<BlockHeader, UtxoScannerError> {
        let tip_info = client.get_tip_info().await?;
        let chain_height = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
        let end_header = client.get_header_by_height(chain_height).await?;

        Ok(end_header)
    }

    async fn get_last_scanned_block(
        &self,
        client: &http::Client,
        current_tip_height: u64,
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
                let header = client.get_header_by_height(sb.height).await.ok();
                match header {
                    Some(header) => {
                        let header_hash = header.hash;
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
        client: &http::Client,
        start_header_hash: HashOutput,
        end_header_hash: HashOutput,
        tip_height: u64,
    ) -> Result<(u64, u64, MicroMinotari), UtxoScannerError> {
        info!(
            target: LOG_TARGET,
            "Starting UTXO scanning from header hash {} to header hash {} at tip height {}",
            start_header_hash.to_hex(),
            end_header_hash.to_hex(),
            tip_height
        );
        // Setting how often the progress event and log should occur during scanning. Defined in blocks
        const PROGRESS_REPORT_INTERVAL: u64 = 100;

        let mut num_recovered = 0u64;
        let mut total_amount = MicroMinotari::from(0);
        let mut total_scanned = 0;

        let start = Instant::now();
            info!(target: LOG_TARGET, "here");
        let mut utxo_stream = client
            .sync_utxos_by_block(start_header_hash.to_vec(), end_header_hash.to_vec(), self.shutdown_signal.clone())
            .await?;
        info!(
            target: LOG_TARGET,
            "bulletproof rewind profile - UTXO stream request time {} ms",
            start.elapsed().as_millis(),
        );

        // let mut utxo_next_await_profiling = Vec::new();
        let mut prev_scanned_block: Option<ScannedBlock> = None;
        while let Some(response) = 
            utxo_stream.recv().await
         {
            
            info!(target: LOG_TARGET, "here");
            if self.shutdown_signal.is_triggered() {
                // if running is set to false, we know its been canceled upstream so lets exit the loop
                return Ok((num_recovered, total_scanned as u64, total_amount));
            }

            let response = response.map_err(|e| UtxoScannerError::RpcStatus(e.to_string()))?;
            for response in response.utxos {
                let current_height = response.height;
                let current_header_hash = response.header_hash;
                let mined_timestamp = DateTime::<Utc>::from_timestamp(response.mined_timestamp as i64, 0)
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                let outputs = response.outputs;
                total_scanned += outputs.len();
            info!(target: LOG_TARGET, "here");

                let start = Instant::now();
                let found_outputs = self.scan_for_outputs(outputs).await?;

            info!(target: LOG_TARGET, "here");
                if found_outputs.is_empty() {
                    debug!(
                        target: LOG_TARGET,
                        "No recoverable outputs found in block at height {} with header hash {}",
                        current_height,
                        current_header_hash.to_hex()
                    );
                          // Now download the whole block and import the outputs
                   // continue;
                }
                else {
                    info!(
                        target: LOG_TARGET,
                        "Found {} recoverable outputs in block at height {} with header hash {}",
                        found_outputs.len(),
                        current_height,
                        current_header_hash.to_hex()
                    );
                    let block  = client.get_utxos_by_block(
                    current_header_hash.to_vec(),
                ).await?;
                   self
                    .import_utxos_to_transaction_service( current_height, mined_timestamp)
                    .await?;
                 
                }

            info!(target: LOG_TARGET, "here");

          
                // todo!();

            info!(target: LOG_TARGET, "here");
             
                let block_hash = current_header_hash.try_into()?;
                if let Some(scanned_block) = prev_scanned_block {
                    if block_hash == scanned_block.header_hash {
                        // count += scanned_block.num_outputs.unwrap_or(0);
                        // amount += scanned_block.amount.unwrap_or_else(|| 0.into())
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

                        // num_recovered = num_recovered.saturating_add(count);
                        // total_amount += amount;
                    }
                }
                prev_scanned_block = Some(ScannedBlock {
                    header_hash: block_hash,
                    height: current_height,
                    num_outputs: Some(0),
                    amount: None,
                    timestamp: Utc::now().naive_utc(),
                });
            }
        }
            info!(target: LOG_TARGET, "here");
        // We need to update the last one
        if let Some(scanned_block) = prev_scanned_block {
            self.resources.db.clear_scanned_blocks_before_height(
                scanned_block.height.saturating_sub(SCANNED_BLOCK_CACHE_SIZE),
                true,
            )?;
            self.resources.db.save_scanned_block(scanned_block)?;
        }

        Ok((num_recovered, total_scanned as u64, total_amount))
    }

    async fn scan_for_outputs(
        &mut self,
        outputs: Vec<MinimalUtxoInfo>,
    ) -> Result<Vec<(MinimalUtxoInfo)>, UtxoScannerError> {
        let mut found_outputs: Vec<(MinimalUtxoInfo)> = Vec::new();
        let start = Instant::now();
        // found_outputs.append(
        //     &mut self
        //         .resources
        //         .output_manager_service
        //         .scan_for_recoverable_outputs(outputs.clone().into_iter().map(|o| (o, None)).collect())
        //         .await?
        //         .into_iter()
        //         .map(|ro| -> Result<_, UtxoScannerError> {
        //             let status = if ro.output.features.is_coinbase() {
        //                 ImportStatus::CoinbaseUnconfirmed
        //             } else {
        //                 ImportStatus::Imported
        //             };
        //             let output = outputs.iter().find(|o| o.hash() == ro.hash).ok_or_else(|| {
        //                 UtxoScannerError::UtxoScanningError(format!("Output '{}' not found", ro.hash.to_hex()))
        //             })?;
        //             Ok((ro.output, status, ro.tx_id, output.clone()))
        //         })
        //         .collect::<Result<Vec<_>, _>>()?,
        // );
        for output in outputs {
            let commitment = CompressedCommitment::from_canonical_bytes(&output.commitment)
                .map_err(|e| UtxoScannerError::UtxoScanningError(format!("Invalid commitment: {}", e)))?;
            let encrypted = EncryptedData::from_bytes(&output.encrypted_data)
                .map_err(|e| UtxoScannerError::UtxoScanningError(format!("Invalid encrypted data: {}", e)))?;

            let res = match self.key_manager.try_output_key_recovery(&commitment, &encrypted, None).await {
                Ok((key_id, value, payment_id)) => (output.clone(), key_id, value, payment_id),
                Err(e @ TransactionError::EncryptedDataError(_)) => {
                    trace!(
                        target: LOG_TARGET,
                        "Failed to recover output {}: {}, is not ours", output.output_hash.to_hex(), e
                    );
                    continue;
                } 
                Err(e) => {
                    info!(
                        target: LOG_TARGET,
                        "Failed to recover output {}: {}", output.output_hash.to_hex(), e
                    );
                    continue;
                    //continue;
                    // Err(UtxoScannerError::UtxoScanningError(e.to_string()))
                },
            };
            found_outputs.push(output);
        }
        let scanned_time = start.elapsed();
        let start = Instant::now();

        // found_outputs.append(
        //     &mut self
        //         .resources
        //         .output_manager_service
        //         .scan_outputs_for_one_sided_payments(outputs.clone().into_iter().map(|o| (o, None)).collect())
        //         .await?
        //         .into_iter()
        //         .map(|ro| -> Result<_, UtxoScannerError> {
        //             let status = if ro.output.features.is_coinbase() {
        //                 ImportStatus::CoinbaseUnconfirmed
        //             } else {
        //                 ImportStatus::OneSidedUnconfirmed
        //             };
        //             let output = outputs.iter().find(|o| o.hash() == ro.hash).ok_or_else(|| {
        //                 UtxoScannerError::UtxoScanningError(format!("Output '{}' not found", ro.hash.to_hex()))
        //             })?;
        //             Ok((ro.output, status, ro.tx_id, output.clone()))
        //         })
        //         .collect::<Result<Vec<_>, _>>()?,
        // );
        // todo!("recover one sided outputs");
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
        // utxos: Vec<(WalletOutput, ImportStatus, TxId, TransactionOutput)>,
        current_height: u64,
        mined_timestamp: DateTime<Utc>,
    ) -> Result<(), UtxoScannerError> {
        // TODO: Implement the import of UTXOs to the transaction service.
        // let mut num_recovered = 0u64;
        // let mut total_amount = MicroMinotari::from(0);
        // for (wo, import_status, tx_id, to) in utxos {
        //     let source_address = if wo.features.is_coinbase() {
        //         // It's a coinbase, so we know we mined it (we do mining with cold wallets).
        //         self.resources.one_sided_tari_address.clone()
        //     } else {
        //         match &wo.payment_id {
        //             PaymentId::AddressAndData {
        //                 sender_address: address,
        //                 ..
        //             } => address.clone(),
        //             PaymentId::TransactionInfo { .. } => self.resources.one_sided_tari_address.clone(),
        //             _ => TariAddress::default(),
        //         }
        //     };
        //     match self
        //         .import_key_manager_utxo_to_transaction_service(
        //             wo.clone(),
        //             source_address,
        //             import_status,
        //             tx_id,
        //             current_height,
        //             mined_timestamp,
        //             to.clone(),
        //         )
        //         .await
        //     {
        //         Ok(_) => {
        //             num_recovered = num_recovered.saturating_add(1);
        //             total_amount += wo.value;
        //         },
        //         Err(WalletError::TransactionServiceError(TransactionServiceError::TransactionStorageError(
        //             TransactionStorageError::DuplicateOutput,
        //         ))) => {
        //             info!(
        //                 target: LOG_TARGET,
        //                 "Recoverer attempted to add a duplicate output to the database for faux transaction ({}); \
        //                  ignoring it as this is not a real error",
        //                 tx_id
        //             );
        //         },
        //         Err(e) => return Err(UtxoScannerError::UtxoImportError(e.to_string())),
        //     }
        // }

        Ok(())
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
        import_status: ImportStatus,
        tx_id: TxId,
        current_height: u64,
        mined_timestamp: DateTime<Utc>,
        scanned_output: TransactionOutput,
    ) -> Result<TxId, WalletError> {
        let tx_id = self
            .resources
            .transaction_service
            .import_utxo_with_status(
                wallet_output.value,
                source_address,
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

    // fn get_next_peer(&mut self) -> Option<NodeId> {
    //     let peer = self.peer_seeds.get(self.peer_index).map(NodeId::from_public_key);
    //     self.peer_index += 1;
    //     peer
    // }

    async fn get_scanning_start_header_height_hash(
        &self,
        client: &http::Client,
        option_birthday: Option<u16>,
    ) -> Result<HeightHash, UtxoScannerError> {
        let birthday = match option_birthday {
            Some(birthday) => birthday,
            None => self.resources.db.get_wallet_birthday()?,
        };
        let epoch_time_birthday = get_birthday_from_unix_epoch_in_seconds(birthday, 0);
        let block_height_birthday = client
            .get_height_at_time(epoch_time_birthday)
            .await
            .unwrap_or_else(|e| {
                warn!(target: LOG_TARGET, "Problem requesting `height_at_time` from Base Node: {}", e);
                0
            });
        // Calculate the unix epoch time of 2 days, in seconds, before the
        // wallet birthday. The latter avoids any possible issues with reorgs.
        let epoch_time_scanning_start = get_birthday_from_unix_epoch_in_seconds(birthday, self.birthday_offset);
        let block_height_scanning_start = client
            .get_height_at_time(epoch_time_scanning_start)
            .await
            .unwrap_or_else(|e| {
                warn!(target: LOG_TARGET, "Problem requesting `height_at_time` from Base Node: {}", e);
                0
            });
        let header = client.get_header_by_height(block_height_scanning_start).await?;
        let header_hash_scanning_start = header.hash;
        info!(
            target: LOG_TARGET,
            "Fresh wallet recovery/scanning: Wallet birthday '{}' at epoch time '{}' with block height '{}', scanning \
            from epoch time '{}' at block height '{}' with header hash '{}'",
            birthday,
            epoch_time_birthday,
            block_height_birthday,
            epoch_time_scanning_start,
            block_height_scanning_start,
            header_hash_scanning_start.to_hex(),
        );
        Ok(HeightHash {
            height: block_height_scanning_start,
            header_hash: header_hash_scanning_start,
        })
    }
}

struct HeightHash {
    height: u64,
    header_hash: HashOutput,
}

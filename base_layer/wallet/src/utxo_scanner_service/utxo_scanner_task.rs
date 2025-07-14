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

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::{ImportStatus, TxId},
    types::{BlockHash, FixedHash, HashOutput},
    wallet_types::WalletType,
};
use tari_core::{
    base_node::rpc::models::MinimalUtxoSyncInfo,
    one_sided::shared_secret_to_output_encryption_key,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{payment_id::PaymentId, EncryptedData, TransactionOutput, WalletOutput},
        transaction_key_manager::TransactionKeyManagerInterface,
    },
};
use tari_crypto::{compressed_commitment::CompressedCommitment, compressed_key::CompressedKey};
use tari_key_manager::get_birthday_from_unix_epoch_in_seconds;
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{sync::broadcast, time::sleep};

use crate::{
    client::http_client_factory::HttpClientFactory,
    error::WalletError,
    storage::database::WalletBackend,
    transaction_service::{
        error::{TransactionServiceError, TransactionStorageError},
        protocols::check_faux_transaction_status::SAFETY_HEIGHT_MARGIN,
    },
    utxo_scanner_service::{
        handle::UtxoScannerEvent,
        service::{ScannedBlock, UtxoScannerResources, SCANNED_BLOCK_CACHE_SIZE},
        uxto_scanner_service_builder::UtxoScannerMode,
        RECOVERY_KEY,
    },
};

pub const LOG_TARGET: &str = "wallet::utxo_scanning";

struct SyncResult {
    final_height: u64,
    num_recovered: u64,
    scanned_blocks: u64,
    value_recovered: MicroMinotari,
    elapsed: Duration,
    latency: Duration,
    node: String,
}

pub struct UtxoScannerTask<
    TBackend,
    TKeyManager,
    TWalletClientFactory: HttpClientFactory + Clone + Send + Sync + 'static,
> {
    pub(crate) resources: UtxoScannerResources<TBackend, TWalletClientFactory>,
    pub(crate) event_sender: broadcast::Sender<UtxoScannerEvent>,
    pub(crate) retry_limit: usize,
    pub(crate) num_retries: usize,
    pub(crate) mode: UtxoScannerMode,
    pub(crate) shutdown_signal: ShutdownSignal,
    pub birthday_offset: u16,
    pub key_manager: TKeyManager,
}
impl<TBackend, TKeyManager, TWalletClientFactory> UtxoScannerTask<TBackend, TKeyManager, TWalletClientFactory>
where
    TBackend: WalletBackend + 'static,
    TKeyManager: TransactionKeyManagerInterface,
    TWalletClientFactory: HttpClientFactory + Clone + Send + Sync + 'static,
{
    pub async fn run(mut self) -> Result<(), anyhow::Error> {
        if self.mode == UtxoScannerMode::Recovery {
            self.set_recovery_mode()?;
        }

        loop {
            if self.shutdown_signal.is_triggered() {
                return Ok(());
            }
            match self.attempt_sync().await {
                Ok(sync_result) => {
                    debug!(target: LOG_TARGET, "Scanned to height #{}", sync_result.final_height);
                    if sync_result.scanned_blocks > SAFETY_HEIGHT_MARGIN {
                        // if the TMS validates the transactions before the OMS does, it can invalidate some
                        // transactions, so we need to reset them to ensure we can revalidate them
                        let _result = self
                            .resources
                            .transaction_service
                            .revalidate_rejected_transactions()
                            .await;
                    }
                    self.finalize(sync_result).await?;

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
                    // Wait a bit of time otherwise we spam the node with requests
                    sleep(Duration::from_secs(5)).await;
                    continue;
                },
            };
        }
    }

    async fn finalize(&mut self, sync_result: SyncResult) -> Result<(), anyhow::Error> {
        // this is a best effort, if this fails, its very likely that it's already busy with a validation. We have
        // updated the scanned, so we need to update txns
        let _result = self.resources.output_manager_service.validate_txos().await;
        let _result = self.resources.transaction_service.validate_transactions().await;
        let SyncResult {
            final_height,
            num_recovered,
            value_recovered,
            elapsed,
            latency,
            node: current_node,
            scanned_blocks: _,
        } = sync_result;
        self.publish_event(UtxoScannerEvent::Progress {
            current_height: final_height,
            tip_height: final_height,
            current_node: current_node.clone(),
            latency,
        });
        self.publish_event(UtxoScannerEvent::Completed {
            final_height,
            num_recovered,
            value_recovered,
            time_taken: elapsed,
            latency,
            current_node,
        });
        debug!(
            target: LOG_TARGET,
            "{:?}: Published events 'UtxoScannerEvent::Progress(..{})' and 'UtxoScannerEvent::Completed(..{})'",
            self.mode, final_height, final_height,
        );

        if self.mode == UtxoScannerMode::Recovery {
            // Presence of scanning keys are used to determine if a wallet is busy with recovery or not.
            self.clear_recovery_mode()?;
        }
        Ok(())
    }

    /// Try to instantiate a Base Node Wallet Service client.
    fn base_node_wallet_service_client(&self) -> Result<TWalletClientFactory::Client, anyhow::Error> {
        Ok(self.resources.client_factory.create_http_client())
    }

    async fn determine_next_block_to_scan(
        &self,
        last_scanned_block: &Option<ScannedBlock>,
        wallet_service_client: &TWalletClientFactory::Client,
    ) -> Result<ScannedBlock, anyhow::Error> {
        if let Some(last_scanned_block) = last_scanned_block {
            let mut height = last_scanned_block.height;
            let mut next_header;
            // Keep going backwards until we find a header that is known to the base node
            loop {
                next_header = wallet_service_client.get_header_by_height(height + 1).await?;
                if next_header.is_some() {
                    break;
                }
                height = height.saturating_sub(1);
            }
            let next_header = next_header.expect("we check this above");
            let next_header_hash = next_header.hash;

            Ok(ScannedBlock {
                height: next_header.height,
                header_hash: next_header_hash,
                timestamp: Utc::now().naive_utc(),
            })
        } else {
            // The node does not know of any of our cached headers so we will start the scan anew from the
            // wallet birthday
            self.resources.db.clear_scanned_blocks()?;
            let wallet_birthday = match self.resources.db.get_wallet_type()? {
                Some(WalletType::ProvidedKeys(wallet)) => Some(wallet.birthday.unwrap_or_default()),
                _ => None,
            };
            let scanning_start_height_hash = self
                .get_scanning_start_header_height_hash(wallet_service_client, wallet_birthday)
                .await?;

            Ok(ScannedBlock {
                height: scanning_start_height_hash.height,
                header_hash: scanning_start_height_hash.header_hash,
                timestamp: Utc::now().naive_utc(),
            })
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn attempt_sync(&mut self) -> Result<SyncResult, anyhow::Error> {
        info!(target: LOG_TARGET, "Starting UTXO scanning task");

        let wallet_service_client = self.base_node_wallet_service_client()?;

        let timer = Instant::now();
        let mut total_num_recovered = 0;
        let mut total_value_recovered = MicroMinotari::zero();
        let mut scanned_blocks = 0;
        loop {
            let (tip_hash, tip_height) = self.get_chain_tip_header(&wallet_service_client).await?;
            let last_scanned_block = self.get_last_scanned_block(&wallet_service_client, tip_height).await?;

            // check if we are already synced.
            if let Some(last_scanned_block) = &last_scanned_block {
                if last_scanned_block.header_hash == tip_hash {
                    debug!(
                        target: LOG_TARGET,
                        "{:?}: Scanning complete to current tip (height: {}) in {:.2?}",
                        self.mode,
                        last_scanned_block.height,
                        timer.elapsed()
                    );
                    let latency = wallet_service_client
                        .get_last_request_latency()
                        .await
                        .unwrap_or_default();
                    let node = wallet_service_client.get_address().await;
                    return Ok(SyncResult {
                        final_height: last_scanned_block.height,
                        num_recovered: total_num_recovered,
                        value_recovered: total_value_recovered,
                        scanned_blocks,
                        elapsed: timer.elapsed(),
                        latency,
                        node,
                    });
                }
            }

            // Otherwise choose a starting point for the scan
            let next_block_to_scan = self
                .determine_next_block_to_scan(&last_scanned_block, &wallet_service_client)
                .await?;

            if self.shutdown_signal.is_triggered() {
                return Err(anyhow!("Shutdown signal received, stopping UTXO scanning task"));
            }

            info!(
                target: LOG_TARGET,
                "{:?}: Scanning UTXO's from height = {} to current tip_height = {} (starting header_hash: {})",
                self.mode,
                next_block_to_scan.height,
                tip_height,
                next_block_to_scan.header_hash.to_hex(),
            );

            let (num_scanned, num_recovered, amount_recovered) = self
                .scan_utxos(
                    &wallet_service_client,
                    next_block_to_scan.header_hash,
                    tip_hash,
                    tip_height,
                )
                .await?;
            scanned_blocks += 1;
            total_num_recovered += num_recovered;
            total_value_recovered += amount_recovered;
            debug!(
                target: LOG_TARGET,
                "Scanning round completed up to height {} in {:.2?} ({} outputs scanned)",
                tip_height,
                timer.elapsed(),
                num_scanned,
            );
        }
    }

    async fn get_chain_tip_header(
        &self,
        client: &TWalletClientFactory::Client,
    ) -> Result<(BlockHash, u64), anyhow::Error> {
        let tip_info = client.get_tip_info().await?;

        Ok((
            tip_info
                .metadata
                .as_ref()
                .map(|m| *m.best_block_hash())
                .unwrap_or_else(FixedHash::default),
            tip_info.metadata.as_ref().map(|m| m.best_block_height()).unwrap_or(0),
        ))
    }

    async fn get_last_scanned_block(
        &self,
        client: &TWalletClientFactory::Client,
        current_tip_height: u64,
    ) -> Result<Option<ScannedBlock>, anyhow::Error> {
        let scanned_blocks = self.resources.db.get_scanned_blocks()?;
        debug!(
            target: LOG_TARGET,
            "{:?}: Found {} cached previously scanned blocks",
            self.mode,
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
        for sb in scanned_blocks {
            // The scanned block has a higher height than the current tip, meaning the previously scanned block was
            // reorged out.
            if sb.height > current_tip_height {
                last_missing_scanned_block = Some(sb);
                continue;
            }

            if found_scanned_block.is_none() {
                let header = client.get_header_by_height(sb.height).await?;
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
        }

        if let Some(block) = last_missing_scanned_block {
            warn!(
                target: LOG_TARGET,
                "{:?}: Reorg detected on base node. Removing scanned blocks from height {}", self.mode, block.height
            );
            self.resources.db.clear_scanned_blocks_from_and_higher(block.height)?;
        }

        if let Some(sb) = found_scanned_block {
            debug!(
                target: LOG_TARGET,
                "{:?}: Last scanned block found at height {} (Header Hash: {})",
                self.mode,
                sb.height,
                sb.header_hash.to_hex()
            );
            Ok(Some(ScannedBlock {
                height: sb.height,
                header_hash: sb.header_hash,
                timestamp: Utc::now().naive_utc(),
            }))
        } else {
            warn!(
                target: LOG_TARGET,
                "{:?}: Reorg detected on base node. No previously scanned block headers found, resuming scan from wallet \
                 birthday", self.mode
            );
            Ok(None)
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn scan_utxos(
        &mut self,
        client: &TWalletClientFactory::Client,
        start_header_hash: HashOutput,
        end_header_hash: HashOutput,
        tip_height: u64,
    ) -> Result<(usize, u64, MicroMinotari), anyhow::Error> {
        info!(
            target: LOG_TARGET,
            "Starting UTXO scanning from header hash {} to header hash {} at tip height {}",
            start_header_hash.to_hex(),
            end_header_hash.to_hex(),
            tip_height
        );
        // Setting how often the progress event and log should occur during scanning. Defined in blocks
        const PROGRESS_REPORT_INTERVAL: u64 = 10;

        let mut total_scanned = 0;
        let mut total_num_recovered = 0;
        let mut total_value_recovered = MicroMinotari::zero();

        let mut utxo_stream = client
            .sync_utxos_by_block(
                start_header_hash.to_vec(),
                end_header_hash.to_vec(),
                self.shutdown_signal.clone(),
            )
            .await?;

        let mut prev_scanned_block: Option<ScannedBlock> = None;
        while let Some(response) = utxo_stream.recv().await {
            if self.shutdown_signal.is_triggered() {
                return Ok((total_scanned, total_num_recovered, total_value_recovered));
            }

            let response = response?;
            #[allow(clippy::cast_possible_wrap)]
            for response in response.blocks {
                let current_height = response.height;
                let current_header_hash = response.header_hash;
                let mined_timestamp = DateTime::<Utc>::from_timestamp(response.mined_timestamp as i64, 0)
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                let outputs = response.outputs;
                total_scanned += outputs.len();

                let found_outputs = self.search_for_owned_outputs(outputs).await?;

                if found_outputs.is_empty() {
                    debug!(
                        target: LOG_TARGET,
                        "No recoverable outputs found in block at height {} with header hash {}",
                        current_height,
                        current_header_hash.to_hex()
                    );
                } else {
                    // Now download the whole block and import the outputs
                    info!(
                        target: LOG_TARGET,
                        "Found {} recoverable outputs in block at height {} with header hash {}",
                        found_outputs.len(),
                        current_height,
                        current_header_hash.to_hex()
                    );
                    let block = client.get_utxos_by_block(current_header_hash.to_vec()).await?;

                    let outputs = block
                        .outputs
                        .iter()
                        .filter(|o| found_outputs.iter().any(|f| f.commitment == o.commitment.as_bytes()))
                        .cloned()
                        .collect::<Vec<_>>();

                    let imported_outputs = self.scan_for_outputs(outputs).await?;

                    let (num_recovered, amount) = self
                        .import_utxos_to_transaction_service(&imported_outputs, current_height, mined_timestamp)
                        .await?;
                    total_num_recovered += num_recovered;
                    total_value_recovered += amount;
                }

                let block_hash: FixedHash = current_header_hash.try_into()?;
                if let Some(scanned_block) = prev_scanned_block {
                    if block_hash != scanned_block.header_hash {
                        debug!(
                            target: LOG_TARGET,
                            "Saving scanned block at height {} with header hash {}",
                            current_height,
                            block_hash.to_hex()
                        );
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

                            let latency = client.get_last_request_latency().await.unwrap_or_default();
                            let node = client.get_address().await;
                            self.publish_event(UtxoScannerEvent::Progress {
                                current_height,
                                tip_height,
                                current_node: node,
                                latency,
                            });
                        }
                    }
                }
                prev_scanned_block = Some(ScannedBlock {
                    header_hash: block_hash,
                    height: current_height,
                    timestamp: Utc::now().naive_utc(),
                });
            }
        }
        // We need to update the last one
        if let Some(scanned_block) = prev_scanned_block {
            self.resources.db.clear_scanned_blocks_before_height(
                scanned_block.height.saturating_sub(SCANNED_BLOCK_CACHE_SIZE),
                true,
            )?;
            self.resources.db.save_scanned_block(scanned_block)?;
        }

        Ok((total_scanned, total_num_recovered, total_value_recovered))
    }

    async fn search_for_owned_outputs(
        &mut self,
        outputs: Vec<MinimalUtxoSyncInfo>,
    ) -> Result<Vec<MinimalUtxoSyncInfo>, anyhow::Error> {
        let mut found_outputs: Vec<MinimalUtxoSyncInfo> = Vec::new();
        let start = Instant::now();
        let view_key = self.key_manager.get_view_key().await?;
        for output in outputs {
            let commitment = CompressedCommitment::from_canonical_bytes(&output.commitment)
                .map_err(|e| anyhow!("Not a valid commitment: {}", e.to_string()))?;
            let encrypted = EncryptedData::from_bytes(&output.encrypted_data)?;

            // Change outputs just use the view key.
            if self
                .key_manager
                .try_output_key_recovery(&commitment, &encrypted, None)
                .await
                .ok()
                .is_some()
            {
                found_outputs.push(output.clone());
                continue;
            }

            // Received output use the DH of view key and sender offset.
            let offset_pub_key = CompressedKey::from_canonical_bytes(&output.sender_offset_public_key)
                .map_err(|e| anyhow!("Sender offset is not a valid public key:{}", e.to_string()))?;
            let shared_secret = self
                .key_manager
                .get_diffie_hellman_shared_secret(&view_key.key_id, &offset_pub_key)
                .await?;
            let recovery_key = shared_secret_to_output_encryption_key(&shared_secret)
                .map_err(|e| anyhow!("Could not hash key :{}", e.to_string()))?;
            if EncryptedData::decrypt_data(&recovery_key, &commitment, &encrypted)
                .ok()
                .is_some()
            {
                found_outputs.push(output.clone());
            }
        }
        let scanned_time = start.elapsed();
        let start = Instant::now();

        let one_sided_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "Scanned for outputs: outputs took {} ms , one-sided took {} ms",
            scanned_time.as_millis(),
            one_sided_time.as_millis(),
        );
        Ok(found_outputs)
    }

    async fn scan_for_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<(WalletOutput, ImportStatus, TxId, TransactionOutput)>, anyhow::Error> {
        let mut found_outputs: Vec<(WalletOutput, ImportStatus, TxId, TransactionOutput)> = Vec::new();
        let start = Instant::now();
        found_outputs.append(
            &mut self
                .resources
                .output_manager_service
                .scan_for_recoverable_outputs(outputs.clone().into_iter().map(|o| (o, None)).collect())
                .await?
                .into_iter()
                .map(|ro| -> Result<_, anyhow::Error> {
                    let status = if ro.output.features.is_coinbase() {
                        ImportStatus::CoinbaseUnconfirmed
                    } else {
                        ImportStatus::Imported
                    };
                    let output = outputs
                        .iter()
                        .find(|o| o.hash() == ro.hash)
                        .ok_or_else(|| anyhow!("Output '{}' not found", ro.hash.to_hex()))?;
                    Ok((ro.output, status, ro.tx_id, output.clone()))
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
                .map(|ro| -> Result<_, anyhow::Error> {
                    let status = if ro.output.features.is_coinbase() {
                        ImportStatus::CoinbaseUnconfirmed
                    } else {
                        ImportStatus::OneSidedUnconfirmed
                    };
                    let output = outputs
                        .iter()
                        .find(|o| o.hash() == ro.hash)
                        .ok_or_else(|| anyhow!("Output '{}' not found", ro.hash.to_hex()))?;
                    Ok((ro.output, status, ro.tx_id, output.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let one_sided_time = start.elapsed();
        trace!(
            target: LOG_TARGET,
            "{:?}: Scanned for outputs: outputs took {} ms , one-sided took {} ms",
            self.mode,
            scanned_time.as_millis(),
            one_sided_time.as_millis(),
        );
        Ok(found_outputs)
    }

    async fn import_utxos_to_transaction_service(
        &mut self,
        utxos: &[(WalletOutput, ImportStatus, TxId, TransactionOutput)],
        current_height: u64,
        mined_timestamp: DateTime<Utc>,
    ) -> Result<(u64, MicroMinotari), anyhow::Error> {
        let mut num_recovered = 0u64;
        let mut total_amount = MicroMinotari::from(0);
        for (wo, import_status, tx_id, to) in utxos {
            let source_address = if wo.features.is_coinbase() {
                // It's a coinbase, so we know we mined it (we do mining with cold wallets).
                self.resources.one_sided_tari_address.clone()
            } else {
                match &wo.payment_id {
                    PaymentId::AddressAndData {
                        sender_address: address,
                        ..
                    } => address.clone(),
                    PaymentId::TransactionInfo { .. } => self.resources.one_sided_tari_address.clone(),
                    _ => TariAddress::default(),
                }
            };
            match self
                .import_key_manager_utxo_to_transaction_service(
                    wo.clone(),
                    source_address,
                    import_status.clone(),
                    *tx_id,
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
                        "{:?}: Recoverer attempted to add a duplicate output to the database for faux transaction ({}); \
                         ignoring it as this is not a real error",
                        self.mode,
                        tx_id
                    );
                },
                Err(e) => return Err(e.into()),
            }
        }
        Ok((num_recovered, total_amount))
    }

    fn set_recovery_mode(&self) -> Result<(), anyhow::Error> {
        self.resources
            .db
            .set_client_key_value(RECOVERY_KEY.to_owned(), Utc::now().to_string())?;
        Ok(())
    }

    fn clear_recovery_mode(&self) -> Result<(), anyhow::Error> {
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
            "{:?}: UTXO with value {},  imported into wallet as 'ImportStatus::{}'",
            self.mode, wallet_output.value, import_status
        );

        Ok(tx_id)
    }

    async fn get_scanning_start_header_height_hash(
        &self,
        client: &TWalletClientFactory::Client,
        option_birthday: Option<u16>,
    ) -> Result<HeightHash, anyhow::Error> {
        let birthday = match option_birthday {
            Some(birthday) => birthday,
            None => self.resources.db.get_wallet_birthday()?,
        };
        let epoch_time_birthday = get_birthday_from_unix_epoch_in_seconds(birthday, 0);
        debug!(
            target: LOG_TARGET,
            "Wallet birthday is {} at epoch time {}",
            birthday,
            epoch_time_birthday
        );
        let epoch_time_scanning_start = get_birthday_from_unix_epoch_in_seconds(birthday, self.birthday_offset);
        let block_height_scanning_start = client
            .get_height_at_time(epoch_time_scanning_start)
            .await
            .unwrap_or_else(|e| {
                warn!(target: LOG_TARGET, "{:?}: Problem requesting `height_at_time` from Base Node: {}", self.mode, e);
                0
            });
        let header = match client.get_header_by_height(block_height_scanning_start).await? {
            Some(header) => header,
            None => {
                warn!(
                    target: LOG_TARGET,
                    "No block header found at height {} for birthday {}",
                    block_height_scanning_start,
                    birthday
                );
                return Err(anyhow!("No block header found at scanning start height"));
            },
        };
        let header_hash_scanning_start = header.hash;
        info!(
            target: LOG_TARGET,
            "Fresh wallet recovery/scanning: Wallet birthday '{}' at epoch time '{}' , scanning \
            from epoch time '{}' at block height '{}' with header hash '{}'",
            birthday,
            epoch_time_birthday,
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

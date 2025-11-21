// Copyright 2019. The Tari Project
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
use std::{collections::HashMap, fmt, fmt::Display, ops::Range, sync::Arc};

use diesel::result::{DatabaseErrorKind, Error as DieselError};
use futures::{pin_mut, StreamExt};
use log::*;
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use minotari_node_wallet_client::BaseNodeWalletClient;
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::TxId,
    types::{
        BlockHash,
        CompressedCommitment,
        CompressedPublicKey,
        FixedHash,
        HashOutput,
        PrivateKey,
        UncompressedCommitment,
        UncompressedPublicKey,
    },
};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_script::{
    inputs,
    push_pubkey_script,
    script,
    CompressedCheckSigSchnorrSignature,
    ExecutionStack,
    Opcode,
    StackItem,
    TariScript,
};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    consensus::ConsensusConstants,
    crypto_factories::CryptoFactories,
    fee::Fee,
    helpers::borsh::SerializedSize,
    key_manager::{SerializedKeyString, TariKeyAndId, TariKeyId},
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        one_sided::{public_key_to_output_encryption_key, public_key_to_output_spending_key},
        EncryptedData,
        KernelFeatures,
        OutputFeatures,
        RangeProofType,
        Transaction,
        TransactionError,
        TransactionOutput,
        TransactionOutputVersion,
        WalletOutput,
        WalletOutputBuilder,
    },
    tx_outputs_to_tx_id,
    MicroMinotari,
    TransactionBuilder,
};
use tari_transaction_key_manager::legacy_key_manager::{wallet_types::FeeType, LegacyTransactionKeyManagerInterface};
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{sync::Mutex, time::Instant};

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerProtocolError, OutputManagerStorageError},
        handle::{
            OutputManagerEvent,
            OutputManagerEventSender,
            OutputManagerRequest,
            OutputManagerResponse,
            RecoveredOutput,
        },
        input_selection::UtxoSelectionCriteria,
        recovery::StandardUtxoRecoverer,
        resources::OutputManagerResources,
        storage::{
            database::{OutputBackendQuery, OutputManagerBackend, OutputManagerDatabase},
            models::{DbWalletOutput, KnownOneSidedPaymentScript, SpendingPriority},
            sqlite_db::CoinBucket,
            OutputSource,
            OutputStatus,
        },
        tasks::TxoValidationTask,
        RangeLimit,
        TRANSACTION_INPUTS_LIMIT,
        TRANSACTION_OUTPUTS_LIMIT,
    },
    utxo_scanner_service::handle::{UtxoScannerEvent, UtxoScannerHandle},
};

const LOG_TARGET: &str = "wallet::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    resources: OutputManagerResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    request_stream: Option<
        reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse<TKeyManagerInterface>, OutputManagerError>,
        >,
    >,
    base_node_service: BaseNodeServiceHandle,
    validation_in_progress: Arc<Mutex<()>>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OutputManagerService<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: LegacyTransactionKeyManagerInterface,
{
    pub async fn new(
        config: OutputManagerServiceConfig,
        request_stream: reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse<TKeyManagerInterface>, OutputManagerError>,
        >,
        db: OutputManagerDatabase<TBackend>,
        event_publisher: OutputManagerEventSender,
        factories: CryptoFactories,
        consensus_constants: ConsensusConstants,
        shutdown_signal: ShutdownSignal,
        base_node_service: BaseNodeServiceHandle,
        network: Network,
        connectivity: TWalletConnectivity,
        key_manager: TKeyManagerInterface,
        utxo_scanner_handle: UtxoScannerHandle,
    ) -> Result<Self, OutputManagerError> {
        let view_key = key_manager.get_view_key();
        let spend_key = key_manager.get_spend_key();
        let one_sided_tari_address = TariAddress::new_dual_address(
            view_key.pub_key.clone(),
            spend_key.pub_key.clone(),
            network,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )?;
        let resources = OutputManagerResources {
            config,
            db,
            factories,
            connectivity,
            event_publisher,
            key_manager,
            consensus_constants,
            shutdown_signal,
            one_sided_tari_address,
            utxo_scanner_handle,
            network,
        };

        Ok(Self {
            resources,
            request_stream: Some(request_stream),
            base_node_service,
            validation_in_progress: Arc::new(Mutex::new(())),
        })
    }

    pub fn clear_short_term_encumberances(&self) -> Result<(), OutputManagerError> {
        self.resources
            .db
            .clear_short_term_encumberances()
            .map_err(OutputManagerError::from)
    }

    pub async fn start(mut self) -> Result<(), OutputManagerError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("OutputManagerService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown = self.resources.shutdown_signal.clone();

        let mut base_node_service_event_stream = self.base_node_service.get_event_stream();

        let mut utxo_scanner_events = self.resources.utxo_scanner_handle.get_event_receiver();

        debug!(target: LOG_TARGET, "Output Manager Service started");
        // Outputs marked as shorttermencumbered are not yet stored as transactions in the TMS, so lets clear them
        self.resources.db.clear_short_term_encumberances()?;
        loop {
            tokio::select! {
                event = base_node_service_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_base_node_service_event(msg),
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {e}"),
                    }
                },
                event = utxo_scanner_events.recv() => {
                    match event {
                        Ok(msg) => self.handle_utxo_scanner_service_event(msg),
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on utxo scanner event broadcast channel: {e}"),
                    }
                },
                Some(request_context) = request_stream.next() => {
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await.inspect_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {e:?}");
                    });
                    let _result = reply_tx.send(response).inspect_err(|_| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                    });
                },
                _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Output manager service shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
        info!(target: LOG_TARGET, "Output Manager Service ended");
        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    #[allow(clippy::too_many_lines)]
    async fn handle_request(
        &mut self,
        request: OutputManagerRequest,
    ) -> Result<OutputManagerResponse<TKeyManagerInterface>, OutputManagerError> {
        trace!(target: LOG_TARGET, "Handling Service Request: {request}");
        match request {
            OutputManagerRequest::AddOutput((uo, spend_priority)) => self
                .add_output(None, *uo, spend_priority)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddOutputWithTxId((tx_id, uo, spend_priority)) => self
                .add_output(Some(tx_id), *uo, spend_priority)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::EncumberAggregateUtxo {
                fee_per_gram,
                expected_commitment,
                script_input_shares,
                script_signature_public_nonces,
                sender_offset_public_key_shares,
                metadata_ephemeral_public_key_shares,
                dh_shared_secret_shares,
                recipient_address,
                original_maturity,
                use_output,
                payment_id,
            } => {
                self.encumber_aggregate_utxo(
                    fee_per_gram,
                    expected_commitment,
                    script_input_shares,
                    script_signature_public_nonces,
                    sender_offset_public_key_shares,
                    metadata_ephemeral_public_key_shares,
                    dh_shared_secret_shares,
                    recipient_address,
                    payment_id,
                    original_maturity,
                    RangeProofType::BulletProofPlus,
                    0.into(),
                    use_output,
                )
                .await
            },
            OutputManagerRequest::SpendBackupPreMineUtxo {
                fee_per_gram,
                output_hash,
                expected_commitment,
                recipient_address,
            } => self
                .spend_backup_pre_mine_utxo(
                    fee_per_gram,
                    output_hash,
                    expected_commitment,
                    recipient_address,
                    MemoField::new_open(output_hash.to_vec(), TxType::PaymentToOther)
                        .map_err(|e| OutputManagerError::ServiceError(format!("Invalid payment ID: {e}")))?,
                    0,
                    RangeProofType::BulletProofPlus,
                    0.into(),
                )
                .await
                .map(OutputManagerResponse::SpendBackupPreMineUtxo),
            OutputManagerRequest::AddUnvalidatedOutput((tx_id, uo, spend_priority)) => self
                .add_unvalidated_output(tx_id, *uo, spend_priority)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::UpdateOutputMetadataSignature(uo) => self
                .update_output_metadata_signature(*uo)
                .map(|_| OutputManagerResponse::OutputMetadataSignatureUpdated),
            OutputManagerRequest::GetBalance => {
                let current_tip_for_time_lock_calculation = self.resources.db.get_last_scanned_height()?;
                self.get_balance(current_tip_for_time_lock_calculation)
                    .map(OutputManagerResponse::Balance)
            },
            OutputManagerRequest::GetCoinBuckets { ranges } => {
                let current_tip_for_time_lock_calculation = self.resources.db.get_last_scanned_height()?;
                self.count_outputs_in_ranges(ranges, current_tip_for_time_lock_calculation)
                    .map(OutputManagerResponse::GetCoinBuckets)
            },
            OutputManagerRequest::GetBalancePaymentId(payment_id) => {
                let current_tip_for_time_lock_calculation = self.resources.db.get_last_scanned_height()?;
                self.get_balance_payment_id(current_tip_for_time_lock_calculation, payment_id)
                    .map(OutputManagerResponse::Balance)
            },
            OutputManagerRequest::GetTransactionBuilder {
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                script,
                covenant,
            } => self
                .prepare_transaction_to_send(
                    tx_id,
                    amount,
                    selection_criteria,
                    fee_per_gram,
                    *output_features,
                    script,
                    covenant,
                )
                .map(|tx_builder| OutputManagerResponse::TransactionBuilderToSend(Box::new(tx_builder))),
            OutputManagerRequest::GetTransactionBuilderRangeLimitedCoinJoin {
                tx_id,
                selection_criteria,
                output_features,
                fee,
                script,
                covenant,
            } => self
                .prepare_range_limited_coin_join_transaction_to_send(
                    tx_id,
                    selection_criteria,
                    fee,
                    *output_features,
                    script,
                    covenant,
                )
                .await
                .map(|tx_builder| OutputManagerResponse::TransactionBuilderToSend(Box::new(tx_builder))),
            OutputManagerRequest::CreatePayToSelfTransaction {
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                lock_height,
                payment_id,
                minimum_value_promise,
            } => self
                .create_pay_to_self_transaction(
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    lock_height,
                    payment_id,
                    minimum_value_promise,
                )
                .map(OutputManagerResponse::PayToSelfTransaction),
            OutputManagerRequest::FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => self
                .fee_estimate(amount, selection_criteria, fee_per_gram, num_kernels, num_outputs)
                .map(|(fee, input_count_selected, change)| {
                    OutputManagerResponse::FeeEstimate(fee, input_count_selected, change)
                }),
            OutputManagerRequest::ConfirmPendingTransaction {
                tx_id,
                tx_id_update,
                change_outputs,
            } => {
                let change_outputs = change_outputs.unwrap_or(Vec::new());
                self.confirm_encumberance(tx_id, tx_id_update, change_outputs)
                    .map(|_| OutputManagerResponse::PendingTransactionConfirmed)
            },
            OutputManagerRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .map(|_| OutputManagerResponse::TransactionCancelled),
            OutputManagerRequest::GetSpentOutputs => {
                let outputs = self.fetch_spent_outputs()?;
                Ok(OutputManagerResponse::SpentOutputs(outputs))
            },
            OutputManagerRequest::GetUnspentOutputs => {
                let outputs = self.fetch_unspent_outputs()?;
                Ok(OutputManagerResponse::UnspentOutputs(outputs))
            },
            OutputManagerRequest::GetOutputsByQuery(query) => {
                let outputs = self.fetch_outputs_by_query(query)?;
                Ok(OutputManagerResponse::SpentOutputs(outputs))
            },
            OutputManagerRequest::ValidateTxos => {
                self.validate_outputs().map(OutputManagerResponse::TxoValidationStarted)
            },
            OutputManagerRequest::RevalidateTxos => self
                .revalidate_outputs()
                .map(OutputManagerResponse::TxoValidationStarted),
            OutputManagerRequest::GetInvalidOutputs => {
                let outputs = self.fetch_invalid_outputs()?.into_iter().map(|v| v.into()).collect();
                Ok(OutputManagerResponse::InvalidOutputs(outputs))
            },
            OutputManagerRequest::GetManyOutputs { outputs } => {
                let outputs = self
                    .fetch_many_outputs(&outputs)?
                    .into_iter()
                    .map(|v| v.into())
                    .collect();
                Ok(OutputManagerResponse::Outputs(outputs))
            },
            OutputManagerRequest::PreviewCoinJoin((commitments, fee_per_gram)) => {
                Ok(OutputManagerResponse::CoinPreview(
                    self.preview_coin_join_with_commitments(commitments, fee_per_gram)
                        .await?,
                ))
            },
            OutputManagerRequest::ScrapeWallet { tx_id, fee_per_gram } => self
                .scrape_wallet(tx_id, fee_per_gram)
                .map(|tx_builder| OutputManagerResponse::TransactionBuilderToSend(Box::new(tx_builder))),

            OutputManagerRequest::PreviewCoinSplitEven((commitments, number_of_splits, fee_per_gram)) => {
                Ok(OutputManagerResponse::CoinPreview(
                    self.preview_coin_split_with_commitments_no_amount(commitments, number_of_splits, fee_per_gram)?,
                ))
            },
            OutputManagerRequest::CreateCoinSplit((commitments, amount_per_split, split_count, fee_per_gram)) => {
                if commitments.is_empty() {
                    self.create_coin_split_auto(Some(amount_per_split), split_count, fee_per_gram)
                        .map(OutputManagerResponse::Transaction)
                } else {
                    self.create_coin_split_with_commitments(
                        commitments,
                        Some(amount_per_split),
                        split_count,
                        fee_per_gram,
                    )
                    .map(OutputManagerResponse::Transaction)
                }
            },
            OutputManagerRequest::CreateCoinSplitEven((commitments, split_count, fee_per_gram)) => {
                if commitments.is_empty() {
                    self.create_coin_split_auto(None, split_count, fee_per_gram)
                        .map(OutputManagerResponse::Transaction)
                } else {
                    self.create_coin_split_with_commitments(commitments, None, split_count, fee_per_gram)
                        .map(OutputManagerResponse::Transaction)
                }
            },
            OutputManagerRequest::CreateCoinJoin {
                commitments,
                fee_per_gram,
                payment_id,
            } => self
                .create_coin_join(commitments, fee_per_gram, payment_id)
                .map(OutputManagerResponse::Transaction),

            OutputManagerRequest::ScanForRecoverableOutputs(outputs) => {
                StandardUtxoRecoverer::new(self.resources.key_manager.clone(), self.resources.db.clone())
                    .scan_and_recover_outputs(outputs)
                    .await
                    .map(OutputManagerResponse::RewoundOutputs)
            },
            OutputManagerRequest::ScanOutputs(outputs) => self
                .scan_outputs_for_one_sided_payments(outputs)
                .map(OutputManagerResponse::ScanOutputs),
            OutputManagerRequest::ScanOutputsForMultisig(outputs) => self
                .scan_outputs_for_multisig(outputs)
                .map(OutputManagerResponse::ScanOutputs),
            OutputManagerRequest::AddKnownOneSidedPaymentScript(known_script) => self
                .add_known_script(known_script)
                .map(|_| OutputManagerResponse::AddKnownOneSidedPaymentScript),
            OutputManagerRequest::ReinstateCancelledInboundTx(tx_id) => self
                .reinstate_cancelled_inbound_transaction_outputs(tx_id)
                .map(|_| OutputManagerResponse::ReinstatedCancelledInboundTx),
            OutputManagerRequest::CreateOutputWithFeatures { value, features } => {
                let wallet_output = self.create_output_with_features(value, *features)?;
                Ok(OutputManagerResponse::CreateOutputWithFeatures {
                    output: Box::new(wallet_output),
                })
            },

            OutputManagerRequest::CreateClaimShaAtomicSwapTransaction(output_hash, pre_image, fee_per_gram) => {
                self.claim_sha_atomic_swap_with_hash(output_hash, pre_image, fee_per_gram)
                    .await
            },
            OutputManagerRequest::CreateHtlcRefundTransaction(output, fee_per_gram) => self
                .create_htlc_refund_transaction(output, fee_per_gram)
                .map(OutputManagerResponse::ClaimHtlcTransaction),
            OutputManagerRequest::GetOutputInfoByTxId(tx_id) => {
                let output_statuses_by_tx_id = self.get_output_info_by_tx_id(tx_id)?;
                Ok(OutputManagerResponse::OutputInfoByTxId(output_statuses_by_tx_id))
            },

            OutputManagerRequest::FetchUnspentOutputs(hashes) => {
                let mut outputs = Vec::new();
                for hash in hashes {
                    if let Some(output) = self.fetch_unspent_outputs_from_node(hash).await? {
                        outputs.push(output);
                    }
                }
                Ok(OutputManagerResponse::FetchUnspentOutputs(outputs))
            },
            OutputManagerRequest::ClearShortTermEncumberances => self
                .clear_short_term_encumberances()
                .map(|_| OutputManagerResponse::ClearShortTermEncumberances),
        }
    }

    fn get_output_info_by_tx_id(&self, tx_id: TxId) -> Result<OutputInfoByTxId, OutputManagerError> {
        let outputs = self
            .resources
            .db
            .fetch_outputs_by_tx_id(tx_id, &self.resources.key_manager)?;
        let statuses = outputs.clone().into_iter().map(|uo| uo.status).collect();
        // We need the maximum mined height and corresponding block hash (faux transactions outputs can have different
        // mined heights)
        let (mut last_height, mut max_mined_height, mut block_hash) = (0u64, None, None);
        for uo in outputs {
            if let Some(height) = uo.mined_height {
                if last_height < height {
                    last_height = height;
                    max_mined_height = uo.mined_height;
                    block_hash = uo.mined_in_block;
                }
            }
        }
        Ok(OutputInfoByTxId {
            statuses,
            mined_height: max_mined_height,
            block_hash,
        })
    }

    async fn claim_sha_atomic_swap_with_hash(
        &mut self,
        output_hash: HashOutput,
        pre_image: CompressedPublicKey,
        fee_per_gram: MicroMinotari,
    ) -> Result<OutputManagerResponse<TKeyManagerInterface>, OutputManagerError> {
        let output = self
            .resources
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .fetch_utxo(output_hash.to_vec())
            .await
            .map_err(|e| OutputManagerError::BaseNodeClientError(e.to_string()))?
            .ok_or_else(|| {
                OutputManagerError::BaseNodeClientError(format!("No output found for hash {}", output_hash.to_hex()))
            })?;

        self.create_claim_sha_atomic_swap_transaction(output, pre_image, fee_per_gram)
            .map(OutputManagerResponse::ClaimHtlcTransaction)
    }

    fn handle_utxo_scanner_service_event(&mut self, event: UtxoScannerEvent) {
        match event {
            UtxoScannerEvent::ScanningRoundFailed { .. } => {},
            UtxoScannerEvent::Progress { .. } => {},
            UtxoScannerEvent::Completed { .. } => {
                let _id = self.validate_outputs().inspect_err(|e| {
                    warn!(target: LOG_TARGET, "Error validating  txos: {e:?}");
                });
            },
        }
    }

    fn handle_base_node_service_event(&mut self, event: Arc<BaseNodeEvent>) {
        match (*event).clone() {
            BaseNodeEvent::BaseNodeStateChanged(_state) => {
                trace!(
                    target: LOG_TARGET,
                    "Received Base Node State Change but no block changes"
                );
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn validate_outputs(&mut self) -> Result<u64, OutputManagerError> {
        let id = OsRng.next_u64();
        let txo_validation = TxoValidationTask::new(
            id,
            self.resources.db.clone(),
            self.resources.connectivity.clone(),
            self.resources.event_publisher.clone(),
            self.resources.config.clone(),
            self.resources.key_manager.clone(),
        );

        let mut shutdown = self.resources.shutdown_signal.clone();
        let event_publisher = self.resources.event_publisher.clone();
        let validation_in_progress = self.validation_in_progress.clone();
        let mut utxo_scanner_service_event_stream = self.resources.utxo_scanner_handle.get_event_receiver();
        let mut num_resets = 0;
        tokio::spawn(async move {
            // Note: We do not want the validation task to be queued
            let mut _lock = match validation_in_progress.try_lock() {
                Ok(val) => val,
                _ => {
                    if let Err(e) = event_publisher.send(Arc::new(OutputManagerEvent::TxoValidationAlreadyBusy(id))) {
                        debug!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {e:?}"
                        );
                    }
                    debug!(
                        target: LOG_TARGET,
                        "UTXO Validation Protocol (Id: {id}) spawned while a previous protocol was busy, ignored"
                    );
                    return;
                },
            };
            'outer: loop {
                let local_run = txo_validation.clone();
                let exec_fut = local_run.execute();
                tokio::pin!(exec_fut);
                loop {
                    tokio::select! {
                        result = &mut exec_fut => {
                            match result {
                                Ok(id) => {
                                    info!(
                                        target: LOG_TARGET,
                                        "UTXO Validation Protocol (Id: {id}) completed successfully"
                                    );
                                    return;
                                },
                                Err(OutputManagerProtocolError { id, error }) => {
                                    warn!(
                                        target: LOG_TARGET,
                                        "Error completing UTXO Validation Protocol (Id: {id}): {error}"
                                    );
                                    let event_payload = match error {
                                        OutputManagerError::InconsistentBaseNodeDataError(_) |
                                        OutputManagerError::BaseNodeChanged |
                                        OutputManagerError::Shutdown |
                                        OutputManagerError::RpcError(_) =>
                                            OutputManagerEvent::TxoValidationCommunicationFailure(id),
                                        _ => OutputManagerEvent::TxoValidationInternalFailure(id),
                                    };
                                    if let Err(e) = event_publisher.send(Arc::new(event_payload)) {
                                        debug!(
                                            target: LOG_TARGET,
                                            "Error sending event because there are no subscribers: {e:?}"
                                        );
                                    }

                                    return;
                                },
                            }
                        },
                        _ = shutdown.wait() => {
                            debug!(target: LOG_TARGET, "TXO Validation Protocol (Id: {id}) shutting down because the system \
                                is shutting down");
                            return;
                        },
                        event = utxo_scanner_service_event_stream.recv() => {
                            if let Ok(UtxoScannerEvent::Completed{..}) = event {
                                num_resets += 1;
                                debug!(target: LOG_TARGET, "TXO Validation Protocol (Id: {id}) resetting because base node height changed");
                                // We limit the number of resets to avoid infinite loops, if the block validation takes longer than new blocks coming in, we want to at least finish the validation
                                if num_resets < 1{
                                    continue 'outer;
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(id)
    }

    fn revalidate_outputs(&mut self) -> Result<u64, OutputManagerError> {
        self.resources.db.set_outputs_to_be_revalidated()?;
        self.validate_outputs()
    }

    /// Add a key manager recoverable output to the outputs table and mark it as `Unspent`.
    pub fn add_output(
        &mut self,
        tx_id: Option<TxId>,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value()
        );

        let output = DbWalletOutput::from_wallet_output(output, spend_priority, OutputSource::default(), tx_id, None);
        debug!(
            target: LOG_TARGET,
            "saving output of hash {} to Output Manager",
            output.hash.to_hex()
        );
        match tx_id {
            None => self
                .resources
                .db
                .add_unspent_output(output, &self.resources.key_manager)?,
            Some(t) => self
                .resources
                .db
                .add_unspent_output_with_tx_id(t, output, &self.resources.key_manager)?,
        }
        Ok(())
    }

    /// Add a key manager output to the outputs table and marks is as `EncumberedToBeReceived`. This is so that it will
    /// require a successful validation to confirm that it indeed spendable.
    pub fn add_unvalidated_output(
        &mut self,
        tx_id: TxId,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add unvalidated output of value {} to Output Manager with TxId {}", output.value(), tx_id
        );
        let output =
            DbWalletOutput::from_wallet_output(output, spend_priority, OutputSource::default(), Some(tx_id), None);
        trace!(target: LOG_TARGET, "TxId: {tx_id}, {output:?}");
        self.resources.db.add_unvalidated_output(tx_id, output)?;

        // Because we added new outputs, let try to trigger a validation for them
        self.validate_outputs()?;
        Ok(())
    }

    /// Update an output's metadata signature, akin to 'finalize output'
    pub fn update_output_metadata_signature(&mut self, output: TransactionOutput) -> Result<(), OutputManagerError> {
        self.resources.db.update_output_metadata_signature(output)?;
        Ok(())
    }

    fn create_output_with_features(
        &mut self,
        value: MicroMinotari,
        features: OutputFeatures,
    ) -> Result<WalletOutputBuilder, OutputManagerError> {
        let (commitment_mask_key, script_key) = self.resources.key_manager.get_next_commitment_mask_and_script_key()?;
        let input_data = ExecutionStack::default();
        let script = TariScript::default();

        Ok(WalletOutputBuilder::new(value, commitment_mask_key.key_id)
            .with_features(features)
            .with_script(script)
            .with_input_data(input_data)
            .with_script_key(script_key.key_id))
    }

    fn get_balance(&self, current_tip_for_time_lock_calculation: Option<u64>) -> Result<Balance, OutputManagerError> {
        let balance = self.resources.db.get_balance(current_tip_for_time_lock_calculation)?;
        trace!(target: LOG_TARGET, "Balance: {balance:?}");
        Ok(balance)
    }

    fn count_outputs_in_ranges(
        &self,
        ranges: Vec<Range<u64>>,
        tip_height: Option<u64>,
    ) -> Result<Vec<CoinBucket>, OutputManagerError> {
        let coin_buckets = self.resources.db.count_outputs_in_ranges(ranges, tip_height)?;
        trace!(target: LOG_TARGET, "Coin buckets: {:?}", coin_buckets
            .iter()
            .map(|v| {
                format!(
                    "count: {}, value: {}, range: {}..{}",
                    v.number_of_outputs, v.total_value, v.range.start, v.range.end
                )
            })
            .collect::<Vec<_>>());
        Ok(coin_buckets)
    }

    fn get_balance_payment_id(
        &self,
        current_tip_for_time_lock_calculation: Option<u64>,
        payment_id: Vec<u8>,
    ) -> Result<Balance, OutputManagerError> {
        let balance = self
            .resources
            .db
            .get_balance_payment_id(current_tip_for_time_lock_calculation, payment_id)?;
        trace!(target: LOG_TARGET, "Balance: {balance:?}");
        Ok(balance)
    }

    /// Get a fee estimate for an amount of MicroMinotari, at a specified fee per gram and given number of kernels and
    /// outputs.
    fn fee_estimate(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<(MicroMinotari, usize, bool), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Getting fee estimate. Amount: {amount}. Fee per gram: {fee_per_gram}. Num kernels: {num_kernels}. Num outputs: {num_outputs}"
        );
        // We assume that default OutputFeatures and PushPubKey TariScript is used
        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                OutputFeatures::default()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    TariScript::default()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    Covenant::new()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            );

        let utxo_selection = match self.select_utxos(
            amount,
            selection_criteria,
            fee_per_gram,
            num_outputs,
            features_and_scripts_byte_size * num_outputs,
        ) {
            Ok(v) => Ok(v),
            Err(OutputManagerError::FundsPending | OutputManagerError::NotEnoughFunds) => {
                debug!(
                    target: LOG_TARGET,
                    "We dont have enough funds available to make a fee estimate, so we estimate 1 input, no change"
                );
                let fee_calc = self.get_fee_calc();
                let output_features_estimate = OutputFeatures::default();

                let default_features_and_scripts_size = fee_calc.weighting().round_up_features_and_scripts_size(
                    output_features_estimate
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                        TariScript::default()
                            .get_serialized_size()
                            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                        Covenant::new()
                            .get_serialized_size()
                            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
                );
                let fee = fee_calc.calculate(fee_per_gram, 1, 1, num_outputs, default_features_and_scripts_size);
                return Ok((fee, 1, false));
            },
            Err(e) => Err(e),
        }?;

        debug!(target: LOG_TARGET, "{} utxos selected.", utxo_selection.utxos.len());

        let fee = utxo_selection.as_final_fee();
        let utxo_count = utxo_selection.num_selected();
        let change_count = utxo_selection.requires_change_output();

        debug!(target: LOG_TARGET, "Fee calculated: {fee}");
        Ok((fee, utxo_count, change_count))
    }

    /// Prepare a Sender Transaction Protocol for the amount and fee_per_gram specified. If required a change output
    /// will be produced.
    pub fn prepare_transaction_to_send(
        &mut self,
        tx_id: TxId,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        recipient_output_features: OutputFeatures,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
    ) -> Result<TransactionBuilder<TKeyManagerInterface>, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Preparing to send transaction - TxId: {tx_id}, amount: {amount}, fee per gram: {fee_per_gram}, selection: {selection_criteria}"
        );
        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                recipient_output_features
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    recipient_script
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    recipient_covenant
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            );

        let input_selection = self.select_utxos(
            amount,
            selection_criteria,
            fee_per_gram,
            1,
            features_and_scripts_byte_size,
        )?;

        let mut builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        builder
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount);

        for uo in input_selection.iter() {
            builder.with_input(uo.wallet_output.clone())?;
        }
        debug!(
            target: LOG_TARGET,
            "Calculated fee for tx: Fee per gram: {}. Fee {}. Num inputs: {}.",
            fee_per_gram,
            input_selection.as_final_fee(),
            input_selection.num_selected(),
        );

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), vec![])?;

        debug!(target: LOG_TARGET, "Prepared transaction (TxId: {tx_id}) to send");

        Ok(builder)
    }

    /// Prepare a Sender Transaction Protocol for a range limited coin-join and fee_per_gram specified. No change output
    /// will be produced.
    pub async fn prepare_range_limited_coin_join_transaction_to_send(
        &mut self,
        tx_id: TxId,
        selection_criteria: UtxoSelectionCriteria,
        fee: FeeType,
        recipient_output_features: OutputFeatures,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
    ) -> Result<TransactionBuilder<TKeyManagerInterface>, OutputManagerError> {
        let target_minimum_amount = selection_criteria
            .clone()
            .range_limit
            .ok_or_else(|| OutputManagerError::RangeLimitError {
                reason: "Range limit must be specified for range limited coin-join UTXO selection".to_string(),
                range_exhausted: false,
            })?
            .target_minimum_amount;
        debug!(
            target: LOG_TARGET,
            "Preparing to send range limited coin join transaction - TxId: {tx_id}, target_minimum_amount: \
            {target_minimum_amount}, fee: {fee}, selection: {selection_criteria}"
        );
        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                recipient_output_features
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    recipient_script
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    recipient_covenant
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            );

        let input_selection = self
            .select_utxos_for_range_limited_coin_join(selection_criteria, fee, features_and_scripts_byte_size)
            .await?;

        let mut builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        builder
            .with_fee(input_selection.as_final_fee())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount);

        for uo in input_selection.iter() {
            builder.with_input(uo.wallet_output.clone())?;
        }
        debug!(
            target: LOG_TARGET,
            "TxId: {}, input(s) value: {}, amount: {}, fee {}, final fee: {}, num inputs: {}.",
            tx_id,
            input_selection.total_value(),
            input_selection.total_value() - input_selection.as_final_fee(),
            fee,
            input_selection.as_final_fee(),
            input_selection.num_selected(),
        );

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), vec![])?;

        debug!(target: LOG_TARGET, "Prepared transaction (TxId: {tx_id}) to send");

        Ok(builder)
    }

    async fn pre_mine_script_key_from_payment_id(
        &self,
        payment_id: MemoField,
        tx_id: TxId,
    ) -> Result<TariKeyAndId, OutputManagerError> {
        let index = payment_id
            .get_u64_data()
            .map_err(|e| OutputManagerError::InvalidPaymentIdFormat(format!("TxId: {tx_id}, {e}")))?;
        let script_key_id = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::PreMine,
            index,
        };
        Ok(TariKeyAndId {
            pub_key: self.resources.key_manager.get_public_key_at_key_id(&script_key_id)?,
            key_id: script_key_id,
        })
    }

    /// Create a partial transaction in order to prepare output
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::mutable_key_type)]
    pub async fn encumber_aggregate_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        expected_commitment: CompressedCommitment,
        mut script_input_shares: HashMap<CompressedPublicKey, CompressedCheckSigSchnorrSignature>,
        script_signature_public_nonces: Vec<CompressedPublicKey>,
        sender_offset_public_key_shares: Vec<CompressedPublicKey>,
        metadata_ephemeral_public_key_shares: Vec<CompressedPublicKey>,
        dh_shared_secret_shares: Vec<CompressedPublicKey>,
        recipient_address: TariAddress,
        tx_payment_id: MemoField,
        original_maturity: u64,
        range_proof_type: RangeProofType,
        minimum_value_promise: MicroMinotari,
        use_output: UseOutput,
    ) -> Result<OutputManagerResponse<TKeyManagerInterface>, OutputManagerError> {
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: start");
        // Fetch the output from the blockchain or use provided
        let output = match use_output {
            UseOutput::FromBlockchain(output_hash) => self
                .fetch_unspent_outputs_from_node(output_hash)
                .await?
                .ok_or_else(|| {
                    OutputManagerError::ServiceError(format!(
                        "Output with hash {output_hash} not found in blockchain (TxId: 0)"
                    ))
                })?,
            UseOutput::AsProvided(ref val) => *val.clone(),
        };
        if output.commitment != expected_commitment {
            return Err(OutputManagerError::ServiceError(
                "Output commitment does not match expected commitment (TxId: 0)".to_string(),
            ));
        }
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: fetched outputs");
        // Retrieve the list of n public keys from the script
        let (multi_sig_public_keys, threshold) = get_multi_sig_script_components(&output.script, TxId::from(0u64))?;
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: retrieved public keys from script");
        // Create a deterministic encryption key from the sum of the public keys
        let mut sum_public_keys = UncompressedPublicKey::default();
        for key in &multi_sig_public_keys {
            sum_public_keys = &sum_public_keys + key.to_public_key()?;
        }
        let encryption_private_key =
            public_key_to_output_encryption_key(&CompressedPublicKey::new_from_pk(sum_public_keys))?;
        let mut aggregated_script_public_key_shares = UncompressedPublicKey::default();
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: created deterministic encryption key");
        // Decrypt the output secrets and create a new input as WalletOutput (unblinded)
        let (input, payment_id) = if let Ok((amount, commitment_mask, payment_id)) =
            EncryptedData::decrypt_data(&encryption_private_key, &output.commitment, &output.encrypted_data)
        {
            if output.verify_mask(&self.resources.factories.range_proof, &commitment_mask, amount.as_u64())? {
                let script_key = self
                    .pre_mine_script_key_from_payment_id(payment_id.clone(), TxId::from(0u64))
                    .await?;
                let mut script_signatures = Vec::new();
                // lets add our own signature to the list
                let self_signature = self
                    .resources
                    .key_manager
                    .sign_script_message(&script_key.key_id, output.commitment.as_bytes())?;
                script_input_shares.insert(script_key.pub_key.clone(), self_signature);

                // the order here is important, we need to add the signatures in the same order as public keys were
                // added to the script originally
                for key in &multi_sig_public_keys {
                    if let Some(signature) = script_input_shares.get(key) {
                        script_signatures.push(StackItem::Signature(signature.clone()));
                        // our own key should not be aggregated yet, it will be added with the script signing
                        if key != &script_key.pub_key {
                            aggregated_script_public_key_shares =
                                aggregated_script_public_key_shares + key.to_public_key()?;
                        }
                    }
                }
                if script_signatures.len() != usize::from(threshold) {
                    return Err(OutputManagerError::ServiceError(format!(
                        "Invalid number of signatures (TxId: 0), expected {}, received {}",
                        threshold,
                        script_signatures.len()
                    )));
                }
                let commitment_mask_key_id = self.resources.key_manager.create_encrypted_key(commitment_mask, None)?;
                (
                    WalletOutput::new_from_transaction_output(
                        amount,
                        commitment_mask_key_id,
                        payment_id.clone(),
                        output,
                        ExecutionStack::new(script_signatures),
                        script_key.key_id,
                    ),
                    payment_id,
                )
            } else {
                return Err(OutputManagerError::ServiceError(
                    "Could not verify mask (TxId: 0)".to_string(),
                ));
            }
        } else {
            return Err(OutputManagerError::ServiceError(
                "Could not decrypt output (TxId: 0)".to_string(),
            ));
        };
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: decrypt secrets, created unblinded input");
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: {:?}", input.input_data());

        // The entire input will be spent to a single recipient with no change
        let output_features = OutputFeatures {
            maturity: original_maturity,
            range_proof_type,
            ..Default::default()
        };
        // we assign a temp script to calculate all the sizes for now, we override this with the stealth one later if
        // needed
        let temp_script = script!(PushPubKey(Box::new(recipient_address.public_spend_key().clone())))?;
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                output_features.get_serialized_size()? +
                    temp_script.get_serialized_size()? +
                    Covenant::default().get_serialized_size()?,
            );
        let fee = self.get_fee_calc();
        let fee = fee.calculate(fee_per_gram, 1, 1, 1, metadata_byte_size);
        let amount = input.value().saturating_sub(fee);
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: created script, with fee {fee}");

        // Create sender transaction protocol builder with recipient data and no change
        let mut builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_input(input.clone())?
            .with_memo(payment_id);
        let sender_offset_private_key_id_self = self.resources.key_manager.get_random_key(None, true)?;
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: created sender transaction protocol");

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending and encryption keys. All player's shares are added together to produce the
        // shared secret.

        let shared_secret = {
            let mut key_sum = UncompressedPublicKey::default();
            for key in &dh_shared_secret_shares {
                key_sum = key_sum + key.to_public_key()?;
            }
            let shared_secret_self = self.resources.key_manager.get_diffie_hellman_shared_secret(
                &sender_offset_private_key_id_self.key_id,
                recipient_address
                    .public_view_key()
                    .ok_or(OutputManagerError::ServiceError(
                        "Missing public view key (TxId: 0)".to_string(),
                    ))?,
            )?;
            key_sum = key_sum + &UncompressedPublicKey::from_vec(&shared_secret_self.as_bytes().to_vec())?;
            CompressedPublicKey::new_from_pk(key_sum)
        };
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: created dh shared secret");

        let spending_key = public_key_to_output_spending_key(&shared_secret)?;
        let spending_key_id = self.resources.key_manager.create_encrypted_key(spending_key, None)?;

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret)?;
        let encryption_key_id = self
            .resources
            .key_manager
            .create_encrypted_key(encryption_private_key, None)?;

        let sender_offset_public_key_self = self
            .resources
            .key_manager
            .get_public_key_at_key_id(&sender_offset_private_key_id_self.key_id)?;
        let mut aggregated_sender_offset_public_key_shares = UncompressedPublicKey::default();
        for key in &sender_offset_public_key_shares {
            aggregated_sender_offset_public_key_shares =
                aggregated_sender_offset_public_key_shares + &key.to_public_key()?;
        }

        let sender_offset_public_key =
            &aggregated_sender_offset_public_key_shares + sender_offset_public_key_self.to_public_key()?;

        let mut aggregated_metadata_ephemeral_public_key_shares = UncompressedPublicKey::default();
        for key in &metadata_ephemeral_public_key_shares {
            aggregated_metadata_ephemeral_public_key_shares =
                aggregated_metadata_ephemeral_public_key_shares + &key.to_public_key()?;
        }
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: prepared inputs for partial metadata signature");

        let script_spending_key = self
            .resources
            .key_manager
            .stealth_address_script_spending_key(&spending_key_id, recipient_address.public_spend_key())?;
        let script = push_pubkey_script(&script_spending_key);

        // Create the output with a partially signed metadata signature
        let output = WalletOutputBuilder::new(amount, spending_key_id)
            .with_features(
                output_features,
            )
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.key_manager,
                Some(&encryption_key_id),
                tx_payment_id.clone(),
            )
            ?
            .with_input_data(ExecutionStack::default()) // Just a placeholder in the wallet
            .with_sender_offset_public_key(CompressedPublicKey::new_from_pk(sender_offset_public_key))
            .with_script_key(self.resources.key_manager.get_spend_key().key_id)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_partial_as_sender_and_receiver(
                &self.resources.key_manager,
                &sender_offset_private_key_id_self.key_id,
                &CompressedPublicKey::new_from_pk(aggregated_sender_offset_public_key_shares),
                &CompressedPublicKey::new_from_pk(aggregated_metadata_ephemeral_public_key_shares.clone()),
            )
            .map_err(|e|service_error_with_id(TxId::from(0u64), e.to_string(), true))?
            .try_build(&self.resources.key_manager)
            .map_err(|e|service_error_with_id(TxId::from(0u64), e.to_string(), true))?;

        builder.add_recipient(
            recipient_address.clone(),
            output.clone(),
            Some(sender_offset_private_key_id_self.key_id),
            Some(encryption_key_id),
        )?;

        // Finalize
        let finalized = builder.build()?;
        self.confirm_encumberance(finalized.tx_id, None, Vec::new())?;

        let total_metadata_ephemeral_public_key = aggregated_metadata_ephemeral_public_key_shares +
            output.metadata_signature().ephemeral_pubkey().to_public_key()?;
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: created output with partial metadata signature");

        info!(target: LOG_TARGET, "Finalized partial one-side transaction TxId: {} (trace TxId: 0)", finalized.tx_id);
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: finalized partial transaction");

        let mut aggregated_script_signature_public_nonces = UncompressedPublicKey::default();
        for key in &script_signature_public_nonces {
            aggregated_script_signature_public_nonces =
                aggregated_script_signature_public_nonces + &key.to_public_key()?;
        }

        // Update the input's script signature
        let (updated_input, total_script_public_key) = input.to_transaction_input_with_multi_party_script_signature(
            &CompressedPublicKey::new_from_pk(aggregated_script_signature_public_nonces.clone()),
            &CompressedPublicKey::new_from_pk(aggregated_script_public_key_shares),
            &self.resources.key_manager,
        )?;
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: updated script input signature");

        let total_script_nonce = aggregated_script_signature_public_nonces +
            &updated_input.script_signature.ephemeral_pubkey().to_public_key()?;
        let fee = finalized.fee;
        let mut tx = finalized.transaction;
        let mut tx_body = tx.body;
        tx_body.update_script_signature(updated_input.commitment()?, updated_input.script_signature.clone())?;
        tx.body = tx_body;
        trace!(target: LOG_TARGET, "encumber_aggregate_utxo: updated script signature");

        // shared secret does not support debug so we manually convert this to a public key
        let shared_secret_bytes = shared_secret.as_bytes();
        let shared_secret_public_key = CompressedPublicKey::from_canonical_bytes(shared_secret_bytes)?;

        // Transaction balance log
        //   sum(output commitments) - sum(input  commitments) =  sum(kernel excesses) + total_offset
        let mut utxo_sum = UncompressedCommitment::default();
        for output in tx.body.outputs() {
            utxo_sum = &utxo_sum + &output.commitment.to_commitment()?;
        }
        for input in tx.body.inputs() {
            utxo_sum = &utxo_sum - &input.commitment()?.to_commitment()?;
        }
        let mut kernel_sum = UncompressedCommitment::default();
        for kernel in tx.body.kernels() {
            kernel_sum = &kernel_sum + &kernel.excess.to_commitment()?;
        }
        let total_offset = self.resources.factories.commitment.commit_value(&tx.offset, 0);
        trace!(target: LOG_TARGET, "total_offset:               {}", total_offset.to_hex());
        trace!(target: LOG_TARGET, "utxo_sum:                   {}", utxo_sum.to_hex());
        trace!(target: LOG_TARGET, "kernel_sum:                 {}", kernel_sum.to_hex());
        trace!(target: LOG_TARGET, "kernel_sum + sender_offset: {}", (&kernel_sum + &total_offset).to_hex());

        Ok(OutputManagerResponse::EncumberAggregateUtxo {
            tx_id: finalized.tx_id,
            transaction: Box::new(tx),
            amount,
            fee,
            total_script_public_key: Box::new(total_script_public_key),
            total_metadata_ephemeral_public_key: Box::new(CompressedPublicKey::new_from_pk(
                total_metadata_ephemeral_public_key,
            )),
            total_script_nonce: Box::new(CompressedPublicKey::new_from_pk(total_script_nonce)),
            shared_secret_public_key: Box::new(shared_secret_public_key),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn spend_backup_pre_mine_utxo(
        &mut self,
        fee_per_gram: MicroMinotari,
        output_hash: HashOutput,
        expected_commitment: CompressedCommitment,
        recipient_address: TariAddress,
        payment_id: MemoField,
        maturity: u64,
        range_proof_type: RangeProofType,
        minimum_value_promise: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari, MicroMinotari), OutputManagerError> {
        // Fetch the output from the blockchain
        let output = self
            .fetch_unspent_outputs_from_node(output_hash)
            .await?
            .ok_or_else(|| {
                OutputManagerError::ServiceError(format!(
                    "Output with hash {} not found in blockchain (TxId: 0)",
                    output_hash
                ))
            })?;
        if output.commitment != expected_commitment {
            return Err(OutputManagerError::ServiceError(
                "Output commitment does not match expected commitment (TxId: 0)".to_string(),
            ));
        }
        // Retrieve the list of n public keys from the script
        let public_keys = if let Some(Opcode::CheckMultiSigVerifyAggregatePubKey(_n, _m, keys, _msg)) =
            output.script.as_slice().get(3)
        {
            keys.clone()
        } else {
            return Err(OutputManagerError::ServiceError("Invalid script (TxId: 0)".to_string()));
        };
        // Create a deterministic encryption key from the sum of the public keys
        let mut sum_public_keys = UncompressedPublicKey::default();
        for key in &public_keys {
            sum_public_keys = &sum_public_keys + key.to_public_key()?;
        }
        let encryption_private_key =
            public_key_to_output_encryption_key(&CompressedPublicKey::new_from_pk(sum_public_keys))?;
        // Decrypt the output secrets and create a new input as WalletOutput (unblinded)
        let input = if let Ok((amount, spending_key, payment_id)) =
            EncryptedData::decrypt_data(&encryption_private_key, &output.commitment, &output.encrypted_data)
        {
            if output.verify_mask(&self.resources.factories.range_proof, &spending_key, amount.as_u64())? {
                let spending_key_id = self.resources.key_manager.create_encrypted_key(spending_key, None)?;
                let script_key = self
                    .pre_mine_script_key_from_payment_id(payment_id.clone(), TxId::from(0u64))
                    .await?;

                WalletOutput::new_from_transaction_output(
                    amount,
                    spending_key_id,
                    payment_id,
                    output,
                    Default::default(),
                    script_key.key_id,
                )
            } else {
                return Err(OutputManagerError::ServiceError(
                    "Could not verify mask (TxId: 0)".to_string(),
                ));
            }
        } else {
            return Err(OutputManagerError::ServiceError(
                "Could not decrypt output (TxId: 0)".to_string(),
            ));
        };

        // The entire input will be spent to a single recipient with no change
        let output_features = OutputFeatures {
            maturity,
            range_proof_type,
            ..Default::default()
        };
        let temp_script = script!(PushPubKey(Box::default()))?;
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                output_features.get_serialized_size()? +
                    temp_script.get_serialized_size()? +
                    Covenant::default().get_serialized_size()?,
            );
        let fee = self.get_fee_calc();
        let fee = fee.calculate(fee_per_gram, 1, 1, 1, metadata_byte_size);
        let amount = input.value().saturating_sub(fee);

        // Create sender transaction protocol builder with recipient data and no change
        let mut tx_builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_kernel_features(KernelFeatures::empty())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_memo(payment_id.clone())
            .with_input(input.clone())?;
        let sender_offset_private_key_id_self = self.resources.key_manager.get_random_key(None, true)?;

        // Prepare receiver part of the transaction

        // Diffie-Hellman shared secret `k_Ob * K_Sb = K_Ob * k_Sb` results in a public key, which is fed into
        // KDFs to produce the spending and encryption keys.

        let shared_secret = self.resources.key_manager.get_diffie_hellman_shared_secret(
            &sender_offset_private_key_id_self.key_id,
            recipient_address
                .public_view_key()
                .ok_or(OutputManagerError::ServiceError(
                    "Missing public view key (TxId: 0)".to_string(),
                ))?,
        )?;

        let commitment_mask_key = public_key_to_output_spending_key(&shared_secret)?;
        let commitment_mask_key_id = self
            .resources
            .key_manager
            .create_encrypted_key(commitment_mask_key, None)?;

        let encryption_private_key = public_key_to_output_encryption_key(&shared_secret)?;
        let encryption_key_id = self
            .resources
            .key_manager
            .create_encrypted_key(encryption_private_key, None)?;

        let sender_offset_public_key = self
            .resources
            .key_manager
            .get_public_key_at_key_id(&sender_offset_private_key_id_self.key_id)?;

        let script_spending_key = self
            .resources
            .key_manager
            .stealth_address_script_spending_key(&commitment_mask_key_id, recipient_address.public_spend_key())?;
        let script = push_pubkey_script(&script_spending_key);
        let payment_id = payment_id
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                true,
                fee,
                Some(TxType::PaymentToOther),
            )
            .map_err(OutputManagerError::InvalidPaymentIdFormat)?;

        let output = WalletOutputBuilder::new(amount, commitment_mask_key_id)
            .with_features(output_features
            )
            .with_script(script)
            .encrypt_data_for_recovery(
                &self.resources.key_manager,
                Some(&encryption_key_id),
                payment_id,
            )
            ?
            .with_input_data(ExecutionStack::default()) // Just a placeholder in the wallet
            .with_sender_offset_public_key(sender_offset_public_key)
            .with_script_key(TariKeyId::Zero)
            .with_minimum_value_promise(minimum_value_promise)
            .sign_as_sender_and_receiver_verified(
                &self.resources.key_manager,
                &sender_offset_private_key_id_self.key_id,
                &recipient_address,
            )

            .map_err(|e|service_error_with_id(TxId::from(0u64), e.to_string(), true))?
            .try_build(&self.resources.key_manager)

            .map_err(|e|service_error_with_id(TxId::from(0u64), e.to_string(), true))?;

        tx_builder.add_recipient(
            self.resources.one_sided_tari_address.clone(),
            output.clone(),
            Some(sender_offset_private_key_id_self.key_id),
            Some(encryption_key_id),
        )?;

        let finalized = tx_builder.build()?;
        self.confirm_encumberance(finalized.tx_id, None, Vec::new())?;
        info!(target: LOG_TARGET, "Finalized partial one-side transaction TxId: {} (Trace TxId: 0)", finalized.tx_id);
        let fee = finalized.fee;
        let tx = finalized.transaction;

        Ok((finalized.tx_id, tx, amount, fee))
    }

    #[allow(clippy::too_many_lines)]
    fn create_pay_to_self_transaction(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        lock_height: Option<u64>,
        payment_id: MemoField,
        minimum_value_promise: MicroMinotari,
    ) -> Result<(MicroMinotari, Transaction, TxId), OutputManagerError> {
        if selection_criteria.range_limit.is_some() {
            return Err(OutputManagerError::RangeLimitError {
                reason: "Range limit coin-join cannot be set for create_pay_to_self_transaction".to_string(),
                range_exhausted: false,
            });
        }
        let covenant = Covenant::default();

        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                output_features
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    TariScript::default()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    covenant
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            );

        let input_selection = self.select_utxos(
            amount,
            selection_criteria,
            fee_per_gram,
            1,
            features_and_scripts_byte_size,
        )?;

        // Create builder with no recipients (other than ourselves)
        let mut tx_builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        tx_builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_kernel_features(KernelFeatures::empty())
            .with_memo(payment_id.clone());

        for kmo in input_selection.iter() {
            tx_builder.with_input(kmo.wallet_output.clone())?;
        }

        let (output, sender_offset_key_id) = self.output_to_self(
            output_features,
            amount,
            covenant,
            payment_id,
            input_selection.as_final_fee(),
            minimum_value_promise,
        )?;

        tx_builder
            .with_output(output.wallet_output.clone(), sender_offset_key_id.clone(), None)
            .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

        let mut outputs = vec![output];

        let finalized = tx_builder.build()?;

        let fee = finalized.fee;
        if let Some(change) = finalized.change {
            let change_output =
                DbWalletOutput::from_wallet_output(change, None, OutputSource::default(), Some(finalized.tx_id), None);
            outputs.push(change_output);
        }
        trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", finalized.tx_id);

        trace!(
            target: LOG_TARGET,
            "Encumber send to self transaction ({}) outputs.",
            finalized.tx_id
        );
        self.resources
            .db
            .encumber_outputs(finalized.tx_id, input_selection.into_selected(), outputs)?;
        self.confirm_encumberance(finalized.tx_id, None, Vec::new())?;
        trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", finalized.tx_id);

        Ok((fee, finalized.transaction, finalized.tx_id))
    }

    /// Confirm that a transaction has finished being negotiated between parties so the short-term encumberance can be
    /// made official
    fn confirm_encumberance(
        &mut self,
        tx_id: TxId,
        tx_id_update: Option<TxId>,
        change_outputs: Vec<WalletOutput>,
    ) -> Result<(), OutputManagerError> {
        let mut change = Vec::new();
        for output in change_outputs {
            change.push(DbWalletOutput::from_wallet_output(
                output,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            ));
        }
        self.resources
            .db
            .confirm_encumbered_outputs(tx_id, tx_id_update, change)?;
        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Cancelling pending transaction outputs for TxId: {tx_id}"
        );
        Ok(self.resources.db.cancel_pending_transaction_outputs(tx_id)?)
    }

    /// Restore the pending transaction encumberance and output for an inbound transaction that was previously
    /// cancelled.
    fn reinstate_cancelled_inbound_transaction_outputs(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        self.resources.db.reinstate_cancelled_inbound_output(tx_id)?;

        Ok(())
    }

    /// Select which unspent transaction outputs to use to send a transaction of the specified amount. Use the specified
    /// selection strategy to choose the outputs. It also determines if a change output is required.
    #[allow(clippy::too_many_lines)]
    fn select_utxos(
        &mut self,
        amount: MicroMinotari,
        mut selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        num_outputs: usize,
        total_output_features_and_scripts_byte_size: usize,
    ) -> Result<UtxoSelection, OutputManagerError> {
        let start = Instant::now();
        debug!(
            target: LOG_TARGET,
            "select_utxos amount: {amount}, fee_per_gram: {fee_per_gram}, num_outputs: {num_outputs}, output_features_and_scripts_byte_size: {total_output_features_and_scripts_byte_size}, \
             selection_criteria: {selection_criteria:?}"
        );
        let mut utxos = Vec::new();

        let fee_calc = self.get_fee_calc();

        // Attempt to get the chain tip height
        let tip_height = self.resources.db.get_last_scanned_height()?;

        // Respecting the setting to not choose outputs that reveal the address
        if self.resources.config.autoignore_onesided_utxos {
            selection_criteria.excluding_onesided = self.resources.config.autoignore_onesided_utxos;
        }

        selection_criteria.excluding_multisig = true;

        debug!(
            target: LOG_TARGET,
            "select_utxos selection criteria: {selection_criteria}"
        );
        let start_new = Instant::now();
        let uo: Vec<DbWalletOutput> = self.resources.db.fetch_unspent_outputs_for_spending(
            &selection_criteria,
            amount,
            tip_height,
            &self.resources.key_manager,
        )?;

        // OutputSource

        let uo_len = uo.len();
        trace!(
            target: LOG_TARGET,
            "select_utxos profile - fetch_unspent_outputs_for_spending: {} outputs, {} ms (at {} ms)",
            uo_len,
            start_new.elapsed().as_millis(),
            start.elapsed().as_millis(),
        );
        let start_new = Instant::now();

        // For non-standard queries, we want to ensure that the intended UTXOs are selected
        if !selection_criteria.filter.is_standard() && uo.is_empty() {
            return Err(OutputManagerError::NoUtxosSelected {
                criteria: selection_criteria,
            });
        }

        // Assumes that default Outputfeatures are used for change utxo
        let output_features_estimate = OutputFeatures::default();
        let default_features_and_scripts_size = fee_calc.weighting().round_up_features_and_scripts_size(
            output_features_estimate
                .get_serialized_size()
                .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                Covenant::new()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                TariScript::default()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
        );

        trace!(target: LOG_TARGET, "We found {uo_len} UTXOs to select from");

        let mut requires_change_output = false;
        let mut utxos_total_value = MicroMinotari::from(0);
        let mut fee_without_change = MicroMinotari::from(0);
        let mut fee_with_change = MicroMinotari::from(0);
        for o in uo {
            utxos_total_value += o.wallet_output.value();

            trace!(target: LOG_TARGET, "-- utxos_total_value = {utxos_total_value}");
            utxos.push(o);
            // The assumption here is that the only output will be the payment output and change if required
            fee_without_change = fee_calc.calculate(
                fee_per_gram,
                1,
                utxos.len(),
                num_outputs,
                total_output_features_and_scripts_byte_size,
            );
            if utxos_total_value == amount + fee_without_change {
                break;
            }
            fee_with_change = fee_calc.calculate(
                fee_per_gram,
                1,
                utxos.len(),
                num_outputs + 1,
                total_output_features_and_scripts_byte_size + default_features_and_scripts_size,
            );

            trace!(target: LOG_TARGET, "-- amt+fee = {amount} + {fee_with_change}");
            if utxos_total_value > amount + fee_with_change {
                requires_change_output = true;
                break;
            }
        }

        let perfect_utxo_selection = utxos_total_value == amount + fee_without_change;
        let enough_spendable = utxos_total_value > amount + fee_with_change;
        trace!(
            target: LOG_TARGET,
            "select_utxos profile - final_selection: {} outputs from {}, {} ms (at {} ms)",
            utxos.len(),
            uo_len,
            start_new.elapsed().as_millis(),
            start.elapsed().as_millis(),
        );

        if !perfect_utxo_selection && !enough_spendable {
            if uo_len == TRANSACTION_INPUTS_LIMIT as usize {
                return Err(OutputManagerError::TooManyInputsToFulfillTransaction(format!(
                    "Input limit '{TRANSACTION_INPUTS_LIMIT}' reached"
                )));
            }
            let current_tip_for_time_lock_calculation = tip_height;
            let balance = self.get_balance(current_tip_for_time_lock_calculation)?;
            let pending_incoming = balance.pending_incoming_balance;
            if utxos_total_value + pending_incoming >= amount + fee_with_change {
                return Err(OutputManagerError::FundsPending);
            } else {
                return Err(OutputManagerError::NotEnoughFunds);
            }
        }

        Ok(UtxoSelection {
            utxos,
            requires_change_output,
            total_value: utxos_total_value,
            fee_without_change,
            fee_with_change,
        })
    }

    /// Select which unspent transaction outputs to use to send a range limited coin join transaction. Use the specified
    /// selection strategy to choose the outputs. No change output will be produced, and the total value selected will
    /// be >= the target amount plus fee.
    #[allow(clippy::too_many_lines)]
    async fn select_utxos_for_range_limited_coin_join(
        &mut self,
        selection_criteria: UtxoSelectionCriteria,
        fee: FeeType,
        total_output_features_and_scripts_byte_size: usize,
    ) -> Result<UtxoSelection, OutputManagerError> {
        let start = Instant::now();
        let range_limit_criteria =
            selection_criteria
                .clone()
                .range_limit
                .ok_or_else(|| OutputManagerError::RangeLimitError {
                    reason: "Range limit must be specified for range limited coin-join UTXO selection".to_string(),
                    range_exhausted: false,
                })?;
        debug!(
            target: LOG_TARGET,
            "select_utxos_for_range_limited_coin_join target_minimum_amount: {}, fee: {fee}, \
            output_features_and_scripts_byte_size:  {total_output_features_and_scripts_byte_size}, \
            selection_criteria: {selection_criteria:?}",
            range_limit_criteria.target_minimum_amount
        );
        if range_limit_criteria.target_minimum_amount <= range_limit_criteria.range.end {
            return Err(OutputManagerError::RangeLimitError {
                reason: format!(
                    "Target minimum amount {} cannot be less or equal than range end {}",
                    range_limit_criteria.target_minimum_amount, range_limit_criteria.range.end
                ),
                range_exhausted: false,
            });
        }

        // Attempt to get the chain tip height
        let tip_height = self.resources.db.get_last_scanned_height()?;

        let start_new = Instant::now();

        // Find the UTXOs that satisfy the range limit criteria and actual fee
        let fee_estimate = match fee {
            FeeType::TotalFee(fee) => MicroMinotari(fee),
            FeeType::FeePerGram(fee_per_gram) => {
                #[allow(clippy::single_range_in_vec_init)]
                let ranges = vec![range_limit_criteria.range.start..range_limit_criteria.range.end];
                let number_of_outputs =
                    if let Some(bucket) = self.resources.db.count_outputs_in_ranges(ranges, None)?.first() {
                        // 'range_limit_criteria.target_minimum_amount' cannot be zero here as checked above
                        usize::try_from(bucket.total_value / range_limit_criteria.target_minimum_amount)
                            .unwrap_or(usize::MAX)
                    } else {
                        return Err(OutputManagerError::RangeLimitError {
                            reason: format!(
                                "No outputs could be selected for the specified range: {:?}",
                                range_limit_criteria
                            ),
                            range_exhausted: true,
                        });
                    };
                if number_of_outputs > TRANSACTION_OUTPUTS_LIMIT {
                    return Err(OutputManagerError::TooManyOutputsToFulfillTransaction(format!(
                        "{number_of_outputs} > {TRANSACTION_OUTPUTS_LIMIT}"
                    )));
                }
                let fee_calc = self.get_fee_calc();
                fee_calc.calculate(
                    MicroMinotari(fee_per_gram),
                    1,
                    usize::try_from(range_limit_criteria.transaction_input_limit)
                        .unwrap_or(TRANSACTION_INPUTS_LIMIT as usize),
                    number_of_outputs,
                    total_output_features_and_scripts_byte_size * number_of_outputs,
                )
            },
        }
        .as_u64();
        if range_limit_criteria.target_minimum_amount < fee_estimate {
            return Err(OutputManagerError::RangeLimitError {
                reason: format!(
                    "Target minimum amount {} is less than the estimated fee {}",
                    range_limit_criteria.target_minimum_amount, fee_estimate
                ),
                range_exhausted: false,
            });
        }

        let selection_criteria = UtxoSelectionCriteria {
            range_limit: Some(RangeLimit {
                target_minimum_amount: range_limit_criteria.target_minimum_amount + fee_estimate,
                ..range_limit_criteria.clone()
            }),
            ..selection_criteria
        };
        let (utxos, total_value) = self.resources.db.get_range_limited_outputs_for_spending(
            &selection_criteria,
            tip_height,
            &self.resources.key_manager,
        )?;
        if utxos.is_empty() {
            return Err(OutputManagerError::RangeLimitError {
                reason: format!(
                    "No outputs could be selected for the specified range: {:?}",
                    range_limit_criteria
                ),
                range_exhausted: true,
            });
        }

        let number_of_outputs = usize::try_from(
            total_value.as_u64().saturating_sub(fee_estimate) / range_limit_criteria.target_minimum_amount,
        )
        .map_err(|_e| OutputManagerError::ConversionError("number_of_outputs".to_string()))?
        .max(1);

        if number_of_outputs > TRANSACTION_OUTPUTS_LIMIT {
            return Err(OutputManagerError::TooManyOutputsToFulfillTransaction(format!(
                "{number_of_outputs} > {TRANSACTION_OUTPUTS_LIMIT}"
            )));
        }

        let fee_without_change = match fee {
            FeeType::TotalFee(fee) => MicroMinotari(fee),
            FeeType::FeePerGram(fee_per_gram) => {
                let fee_calc = self.get_fee_calc();
                fee_calc.calculate(
                    MicroMinotari(fee_per_gram),
                    1,
                    utxos.len(),
                    number_of_outputs,
                    total_output_features_and_scripts_byte_size * number_of_outputs,
                )
            },
        };
        trace!(
            target: LOG_TARGET,
            "select_utxos_for_range_limited_coin_join profile - get_range_limited_outputs_for_spending: inputs: {}, \
            outputs: {}, ms (at {} ms)",
            utxos.len(),
            start_new.elapsed().as_millis(),
            start.elapsed().as_millis(),
        );

        // For non-standard queries, we want to ensure that the intended UTXOs are selected
        if !selection_criteria.filter.is_standard() && utxos.is_empty() {
            return Err(OutputManagerError::NoUtxosSelected {
                criteria: selection_criteria,
            });
        }

        if total_value - fee_without_change < MicroMinotari(range_limit_criteria.target_minimum_amount) {
            return Err(OutputManagerError::RangeLimitError {
                reason: format!(
                    "Total available in range less fee exceeds target value: {} vs. {}",
                    total_value - fee_without_change,
                    MicroMinotari(range_limit_criteria.target_minimum_amount)
                ),
                range_exhausted: false,
            });
        }

        Ok(UtxoSelection {
            utxos,
            requires_change_output: false,
            total_value,
            fee_without_change,
            fee_with_change: fee_without_change,
        })
    }

    pub fn fetch_spent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_spent_outputs(&self.resources.key_manager)?)
    }

    pub fn fetch_unspent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self
            .resources
            .db
            .fetch_all_unspent_outputs(&self.resources.key_manager)?)
    }

    pub fn fetch_outputs_by_query(&self, q: OutputBackendQuery) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self
            .resources
            .db
            .fetch_outputs_by_query(q, &self.resources.key_manager)?)
    }

    pub fn fetch_invalid_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.get_invalid_outputs(&self.resources.key_manager)?)
    }

    pub fn fetch_many_outputs(&self, outputs: &[FixedHash]) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self
            .resources
            .db
            .fetch_many_outputs(outputs, &self.resources.key_manager)?)
    }

    fn default_features_and_scripts_size(&self) -> Result<usize, OutputManagerError> {
        Ok(self
            .resources
            .consensus_constants
            .transaction_weight_params()
            .round_up_features_and_scripts_size(
                TariScript::default()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? +
                    OutputFeatures::default()
                        .get_serialized_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
            ))
    }

    pub async fn preview_coin_join_with_commitments(
        &self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), OutputManagerError> {
        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
            &self.resources.key_manager,
        )?;

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value());

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            1,
            self.default_features_and_scripts_size()
                .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?,
        );

        Ok((vec![accumulated_amount.saturating_sub(fee)], fee))
    }

    pub fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        number_of_splits: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), OutputManagerError> {
        if commitments.is_empty() {
            return Err(OutputManagerError::NoCommitmentsProvided);
        }

        if number_of_splits == 0 {
            return Err(OutputManagerError::InvalidArgument(
                "number_of_splits must be greater than 0".to_string(),
            ));
        }

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
            &self.resources.key_manager,
        )?;

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            number_of_splits,
            self.default_features_and_scripts_size()
                .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? *
                number_of_splits,
        );

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value());

        let aftertax_amount = accumulated_amount.saturating_sub(fee);
        let amount_per_split = MicroMinotari(aftertax_amount.as_u64() / number_of_splits as u64);
        let unspent_remainder = MicroMinotari(aftertax_amount.as_u64() % amount_per_split.as_u64());
        let mut expected_outputs = vec![];

        for i in 1..=number_of_splits {
            expected_outputs.push(if i == number_of_splits {
                amount_per_split + unspent_remainder
            } else {
                amount_per_split
            });
        }

        Ok((expected_outputs, fee))
    }

    fn create_coin_split_with_commitments(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        amount_per_split: Option<MicroMinotari>,
        number_of_splits: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        if commitments.is_empty() {
            return Err(OutputManagerError::NoCommitmentsProvided);
        }

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
            &self.resources.key_manager,
        )?;

        match amount_per_split {
            None => self.create_coin_split_even(src_outputs, number_of_splits, fee_per_gram),
            Some(amount_per_split) => {
                self.create_coin_split(src_outputs, amount_per_split, number_of_splits, fee_per_gram)
            },
        }
    }

    fn create_coin_split_auto(
        &mut self,
        amount_per_split: Option<MicroMinotari>,
        number_of_splits: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        match amount_per_split {
            None => Err(OutputManagerError::InvalidArgument(
                "coin split without `amount_per_split` is not supported yet".to_string(),
            )),
            Some(amount_per_split) => {
                let selection = self.select_utxos(
                    amount_per_split * MicroMinotari(number_of_splits as u64),
                    UtxoSelectionCriteria::largest_first(self.resources.config.dust_ignore_value),
                    fee_per_gram,
                    number_of_splits,
                    self.default_features_and_scripts_size()
                        .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? *
                        number_of_splits,
                )?;

                self.create_coin_split(selection.utxos, amount_per_split, number_of_splits, fee_per_gram)
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn create_coin_split_even(
        &mut self,
        src_outputs: Vec<DbWalletOutput>,
        number_of_splits: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        if number_of_splits == 0 {
            return Err(OutputManagerError::InvalidArgument(
                "number_of_splits must be greater than 0".to_string(),
            ));
        }

        let default_features_and_scripts_size = self.default_features_and_scripts_size();
        let mut dest_outputs = Vec::with_capacity(number_of_splits + 1);

        // accumulated value amount from given source outputs
        let accumulated_amount_with_fee = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value());

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            number_of_splits,
            default_features_and_scripts_size.map_err(|e| OutputManagerError::ConversionError(e.to_string()))? *
                number_of_splits,
        );

        let accumulated_amount = accumulated_amount_with_fee.saturating_sub(fee);
        let amount_per_split = MicroMinotari(accumulated_amount.as_u64() / number_of_splits as u64);
        let unspent_remainder = MicroMinotari(accumulated_amount.as_u64() % amount_per_split.as_u64());

        // preliminary balance check
        if self.get_balance(None)?.available_balance < (accumulated_amount + fee) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        trace!(target: LOG_TARGET, "initializing new split (even) transaction");

        let mut tx_builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        tx_builder
            .with_memo(
                MemoField::new_open_from_string(
                    &format!("Coin split transaction, {accumulated_amount} into {number_of_splits} outputs"),
                    TxType::CoinSplit,
                )
                .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
            )
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_kernel_features(KernelFeatures::empty());

        // collecting inputs from source outputs
        for input in &src_outputs {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                input.hash
            );
            tx_builder.with_input(input.wallet_output.clone())?;
        }

        for i in 1..=number_of_splits {
            // NOTE: adding the unspent `change` to the last output
            let amount_per_split = if i == number_of_splits {
                amount_per_split + unspent_remainder
            } else {
                amount_per_split
            };

            let (output, sender_offset_key_id) = self.output_to_self(
                OutputFeatures::default(),
                amount_per_split,
                Covenant::default(),
                MemoField::new_open_from_string(&format!("{number_of_splits} even coin splits"), TxType::CoinSplit)
                    .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
                fee,
                MicroMinotari::zero(),
            )?;

            tx_builder
                .with_output(output.wallet_output.clone(), sender_offset_key_id, None)
                .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

            dest_outputs.push(output);
        }

        let finalized = tx_builder.build()?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = tx_outputs_to_tx_id(
            self.resources.key_manager.get_view_key().pub_key.as_bytes(),
            finalized.transaction.body.outputs(),
        );

        trace!(
            target: LOG_TARGET,
            "Encumber coin split (even) transaction (tx_id={tx_id}) outputs"
        );

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), dest_outputs)?;
        self.confirm_encumberance(tx_id, None, Vec::new())?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin split transaction (tx_id={tx_id})."
        );

        Ok((tx_id, finalized.transaction, accumulated_amount + fee))
    }

    #[allow(clippy::too_many_lines)]
    fn create_coin_split(
        &mut self,
        src_outputs: Vec<DbWalletOutput>,
        amount_per_split: MicroMinotari,
        number_of_splits: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        if number_of_splits == 0 {
            return Err(OutputManagerError::InvalidArgument(
                "number_of_splits must be greater than 0".to_string(),
            ));
        }

        if amount_per_split == MicroMinotari::zero() {
            return Err(OutputManagerError::InvalidArgument(
                "amount_per_split must be greater than 0".to_string(),
            ));
        }

        let default_features_and_scripts_size = self
            .default_features_and_scripts_size()
            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?;
        let mut dest_outputs = Vec::with_capacity(number_of_splits + 1);
        let total_split_amount = MicroMinotari::from(amount_per_split.as_u64() * number_of_splits as u64);

        // accumulated value amount from given source outputs
        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value());

        if total_split_amount >= accumulated_amount {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        let fee_without_change = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            number_of_splits,
            default_features_and_scripts_size * number_of_splits,
        );

        // checking whether a total output value is enough
        if accumulated_amount < (total_split_amount + fee_without_change) {
            error!(
                target: LOG_TARGET,
                "failed to split coins, not enough funds with `fee_without_change` included"
            );
            return Err(OutputManagerError::NotEnoughFunds);
        }

        let final_fee = match accumulated_amount
            .saturating_sub(total_split_amount + fee_without_change)
            .as_u64()
        {
            0 => fee_without_change,
            _ => self.get_fee_calc().calculate(
                fee_per_gram,
                1,
                src_outputs.len(),
                number_of_splits + 1,
                default_features_and_scripts_size * (number_of_splits + 1),
            ),
        };

        // checking, again, whether a total output value is enough
        if accumulated_amount < (total_split_amount + final_fee) {
            error!(
                target: LOG_TARGET,
                "failed to split coins, not enough funds with `final_fee` included"
            );
            return Err(OutputManagerError::NotEnoughFunds);
        }

        // preliminary balance check
        if self.get_balance(None)?.available_balance < (total_split_amount + final_fee) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        let change = accumulated_amount.saturating_sub(total_split_amount + final_fee);

        // ----------------------------------------------------------------------------
        // initializing new transaction

        trace!(target: LOG_TARGET, "initializing new split transaction");

        let mut tx_builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        let payment_id = MemoField::new_open_from_string(
            &format!("Coin split, {accumulated_amount} into {number_of_splits} outputs"),
            TxType::CoinSplit,
        )
        .map_err(OutputManagerError::InvalidPaymentIdFormat)?;
        tx_builder
            .with_memo(payment_id.clone())
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_kernel_features(KernelFeatures::empty());

        // collecting inputs from source outputs
        for output in &src_outputs {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                output.hash
            );
            tx_builder.with_input(output.wallet_output.clone())?;
        }

        // ----------------------------------------------------------------------------
        // initializing primary outputs

        for _ in 0..number_of_splits {
            let (output, sender_offset_key_id) = self.output_to_self(
                OutputFeatures::default(),
                amount_per_split,
                Covenant::default(),
                payment_id.clone(),
                final_fee,
                MicroMinotari::zero(),
            )?;

            tx_builder
                .with_output(output.wallet_output.clone(), sender_offset_key_id, None)
                .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

            dest_outputs.push(output);
        }

        let has_leftover_change = change > MicroMinotari::zero();

        let finalized = tx_builder.build()?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = tx_outputs_to_tx_id(
            self.resources.key_manager.get_view_key().pub_key.as_bytes(),
            finalized.transaction.body.outputs(),
        );

        trace!(
            target: LOG_TARGET,
            "Encumber coin split transaction (tx_id={tx_id}) outputs"
        );

        trace!(
            target: LOG_TARGET,
            "finalizing coin split transaction (tx_id={tx_id})."
        );

        // again, to obtain output for leftover change
        if let Some(change) = finalized.change {
            // obtaining output for the `change`

            // appending `change` output to the result
            dest_outputs.push(DbWalletOutput::from_wallet_output(
                change,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            ));
        }

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), dest_outputs)?;
        self.confirm_encumberance(tx_id, None, Vec::new())?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin split transaction (tx_id={tx_id})."
        );

        let value = if has_leftover_change {
            total_split_amount
        } else {
            total_split_amount + final_fee
        };

        Ok((tx_id, finalized.transaction, value))
    }

    fn output_to_self(
        &mut self,
        output_features: OutputFeatures,
        amount: MicroMinotari,
        covenant: Covenant,
        payment_id: MemoField,
        fee: MicroMinotari,
        minimum_value_promise: MicroMinotari,
    ) -> Result<(DbWalletOutput, TariKeyId), OutputManagerError> {
        let (commitment_mask_key, script_key) = self.resources.key_manager.get_next_commitment_mask_and_script_key()?;
        let script = script!(PushPubKey(Box::new(script_key.pub_key.clone())))?;
        let payment_id = payment_id
            .add_sender_address(
                self.resources.one_sided_tari_address.clone(),
                false,
                fee,
                Some(TxType::PaymentToSelf),
            )
            .map_err(OutputManagerError::InvalidPaymentIdFormat)?;

        let encrypted_data = self.resources.key_manager.encrypt_data_for_recovery(
            &commitment_mask_key.key_id,
            None,
            amount.as_u64(),
            payment_id.clone(),
        )?;
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            TransactionOutputVersion::get_current_version(),
            &script,
            &output_features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );
        let sender_offset = self.resources.key_manager.get_random_key(None, true)?;
        let metadata_signature = self.resources.key_manager.get_metadata_signature(
            &commitment_mask_key.key_id,
            &PrivateKey::from(amount),
            &sender_offset.key_id,
            TransactionOutputVersion::get_current_version(),
            &metadata_message,
            output_features.range_proof_type,
        )?;

        let output = DbWalletOutput::from_wallet_output(
            WalletOutput::new_current_version(
                amount,
                commitment_mask_key.key_id,
                output_features,
                script,
                ExecutionStack::default(),
                script_key.key_id,
                sender_offset.pub_key,
                metadata_signature,
                0,
                covenant,
                encrypted_data,
                minimum_value_promise,
                payment_id,
                &self.resources.key_manager,
            )?,
            None,
            OutputSource::default(),
            None,
            None,
        );

        Ok((output, sender_offset.key_id))
    }

    #[allow(clippy::too_many_lines)]
    pub fn create_coin_join(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        let default_features_and_scripts_size = self
            .default_features_and_scripts_size()
            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?;

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
            &self.resources.key_manager,
        )?;

        let accumulated_amount_with_fee = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value());

        let fee =
            self.get_fee_calc()
                .calculate(fee_per_gram, 1, src_outputs.len(), 1, default_features_and_scripts_size);

        let accumulated_amount = accumulated_amount_with_fee.saturating_sub(fee);

        // checking, again, whether a total output value is enough
        if accumulated_amount == MicroMinotari::zero() {
            error!(target: LOG_TARGET, "failed to join coins, not enough funds");
            return Err(OutputManagerError::NotEnoughFunds);
        }

        // preliminary balance check
        if self.get_balance(None)?.available_balance < accumulated_amount {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        // ----------------------------------------------------------------------------
        // initializing new transaction

        trace!(target: LOG_TARGET, "initializing new join transaction");

        let mut tx_builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        tx_builder
            .with_memo(payment_id.clone())
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_kernel_features(KernelFeatures::empty());

        // collecting inputs from source outputs
        for input in &src_outputs {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                input.hash
            );
            tx_builder.with_input(input.wallet_output.clone())?;
        }

        let (output, sender_offset_key_id) = self.output_to_self(
            OutputFeatures::default(),
            accumulated_amount,
            Covenant::default(),
            payment_id.clone(),
            fee,
            MicroMinotari::zero(),
        )?;

        tx_builder.with_output(output.wallet_output.clone(), sender_offset_key_id, None)?;

        let finalized = tx_builder.build()?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = tx_outputs_to_tx_id(
            self.resources.key_manager.get_view_key().pub_key.as_bytes(),
            finalized.transaction.body.outputs(),
        );

        trace!(
            target: LOG_TARGET,
            "Encumber coin join transaction (tx_id={tx_id}) outputs"
        );

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), vec![output])?;
        self.confirm_encumberance(tx_id, None, Vec::new())?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin join transaction (tx_id={tx_id})."
        );

        Ok((tx_id, finalized.transaction, accumulated_amount + fee))
    }

    pub fn scrape_wallet(
        &mut self,
        tx_id: TxId,
        fee_per_gram: MicroMinotari,
    ) -> Result<TransactionBuilder<TKeyManagerInterface>, OutputManagerError> {
        let src_outputs = self
            .resources
            .db
            .fetch_all_unspent_outputs(&self.resources.key_manager)?;

        let mut builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        builder
            .with_fee_per_gram(fee_per_gram)
            .with_memo(
                MemoField::new_open_from_string("scraping wallet", TxType::PaymentToOther)
                    .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
            )
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount);

        for uo in &src_outputs {
            builder.with_input(uo.wallet_output.clone())?;
        }

        // encumbering transaction
        self.resources.db.encumber_outputs(tx_id, src_outputs.clone(), vec![])?;
        Ok(builder)
    }

    pub async fn fetch_unspent_outputs_from_node(
        &mut self,
        hash: HashOutput,
    ) -> Result<Option<TransactionOutput>, OutputManagerError> {
        self.resources
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .fetch_utxo(hash.to_vec())
            .await
            .map_err(|e| OutputManagerError::BaseNodeClientError(e.to_string()))
    }

    #[allow(clippy::too_many_lines)]
    pub fn create_claim_sha_atomic_swap_transaction(
        &mut self,
        output: TransactionOutput,
        pre_image: CompressedPublicKey,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        let shared_secret = self.resources.key_manager.get_diffie_hellman_shared_secret(
            &self.resources.key_manager.get_view_key().key_id,
            &output.sender_offset_public_key,
        )?;
        let encryption_key = public_key_to_output_encryption_key(&shared_secret)?;
        if let Ok((amount, spending_key, payment_id)) =
            EncryptedData::decrypt_data(&encryption_key, &output.commitment, &output.encrypted_data)
        {
            if output.verify_mask(&self.resources.factories.range_proof, &spending_key, amount.as_u64())? {
                let commitment_mask_key_id = self.resources.key_manager.create_encrypted_key(spending_key, None)?;

                let recovered_output = WalletOutput::new_from_transaction_output(
                    amount,
                    commitment_mask_key_id,
                    payment_id,
                    output,
                    inputs!(pre_image),
                    self.resources.key_manager.get_spend_key().key_id,
                );

                // Create builder with no recipients (other than ourselves)
                let mut builder = TransactionBuilder::new(
                    self.resources.consensus_constants.clone(),
                    self.resources.key_manager.clone(),
                    self.resources.network,
                )?;
                builder
                    .with_lock_height(0)
                    .with_fee_per_gram(fee_per_gram)
                    .with_memo(
                        MemoField::new_open_from_string("SHA-XTR atomic swap", TxType::ClaimAtomicSwap)
                            .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
                    )
                    .with_tx_type(TxType::ClaimAtomicSwap)
                    .with_kernel_features(KernelFeatures::empty())
                    .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
                    .with_input(recovered_output)?;

                let mut outputs = Vec::new();

                let finalized = builder.build()?;

                let fee = finalized.fee;
                if let Some(wallet_output) = finalized.change {
                    let change_output = DbWalletOutput::from_wallet_output(
                        wallet_output,
                        None,
                        OutputSource::AtomicSwap,
                        Some(finalized.tx_id),
                        None,
                    );
                    outputs.push(change_output);
                };
                trace!(target: LOG_TARGET, "Claiming HTLC with transaction ({}).", finalized.tx_id);

                self.resources
                    .db
                    .encumber_outputs(finalized.tx_id, Vec::new(), outputs)?;
                self.confirm_encumberance(finalized.tx_id, None, Vec::new())?;

                Ok((finalized.tx_id, fee, amount - fee, finalized.transaction))
            } else {
                Err(OutputManagerError::TransactionError(TransactionError::RangeProofError(
                    "Atomic swap: Blinding factor could not open the commitment!".to_string(),
                )))
            }
        } else {
            Err(OutputManagerError::TransactionError(TransactionError::RangeProofError(
                "Atomic swap: Encrypted value could not be decrypted!".to_string(),
            )))
        }
    }

    pub fn create_htlc_refund_transaction(
        &mut self,
        output_hash: HashOutput,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        let output = self
            .resources
            .db
            .get_unspent_output(output_hash, &self.resources.key_manager)?
            .wallet_output;

        let amount = output.value();

        // Create builder with no recipients (other than ourselves)
        let mut builder = TransactionBuilder::new(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
            self.resources.network,
        )?;
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_memo(
                MemoField::new_open_from_string("SHA-XTR atomic refund", TxType::HtlcAtomicSwapRefund)
                    .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
            )
            .with_kernel_features(KernelFeatures::empty())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_input(output)?;

        let mut outputs = Vec::new();

        let finalized = builder.build()?;
        let fee = finalized.fee;

        if let Some(wallet_output) = finalized.change {
            let change_output = DbWalletOutput::from_wallet_output(
                wallet_output,
                None,
                OutputSource::HtlcRefund,
                Some(finalized.tx_id),
                None,
            );
            outputs.push(change_output);
        };
        trace!(target: LOG_TARGET, "Claiming HTLC refund with transaction ({}).", finalized.tx_id);

        self.resources
            .db
            .encumber_outputs(finalized.tx_id, Vec::new(), outputs)?;
        self.confirm_encumberance(finalized.tx_id, None, Vec::new())?;
        Ok((finalized.tx_id, fee, amount - fee, finalized.transaction))
    }

    /// Persist a one-sided payment script for a Comms Public/Private key. These are the scripts that this wallet knows
    /// to look for when scanning for one-sided payments
    fn add_known_script(&mut self, known_script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerError> {
        debug!(target: LOG_TARGET, "Adding new script to output manager service");
        // It is not a problem if the script has already been persisted
        match self
            .resources
            .db
            .add_known_script(known_script, &self.resources.key_manager)
        {
            Ok(_) => (),
            Err(OutputManagerStorageError::DieselError(DieselError::DatabaseError(
                DatabaseErrorKind::UniqueViolation,
                _,
            ))) => {
                trace!(target: LOG_TARGET, "Duplicate script not added");
            },
            Err(OutputManagerStorageError::DuplicateScript) => {
                trace!(target: LOG_TARGET, "Duplicate script not added");
            },
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    // Scanning outputs addressed to this wallet
    #[allow(clippy::too_many_lines)]
    fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let mut known_keys = Vec::new();
        let known_scripts = self
            .resources
            .db
            .get_all_known_one_sided_payment_scripts(&self.resources.key_manager)?;
        for known_script in known_scripts {
            known_keys.push((
                self.resources
                    .key_manager
                    .get_public_key_at_key_id(&known_script.script_key_id)?,
                known_script.script_key_id.clone(),
            ));
        }

        let view_key = self.resources.key_manager.get_view_key();

        let mut scanned_outputs = vec![];

        for output in outputs {
            if let [Opcode::PushPubKey(scanned_pk)] = output.script.as_slice() {
                if let Some(matched_key) = known_keys.iter().find(|x| &x.0 == scanned_pk.as_ref()) {
                    let shared_secret = self
                        .resources
                        .key_manager
                        .get_diffie_hellman_shared_secret(&view_key.key_id, &output.sender_offset_public_key)?;

                    let encryption_key = public_key_to_output_encryption_key(&shared_secret)?;
                    let script_private_key = matched_key.clone().1;

                    if let Ok((committed_value, spending_key, payment_id)) =
                        EncryptedData::decrypt_data(&encryption_key, &output.commitment, &output.encrypted_data)
                    {
                        if output.verify_mask(
                            &self.resources.factories.range_proof,
                            &spending_key,
                            committed_value.into(),
                        )? {
                            let commitment_mask_key_id =
                                self.resources.key_manager.create_encrypted_key(spending_key, None)?;

                            let rewound_output = WalletOutput::new_from_transaction_output(
                                committed_value,
                                commitment_mask_key_id,
                                payment_id,
                                output,
                                ExecutionStack::new(vec![]),
                                script_private_key,
                            );

                            scanned_outputs.push((rewound_output, OutputSource::OneSided));
                        }
                    }
                }
                // it is not some known key, so lets try and see if this is a stealth tx for us
                else {
                    let shared_secret = self
                        .resources
                        .key_manager
                        .get_diffie_hellman_shared_secret(&view_key.key_id, &output.sender_offset_public_key)?;

                    let encryption_key = public_key_to_output_encryption_key(&shared_secret)?;
                    if let Ok((committed_value, commitment_mask_private_key, payment_id)) =
                        EncryptedData::decrypt_data(&encryption_key, &output.commitment, &output.encrypted_data)
                    {
                        let commitment_mask_key_id = &self
                            .resources
                            .key_manager
                            .create_encrypted_key(commitment_mask_private_key.clone(), None)?;

                        if output.verify_mask(
                            &self.resources.factories.range_proof,
                            &commitment_mask_private_key,
                            committed_value.into(),
                        )? {
                            let script_spending_key = self.resources.key_manager.stealth_address_script_spending_key(
                                commitment_mask_key_id,
                                &self.resources.key_manager.get_spend_key().pub_key,
                            )?;

                            if script_spending_key != **scanned_pk {
                                continue;
                            }

                            let script_key = TariKeyId::Derived {
                                key: SerializedKeyString::from(commitment_mask_key_id.to_string()),
                            };

                            let recovered_output = WalletOutput::new_from_transaction_output(
                                committed_value,
                                commitment_mask_key_id.clone(),
                                payment_id,
                                output,
                                ExecutionStack::new(vec![]),
                                script_key,
                            );

                            scanned_outputs.push((recovered_output, OutputSource::StealthOneSided));
                        }
                    }
                }
            }
        }

        self.import_onesided_outputs(scanned_outputs, &view_key.pub_key)
    }

    // Scanning outputs addressed to this wallet
    #[allow(clippy::too_many_lines)]
    fn scan_outputs_for_multisig(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        // 1. Get all your wallet's public keys (or just the spend key for now)
        let mut scanned_outputs = vec![];

        let view_key = self.resources.key_manager.get_view_key().pub_key;
        for output in outputs {
            // 2. Check if the script is a multisig script

            if let [Opcode::CheckMultiSigVerify(_m, _n, pubkeys, _msg), Opcode::PushPubKey(scanned_pk)] =
                output.script.as_slice()
            {
                debug!(
                    target: LOG_TARGET,
                    "Found multisig script in output with tx_id: {:?}, pubkeys: {:?}",
                    TxId::new_deterministic(view_key.as_bytes(), &output.hash()),
                    pubkeys
                );

                if let Some((commitment_mask_key_id, committed_value, payment_id)) =
                    self.resources.key_manager.try_output_key_recovery(
                        output.commitment(),
                        output.encrypted_data(),
                        &output.sender_offset_public_key,
                    )?
                {
                    let script_spending_key = self.resources.key_manager.stealth_address_script_spending_key(
                        &commitment_mask_key_id,
                        &self.resources.key_manager.get_spend_key().pub_key,
                    )?;

                    if script_spending_key != **scanned_pk {
                        continue;
                    }

                    let script_key = TariKeyId::Derived {
                        key: SerializedKeyString::from(commitment_mask_key_id.to_string()),
                    };

                    let recovered_output = WalletOutput::new_from_transaction_output(
                        committed_value,
                        commitment_mask_key_id,
                        payment_id,
                        output,
                        ExecutionStack::new(vec![]),
                        script_key,
                    );
                    scanned_outputs.push((recovered_output, OutputSource::Multisig));
                }
            }
        }

        self.import_onesided_outputs(scanned_outputs, &view_key)
    }

    // Import scanned outputs into the wallet
    fn import_onesided_outputs(
        &self,
        scanned_outputs: Vec<(WalletOutput, OutputSource)>,
        view_key: &CompressedPublicKey,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let mut rewound_outputs = Vec::with_capacity(scanned_outputs.len());

        for (output, output_source) in scanned_outputs {
            let tx_id = output.calculate_tx_id(view_key.as_bytes());
            let db_output = DbWalletOutput::from_wallet_output(output.clone(), None, output_source, Some(tx_id), None);
            let hash = db_output.hash;

            match self
                .resources
                .db
                .add_unspent_output_with_tx_id(tx_id, db_output.clone(), &self.resources.key_manager)
            {
                Ok(_) => {
                    trace!(
                        target: LOG_TARGET,
                        "One-sided payment Output {} with value {} recovered",
                        db_output.commitment.to_hex(),
                        db_output.wallet_output.value(),
                    );

                    rewound_outputs.push(RecoveredOutput { output, hash })
                },
                Err(OutputManagerStorageError::DuplicateOutput) => {
                    warn!(
                        target: LOG_TARGET,
                        "Attempt to add scanned output {} that already exists. Ignoring the output.",
                        db_output.commitment.to_hex()
                    );
                },
                Err(err) => {
                    return Err(err.into());
                },
            }
        }

        Ok(rewound_outputs)
    }

    fn get_fee_calc(&self) -> Fee {
        Fee::new(*self.resources.consensus_constants.transaction_weight_params())
    }
}

/// Use the provided output when encumbering an aggregate UTXO or not, for use with
/// `fn encumber_aggregate_utxo`
#[derive(Clone)]
pub enum UseOutput {
    /// The transaction output will be fetched from the blockchain
    FromBlockchain(HashOutput),
    /// The transaction output must be provided
    AsProvided(Box<TransactionOutput>),
}

fn get_multi_sig_script_components(
    script: &TariScript,
    tx_id: TxId,
) -> Result<(Vec<CompressedPublicKey>, u8), OutputManagerError> {
    if let Some(Opcode::CheckMultiSigVerifyAggregatePubKey(m, _n, keys, _msg)) = script.as_slice().get(3) {
        Ok((keys.clone(), *m))
    } else {
        Err(OutputManagerError::ServiceError(format!(
            "Invalid script (TxId: {tx_id})"
        )))
    }
}

fn service_error_with_id(tx_id: TxId, err: String, log_error: bool) -> OutputManagerError {
    let err_str = format!("TxId: {tx_id} ({err})");
    if log_error {
        error!(target: LOG_TARGET, "{err_str}");
    }
    OutputManagerError::ServiceError(err_str)
}

/// This struct holds the detailed balance of the Output Manager Service.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// The current balance that is available to spend
    pub available_balance: MicroMinotari,
    /// The amount of the available balance that is current time-locked, None if no chain tip is provided
    pub time_locked_balance: Option<MicroMinotari>,
    /// The current balance of funds that are due to be received but have not yet been confirmed
    pub pending_incoming_balance: MicroMinotari,
    /// The current balance of funds encumbered in pending outbound transactions that have not been confirmed
    pub pending_outgoing_balance: MicroMinotari,
}

impl Balance {
    pub fn zero() -> Self {
        Self {
            available_balance: Default::default(),
            time_locked_balance: None,
            pending_incoming_balance: Default::default(),
            pending_outgoing_balance: Default::default(),
        }
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Available balance: {}", self.available_balance)?;
        if let Some(locked) = self.time_locked_balance {
            writeln!(f, "Time locked: {locked}")?;
        }
        writeln!(f, "Pending incoming balance: {}", self.pending_incoming_balance)?;
        writeln!(f, "Pending outgoing balance: {}", self.pending_outgoing_balance)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct UtxoSelection {
    utxos: Vec<DbWalletOutput>,
    requires_change_output: bool,
    total_value: MicroMinotari,
    fee_without_change: MicroMinotari,
    fee_with_change: MicroMinotari,
}

#[allow(dead_code)]
impl UtxoSelection {
    pub fn as_final_fee(&self) -> MicroMinotari {
        if self.requires_change_output {
            return self.fee_with_change;
        }
        self.fee_without_change
    }

    pub fn requires_change_output(&self) -> bool {
        self.requires_change_output
    }

    /// Total value of the selected inputs
    pub fn total_value(&self) -> MicroMinotari {
        self.total_value
    }

    pub fn num_selected(&self) -> usize {
        self.utxos.len()
    }

    pub fn into_selected(self) -> Vec<DbWalletOutput> {
        self.utxos
    }

    pub fn iter(&self) -> impl Iterator<Item = &DbWalletOutput> + '_ {
        self.utxos.iter()
    }
}

#[derive(Debug, Clone)]
pub struct OutputInfoByTxId {
    pub statuses: Vec<OutputStatus>,
    pub(crate) mined_height: Option<u64>,
    pub(crate) block_hash: Option<BlockHash>,
}

impl Display for OutputInfoByTxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "OutputInfoByTxId {{ statuses: {:?}, mined_height: {:?}, block_hash: {:?} }}",
            self.statuses,
            self.mined_height,
            if let Some(hash) = self.block_hash {
                hash.to_hex()
            } else {
                "None".to_string()
            }
        )?;
        Ok(())
    }
}

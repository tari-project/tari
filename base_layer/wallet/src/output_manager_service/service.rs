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

use std::{convert::TryInto, fmt, sync::Arc};

use diesel::result::{DatabaseErrorKind, Error as DieselError};
use futures::{pin_mut, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, Commitment, HashOutput, PrivateKey, PublicKey},
};
use tari_comms::types::CommsDHKE;
use tari_core::{
    borsh::SerializedSize,
    consensus::ConsensusConstants,
    covenants::Covenant,
    one_sided::{shared_secret_to_output_encryption_key, stealth_address_script_spending_key},
    proto::base_node::FetchMatchingUtxos,
    transactions::{
        fee::Fee,
        key_manager::{TariKeyId, TransactionKeyManagerBranch, TransactionKeyManagerInterface},
        tari_amount::MicroMinotari,
        transaction_components::{
            EncryptedData,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionError,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
            WalletOutputBuilder,
        },
        transaction_protocol::{sender::TransactionSenderMessage, TransactionMetadata},
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::keys::SecretKey;
use tari_script::{inputs, script, ExecutionStack, Opcode, TariScript};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
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
            OutputSource,
            OutputStatus,
        },
        tasks::TxoValidationTask,
        TRANSACTION_INPUTS_LIMIT,
    },
    util::wallet_identity::WalletIdentity,
};

const LOG_TARGET: &str = "wallet::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<TBackend, TWalletConnectivity, TKeyManagerInterface> {
    resources: OutputManagerResources<TBackend, TWalletConnectivity, TKeyManagerInterface>,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    base_node_service: BaseNodeServiceHandle,
    last_seen_tip_height: Option<u64>,
    validation_in_progress: Arc<Mutex<()>>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OutputManagerService<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: TransactionKeyManagerInterface,
{
    pub async fn new(
        config: OutputManagerServiceConfig,
        request_stream: reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse, OutputManagerError>,
        >,
        db: OutputManagerDatabase<TBackend>,
        event_publisher: OutputManagerEventSender,
        factories: CryptoFactories,
        consensus_constants: ConsensusConstants,
        shutdown_signal: ShutdownSignal,
        base_node_service: BaseNodeServiceHandle,
        connectivity: TWalletConnectivity,
        wallet_identity: WalletIdentity,
        key_manager: TKeyManagerInterface,
    ) -> Result<Self, OutputManagerError> {
        let resources = OutputManagerResources {
            config,
            db,
            factories,
            connectivity,
            event_publisher,
            key_manager,
            consensus_constants,
            shutdown_signal,
            wallet_identity,
        };

        Ok(Self {
            resources,
            request_stream: Some(request_stream),
            base_node_service,
            last_seen_tip_height: None,
            validation_in_progress: Arc::new(Mutex::new(())),
        })
    }

    pub async fn start(mut self) -> Result<(), OutputManagerError> {
        // we need to ensure the wallet identity secret key is stored in the key manager
        let _key_id = self
            .resources
            .key_manager
            .import_key(self.resources.wallet_identity.node_identity.secret_key().clone())
            .await?;

        let request_stream = self
            .request_stream
            .take()
            .expect("OutputManagerService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown = self.resources.shutdown_signal.clone();

        let mut base_node_service_event_stream = self.base_node_service.get_event_stream();

        debug!(target: LOG_TARGET, "Output Manager Service started");
        // Outputs marked as shorttermencumbered are not yet stored as transactions in the TMS, so lets clear them
        self.resources.db.clear_short_term_encumberances()?;
        loop {
            tokio::select! {
                event = base_node_service_event_stream.recv() => {
                    match event {
                        Ok(msg) => self.handle_base_node_service_event(msg),
                        Err(e) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel: {}", e),
                    }
                },
                Some(request_context) = request_stream.next() => {
                trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _result = reply_tx.send(response).map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
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
    ) -> Result<OutputManagerResponse, OutputManagerError> {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            OutputManagerRequest::AddOutput((uo, spend_priority)) => self
                .add_output(None, *uo, spend_priority)
                .await
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddOutputWithTxId((tx_id, uo, spend_priority)) => self
                .add_output(Some(tx_id), *uo, spend_priority)
                .await
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddUnvalidatedOutput((tx_id, uo, spend_priority)) => self
                .add_unvalidated_output(tx_id, *uo, spend_priority)
                .await
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::UpdateOutputMetadataSignature(uo) => self
                .update_output_metadata_signature(*uo)
                .map(|_| OutputManagerResponse::OutputMetadataSignatureUpdated),
            OutputManagerRequest::GetBalance => {
                let current_tip_for_time_lock_calculation = match self.base_node_service.get_chain_metadata().await {
                    Ok(metadata) => metadata.map(|m| m.best_block_height()),
                    Err(_) => None,
                };
                self.get_balance(current_tip_for_time_lock_calculation)
                    .map(OutputManagerResponse::Balance)
            },
            OutputManagerRequest::GetRecipientTransaction(tsm) => self
                .get_default_recipient_transaction(tsm)
                .await
                .map(OutputManagerResponse::RecipientTransactionGenerated),
            OutputManagerRequest::PrepareToSendTransaction {
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                tx_meta,
                message,
                script,
                covenant,
                minimum_value_promise,
            } => self
                .prepare_transaction_to_send(
                    tx_id,
                    amount,
                    selection_criteria,
                    fee_per_gram,
                    tx_meta,
                    message,
                    *output_features,
                    script,
                    covenant,
                    minimum_value_promise,
                )
                .await
                .map(OutputManagerResponse::TransactionToSend),
            OutputManagerRequest::CreatePayToSelfTransaction {
                tx_id,
                amount,
                selection_criteria,
                output_features,
                fee_per_gram,
                lock_height,
            } => self
                .create_pay_to_self_transaction(
                    tx_id,
                    amount,
                    selection_criteria,
                    *output_features,
                    fee_per_gram,
                    lock_height,
                )
                .await
                .map(OutputManagerResponse::PayToSelfTransaction),
            OutputManagerRequest::FeeEstimate {
                amount,
                selection_criteria,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => self
                .fee_estimate(amount, selection_criteria, fee_per_gram, num_kernels, num_outputs)
                .await
                .map(OutputManagerResponse::FeeEstimate),
            OutputManagerRequest::ConfirmPendingTransaction(tx_id) => self
                .confirm_encumberance(tx_id)
                .map(|_| OutputManagerResponse::PendingTransactionConfirmed),
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
            OutputManagerRequest::ValidateUtxos => {
                self.validate_outputs().map(OutputManagerResponse::TxoValidationStarted)
            },
            OutputManagerRequest::RevalidateTxos => self
                .revalidate_outputs()
                .map(OutputManagerResponse::TxoValidationStarted),
            OutputManagerRequest::GetInvalidOutputs => {
                let outputs = self.fetch_invalid_outputs()?.into_iter().map(|v| v.into()).collect();
                Ok(OutputManagerResponse::InvalidOutputs(outputs))
            },
            OutputManagerRequest::PreviewCoinJoin((commitments, fee_per_gram)) => {
                Ok(OutputManagerResponse::CoinPreview(
                    self.preview_coin_join_with_commitments(commitments, fee_per_gram)
                        .await?,
                ))
            },
            OutputManagerRequest::PreviewCoinSplitEven((commitments, number_of_splits, fee_per_gram)) => {
                Ok(OutputManagerResponse::CoinPreview(
                    self.preview_coin_split_with_commitments_no_amount(commitments, number_of_splits, fee_per_gram)
                        .await?,
                ))
            },
            OutputManagerRequest::CreateCoinSplit((commitments, amount_per_split, split_count, fee_per_gram)) => {
                if commitments.is_empty() {
                    self.create_coin_split_auto(Some(amount_per_split), split_count, fee_per_gram)
                        .await
                        .map(OutputManagerResponse::Transaction)
                } else {
                    self.create_coin_split_with_commitments(
                        commitments,
                        Some(amount_per_split),
                        split_count,
                        fee_per_gram,
                    )
                    .await
                    .map(OutputManagerResponse::Transaction)
                }
            },
            OutputManagerRequest::CreateCoinSplitEven((commitments, split_count, fee_per_gram)) => {
                if commitments.is_empty() {
                    self.create_coin_split_auto(None, split_count, fee_per_gram)
                        .await
                        .map(OutputManagerResponse::Transaction)
                } else {
                    self.create_coin_split_with_commitments(commitments, None, split_count, fee_per_gram)
                        .await
                        .map(OutputManagerResponse::Transaction)
                }
            },
            OutputManagerRequest::CreateCoinJoin {
                commitments,
                fee_per_gram,
            } => self
                .create_coin_join(commitments, fee_per_gram)
                .await
                .map(OutputManagerResponse::Transaction),

            OutputManagerRequest::ScanForRecoverableOutputs(outputs) => {
                StandardUtxoRecoverer::new(self.resources.key_manager.clone(), self.resources.db.clone())
                    .scan_and_recover_outputs(outputs)
                    .await
                    .map(OutputManagerResponse::RewoundOutputs)
            },
            OutputManagerRequest::ScanOutputs(outputs) => self
                .scan_outputs_for_one_sided_payments(outputs)
                .await
                .map(OutputManagerResponse::ScanOutputs),
            OutputManagerRequest::AddKnownOneSidedPaymentScript(known_script) => self
                .add_known_script(known_script)
                .map(|_| OutputManagerResponse::AddKnownOneSidedPaymentScript),
            OutputManagerRequest::ReinstateCancelledInboundTx(tx_id) => self
                .reinstate_cancelled_inbound_transaction_outputs(tx_id)
                .map(|_| OutputManagerResponse::ReinstatedCancelledInboundTx),
            OutputManagerRequest::CreateOutputWithFeatures { value, features } => {
                let wallet_output = self.create_output_with_features(value, *features).await?;
                Ok(OutputManagerResponse::CreateOutputWithFeatures {
                    output: Box::new(wallet_output),
                })
            },
            OutputManagerRequest::CreatePayToSelfWithOutputs {
                outputs,
                fee_per_gram,
                selection_criteria,
            } => {
                let (tx_id, transaction) = self
                    .create_pay_to_self_containing_outputs(outputs, selection_criteria, fee_per_gram)
                    .await?;
                Ok(OutputManagerResponse::CreatePayToSelfWithOutputs {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
            OutputManagerRequest::CreateClaimShaAtomicSwapTransaction(output_hash, pre_image, fee_per_gram) => {
                self.claim_sha_atomic_swap_with_hash(output_hash, pre_image, fee_per_gram)
                    .await
            },
            OutputManagerRequest::CreateHtlcRefundTransaction(output, fee_per_gram) => self
                .create_htlc_refund_transaction(output, fee_per_gram)
                .await
                .map(OutputManagerResponse::ClaimHtlcTransaction),
            OutputManagerRequest::GetOutputInfoByTxId(tx_id) => {
                let output_statuses_by_tx_id = self.get_output_info_by_tx_id(tx_id)?;
                Ok(OutputManagerResponse::OutputInfoByTxId(output_statuses_by_tx_id))
            },
        }
    }

    fn get_output_info_by_tx_id(&self, tx_id: TxId) -> Result<OutputInfoByTxId, OutputManagerError> {
        let outputs = self.resources.db.fetch_outputs_by_tx_id(tx_id)?;
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
        pre_image: PublicKey,
        fee_per_gram: MicroMinotari,
    ) -> Result<OutputManagerResponse, OutputManagerError> {
        let output = self
            .fetch_outputs_from_node(vec![output_hash])
            .await?
            .pop()
            .ok_or_else(|| OutputManagerError::ServiceError("Output not found".to_string()))?;

        self.create_claim_sha_atomic_swap_transaction(output, pre_image, fee_per_gram)
            .await
            .map(OutputManagerResponse::ClaimHtlcTransaction)
    }

    fn handle_base_node_service_event(&mut self, event: Arc<BaseNodeEvent>) {
        match (*event).clone() {
            BaseNodeEvent::BaseNodeStateChanged(_state) => {
                trace!(
                    target: LOG_TARGET,
                    "Received Base Node State Change but no block changes"
                );
            },
            BaseNodeEvent::NewBlockDetected(_hash, height) => {
                self.last_seen_tip_height = Some(height);
                let _id = self.validate_outputs().map_err(|e| {
                    warn!(target: LOG_TARGET, "Error validating  txos: {:?}", e);
                    e
                });
            },
        }
    }

    fn validate_outputs(&mut self) -> Result<u64, OutputManagerError> {
        let current_base_node = self
            .resources
            .connectivity
            .get_current_base_node_id()
            .ok_or(OutputManagerError::NoBaseNodeKeysProvided)?;
        let id = OsRng.next_u64();
        let txo_validation = TxoValidationTask::new(
            id,
            self.resources.db.clone(),
            self.resources.connectivity.clone(),
            self.resources.event_publisher.clone(),
            self.resources.config.clone(),
        );

        let mut shutdown = self.resources.shutdown_signal.clone();
        let mut base_node_watch = self.resources.connectivity.get_current_base_node_watcher();
        let event_publisher = self.resources.event_publisher.clone();
        let validation_in_progress = self.validation_in_progress.clone();
        tokio::spawn(async move {
            // Note: We do not want the validation task to be queued
            let mut _lock = match validation_in_progress.try_lock() {
                Ok(val) => val,
                _ => {
                    if let Err(e) = event_publisher.send(Arc::new(OutputManagerEvent::TxoValidationAlreadyBusy(id))) {
                        debug!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {:?}", e
                        );
                    }
                    debug!(
                        target: LOG_TARGET,
                        "UTXO Validation Protocol (Id: {}) spawned while a previous protocol was busy, ignored", id
                    );
                    return;
                },
            };

            let exec_fut = txo_validation.execute();
            tokio::pin!(exec_fut);
            loop {
                tokio::select! {
                    result = &mut exec_fut => {
                        match result {
                            Ok(id) => {
                                info!(
                                    target: LOG_TARGET,
                                    "UTXO Validation Protocol (Id: {}) completed successfully", id
                                );
                                return;
                            },
                            Err(OutputManagerProtocolError { id, error }) => {
                                warn!(
                                    target: LOG_TARGET,
                                    "Error completing UTXO Validation Protocol (Id: {}): {}", id, error
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
                                        "Error sending event because there are no subscribers: {:?}", e
                                    );
                                }

                                return;
                            },
                        }
                    },
                    _ = shutdown.wait() => {
                        debug!(target: LOG_TARGET, "TXO Validation Protocol (Id: {}) shutting down because the system \
                            is shutting down", id);
                        return;
                    },
                    _ = base_node_watch.changed() => {
                        if let Some(peer) = base_node_watch.borrow().as_ref() {
                            if peer.node_id != current_base_node {
                                debug!(
                                    target: LOG_TARGET,
                                    "TXO Validation Protocol (Id: {}) cancelled because base node changed", id
                                );
                                return;
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
    pub async fn add_output(
        &mut self,
        tx_id: Option<TxId>,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value
        );

        let output = DbWalletOutput::from_wallet_output(
            output,
            &self.resources.key_manager,
            spend_priority,
            OutputSource::default(),
            tx_id,
            None,
        )
        .await?;
        debug!(
            target: LOG_TARGET,
            "saving output of hash {} to Output Manager",
            output.hash.to_hex()
        );
        match tx_id {
            None => self.resources.db.add_unspent_output(output)?,
            Some(t) => self.resources.db.add_unspent_output_with_tx_id(t, output)?,
        }
        Ok(())
    }

    /// Add a key manager output to the outputs table and marks is as `EncumberedToBeReceived`. This is so that it will
    /// require a successful validation to confirm that it indeed spendable.
    pub async fn add_unvalidated_output(
        &mut self,
        tx_id: TxId,
        output: WalletOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add unvalidated output of value {} to Output Manager", output.value
        );
        let output = DbWalletOutput::from_wallet_output(
            output,
            &self.resources.key_manager,
            spend_priority,
            OutputSource::default(),
            Some(tx_id),
            None,
        )
        .await?;
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

    async fn create_output_with_features(
        &mut self,
        value: MicroMinotari,
        features: OutputFeatures,
    ) -> Result<WalletOutputBuilder, OutputManagerError> {
        let (spending_key_id, _spending_key_id, script_key_id, _script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
        let input_data = ExecutionStack::default();
        let script = TariScript::default();

        Ok(WalletOutputBuilder::new(value, spending_key_id)
            .with_features(features)
            .with_script(script)
            .with_input_data(input_data)
            .with_script_key(script_key_id))
    }

    fn get_balance(&self, current_tip_for_time_lock_calculation: Option<u64>) -> Result<Balance, OutputManagerError> {
        let balance = self.resources.db.get_balance(current_tip_for_time_lock_calculation)?;
        trace!(target: LOG_TARGET, "Balance: {:?}", balance);
        Ok(balance)
    }

    /// Request a receiver transaction be generated from the supplied Sender Message
    async fn get_default_recipient_transaction(
        &mut self,
        sender_message: TransactionSenderMessage,
    ) -> Result<ReceiverTransactionProtocol, OutputManagerError> {
        let single_round_sender_data = match sender_message.single() {
            Some(data) => data,
            _ => return Err(OutputManagerError::InvalidSenderMessage),
        };

        // Confirm covenant is default
        if single_round_sender_data.covenant != Covenant::default() {
            return Err(OutputManagerError::InvalidCovenant);
        }

        // Confirm output features is default
        if single_round_sender_data.features != OutputFeatures::default() {
            return Err(OutputManagerError::InvalidOutputFeatures);
        }

        // Confirm lock height is 0
        if single_round_sender_data.metadata.lock_height != 0 {
            return Err(OutputManagerError::InvalidLockHeight);
        }

        // Confirm kernel features
        if single_round_sender_data.metadata.kernel_features != KernelFeatures::default() {
            return Err(OutputManagerError::InvalidKernelFeatures);
        }

        let (spending_key_id, _, script_key_id, script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;

        // Confirm script hash is for the expected script, at the moment assuming Nop or Push_pubkey
        // if the script is Push_pubkey(default_key) we know we have to fill it in.
        let script = if single_round_sender_data.script == script!(Nop) {
            single_round_sender_data.script.clone()
        } else if single_round_sender_data.script == script!(PushPubKey(Box::default())) {
            script!(PushPubKey(Box::new(script_public_key.clone())))
        } else {
            return Err(OutputManagerError::InvalidScriptHash);
        };

        let encrypted_data = self
            .resources
            .key_manager
            .encrypt_data_for_recovery(&spending_key_id, None, single_round_sender_data.amount.as_u64())
            .await
            .unwrap();
        let minimum_value_promise = single_round_sender_data.minimum_value_promise;

        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &TransactionOutputVersion::get_current_version(),
            &script,
            &single_round_sender_data.features.clone(),
            &single_round_sender_data.covenant,
            &encrypted_data,
            &minimum_value_promise,
        );
        let metadata_signature = self
            .resources
            .key_manager
            .get_receiver_partial_metadata_signature(
                &spending_key_id,
                &single_round_sender_data.amount.into(),
                &single_round_sender_data.sender_offset_public_key,
                &single_round_sender_data.ephemeral_public_nonce,
                &TransactionOutputVersion::get_current_version(),
                &metadata_message,
                single_round_sender_data.features.range_proof_type,
            )
            .await?;

        let key_kanager_output = WalletOutput::new_current_version(
            single_round_sender_data.amount,
            spending_key_id.clone(),
            single_round_sender_data.features.clone(),
            script,
            ExecutionStack::default(),
            script_key_id,
            single_round_sender_data.sender_offset_public_key.clone(),
            // Note: The signature at this time is only partially built
            metadata_signature,
            0,
            single_round_sender_data.covenant.clone(),
            encrypted_data,
            minimum_value_promise,
            &self.resources.key_manager,
        )
        .await?;
        let output = DbWalletOutput::from_wallet_output(
            key_kanager_output.clone(),
            &self.resources.key_manager,
            None,
            OutputSource::default(),
            Some(single_round_sender_data.tx_id),
            None,
        )
        .await?;

        self.resources
            .db
            .add_output_to_be_received(single_round_sender_data.tx_id, output)?;

        let rtp = ReceiverTransactionProtocol::new(
            sender_message.clone(),
            key_kanager_output,
            &self.resources.key_manager,
            &self.resources.consensus_constants,
        )
        .await;

        Ok(rtp)
    }

    /// Get a fee estimate for an amount of MicroMinotari, at a specified fee per gram and given number of kernels and
    /// outputs.
    async fn fee_estimate(
        &mut self,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<MicroMinotari, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Getting fee estimate. Amount: {}. Fee per gram: {}. Num kernels: {}. Num outputs: {}",
            amount,
            fee_per_gram,
            num_kernels,
            num_outputs
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

        let utxo_selection = match self
            .select_utxos(
                amount,
                selection_criteria,
                fee_per_gram,
                num_outputs,
                features_and_scripts_byte_size * num_outputs,
            )
            .await
        {
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
                return Ok(Fee::normalize(fee));
            },
            Err(e) => Err(e),
        }?;

        debug!(target: LOG_TARGET, "{} utxos selected.", utxo_selection.utxos.len());

        let fee = Fee::normalize(utxo_selection.as_final_fee());

        debug!(target: LOG_TARGET, "Fee calculated: {}", fee);
        Ok(fee)
    }

    /// Prepare a Sender Transaction Protocol for the amount and fee_per_gram specified. If required a change output
    /// will be produced.
    #[allow(clippy::too_many_lines)]
    pub async fn prepare_transaction_to_send(
        &mut self,
        tx_id: TxId,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
        tx_meta: TransactionMetadata,
        message: String,
        recipient_output_features: OutputFeatures,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
        recipient_minimum_value_promise: MicroMinotari,
    ) -> Result<SenderTransactionProtocol, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Preparing to send transaction. Amount: {}. UTXO Selection: {}. Fee per gram: {}. ",
            amount,
            selection_criteria,
            fee_per_gram,
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
            .select_utxos(
                amount,
                selection_criteria,
                fee_per_gram,
                1,
                features_and_scripts_byte_size,
            )
            .await?;

        let mut builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        builder
            .with_fee_per_gram(fee_per_gram)
            .with_recipient_data(
                recipient_script,
                recipient_output_features,
                recipient_covenant,
                recipient_minimum_value_promise,
                amount,
            )
            .await?
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_lock_height(tx_meta.lock_height)
            .with_kernel_features(tx_meta.kernel_features)
            .with_tx_id(tx_id);

        for uo in input_selection.iter() {
            builder.with_input(uo.wallet_output.clone()).await?;
        }
        debug!(
            target: LOG_TARGET,
            "Calculating fee for tx with: Fee per gram: {}. Num selected inputs: {}",
            amount,
            input_selection.num_selected()
        );

        let (change_spending_key_id, _, change_script_key_id, change_script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
        builder.with_change_data(
            script!(PushPubKey(Box::new(change_script_public_key.clone()))),
            ExecutionStack::default(),
            change_script_key_id,
            change_spending_key_id,
            Covenant::default(),
        );

        let stp = builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let mut change_output = Vec::<DbWalletOutput>::new();
        if input_selection.requires_change_output() {
            let wallet_output = stp.get_change_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            change_output.push(
                DbWalletOutput::from_wallet_output(
                    wallet_output,
                    &self.resources.key_manager,
                    None,
                    OutputSource::default(),
                    Some(tx_id),
                    None,
                )
                .await?,
            );
        }

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), change_output)?;

        debug!(target: LOG_TARGET, "Prepared transaction (TxId: {}) to send", tx_id);

        Ok(stp)
    }

    #[allow(clippy::too_many_lines)]
    async fn create_pay_to_self_containing_outputs(
        &mut self,
        outputs: Vec<WalletOutputBuilder>,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        let total_value = outputs.iter().map(|o| o.value()).sum();
        let nop_script = script![Nop];
        let weighting = self.resources.consensus_constants.transaction_weight_params();
        let mut features_and_scripts_byte_size = 0;
        for output in &outputs {
            let (features, covenant, script) = (
                output
                    .features()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ServiceError(e.to_string()))?,
                output
                    .covenant()
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ServiceError(e.to_string()))?,
                output
                    .script()
                    .unwrap_or(&nop_script)
                    .get_serialized_size()
                    .map_err(|e| OutputManagerError::ServiceError(e.to_string()))?,
            );

            features_and_scripts_byte_size += weighting.round_up_features_and_scripts_size(features + covenant + script)
        }

        let input_selection = self
            .select_utxos(
                total_value,
                selection_criteria,
                fee_per_gram,
                outputs.len(),
                features_and_scripts_byte_size,
            )
            .await?;

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(false)
            .with_kernel_features(KernelFeatures::empty());

        for uo in input_selection.iter() {
            builder.with_input(uo.wallet_output.clone()).await?;
        }

        if input_selection.requires_change_output() {
            let (change_spending_key_id, _, change_script_key_id, change_script_public_key) =
                self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
            builder.with_change_data(
                script!(PushPubKey(Box::new(change_script_public_key))),
                ExecutionStack::default(),
                change_script_key_id,
                change_spending_key_id,
                Covenant::default(),
            );
        }

        let mut db_outputs = vec![];
        for mut wallet_output in outputs {
            let (sender_offset_key_id, _) = self
                .resources
                .key_manager
                .get_next_key(&TransactionKeyManagerBranch::SenderOffset.get_branch_key())
                .await?;
            wallet_output = wallet_output
                .sign_as_sender_and_receiver(&self.resources.key_manager, &sender_offset_key_id)
                .await?;

            let ub = wallet_output.try_build(&self.resources.key_manager).await?;
            builder
                .with_output(ub.clone(), sender_offset_key_id.clone())
                .await
                .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;
            db_outputs.push(
                DbWalletOutput::from_wallet_output(
                    ub,
                    &self.resources.key_manager,
                    None,
                    OutputSource::default(),
                    None,
                    None,
                )
                .await?,
            )
        }

        let mut stp = builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        let tx_id = stp.get_tx_id()?;
        if let Some(wallet_output) = stp.get_change_output()? {
            db_outputs.push(
                DbWalletOutput::from_wallet_output(
                    wallet_output,
                    &self.resources.key_manager,
                    None,
                    OutputSource::default(),
                    Some(tx_id),
                    None,
                )
                .await?,
            );
        }

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), db_outputs)?;
        stp.finalize(&self.resources.key_manager).await?;

        Ok((tx_id, stp.into_transaction()?))
    }

    async fn create_pay_to_self_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroMinotari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroMinotari,
        lock_height: Option<u64>,
    ) -> Result<(MicroMinotari, Transaction), OutputManagerError> {
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

        let input_selection = self
            .select_utxos(
                amount,
                selection_criteria,
                fee_per_gram,
                1,
                features_and_scripts_byte_size,
            )
            .await?;

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_kernel_features(KernelFeatures::empty())
            .with_tx_id(tx_id);

        for kmo in input_selection.iter() {
            builder.with_input(kmo.wallet_output.clone()).await?;
        }

        let (output, sender_offset_key_id) = self.output_to_self(output_features, amount, covenant).await?;

        builder
            .with_output(output.wallet_output.clone(), sender_offset_key_id.clone())
            .await
            .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

        let mut outputs = vec![output];

        let (change_spending_key_id, _spend_public_key, change_script_key_id, change_script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
        builder.with_change_data(
            script!(PushPubKey(Box::new(change_script_public_key.clone()))),
            ExecutionStack::default(),
            change_script_key_id.clone(),
            change_spending_key_id,
            Covenant::default(),
        );

        let mut stp = builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        if input_selection.requires_change_output() {
            let wallet_output = stp.get_change_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            let change_output = DbWalletOutput::from_wallet_output(
                wallet_output,
                &self.resources.key_manager,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            )
            .await?;
            outputs.push(change_output);
        }

        trace!(
            target: LOG_TARGET,
            "Encumber send to self transaction ({}) outputs.",
            tx_id
        );
        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), outputs)?;
        self.confirm_encumberance(tx_id)?;
        let fee = stp.get_fee_amount()?;
        trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", tx_id);
        stp.finalize(&self.resources.key_manager).await?;
        let tx = stp.into_transaction()?;

        Ok((fee, tx))
    }

    /// Confirm that a transaction has finished being negotiated between parties so the short-term encumberance can be
    /// made official
    fn confirm_encumberance(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        self.resources.db.confirm_encumbered_outputs(tx_id)?;

        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Cancelling pending transaction outputs for TxId: {}", tx_id
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
    async fn select_utxos(
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
            "select_utxos amount: {}, fee_per_gram: {}, num_outputs: {}, output_features_and_scripts_byte_size: {}, \
             selection_criteria: {:?}",
            amount,
            fee_per_gram,
            num_outputs,
            total_output_features_and_scripts_byte_size,
            selection_criteria
        );
        let mut utxos = Vec::new();

        let fee_calc = self.get_fee_calc();

        // Attempt to get the chain tip height
        let chain_metadata = self.base_node_service.get_chain_metadata().await?;

        // Respecting the setting to not choose outputs that reveal the address
        if self.resources.config.autoignore_onesided_utxos {
            selection_criteria.excluding_onesided = self.resources.config.autoignore_onesided_utxos;
        }

        debug!(
            target: LOG_TARGET,
            "select_utxos selection criteria: {}", selection_criteria
        );
        let tip_height = chain_metadata.as_ref().map(|m| m.best_block_height());
        let start_new = Instant::now();
        let uo = self
            .resources
            .db
            .fetch_unspent_outputs_for_spending(&selection_criteria, amount, tip_height)?;
        let uo_len = uo.len();
        trace!(
            target: LOG_TARGET,
            "select_utxos profile - fetch_unspent_outputs_for_spending: {} outputs, {} ms (at {})",
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

        trace!(target: LOG_TARGET, "We found {} UTXOs to select from", uo_len);

        let mut requires_change_output = false;
        let mut utxos_total_value = MicroMinotari::from(0);
        let mut fee_without_change = MicroMinotari::from(0);
        let mut fee_with_change = MicroMinotari::from(0);
        for o in uo {
            utxos_total_value += o.wallet_output.value;

            trace!(target: LOG_TARGET, "-- utxos_total_value = {:?}", utxos_total_value);
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

            trace!(target: LOG_TARGET, "-- amt+fee = {:?} {}", amount, fee_with_change);
            if utxos_total_value > amount + fee_with_change {
                requires_change_output = true;
                break;
            }
        }

        let perfect_utxo_selection = utxos_total_value == amount + fee_without_change;
        let enough_spendable = utxos_total_value > amount + fee_with_change;
        trace!(
            target: LOG_TARGET,
            "select_utxos profile - final_selection: {} outputs from {}, {} ms (at {})",
            utxos.len(),
            uo_len,
            start_new.elapsed().as_millis(),
            start.elapsed().as_millis(),
        );

        if !perfect_utxo_selection && !enough_spendable {
            if uo_len == TRANSACTION_INPUTS_LIMIT as usize {
                return Err(OutputManagerError::TooManyInputsToFulfillTransaction(format!(
                    "Input limit '{}' reached",
                    TRANSACTION_INPUTS_LIMIT
                )));
            }
            let current_tip_for_time_lock_calculation = chain_metadata.map(|cm| cm.best_block_height());
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

    pub fn fetch_spent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_spent_outputs()?)
    }

    pub fn fetch_unspent_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_all_unspent_outputs()?)
    }

    pub fn fetch_outputs_by_query(&self, q: OutputBackendQuery) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_outputs_by_query(q)?)
    }

    pub fn fetch_invalid_outputs(&self) -> Result<Vec<DbWalletOutput>, OutputManagerError> {
        Ok(self.resources.db.get_invalid_outputs()?)
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
        commitments: Vec<Commitment>,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), OutputManagerError> {
        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
        )?;

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value);

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

    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<Commitment>,
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
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value);

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

    async fn create_coin_split_with_commitments(
        &mut self,
        commitments: Vec<Commitment>,
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
        )?;

        match amount_per_split {
            None => {
                self.create_coin_split_even(src_outputs, number_of_splits, fee_per_gram)
                    .await
            },
            Some(amount_per_split) => {
                self.create_coin_split(src_outputs, amount_per_split, number_of_splits, fee_per_gram)
                    .await
            },
        }
    }

    async fn create_coin_split_auto(
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
                let selection = self
                    .select_utxos(
                        amount_per_split * MicroMinotari(number_of_splits as u64),
                        UtxoSelectionCriteria::largest_first(self.resources.config.dust_ignore_value),
                        fee_per_gram,
                        number_of_splits,
                        self.default_features_and_scripts_size()
                            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))? *
                            number_of_splits,
                    )
                    .await?;

                self.create_coin_split(selection.utxos, amount_per_split, number_of_splits, fee_per_gram)
                    .await
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn create_coin_split_even(
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
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value);

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

        let mut tx_builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        tx_builder
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
            tx_builder.with_input(input.wallet_output.clone()).await?;
        }

        for i in 1..=number_of_splits {
            // NOTE: adding the unspent `change` to the last output
            let amount_per_split = if i == number_of_splits {
                amount_per_split + unspent_remainder
            } else {
                amount_per_split
            };

            let (output, sender_offset_key_id) = self
                .output_to_self(OutputFeatures::default(), amount_per_split, Covenant::default())
                .await?;

            tx_builder
                .with_output(output.wallet_output.clone(), sender_offset_key_id)
                .await
                .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

            dest_outputs.push(output);
        }

        let mut stp = tx_builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = stp.get_tx_id()?;

        trace!(
            target: LOG_TARGET,
            "Encumber coin split (even) transaction (tx_id={}) outputs",
            tx_id
        );

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), dest_outputs)?;
        self.confirm_encumberance(tx_id)?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin split transaction (tx_id={}).",
            tx_id
        );

        // finalizing transaction
        stp.finalize(&self.resources.key_manager).await?;

        Ok((tx_id, stp.into_transaction()?, accumulated_amount + fee))
    }

    #[allow(clippy::too_many_lines)]
    async fn create_coin_split(
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
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value);

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

        let mut tx_builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        tx_builder
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
            tx_builder.with_input(output.wallet_output.clone()).await?;
        }

        // ----------------------------------------------------------------------------
        // initializing primary outputs

        for _ in 0..number_of_splits {
            let (output, sender_offset_key_id) = self
                .output_to_self(OutputFeatures::default(), amount_per_split, Covenant::default())
                .await?;

            tx_builder
                .with_output(output.wallet_output.clone(), sender_offset_key_id)
                .await
                .map_err(|e| OutputManagerError::BuildError(e.to_string()))?;

            dest_outputs.push(output);
        }

        let has_leftover_change = change > MicroMinotari::zero();

        // extending transaction if there is some `change` left over
        if has_leftover_change {
            let (change_spending_key_id, _, change_script_key_id, change_script_public_key) =
                self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
            tx_builder.with_change_data(
                script!(PushPubKey(Box::new(change_script_public_key))),
                ExecutionStack::default(),
                change_script_key_id,
                change_spending_key_id,
                Covenant::default(),
            );
        }

        let mut stp = tx_builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = stp.get_tx_id()?;

        trace!(
            target: LOG_TARGET,
            "Encumber coin split transaction (tx_id={}) outputs",
            tx_id
        );

        // again, to obtain output for leftover change
        if has_leftover_change {
            // obtaining output for the `change`
            let wallet_output_for_change = stp.get_change_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a `change` output metadata signature available".to_string(),
                )
            })?;

            // appending `change` output to the result
            dest_outputs.push(
                DbWalletOutput::from_wallet_output(
                    wallet_output_for_change,
                    &self.resources.key_manager,
                    None,
                    OutputSource::default(),
                    Some(tx_id),
                    None,
                )
                .await?,
            );
        }

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), dest_outputs)?;
        self.confirm_encumberance(tx_id)?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin split transaction (tx_id={}).",
            tx_id
        );

        // finalizing transaction
        stp.finalize(&self.resources.key_manager).await?;

        let value = if has_leftover_change {
            total_split_amount
        } else {
            total_split_amount + final_fee
        };

        Ok((tx_id, stp.into_transaction()?, value))
    }

    async fn output_to_self(
        &mut self,
        output_features: OutputFeatures,
        amount: MicroMinotari,
        covenant: Covenant,
    ) -> Result<(DbWalletOutput, TariKeyId), OutputManagerError> {
        let (spending_key_id, _, script_key_id, script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
        let script = script!(PushPubKey(Box::new(script_public_key.clone())));

        let encrypted_data = self
            .resources
            .key_manager
            .encrypt_data_for_recovery(&spending_key_id, None, amount.as_u64())
            .await?;
        let minimum_value_promise = MicroMinotari::zero();
        let metadata_message = TransactionOutput::metadata_signature_message_from_parts(
            &TransactionOutputVersion::get_current_version(),
            &script,
            &output_features,
            &covenant,
            &encrypted_data,
            &minimum_value_promise,
        );
        let (sender_offset_key_id, sender_offset_public_key) = self
            .resources
            .key_manager
            .get_next_key(&TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await?;
        let metadata_signature = self
            .resources
            .key_manager
            .get_metadata_signature(
                &spending_key_id,
                &PrivateKey::from(amount),
                &sender_offset_key_id,
                &TransactionOutputVersion::get_current_version(),
                &metadata_message,
                output_features.range_proof_type,
            )
            .await?;

        let output = DbWalletOutput::from_wallet_output(
            WalletOutput::new_current_version(
                amount,
                spending_key_id,
                output_features,
                script,
                ExecutionStack::default(),
                script_key_id,
                sender_offset_public_key,
                metadata_signature,
                0,
                covenant,
                encrypted_data,
                minimum_value_promise,
                &self.resources.key_manager,
            )
            .await?,
            &self.resources.key_manager,
            None,
            OutputSource::default(),
            None,
            None,
        )
        .await?;

        Ok((output, sender_offset_key_id))
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_coin_join(
        &mut self,
        commitments: Vec<Commitment>,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, Transaction, MicroMinotari), OutputManagerError> {
        let default_features_and_scripts_size = self
            .default_features_and_scripts_size()
            .map_err(|e| OutputManagerError::ConversionError(e.to_string()))?;

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroMinotari::zero(),
            None,
        )?;

        let accumulated_amount_with_fee = src_outputs
            .iter()
            .fold(MicroMinotari::zero(), |acc, x| acc + x.wallet_output.value);

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

        let mut tx_builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        tx_builder
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
            tx_builder.with_input(input.wallet_output.clone()).await?;
        }

        let (output, sender_offset_key_id) = self
            .output_to_self(OutputFeatures::default(), accumulated_amount, Covenant::default())
            .await?;

        tx_builder
            .with_output(output.wallet_output.clone(), sender_offset_key_id)
            .await?;

        let mut stp = tx_builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = stp.get_tx_id()?;

        trace!(
            target: LOG_TARGET,
            "Encumber coin join transaction (tx_id={}) outputs",
            tx_id
        );

        // encumbering transaction
        self.resources
            .db
            .encumber_outputs(tx_id, src_outputs.clone(), vec![output])?;
        self.confirm_encumberance(tx_id)?;

        trace!(
            target: LOG_TARGET,
            "finalizing coin join transaction (tx_id={}).",
            tx_id
        );

        // finalizing transaction
        stp.finalize(&self.resources.key_manager).await?;

        Ok((tx_id, stp.into_transaction()?, accumulated_amount + fee))
    }

    async fn fetch_outputs_from_node(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, OutputManagerError> {
        // lets get the output from the blockchain
        let req = FetchMatchingUtxos {
            output_hashes: hashes.iter().map(|v| v.to_vec()).collect(),
        };
        let results: Vec<TransactionOutput> = self
            .resources
            .connectivity
            .obtain_base_node_wallet_rpc_client()
            .await
            .ok_or_else(|| {
                OutputManagerError::InvalidResponseError("Could not connect to base node rpc client".to_string())
            })?
            .fetch_matching_utxos(req)
            .await?
            .outputs
            .into_iter()
            .filter_map(|o| match o.try_into() {
                Ok(output) => Some(output),
                _ => None,
            })
            .collect();
        Ok(results)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_claim_sha_atomic_swap_transaction(
        &mut self,
        output: TransactionOutput,
        pre_image: PublicKey,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        let shared_secret = self
            .resources
            .key_manager
            .get_diffie_hellman_shared_secret(
                &self.resources.wallet_identity.wallet_node_key_id,
                &output.sender_offset_public_key,
            )
            .await?;
        let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        if let Ok((amount, spending_key)) =
            EncryptedData::decrypt_data(&encryption_key, &output.commitment, &output.encrypted_data)
        {
            if output.verify_mask(&self.resources.factories.range_proof, &spending_key, amount.as_u64())? {
                let spending_key_id = self.resources.key_manager.import_key(spending_key).await?;
                let rewound_output = WalletOutput::new(
                    output.version,
                    amount,
                    spending_key_id,
                    output.features,
                    output.script,
                    inputs!(pre_image),
                    self.resources.wallet_identity.wallet_node_key_id.clone(),
                    output.sender_offset_public_key,
                    output.metadata_signature,
                    // Although the technically the script does have a script lock higher than 0, this does not apply
                    // to to us as we are claiming the Hashed part which has a 0 time lock
                    0,
                    output.covenant,
                    output.encrypted_data,
                    output.minimum_value_promise,
                    &self.resources.key_manager,
                )
                .await?;

                let message = "SHA-XTR atomic swap".to_string();

                // Create builder with no recipients (other than ourselves)
                let mut builder = SenderTransactionProtocol::builder(
                    self.resources.consensus_constants.clone(),
                    self.resources.key_manager.clone(),
                );
                builder
                    .with_lock_height(0)
                    .with_fee_per_gram(fee_per_gram)
                    .with_message(message)
                    .with_kernel_features(KernelFeatures::empty())
                    .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
                    .with_input(rewound_output)
                    .await?;

                let mut outputs = Vec::new();

                let (change_spending_key_id, _, change_script_key_id, change_script_public_key) =
                    self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
                builder.with_change_data(
                    script!(PushPubKey(Box::new(change_script_public_key.clone()))),
                    ExecutionStack::default(),
                    change_script_key_id,
                    change_spending_key_id,
                    Covenant::default(),
                );

                let mut stp = builder
                    .build()
                    .await
                    .map_err(|e| OutputManagerError::BuildError(e.message))?;

                let tx_id = stp.get_tx_id()?;

                let wallet_output = stp.get_change_output()?.ok_or_else(|| {
                    OutputManagerError::BuildError(
                        "There should be a change output metadata signature available".to_string(),
                    )
                })?;
                let change_output = DbWalletOutput::from_wallet_output(
                    wallet_output,
                    &self.resources.key_manager,
                    None,
                    OutputSource::AtomicSwap,
                    Some(tx_id),
                    None,
                )
                .await?;
                outputs.push(change_output);

                trace!(target: LOG_TARGET, "Claiming HTLC with transaction ({}).", tx_id);
                self.resources.db.encumber_outputs(tx_id, Vec::new(), outputs)?;
                self.confirm_encumberance(tx_id)?;
                let fee = stp.get_fee_amount()?;
                trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", tx_id);
                stp.finalize(&self.resources.key_manager).await?;
                let tx = stp.into_transaction()?;

                Ok((tx_id, fee, amount - fee, tx))
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

    pub async fn create_htlc_refund_transaction(
        &mut self,
        output_hash: HashOutput,
        fee_per_gram: MicroMinotari,
    ) -> Result<(TxId, MicroMinotari, MicroMinotari, Transaction), OutputManagerError> {
        let output = self.resources.db.get_unspent_output(output_hash)?.wallet_output;

        let amount = output.value;

        let message = "SHA-XTR atomic refund".to_string();

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(
            self.resources.consensus_constants.clone(),
            self.resources.key_manager.clone(),
        );
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_message(message)
            .with_kernel_features(KernelFeatures::empty())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_input(output)
            .await?;

        let mut outputs = Vec::new();

        let (change_spending_key_id, _, change_script_key_id, change_script_public_key) =
            self.resources.key_manager.get_next_spend_and_script_key_ids().await?;
        builder.with_change_data(
            script!(PushPubKey(Box::new(change_script_public_key.clone()))),
            ExecutionStack::default(),
            change_script_key_id,
            change_spending_key_id,
            Covenant::default(),
        );

        let mut stp = builder
            .build()
            .await
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        let tx_id = stp.get_tx_id()?;

        let wallet_output = stp.get_change_output()?.ok_or_else(|| {
            OutputManagerError::BuildError("There should be a change output metadata signature available".to_string())
        })?;

        let change_output = DbWalletOutput::from_wallet_output(
            wallet_output,
            &self.resources.key_manager,
            None,
            OutputSource::HtlcRefund,
            Some(tx_id),
            None,
        )
        .await?;
        outputs.push(change_output);

        trace!(target: LOG_TARGET, "Claiming HTLC refund with transaction ({}).", tx_id);

        let fee = stp.get_fee_amount()?;

        stp.finalize(&self.resources.key_manager).await?;

        let tx = stp.into_transaction()?;

        self.resources.db.encumber_outputs(tx_id, Vec::new(), outputs)?;
        self.confirm_encumberance(tx_id)?;
        Ok((tx_id, fee, amount - fee, tx))
    }

    /// Persist a one-sided payment script for a Comms Public/Private key. These are the scripts that this wallet knows
    /// to look for when scanning for one-sided payments
    fn add_known_script(&mut self, known_script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerError> {
        debug!(target: LOG_TARGET, "Adding new script to output manager service");
        // It is not a problem if the script has already been persisted
        match self.resources.db.add_known_script(known_script) {
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
    async fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let mut known_keys = Vec::new();
        let known_scripts = self.resources.db.get_all_known_one_sided_payment_scripts()?;
        for known_script in known_scripts {
            known_keys.push((
                self.resources
                    .key_manager
                    .get_public_key_at_key_id(&known_script.script_key_id)
                    .await?,
                known_script.script_key_id.clone(),
            ));
        }

        let wallet_sk = self.resources.wallet_identity.wallet_node_key_id.clone();
        let wallet_pk = self.resources.key_manager.get_public_key_at_key_id(&wallet_sk).await?;

        let mut scanned_outputs = vec![];

        for output in outputs {
            match output.script.as_slice() {
                // ----------------------------------------------------------------------------
                // simple one-sided address
                [Opcode::PushPubKey(scanned_pk)] => {
                    match known_keys.iter().find(|x| &x.0 == scanned_pk.as_ref()) {
                        // none of the keys match, skipping
                        None => continue,

                        // match found
                        Some(matched_key) => {
                            let shared_secret = self
                                .resources
                                .key_manager
                                .get_diffie_hellman_shared_secret(&matched_key.1, &output.sender_offset_public_key)
                                .await?;
                            scanned_outputs.push((
                                output.clone(),
                                OutputSource::OneSided,
                                matched_key.1.clone(),
                                shared_secret,
                            ));
                        },
                    }
                },

                // ----------------------------------------------------------------------------
                // one-sided stealth address
                // NOTE: Extracting the nonce R and a spending (public aka scan_key) key from the script
                // NOTE: [RFC 203 on Stealth Addresses](https://rfc.tari.com/RFC-0203_StealthAddresses.html)
                [Opcode::PushPubKey(nonce), Opcode::Drop, Opcode::PushPubKey(scanned_pk)] => {
                    // matching spending (public) keys
                    let stealth_address_hasher = self
                        .resources
                        .key_manager
                        .get_diffie_hellman_stealth_domain_hasher(&wallet_sk, nonce.as_ref())
                        .await?;
                    let script_spending_key = stealth_address_script_spending_key(&stealth_address_hasher, &wallet_pk);
                    if &script_spending_key != scanned_pk.as_ref() {
                        continue;
                    }

                    // Compute the stealth address offset
                    let stealth_address_offset = PrivateKey::from_uniform_bytes(stealth_address_hasher.as_ref())
                        .expect("'DomainSeparatedHash<Blake2b<U64>>' has correct size");
                    let stealth_key = self
                        .resources
                        .key_manager
                        .import_add_offset_to_private_key(&wallet_sk, stealth_address_offset)
                        .await?;

                    let shared_secret = self
                        .resources
                        .key_manager
                        .get_diffie_hellman_shared_secret(&wallet_sk, &output.sender_offset_public_key)
                        .await?;
                    scanned_outputs.push((
                        output.clone(),
                        OutputSource::StealthOneSided,
                        stealth_key,
                        shared_secret,
                    ));
                },

                _ => {},
            }
        }

        self.import_onesided_outputs(scanned_outputs).await
    }

    // Import scanned outputs into the wallet
    async fn import_onesided_outputs(
        &self,
        scanned_outputs: Vec<(TransactionOutput, OutputSource, TariKeyId, CommsDHKE)>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let mut rewound_outputs = Vec::with_capacity(scanned_outputs.len());

        for (output, output_source, script_private_key, shared_secret) in scanned_outputs {
            let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
            if let Ok((committed_value, spending_key)) =
                EncryptedData::decrypt_data(&encryption_key, &output.commitment, &output.encrypted_data)
            {
                if output.verify_mask(
                    &self.resources.factories.range_proof,
                    &spending_key,
                    committed_value.into(),
                )? {
                    let spending_key_id = self.resources.key_manager.import_key(spending_key).await?;
                    let hash = output.hash();
                    let rewound_output = WalletOutput::new_with_rangeproof(
                        output.version,
                        committed_value,
                        spending_key_id,
                        output.features,
                        output.script,
                        tari_script::ExecutionStack::new(vec![]),
                        script_private_key,
                        output.sender_offset_public_key,
                        output.metadata_signature,
                        0,
                        output.covenant,
                        output.encrypted_data,
                        output.minimum_value_promise,
                        output.proof,
                    );

                    let tx_id = TxId::new_random();
                    let db_output = DbWalletOutput::from_wallet_output(
                        rewound_output.clone(),
                        &self.resources.key_manager,
                        None,
                        output_source,
                        Some(tx_id),
                        None,
                    )
                    .await?;

                    match self.resources.db.add_unspent_output_with_tx_id(tx_id, db_output) {
                        Ok(_) => {
                            trace!(
                                target: LOG_TARGET,
                                "One-sided payment Output {} with value {} recovered",
                                output.commitment.to_hex(),
                                committed_value,
                            );

                            rewound_outputs.push(RecoveredOutput {
                                output: rewound_output,
                                tx_id,
                                hash,
                            })
                        },
                        Err(OutputManagerStorageError::DuplicateOutput) => {
                            warn!(
                                target: LOG_TARGET,
                                "Attempt to add scanned output {} that already exists. Ignoring the output.",
                                output.commitment.to_hex()
                            );
                        },
                        Err(err) => {
                            return Err(err.into());
                        },
                    }
                }
            }
        }

        Ok(rewound_outputs)
    }

    fn get_fee_calc(&self) -> Fee {
        Fee::new(*self.resources.consensus_constants.transaction_weight_params())
    }
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
            writeln!(f, "Time locked: {}", locked)?;
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

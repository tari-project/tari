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
use itertools::Itertools;
use log::*;
use rand::{rngs::OsRng, RngCore};
use strum::IntoEnumIterator;
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, Commitment, HashOutput, PrivateKey, PublicKey},
};
use tari_comms::{types::CommsDHKE, NodeIdentity};
use tari_core::{
    borsh::SerializedSize,
    consensus::ConsensusConstants,
    covenants::Covenant,
    proto::base_node::FetchMatchingUtxos,
    transactions::{
        fee::Fee,
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedValue,
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionError,
            TransactionInput,
            TransactionOutput,
            TransactionOutputVersion,
            UnblindedOutput,
            UnblindedOutputBuilder,
        },
        transaction_protocol::{sender::TransactionSenderMessage, RewindData, TransactionMetadata},
        CoinbaseBuilder,
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    errors::RangeProofError,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script, Opcode, TariScript};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray};
use tokio::sync::Mutex;

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
            PublicRewindKeys,
            RecoveredOutput,
        },
        input_selection::UtxoSelectionCriteria,
        recovery::StandardUtxoRecoverer,
        resources::{OutputManagerKeyManagerBranch, OutputManagerResources},
        storage::{
            database::{OutputBackendQuery, OutputManagerBackend, OutputManagerDatabase},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript, SpendingPriority},
            OutputSource,
            OutputStatus,
        },
        tasks::TxoValidationTask,
    },
    util::one_sided::{
        diffie_hellman_stealth_domain_hasher,
        shared_secret_to_output_encryption_key,
        shared_secret_to_output_rewind_key,
        stealth_address_script_spending_key,
    },
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
    node_identity: Arc<NodeIdentity>,
    validation_in_progress: Arc<Mutex<()>>,
}

impl<TBackend, TWalletConnectivity, TKeyManagerInterface>
    OutputManagerService<TBackend, TWalletConnectivity, TKeyManagerInterface>
where
    TBackend: OutputManagerBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
    TKeyManagerInterface: KeyManagerInterface,
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
        node_identity: Arc<NodeIdentity>,
        key_manager: TKeyManagerInterface,
    ) -> Result<Self, OutputManagerError> {
        Self::initialise_key_manager(&key_manager).await?;
        let rewind_blinding_key = key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::RecoveryBlinding.get_branch_key(), 0)
            .await?;
        let encryption_key = key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::ValueEncryption.get_branch_key(), 0)
            .await?;
        let rewind_data = RewindData {
            rewind_blinding_key,
            encryption_key,
        };

        let resources = OutputManagerResources {
            config,
            db,
            factories,
            connectivity,
            event_publisher,
            master_key_manager: key_manager,
            consensus_constants,
            shutdown_signal,
            rewind_data,
        };

        Ok(Self {
            resources,
            request_stream: Some(request_stream),
            base_node_service,
            last_seen_tip_height: None,
            node_identity,
            validation_in_progress: Arc::new(Mutex::new(())),
        })
    }

    async fn initialise_key_manager(key_manager: &TKeyManagerInterface) -> Result<(), OutputManagerError> {
        for branch in OutputManagerKeyManagerBranch::iter() {
            key_manager.add_new_branch(branch.get_branch_key()).await?;
        }
        Ok(())
    }

    /// Return the public rewind keys
    pub fn get_rewind_public_keys(&self) -> PublicRewindKeys {
        PublicRewindKeys {
            rewind_blinding_public_key: PublicKey::from_secret_key(&self.resources.rewind_data.rewind_blinding_key),
        }
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

        debug!(target: LOG_TARGET, "Output Manager Service started");
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
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddRewindableOutput((uo, spend_priority, custom_rewind_data)) => self
                .add_rewindable_output(None, *uo, spend_priority, custom_rewind_data)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddOutputWithTxId((tx_id, uo, spend_priority)) => self
                .add_output(Some(tx_id), *uo, spend_priority)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddRewindableOutputWithTxId((tx_id, uo, spend_priority, custom_rewind_data)) => self
                .add_rewindable_output(Some(tx_id), *uo, spend_priority, custom_rewind_data)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::ConvertToRewindableTransactionOutput(uo) => {
                let transaction_output = self.convert_to_rewindable_transaction_output(*uo)?;
                Ok(OutputManagerResponse::ConvertedToTransactionOutput(Box::new(
                    transaction_output,
                )))
            },
            OutputManagerRequest::AddUnvalidatedOutput((tx_id, uo, spend_priority)) => self
                .add_unvalidated_output(tx_id, *uo, spend_priority)
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::UpdateOutputMetadataSignature(uo) => self
                .update_output_metadata_signature(*uo)
                .map(|_| OutputManagerResponse::OutputMetadataSignatureUpdated),
            OutputManagerRequest::GetBalance => {
                let current_tip_for_time_lock_calculation = match self.base_node_service.get_chain_metadata().await {
                    Ok(metadata) => metadata.map(|m| m.height_of_longest_chain()),
                    Err(_) => None,
                };
                self.get_balance(current_tip_for_time_lock_calculation)
                    .map(OutputManagerResponse::Balance)
            },
            OutputManagerRequest::GetRecipientTransaction(tsm) => self
                .get_recipient_transaction(tsm)
                .await
                .map(OutputManagerResponse::RecipientTransactionGenerated),
            OutputManagerRequest::GetCoinbaseTransaction {
                tx_id,
                reward,
                fees,
                block_height,
                extra,
            } => self
                .get_coinbase_transaction(tx_id, reward, fees, block_height, extra)
                .await
                .map(OutputManagerResponse::CoinbaseTransaction),
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
                let outputs = self.fetch_spent_outputs()?.into_iter().map(|v| v.into()).collect();
                Ok(OutputManagerResponse::SpentOutputs(outputs))
            },
            OutputManagerRequest::GetUnspentOutputs => {
                let outputs = self.fetch_unspent_outputs()?;
                Ok(OutputManagerResponse::UnspentOutputs(outputs))
            },
            OutputManagerRequest::GetOutputsBy(q) => {
                let outputs = self.fetch_outputs_by(q)?.into_iter().map(|v| v.into()).collect();
                Ok(OutputManagerResponse::Outputs(outputs))
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

            OutputManagerRequest::ScanForRecoverableOutputs(outputs) => StandardUtxoRecoverer::new(
                self.resources.master_key_manager.clone(),
                self.resources.rewind_data.clone(),
                self.resources.factories.clone(),
                self.resources.db.clone(),
            )
            .scan_and_recover_outputs(outputs)
            .await
            .map(OutputManagerResponse::RewoundOutputs),
            OutputManagerRequest::ScanOutputs(outputs) => self
                .scan_outputs_for_one_sided_payments(outputs)
                .map(OutputManagerResponse::ScanOutputs),
            OutputManagerRequest::AddKnownOneSidedPaymentScript(known_script) => self
                .add_known_script(known_script)
                .map(|_| OutputManagerResponse::AddKnownOneSidedPaymentScript),
            OutputManagerRequest::ReinstateCancelledInboundTx(tx_id) => self
                .reinstate_cancelled_inbound_transaction_outputs(tx_id)
                .map(|_| OutputManagerResponse::ReinstatedCancelledInboundTx),
            OutputManagerRequest::CreateOutputWithFeatures { value, features } => {
                let unblinded_output = self.create_output_with_features(value, *features).await?;
                Ok(OutputManagerResponse::CreateOutputWithFeatures {
                    output: Box::new(unblinded_output),
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
            OutputManagerRequest::SetCoinbaseAbandoned(tx_id, abandoned) => self
                .set_coinbase_abandoned(tx_id, abandoned)
                .map(|_| OutputManagerResponse::CoinbaseAbandonedSet),
            OutputManagerRequest::CreateClaimShaAtomicSwapTransaction(output_hash, pre_image, fee_per_gram) => {
                self.claim_sha_atomic_swap_with_hash(output_hash, pre_image, fee_per_gram)
                    .await
            },
            OutputManagerRequest::CreateHtlcRefundTransaction(output, fee_per_gram) => self
                .create_htlc_refund_transaction(output, fee_per_gram)
                .await
                .map(OutputManagerResponse::ClaimHtlcTransaction),
            OutputManagerRequest::GetOutputStatusesByTxId(tx_id) => {
                let output_statuses_by_tx_id = self.get_output_status_by_tx_id(tx_id)?;
                Ok(OutputManagerResponse::OutputStatusesByTxId(output_statuses_by_tx_id))
            },
            OutputManagerRequest::GetNextSpendAndScriptKeys => {
                let (spend_key, script_key) = self.get_spend_and_script_keys().await?;
                Ok(OutputManagerResponse::NextSpendAndScriptKeys { spend_key, script_key })
            },
            OutputManagerRequest::GetRewindData => {
                Ok(OutputManagerResponse::RewindData(self.resources.rewind_data.clone()))
            },
        }
    }

    fn get_output_status_by_tx_id(&self, tx_id: TxId) -> Result<OutputStatusesByTxId, OutputManagerError> {
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
        Ok(OutputStatusesByTxId {
            statuses,
            mined_height: max_mined_height,
            block_hash,
        })
    }

    async fn claim_sha_atomic_swap_with_hash(
        &mut self,
        output_hash: HashOutput,
        pre_image: PublicKey,
        fee_per_gram: MicroTari,
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
            BaseNodeEvent::BaseNodeStateChanged(state) => {
                let trigger_validation = match (self.last_seen_tip_height, state.chain_metadata.clone()) {
                    (Some(last_seen_tip_height), Some(cm)) => last_seen_tip_height != cm.height_of_longest_chain(),
                    (None, _) => true,
                    _ => false,
                };
                if trigger_validation {
                    let _id = self.validate_outputs().map_err(|e| {
                        warn!(target: LOG_TARGET, "Error validating  txos: {:?}", e);
                        e
                    });
                }
                self.last_seen_tip_height = state.chain_metadata.map(|cm| cm.height_of_longest_chain());
            },
            BaseNodeEvent::NewBlockDetected(_) => {},
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

    /// Add an unblinded non-rewindable output to the outputs table and mark it as `Unspent`.
    pub fn add_output(
        &mut self,
        tx_id: Option<TxId>,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value
        );

        let output = DbUnblindedOutput::from_unblinded_output(
            output,
            &self.resources.factories,
            spend_priority,
            OutputSource::default(),
            tx_id,
            None,
        )?;
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

    /// Add an unblinded rewindable output to the outputs table and marks is as `Unspent`.
    pub fn add_rewindable_output(
        &mut self,
        tx_id: Option<TxId>,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
        custom_rewind_data: Option<RewindData>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value
        );

        let rewind_data = if let Some(value) = custom_rewind_data {
            value
        } else {
            self.resources.rewind_data.clone()
        };
        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            output,
            &self.resources.factories,
            &rewind_data,
            spend_priority,
            None,
            OutputSource::default(),
            tx_id,
            None,
        )?;
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

    /// Convert an unblinded rewindable output into rewindable transaction output using the key manager's rewind data
    pub fn convert_to_rewindable_transaction_output(
        &mut self,
        output: UnblindedOutput,
    ) -> Result<TransactionOutput, OutputManagerError> {
        let transaction_output =
            output.as_rewindable_transaction_output(&Default::default(), &self.resources.rewind_data, None)?;
        Ok(transaction_output)
    }

    /// Add an unblinded output to the outputs table and marks is as `EncumberedToBeReceived`. This is so that it will
    /// require a successful validation to confirm that it indeed spendable.
    pub fn add_unvalidated_output(
        &mut self,
        tx_id: TxId,
        output: UnblindedOutput,
        spend_priority: Option<SpendingPriority>,
    ) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add unvalidated output of value {} to Output Manager", output.value
        );
        let output = DbUnblindedOutput::from_unblinded_output(
            output,
            &self.resources.factories,
            spend_priority,
            OutputSource::default(),
            Some(tx_id),
            None,
        )?;
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

    async fn get_spend_and_script_keys(&self) -> Result<(PrivateKey, PrivateKey), OutputManagerError> {
        let result = self
            .resources
            .master_key_manager
            .get_next_key(OutputManagerKeyManagerBranch::Spend.get_branch_key())
            .await?;
        let script_key = self
            .resources
            .master_key_manager
            .get_key_at_index(
                OutputManagerKeyManagerBranch::SpendScript.get_branch_key(),
                result.index,
            )
            .await?;
        Ok((result.key, script_key))
    }

    async fn create_output_with_features(
        &mut self,
        value: MicroTari,
        features: OutputFeatures,
    ) -> Result<UnblindedOutputBuilder, OutputManagerError> {
        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
        let input_data = inputs!(PublicKey::from_secret_key(&script_private_key));
        let script = script!(Nop);

        Ok(UnblindedOutputBuilder::new(value, spending_key)
            .with_features(features)
            .with_script(script)
            .with_input_data(input_data)
            .with_rewind_data(self.resources.rewind_data.clone())
            .with_script_private_key(script_private_key))
    }

    fn get_balance(&self, current_tip_for_time_lock_calculation: Option<u64>) -> Result<Balance, OutputManagerError> {
        let balance = self.resources.db.get_balance(current_tip_for_time_lock_calculation)?;
        trace!(target: LOG_TARGET, "Balance: {:?}", balance);
        Ok(balance)
    }

    /// Request a receiver transaction be generated from the supplied Sender Message
    async fn get_recipient_transaction(
        &mut self,
        sender_message: TransactionSenderMessage,
    ) -> Result<ReceiverTransactionProtocol, OutputManagerError> {
        let single_round_sender_data = match sender_message.single() {
            Some(data) => data,
            _ => return Err(OutputManagerError::InvalidSenderMessage),
        };

        // Confirm script hash is for the expected script, at the moment assuming Nop
        if single_round_sender_data.script != script!(Nop) {
            return Err(OutputManagerError::InvalidScriptHash);
        }

        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;

        let commitment = self
            .resources
            .factories
            .commitment
            .commit_value(&spending_key, single_round_sender_data.amount.as_u64());
        let features = single_round_sender_data.features.clone();
        let encrypted_value = EncryptedValue::encrypt_value(
            &self.resources.rewind_data.encryption_key,
            &commitment,
            single_round_sender_data.amount,
        )?;
        let minimum_value_promise = single_round_sender_data.minimum_value_promise;
        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            UnblindedOutput::new_current_version(
                single_round_sender_data.amount,
                spending_key.clone(),
                features.clone(),
                single_round_sender_data.script.clone(),
                // TODO: The input data should be variable; this will only work for a Nop script #LOGGED
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                single_round_sender_data.sender_offset_public_key.clone(),
                // Note: The signature at this time is only partially built
                TransactionOutput::create_receiver_partial_metadata_signature(
                    TransactionOutputVersion::get_current_version(),
                    single_round_sender_data.amount,
                    &spending_key,
                    &single_round_sender_data.script,
                    &features,
                    &single_round_sender_data.sender_offset_public_key,
                    &single_round_sender_data.ephemeral_public_nonce,
                    &single_round_sender_data.covenant,
                    &encrypted_value,
                    minimum_value_promise,
                )?,
                0,
                single_round_sender_data.covenant.clone(),
                encrypted_value,
                minimum_value_promise,
            ),
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
            None,
            OutputSource::default(),
            Some(single_round_sender_data.tx_id),
            None,
        )?;

        self.resources
            .db
            .add_output_to_be_received(single_round_sender_data.tx_id, output, None)?;

        let nonce = PrivateKey::random(&mut OsRng);

        let rtp = ReceiverTransactionProtocol::new_with_rewindable_output(
            sender_message.clone(),
            nonce,
            spending_key,
            &self.resources.factories,
            &self.resources.rewind_data,
        );

        Ok(rtp)
    }

    /// Get a fee estimate for an amount of MicroTari, at a specified fee per gram and given number of kernels and
    /// outputs.
    async fn fee_estimate(
        &mut self,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        num_kernels: usize,
        num_outputs: usize,
    ) -> Result<MicroTari, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Getting fee estimate. Amount: {}. Fee per gram: {}. Num kernels: {}. Num outputs: {}",
            amount,
            fee_per_gram,
            num_kernels,
            num_outputs
        );
        // TODO: Include asset metadata here if required
        // We assume that default OutputFeatures and Nop TariScript is used
        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_features_and_scripts_size(
                OutputFeatures::default().get_serialized_size() +
                    script![Nop].get_serialized_size() +
                    Covenant::new().get_serialized_size(),
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
                    output_features_estimate.get_serialized_size() +
                        script![Nop].get_serialized_size() +
                        Covenant::new().get_serialized_size(),
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
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        tx_meta: TransactionMetadata,
        message: String,
        recipient_output_features: OutputFeatures,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
        recipient_minimum_value_promise: MicroTari,
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
            .transaction_weight()
            .round_up_features_and_scripts_size(
                recipient_output_features.get_serialized_size() +
                    recipient_script.get_serialized_size() +
                    recipient_covenant.get_serialized_size(),
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

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1, self.resources.consensus_constants.clone());
        builder
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount)
            .with_recipient_data(
                0,
                recipient_script,
                PrivateKey::random(&mut OsRng),
                recipient_output_features,
                PrivateKey::random(&mut OsRng),
                recipient_covenant,
                recipient_minimum_value_promise,
            )
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_lock_height(tx_meta.lock_height)
            .with_kernel_features(tx_meta.kernel_features)
            .with_tx_id(tx_id);

        for uo in input_selection.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }
        debug!(
            target: LOG_TARGET,
            "Calculating fee for tx with: Fee per gram: {}. Num selected inputs: {}",
            amount,
            input_selection.num_selected()
        );

        if input_selection.requires_change_output() {
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            builder
                .with_change_secret(spending_key)
                .with_rewindable_outputs(self.resources.rewind_data.clone())
                .with_change_script(
                    script!(Nop),
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                );
        }

        let stp = builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let mut change_output = Vec::<DbUnblindedOutput>::new();
        if input_selection.requires_change_output() {
            let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            change_output.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            )?);
        }

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), change_output)?;

        debug!(target: LOG_TARGET, "Prepared transaction (TxId: {}) to send", tx_id);

        Ok(stp)
    }

    /// Request a Coinbase transaction for a specific block height. All existing pending transactions with
    /// the corresponding output hash will be cancelled.
    /// The key will be derived from the coinbase specific keychain using the blockheight as an index. The coinbase
    /// keychain is based on the wallets master_key and the "coinbase" branch.
    async fn get_coinbase_transaction(
        &mut self,
        tx_id: TxId,
        reward: MicroTari,
        fees: MicroTari,
        block_height: u64,
        extra: Vec<u8>,
    ) -> Result<Transaction, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Building coinbase transaction for block_height {} with TxId: {}", block_height, tx_id
        );

        let spending_key = self
            .resources
            .master_key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::Coinbase.get_branch_key(), block_height)
            .await?;
        let script_private_key = self
            .resources
            .master_key_manager
            .get_key_at_index(
                OutputManagerKeyManagerBranch::CoinbaseScript.get_branch_key(),
                block_height,
            )
            .await?;

        let nonce = PrivateKey::random(&mut OsRng);
        let (tx, unblinded_output) = CoinbaseBuilder::new(self.resources.factories.clone())
            .with_block_height(block_height)
            .with_fees(fees)
            .with_spend_key(spending_key.clone())
            .with_script_key(script_private_key)
            .with_script(script!(Nop))
            .with_nonce(nonce)
            .with_rewind_data(self.resources.rewind_data.clone())
            .with_extra(extra)
            .build_with_reward(&self.resources.consensus_constants, reward)?;

        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            unblinded_output,
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
            None,
            OutputSource::Coinbase,
            Some(tx_id),
            None,
        )?;

        // If there is no existing output available, we store the one we produced.
        match self.resources.db.fetch_by_commitment(output.commitment.clone()) {
            Ok(_) => {},
            Err(OutputManagerStorageError::ValueNotFound) => {
                self.resources
                    .db
                    .add_output_to_be_received(tx_id, output, Some(block_height))?;

                self.confirm_encumberance(tx_id)?;
            },
            Err(e) => return Err(e.into()),
        };

        Ok(tx)
    }

    async fn create_pay_to_self_containing_outputs(
        &mut self,
        outputs: Vec<UnblindedOutputBuilder>,
        selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        let total_value = outputs.iter().map(|o| o.value()).sum();
        let nop_script = script![Nop];
        let weighting = self.resources.consensus_constants.transaction_weight();
        let features_and_scripts_byte_size = outputs.iter().fold(0usize, |total, output| {
            total +
                weighting.round_up_features_and_scripts_size({
                    output.features().get_serialized_size() +
                        output.covenant().get_serialized_size() +
                        output.script().unwrap_or(&nop_script).get_serialized_size()
                })
        });

        let input_selection = self
            .select_utxos(
                total_value,
                selection_criteria,
                fee_per_gram,
                outputs.len(),
                features_and_scripts_byte_size,
            )
            .await?;
        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_prevent_fee_gt_amount(false)
            .with_kernel_features(KernelFeatures::empty());

        for uo in input_selection.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }

        if input_selection.requires_change_output() {
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.rewind_data.clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let mut db_outputs = vec![];
        for mut unblinded_output in outputs {
            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            unblinded_output.sign_as_sender_and_receiver(&sender_offset_private_key)?;

            let ub = unblinded_output.try_build()?;
            builder
                .with_output(ub.clone(), sender_offset_private_key.clone())
                .map_err(|e| OutputManagerError::BuildError(e.message))?;
            db_outputs.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                ub,
                &self.resources.factories,
                &self.resources.rewind_data,
                None,
                None,
                OutputSource::default(),
                None,
                None,
            )?)
        }

        let mut stp = builder
            .build(&self.resources.factories, None, u64::MAX)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        let tx_id = stp.get_tx_id()?;
        if let Some(unblinded_output) = stp.get_change_unblinded_output()? {
            db_outputs.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
                &self.resources.rewind_data,
                None,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            )?);
        }

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), db_outputs)?;
        stp.finalize()?;

        Ok((tx_id, stp.take_transaction()?))
    }

    #[allow(clippy::too_many_lines)]
    async fn create_pay_to_self_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        selection_criteria: UtxoSelectionCriteria,
        output_features: OutputFeatures,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<(MicroTari, Transaction), OutputManagerError> {
        let script = script!(Nop);
        let covenant = Covenant::default();

        let features_and_scripts_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_features_and_scripts_size(
                output_features.get_serialized_size() + script.get_serialized_size() + covenant.get_serialized_size(),
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

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_rewindable_outputs(self.resources.rewind_data.clone())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_kernel_features(KernelFeatures::empty())
            .with_tx_id(tx_id);

        for uo in input_selection.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }

        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
        let commitment = self
            .resources
            .factories
            .commitment
            .commit_value(&spending_key, amount.into());
        let encrypted_value =
            EncryptedValue::encrypt_value(&self.resources.rewind_data.encryption_key, &commitment, amount)?;
        let minimum_amount_promise = MicroTari::zero();
        let metadata_signature = TransactionOutput::create_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            amount,
            &spending_key.clone(),
            &script,
            &output_features,
            &sender_offset_private_key,
            &covenant,
            &encrypted_value,
            minimum_amount_promise,
        )?;
        let utxo = DbUnblindedOutput::rewindable_from_unblinded_output(
            UnblindedOutput::new_current_version(
                amount,
                spending_key.clone(),
                output_features,
                script,
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                PublicKey::from_secret_key(&sender_offset_private_key),
                metadata_signature,
                0,
                covenant,
                encrypted_value,
                minimum_amount_promise,
            ),
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
            None,
            OutputSource::default(),
            Some(tx_id),
            None,
        )?;
        builder
            .with_output(utxo.unblinded_output.clone(), sender_offset_private_key.clone())
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        let mut outputs = vec![utxo];

        if input_selection.requires_change_output() {
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.rewind_data.clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let mut stp = builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        if input_selection.requires_change_output() {
            let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            let change_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
                &self.resources.rewind_data,
                None,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            )?;
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
        stp.finalize()?;
        let tx = stp.take_transaction()?;

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
        amount: MicroTari,
        mut selection_criteria: UtxoSelectionCriteria,
        fee_per_gram: MicroTari,
        num_outputs: usize,
        total_output_features_and_scripts_byte_size: usize,
    ) -> Result<UtxoSelection, OutputManagerError> {
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
        let tip_height = chain_metadata.as_ref().map(|m| m.height_of_longest_chain());
        let uo = self
            .resources
            .db
            .fetch_unspent_outputs_for_spending(&selection_criteria, amount, tip_height)?;

        // For non-standard queries, we want to ensure that the intended UTXOs are selected
        if !selection_criteria.filter.is_standard() && uo.is_empty() {
            return Err(OutputManagerError::NoUtxosSelected {
                criteria: selection_criteria,
            });
        }

        // Assumes that default Outputfeatures are used for change utxo
        let output_features_estimate = OutputFeatures::default();
        let default_features_and_scripts_size = fee_calc.weighting().round_up_features_and_scripts_size(
            output_features_estimate.get_serialized_size() +
                Covenant::new().get_serialized_size() +
                script![Nop].get_serialized_size(),
        );

        trace!(target: LOG_TARGET, "We found {} UTXOs to select from", uo.len());

        let mut requires_change_output = false;
        let mut utxos_total_value = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);
        for o in uo {
            utxos_total_value += o.unblinded_output.value;

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

        if !perfect_utxo_selection && !enough_spendable {
            let current_tip_for_time_lock_calculation = chain_metadata.map(|cm| cm.height_of_longest_chain());
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

    pub fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_spent_outputs()?)
    }

    pub fn fetch_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_all_unspent_outputs()?)
    }

    pub fn fetch_outputs_by(&self, q: OutputBackendQuery) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_outputs_by(q)?)
    }

    pub fn fetch_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.get_invalid_outputs()?)
    }

    pub fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerError> {
        self.resources.db.set_coinbase_abandoned(tx_id, abandoned)?;
        Ok(())
    }

    fn default_features_and_scripts_size(&self) -> usize {
        self.resources
            .consensus_constants
            .transaction_weight()
            .round_up_features_and_scripts_size(
                script!(Nop).get_serialized_size() + OutputFeatures::default().get_serialized_size(),
            )
    }

    pub async fn preview_coin_join_with_commitments(
        &self,
        commitments: Vec<Commitment>,
        fee_per_gram: MicroTari,
    ) -> Result<(Vec<MicroTari>, MicroTari), OutputManagerError> {
        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroTari::zero(),
            None,
        )?;

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroTari::zero(), |acc, x| acc + x.unblinded_output.value);

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            1,
            self.default_features_and_scripts_size(),
        );

        Ok((vec![accumulated_amount.saturating_sub(fee)], fee))
    }

    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<Commitment>,
        number_of_splits: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(Vec<MicroTari>, MicroTari), OutputManagerError> {
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
            MicroTari::zero(),
            None,
        )?;

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            number_of_splits,
            self.default_features_and_scripts_size() * number_of_splits,
        );

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroTari::zero(), |acc, x| acc + x.unblinded_output.value);

        let aftertax_amount = accumulated_amount.saturating_sub(fee);
        let amount_per_split = MicroTari(aftertax_amount.as_u64() / number_of_splits as u64);
        let unspent_remainder = MicroTari(aftertax_amount.as_u64() % amount_per_split.as_u64());
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
        amount_per_split: Option<MicroTari>,
        number_of_splits: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        if commitments.is_empty() {
            return Err(OutputManagerError::NoCommitmentsProvided);
        }

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroTari::zero(),
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
        amount_per_split: Option<MicroTari>,
        number_of_splits: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        match amount_per_split {
            None => Err(OutputManagerError::InvalidArgument(
                "coin split without `amount_per_split` is not supported yet".to_string(),
            )),
            Some(amount_per_split) => {
                let selection = self
                    .select_utxos(
                        amount_per_split * MicroTari(number_of_splits as u64),
                        UtxoSelectionCriteria::largest_first(),
                        fee_per_gram,
                        number_of_splits,
                        self.default_features_and_scripts_size() * number_of_splits,
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
        src_outputs: Vec<DbUnblindedOutput>,
        number_of_splits: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        if number_of_splits == 0 {
            return Err(OutputManagerError::InvalidArgument(
                "number_of_splits must be greater than 0".to_string(),
            ));
        }

        let covenant = Covenant::default();
        let default_features_and_scripts_size = self.default_features_and_scripts_size();
        let mut dest_outputs = Vec::with_capacity(number_of_splits + 1);

        // accumulated value amount from given source outputs
        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroTari::zero(), |acc, x| acc + x.unblinded_output.value);

        let fee = self.get_fee_calc().calculate(
            fee_per_gram,
            1,
            src_outputs.len(),
            number_of_splits,
            default_features_and_scripts_size * number_of_splits,
        );

        let aftertax_amount = accumulated_amount.saturating_sub(fee);
        let amount_per_split = MicroTari(aftertax_amount.as_u64() / number_of_splits as u64);
        let unspent_remainder = MicroTari(aftertax_amount.as_u64() % amount_per_split.as_u64());

        // preliminary balance check
        if self.get_balance(None)?.available_balance < (aftertax_amount + fee) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        trace!(target: LOG_TARGET, "initializing new split (even) transaction");

        let mut tx_builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(PrivateKey::random(&mut OsRng))
            .with_private_nonce(PrivateKey::random(&mut OsRng))
            .with_kernel_features(KernelFeatures::empty())
            .with_rewindable_outputs(self.resources.rewind_data.clone());

        // collecting inputs from source outputs
        let inputs: Vec<TransactionInput> = src_outputs
            .iter()
            .map(|src_out| {
                src_out
                    .unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)
            })
            .try_collect()?;

        // adding inputs to the transaction
        src_outputs.iter().zip(inputs).for_each(|(src_output, input)| {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                src_output.hash
            );
            tx_builder.with_input(input, src_output.unblinded_output.clone());
        });

        for i in 1..=number_of_splits {
            // NOTE: adding the unspent `change` to the last output
            let amount_per_split = if i == number_of_splits {
                amount_per_split + unspent_remainder
            } else {
                amount_per_split
            };

            let noop_script = script!(Nop);
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            let output_features = OutputFeatures::default();

            // generating sender's keypair
            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
            let commitment = self
                .resources
                .factories
                .commitment
                .commit_value(&spending_key, amount_per_split.into());
            let encrypted_value = EncryptedValue::encrypt_value(
                &self.resources.rewind_data.encryption_key,
                &commitment,
                amount_per_split,
            )?;

            let minimum_amount_promise = MicroTari::zero();
            let commitment_signature = TransactionOutput::create_metadata_signature(
                TransactionOutputVersion::get_current_version(),
                amount_per_split,
                &spending_key,
                &noop_script,
                &output_features,
                &sender_offset_private_key,
                &covenant,
                &encrypted_value,
                minimum_amount_promise,
            )?;

            let output = DbUnblindedOutput::rewindable_from_unblinded_output(
                UnblindedOutput::new_current_version(
                    amount_per_split,
                    spending_key,
                    output_features,
                    noop_script,
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                    sender_offset_public_key,
                    commitment_signature,
                    0,
                    covenant.clone(),
                    encrypted_value,
                    minimum_amount_promise,
                ),
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
                OutputSource::default(),
                None,
                None,
            )?;

            tx_builder
                .with_output(output.unblinded_output.clone(), sender_offset_private_key)
                .map_err(|e| OutputManagerError::BuildError(e.message))?;

            dest_outputs.push(output);
        }

        let mut stp = tx_builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
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
        stp.finalize()?;

        Ok((tx_id, stp.take_transaction()?, aftertax_amount + fee))
    }

    #[allow(clippy::too_many_lines)]
    async fn create_coin_split(
        &mut self,
        src_outputs: Vec<DbUnblindedOutput>,
        amount_per_split: MicroTari,
        number_of_splits: usize,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        if number_of_splits == 0 {
            return Err(OutputManagerError::InvalidArgument(
                "number_of_splits must be greater than 0".to_string(),
            ));
        }

        if amount_per_split == MicroTari::zero() {
            return Err(OutputManagerError::InvalidArgument(
                "amount_per_split must be greater than 0".to_string(),
            ));
        }

        let covenant = Covenant::default();
        let default_features_and_scripts_size = self.default_features_and_scripts_size();
        let mut dest_outputs = Vec::with_capacity(number_of_splits + 1);
        let total_split_amount = MicroTari::from(amount_per_split.as_u64() * number_of_splits as u64);

        // accumulated value amount from given source outputs
        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroTari::zero(), |acc, x| acc + x.unblinded_output.value);

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

        // NOTE: called `leftover` to remove possible brainlag by confusing `change` as a verb
        let leftover_change = accumulated_amount.saturating_sub(total_split_amount + final_fee);

        // ----------------------------------------------------------------------------
        // initializing new transaction

        trace!(target: LOG_TARGET, "initializing new split transaction");

        let mut tx_builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(PrivateKey::random(&mut OsRng))
            .with_private_nonce(PrivateKey::random(&mut OsRng))
            .with_rewindable_outputs(self.resources.rewind_data.clone())
            .with_kernel_features(KernelFeatures::empty());

        // collecting inputs from source outputs
        let inputs: Vec<TransactionInput> = src_outputs
            .iter()
            .map(|src_out| {
                src_out
                    .unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)
            })
            .try_collect()?;

        // adding inputs to the transaction
        src_outputs.iter().zip(inputs).for_each(|(src_output, input)| {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                src_output.hash
            );
            tx_builder.with_input(input, src_output.unblinded_output.clone());
        });

        // ----------------------------------------------------------------------------
        // initializing primary outputs

        for _ in 0..number_of_splits {
            let noop_script = script!(Nop);
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            let output_features = OutputFeatures::default();

            // generating sender's keypair
            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
            let commitment = self
                .resources
                .factories
                .commitment
                .commit_value(&spending_key, amount_per_split.into());
            let encrypted_value = EncryptedValue::encrypt_value(
                &self.resources.rewind_data.encryption_key,
                &commitment,
                amount_per_split,
            )?;
            let minimum_value_promise = MicroTari::zero();
            let commitment_signature = TransactionOutput::create_metadata_signature(
                TransactionOutputVersion::get_current_version(),
                amount_per_split,
                &spending_key,
                &noop_script,
                &output_features,
                &sender_offset_private_key,
                &covenant,
                &encrypted_value,
                minimum_value_promise,
            )?;

            let output = DbUnblindedOutput::rewindable_from_unblinded_output(
                UnblindedOutput::new_current_version(
                    amount_per_split,
                    spending_key,
                    output_features,
                    noop_script,
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                    sender_offset_public_key,
                    commitment_signature,
                    0,
                    covenant.clone(),
                    encrypted_value,
                    minimum_value_promise,
                ),
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
                OutputSource::default(),
                None,
                None,
            )?;

            tx_builder
                .with_output(output.unblinded_output.clone(), sender_offset_private_key)
                .map_err(|e| OutputManagerError::BuildError(e.message))?;

            dest_outputs.push(output);
        }

        let has_leftover_change = leftover_change > MicroTari::zero();

        // extending transaction if there is some `change` left over
        if has_leftover_change {
            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            tx_builder.with_change_secret(spending_key);
            tx_builder.with_rewindable_outputs(self.resources.rewind_data.clone());
            tx_builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let mut stp = tx_builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
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
            let unblinded_output_for_change = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a `change` output metadata signature available".to_string(),
                )
            })?;

            // appending `change` output to the result
            dest_outputs.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output_for_change,
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
                OutputSource::default(),
                Some(tx_id),
                None,
            )?);
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
        stp.finalize()?;

        let value = if has_leftover_change {
            total_split_amount
        } else {
            total_split_amount + final_fee
        };

        Ok((tx_id, stp.take_transaction()?, value))
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_coin_join(
        &mut self,
        commitments: Vec<Commitment>,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        let covenant = Covenant::default();
        let noop_script = script!(Nop);
        let default_features_and_scripts_size = self.default_features_and_scripts_size();

        let src_outputs = self.resources.db.fetch_unspent_outputs_for_spending(
            &UtxoSelectionCriteria::specific(commitments),
            MicroTari::zero(),
            None,
        )?;

        let accumulated_amount = src_outputs
            .iter()
            .fold(MicroTari::zero(), |acc, x| acc + x.unblinded_output.value);

        let fee =
            self.get_fee_calc()
                .calculate(fee_per_gram, 1, src_outputs.len(), 1, default_features_and_scripts_size);

        let aftertax_amount = accumulated_amount.saturating_sub(fee);

        // checking, again, whether a total output value is enough
        if aftertax_amount == MicroTari::zero() {
            error!(target: LOG_TARGET, "failed to join coins, not enough funds");
            return Err(OutputManagerError::NotEnoughFunds);
        }

        // preliminary balance check
        if self.get_balance(None)?.available_balance < aftertax_amount {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        // ----------------------------------------------------------------------------
        // initializing new transaction

        trace!(target: LOG_TARGET, "initializing new join transaction");

        let mut tx_builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        tx_builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(PrivateKey::random(&mut OsRng))
            .with_private_nonce(PrivateKey::random(&mut OsRng))
            .with_rewindable_outputs(self.resources.rewind_data.clone())
            .with_kernel_features(KernelFeatures::empty());

        // collecting inputs from source outputs
        let inputs: Vec<TransactionInput> = src_outputs
            .iter()
            .map(|src_out| {
                src_out
                    .unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)
            })
            .try_collect()?;

        // adding inputs to the transaction
        src_outputs.iter().zip(inputs).for_each(|(src_output, input)| {
            trace!(
                target: LOG_TARGET,
                "adding transaction input: output_hash=: {:?}",
                src_output.hash
            );
            tx_builder.with_input(input, src_output.unblinded_output.clone());
        });

        // initializing primary output
        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
        let output_features = OutputFeatures::default();

        // generating sender's keypair
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
        let commitment = self
            .resources
            .factories
            .commitment
            .commit_value(&spending_key, aftertax_amount.into());
        let encrypted_value =
            EncryptedValue::encrypt_value(&self.resources.rewind_data.encryption_key, &commitment, aftertax_amount)?;
        let minimum_value_promise = MicroTari::zero();
        let commitment_signature = TransactionOutput::create_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            aftertax_amount,
            &spending_key,
            &noop_script,
            &output_features,
            &sender_offset_private_key,
            &covenant,
            &encrypted_value,
            minimum_value_promise,
        )?;

        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            UnblindedOutput::new_current_version(
                aftertax_amount,
                spending_key,
                output_features,
                noop_script,
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                sender_offset_public_key,
                commitment_signature,
                0,
                covenant.clone(),
                encrypted_value,
                minimum_value_promise,
            ),
            &self.resources.factories,
            &self.resources.rewind_data.clone(),
            None,
            None,
            OutputSource::default(),
            None,
            None,
        )?;

        tx_builder
            .with_output(output.unblinded_output.clone(), sender_offset_private_key)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        let mut stp = tx_builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
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
        stp.finalize()?;

        Ok((tx_id, stp.take_transaction()?, aftertax_amount + fee))
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
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, MicroTari, MicroTari, Transaction), OutputManagerError> {
        let shared_secret = CommsDHKE::new(
            self.node_identity.as_ref().secret_key(),
            &output.sender_offset_public_key,
        );
        let blinding_key = shared_secret_to_output_rewind_key(&shared_secret)?;
        let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
        if let Ok(amount) = EncryptedValue::decrypt_value(&encryption_key, &output.commitment, &output.encrypted_value)
        {
            let blinding_factor = output.recover_mask(&self.resources.factories.range_proof, &blinding_key)?;
            if output.verify_mask(&self.resources.factories.range_proof, &blinding_factor, amount.as_u64())? {
                let rewound_output = UnblindedOutput::new(
                    output.version,
                    amount,
                    blinding_factor,
                    output.features,
                    output.script,
                    inputs!(pre_image),
                    self.node_identity.as_ref().secret_key().clone(),
                    output.sender_offset_public_key,
                    output.metadata_signature,
                    // Although the technically the script does have a script lock higher than 0, this does not apply
                    // to to us as we are claiming the Hashed part which has a 0 time lock
                    0,
                    output.covenant,
                    output.encrypted_value,
                    output.minimum_value_promise,
                );

                let offset = PrivateKey::random(&mut OsRng);
                let nonce = PrivateKey::random(&mut OsRng);
                let message = "SHA-XTR atomic swap".to_string();

                // Create builder with no recipients (other than ourselves)
                let mut builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
                builder
                    .with_lock_height(0)
                    .with_fee_per_gram(fee_per_gram)
                    .with_offset(offset.clone())
                    .with_private_nonce(nonce.clone())
                    .with_message(message)
                    .with_kernel_features(KernelFeatures::empty())
                    .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
                    .with_input(
                        rewound_output.as_transaction_input(&self.resources.factories.commitment)?,
                        rewound_output,
                    );

                let mut outputs = Vec::new();

                let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
                builder.with_change_secret(spending_key);
                builder.with_rewindable_outputs(self.resources.rewind_data.clone());
                builder.with_change_script(
                    script!(Nop),
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                );

                let mut stp = builder
                    .build(
                        &self.resources.factories,
                        None,
                        self.last_seen_tip_height.unwrap_or(u64::MAX),
                    )
                    .map_err(|e| OutputManagerError::BuildError(e.message))?;

                let tx_id = stp.get_tx_id()?;

                let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                    OutputManagerError::BuildError(
                        "There should be a change output metadata signature available".to_string(),
                    )
                })?;
                let change_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                    unblinded_output,
                    &self.resources.factories,
                    &self.resources.rewind_data,
                    None,
                    None,
                    OutputSource::AtomicSwap,
                    Some(tx_id),
                    None,
                )?;
                outputs.push(change_output);

                trace!(target: LOG_TARGET, "Claiming HTLC with transaction ({}).", tx_id);
                self.resources.db.encumber_outputs(tx_id, Vec::new(), outputs)?;
                self.confirm_encumberance(tx_id)?;
                let fee = stp.get_fee_amount()?;
                trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", tx_id);
                stp.finalize()?;
                let tx = stp.take_transaction()?;

                Ok((tx_id, fee, amount - fee, tx))
            } else {
                Err(OutputManagerError::TransactionError(TransactionError::RangeProofError(
                    RangeProofError::InvalidRewind(
                        "Atomic swap: Blinding factor could not open the commitment!".to_string(),
                    ),
                )))
            }
        } else {
            Err(OutputManagerError::TransactionError(TransactionError::RangeProofError(
                RangeProofError::InvalidRewind("Atomic swap: Encrypted value could not be decrypted!".to_string()),
            )))
        }
    }

    pub async fn create_htlc_refund_transaction(
        &mut self,
        output_hash: HashOutput,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, MicroTari, MicroTari, Transaction), OutputManagerError> {
        let output = self.resources.db.get_unspent_output(output_hash)?.unblinded_output;

        let amount = output.value;

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);
        let message = "SHA-XTR atomic refund".to_string();

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_message(message)
            .with_kernel_features(KernelFeatures::empty())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_input(
                output.as_transaction_input(&self.resources.factories.commitment)?,
                output,
            );

        let mut outputs = Vec::new();

        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
        builder.with_change_secret(spending_key);
        builder.with_rewindable_outputs(self.resources.rewind_data.clone());
        builder.with_change_script(
            script!(Nop),
            inputs!(PublicKey::from_secret_key(&script_private_key)),
            script_private_key,
        );

        let mut stp = builder
            .build(
                &self.resources.factories,
                None,
                self.last_seen_tip_height.unwrap_or(u64::MAX),
            )
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        let tx_id = stp.get_tx_id()?;

        let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
            OutputManagerError::BuildError("There should be a change output metadata signature available".to_string())
        })?;

        let change_output = DbUnblindedOutput::rewindable_from_unblinded_output(
            unblinded_output,
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
            None,
            OutputSource::Refund,
            Some(tx_id),
            None,
        )?;
        outputs.push(change_output);

        trace!(target: LOG_TARGET, "Claiming HTLC refund with transaction ({}).", tx_id);

        let fee = stp.get_fee_amount()?;

        stp.finalize()?;

        let tx = stp.take_transaction()?;

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
    fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        // TODO: use MultiKey
        // NOTE: known keys is a list consisting of an actual and deprecated wallet keys
        let known_keys = self.resources.db.get_all_known_one_sided_payment_scripts()?;

        let wallet_sk = self.node_identity.secret_key().clone();
        let wallet_pk = self.node_identity.public_key();

        let mut scanned_outputs = vec![];

        for output in outputs {
            match output.script.as_slice() {
                // ----------------------------------------------------------------------------
                // simple one-sided address
                [Opcode::PushPubKey(scanned_pk)] => {
                    match known_keys
                        .iter()
                        .find(|x| &PublicKey::from_secret_key(&x.private_key) == scanned_pk.as_ref())
                    {
                        // none of the keys match, skipping
                        None => continue,

                        // match found
                        Some(matched_key) => {
                            let shared_secret =
                                CommsDHKE::new(&matched_key.private_key, &output.sender_offset_public_key);
                            scanned_outputs.push((
                                output.clone(),
                                OutputSource::OneSided,
                                matched_key.private_key.clone(),
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
                    // Compute the stealth address offset
                    let stealth_address_hasher = diffie_hellman_stealth_domain_hasher(&wallet_sk, nonce.as_ref());
                    let stealth_address_offset = PrivateKey::from_bytes(stealth_address_hasher.as_ref())
                        .expect("'DomainSeparatedHash<Blake256>' has correct size");

                    // matching spending (public) keys
                    let script_spending_key = stealth_address_script_spending_key(&stealth_address_hasher, wallet_pk);
                    if &script_spending_key != scanned_pk.as_ref() {
                        continue;
                    }

                    let shared_secret = CommsDHKE::new(&wallet_sk, &output.sender_offset_public_key);
                    scanned_outputs.push((
                        output.clone(),
                        OutputSource::StealthOneSided,
                        wallet_sk.clone() + stealth_address_offset,
                        shared_secret,
                    ));
                },

                _ => {},
            }
        }

        self.import_onesided_outputs(scanned_outputs)
    }

    // Imports scanned outputs into the wallet
    fn import_onesided_outputs(
        &self,
        scanned_outputs: Vec<(TransactionOutput, OutputSource, PrivateKey, CommsDHKE)>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let mut rewound_outputs = Vec::with_capacity(scanned_outputs.len());

        for (output, output_source, script_private_key, shared_secret) in scanned_outputs {
            let rewind_blinding_key = shared_secret_to_output_rewind_key(&shared_secret)?;
            let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
            let committed_value =
                EncryptedValue::decrypt_value(&encryption_key, &output.commitment, &output.encrypted_value);

            if let Ok(committed_value) = committed_value {
                let blinding_factor =
                    output.recover_mask(&self.resources.factories.range_proof, &rewind_blinding_key)?;

                if output.verify_mask(
                    &self.resources.factories.range_proof,
                    &blinding_factor,
                    committed_value.into(),
                )? {
                    let rewound_output = UnblindedOutput::new(
                        output.version,
                        committed_value,
                        blinding_factor.clone(),
                        output.features,
                        output.script,
                        tari_script::ExecutionStack::new(vec![]),
                        script_private_key,
                        output.sender_offset_public_key,
                        output.metadata_signature,
                        0,
                        output.covenant,
                        output.encrypted_value,
                        output.minimum_value_promise,
                    );

                    let tx_id = TxId::new_random();
                    let db_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                        rewound_output.clone(),
                        &self.resources.factories,
                        &RewindData {
                            rewind_blinding_key,
                            encryption_key,
                        },
                        None,
                        Some(&output.proof),
                        output_source,
                        Some(tx_id),
                        None,
                    )?;

                    let output_hex = output.commitment.to_hex();

                    match self.resources.db.add_unspent_output_with_tx_id(tx_id, db_output) {
                        Ok(_) => {
                            trace!(
                                target: LOG_TARGET,
                                "One-sided payment Output {} with value {} recovered",
                                output_hex,
                                committed_value,
                            );

                            rewound_outputs.push(RecoveredOutput {
                                output: rewound_output,
                                tx_id,
                            })
                        },
                        Err(OutputManagerStorageError::DuplicateOutput) => {
                            warn!(
                                target: LOG_TARGET,
                                "Attempt to add scanned output {} that already exists. Ignoring the output.",
                                output_hex
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
        Fee::new(*self.resources.consensus_constants.transaction_weight())
    }
}

/// This struct holds the detailed balance of the Output Manager Service.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// The current balance that is available to spend
    pub available_balance: MicroTari,
    /// The amount of the available balance that is current time-locked, None if no chain tip is provided
    pub time_locked_balance: Option<MicroTari>,
    /// The current balance of funds that are due to be received but have not yet been confirmed
    pub pending_incoming_balance: MicroTari,
    /// The current balance of funds encumbered in pending outbound transactions that have not been confirmed
    pub pending_outgoing_balance: MicroTari,
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
    utxos: Vec<DbUnblindedOutput>,
    requires_change_output: bool,
    total_value: MicroTari,
    fee_without_change: MicroTari,
    fee_with_change: MicroTari,
}

#[allow(dead_code)]
impl UtxoSelection {
    pub fn as_final_fee(&self) -> MicroTari {
        if self.requires_change_output {
            return self.fee_with_change;
        }
        self.fee_without_change
    }

    pub fn requires_change_output(&self) -> bool {
        self.requires_change_output
    }

    /// Total value of the selected inputs
    pub fn total_value(&self) -> MicroTari {
        self.total_value
    }

    pub fn num_selected(&self) -> usize {
        self.utxos.len()
    }

    pub fn into_selected(self) -> Vec<DbUnblindedOutput> {
        self.utxos
    }

    pub fn iter(&self) -> impl Iterator<Item = &DbUnblindedOutput> + '_ {
        self.utxos.iter()
    }
}

#[derive(Debug, Clone)]
pub struct OutputStatusesByTxId {
    pub statuses: Vec<OutputStatus>,
    pub(crate) mined_height: Option<u64>,
    pub(crate) block_hash: Option<BlockHash>,
}

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

use std::{convert::TryInto, fmt, fmt::Display, sync::Arc};

use blake2::Digest;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use futures::{pin_mut, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    transaction::TxId,
    types::{BlockHash, HashOutput, PrivateKey, PublicKey},
};
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_core::{
    consensus::{ConsensusConstants, ConsensusEncodingSized},
    covenants::Covenant,
    proto::base_node::FetchMatchingUtxos,
    transactions::{
        fee::Fee,
        tari_amount::MicroTari,
        transaction_components::{
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionOutput,
            TransactionOutputVersion,
            UnblindedOutput,
            UnblindedOutputBuilder,
        },
        transaction_protocol::{sender::TransactionSenderMessage, RewindData},
        CoinbaseBuilder,
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{DiffieHellmanSharedSecret, PublicKey as PublicKeyTrait, SecretKey},
    range_proof::REWIND_USER_MESSAGE_LENGTH,
};
use tari_script::{inputs, script, TariScript};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    base_node_service::handle::{BaseNodeEvent, BaseNodeServiceHandle},
    connectivity_service::WalletConnectivityInterface,
    key_manager_service::KeyManagerInterface,
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
        recovery::StandardUtxoRecoverer,
        resources::{OutputManagerKeyManagerBranch, OutputManagerResources},
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript, SpendingPriority},
            OutputStatus,
        },
        tasks::TxoValidationTask,
    },
    types::HashDigest,
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
        let rewind_key = key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::RecoveryViewOnly.get_branch_key(), 0)
            .await?;
        let rewind_blinding_key = key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::RecoveryBlinding.get_branch_key(), 0)
            .await?;
        let recovery_byte_key = key_manager
            .get_key_at_index(OutputManagerKeyManagerBranch::RecoveryByte.get_branch_key(), 0)
            .await?;
        let rewind_data = RewindData {
            rewind_key,
            rewind_blinding_key,
            recovery_byte_key,
            proof_message: [0u8; REWIND_USER_MESSAGE_LENGTH],
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
        })
    }

    async fn initialise_key_manager(key_manager: &TKeyManagerInterface) -> Result<(), OutputManagerError> {
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::Spend.get_branch_key())
            .await?;
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::SpendScript.get_branch_key())
            .await?;
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::Coinbase.get_branch_key())
            .await?;
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::CoinbaseScript.get_branch_key())
            .await?;
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::RecoveryViewOnly.get_branch_key())
            .await?;
        key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::RecoveryByte.get_branch_key())
            .await?;
        match key_manager
            .add_new_branch(OutputManagerKeyManagerBranch::RecoveryBlinding.get_branch_key())
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(OutputManagerError::KeyManagerServiceError(e)),
        }
    }

    /// Return the public rewind keys
    pub fn get_rewind_public_keys(&self) -> PublicRewindKeys {
        PublicRewindKeys {
            rewind_public_key: PublicKey::from_secret_key(&self.resources.rewind_data.rewind_key),
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
            OutputManagerRequest::GetCoinbaseTransaction((tx_id, reward, fees, block_height)) => self
                .get_coinbase_transaction(tx_id, reward, fees, block_height)
                .await
                .map(OutputManagerResponse::CoinbaseTransaction),
            OutputManagerRequest::PrepareToSendTransaction {
                tx_id,
                amount,
                unique_id,
                parent_public_key,
                fee_per_gram,
                lock_height,
                message,
                script,
                covenant,
            } => self
                .prepare_transaction_to_send(
                    tx_id,
                    amount,
                    unique_id,
                    parent_public_key,
                    fee_per_gram,
                    lock_height,
                    message,
                    script,
                    covenant,
                )
                .await
                .map(OutputManagerResponse::TransactionToSend),
            OutputManagerRequest::CreatePayToSelfTransaction {
                tx_id,
                amount,
                unique_id,
                parent_public_key,
                fee_per_gram,
                lock_height,
                message,
            } => self
                .create_pay_to_self_transaction(
                    tx_id,
                    amount,
                    unique_id,
                    parent_public_key,
                    fee_per_gram,
                    lock_height,
                    message,
                )
                .await
                .map(OutputManagerResponse::PayToSelfTransaction),
            OutputManagerRequest::FeeEstimate {
                amount,
                fee_per_gram,
                num_kernels,
                num_outputs,
            } => self
                .fee_estimate(amount, fee_per_gram, num_kernels, num_outputs)
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
                let outputs = self.fetch_unspent_outputs()?.into_iter().map(|v| v.into()).collect();
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
            OutputManagerRequest::CreateCoinSplit((amount_per_split, split_count, fee_per_gram, lock_height)) => self
                .create_coin_split(amount_per_split, split_count, fee_per_gram, lock_height)
                .await
                .map(OutputManagerResponse::Transaction),
            OutputManagerRequest::ApplyEncryption(cipher) => self
                .resources
                .db
                .apply_encryption(*cipher)
                .map(|_| OutputManagerResponse::EncryptionApplied)
                .map_err(OutputManagerError::OutputManagerStorageError),
            OutputManagerRequest::RemoveEncryption => self
                .resources
                .db
                .remove_encryption()
                .map(|_| OutputManagerResponse::EncryptionRemoved)
                .map_err(OutputManagerError::OutputManagerStorageError),

            OutputManagerRequest::GetPublicRewindKeys => Ok(OutputManagerResponse::PublicRewindKeys(Box::new(
                self.get_rewind_public_keys(),
            ))),
            OutputManagerRequest::CalculateRecoveryByte {
                spending_key,
                value,
                with_rewind_data,
            } => Ok(OutputManagerResponse::RecoveryByte(self.calculate_recovery_byte(
                spending_key,
                value,
                with_rewind_data,
            )?)),
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
                spending_unique_id,
                spending_parent_public_key,
            } => {
                let (tx_id, transaction) = self
                    .create_pay_to_self_containing_outputs(
                        outputs,
                        fee_per_gram,
                        spending_unique_id.as_ref(),
                        spending_parent_public_key.as_ref(),
                    )
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
                    block_hash = uo.mined_in_block.clone();
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
        if !self.resources.connectivity.is_base_node_set() {
            return Err(OutputManagerError::NoBaseNodeKeysProvided);
        }
        let id = OsRng.next_u64();
        let utxo_validation = TxoValidationTask::new(
            id,
            self.resources.db.clone(),
            self.resources.connectivity.clone(),
            self.resources.event_publisher.clone(),
            self.resources.config.clone(),
        );

        let shutdown = self.resources.shutdown_signal.clone();
        let event_publisher = self.resources.event_publisher.clone();
        tokio::spawn(async move {
            match utxo_validation.execute(shutdown).await {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "UTXO Validation Protocol (Id: {}) completed successfully", id
                    );
                },
                Err(OutputManagerProtocolError { id, error }) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error completing UTXO Validation Protocol (Id: {}): {:?}", id, error
                    );
                    if let Err(e) = event_publisher.send(Arc::new(OutputManagerEvent::TxoValidationFailure(id))) {
                        debug!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {:?}", e
                        );
                    }
                },
            }
        });
        Ok(id)
    }

    fn revalidate_outputs(&mut self) -> Result<u64, OutputManagerError> {
        self.resources.db.set_outputs_to_be_revalidated()?;
        self.validate_outputs()
    }

    pub fn calculate_recovery_byte(
        &mut self,
        spending_key: PrivateKey,
        value: u64,
        with_rewind_data: bool,
    ) -> Result<u8, OutputManagerError> {
        let commitment = self.resources.factories.commitment.commit_value(&spending_key, value);
        let rewind_data = if with_rewind_data {
            Some(&self.resources.rewind_data)
        } else {
            None
        };
        let recovery_byte = OutputFeatures::create_unique_recovery_byte(&commitment, rewind_data);
        Ok(recovery_byte)
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

        let output = DbUnblindedOutput::from_unblinded_output(output, &self.resources.factories, spend_priority)?;
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
        let output = DbUnblindedOutput::from_unblinded_output(output, &self.resources.factories, spend_priority)?;
        self.resources.db.add_unvalidated_output(tx_id, output)?;
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

        let commitment = self
            .resources
            .factories
            .commitment
            .commit_value(&spending_key, value.as_u64());
        let updated_features = OutputFeatures::features_with_updated_recovery_byte(
            &commitment,
            Some(&self.resources.rewind_data),
            &features.clone(),
        );

        Ok(UnblindedOutputBuilder::new(value, spending_key)
            .with_features(updated_features)
            .with_script(script)
            .with_input_data(input_data)
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
        let updated_features = OutputFeatures::features_with_updated_recovery_byte(
            &commitment,
            Some(&self.resources.rewind_data),
            &single_round_sender_data.features.clone(),
        );
        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            UnblindedOutput::new_current_version(
                single_round_sender_data.amount,
                spending_key.clone(),
                updated_features.clone(),
                single_round_sender_data.script.clone(),
                // TODO: The input data should be variable; this will only work for a Nop script #LOGGED
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                single_round_sender_data.sender_offset_public_key.clone(),
                // Note: The commitment signature at this time is only partially built
                TransactionOutput::create_partial_metadata_signature(
                    TransactionOutputVersion::get_current_version(),
                    single_round_sender_data.amount,
                    &spending_key,
                    &single_round_sender_data.script,
                    &updated_features,
                    &single_round_sender_data.sender_offset_public_key,
                    &single_round_sender_data.public_commitment_nonce,
                    &single_round_sender_data.covenant,
                )?,
                0,
                single_round_sender_data.covenant.clone(),
            ),
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
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
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_metadata_size(
                OutputFeatures::default().consensus_encode_exact_size() +
                    script![Nop].consensus_encode_exact_size() +
                    Covenant::new().consensus_encode_exact_size(),
            );

        let utxo_selection = self
            .select_utxos(
                amount,
                fee_per_gram,
                num_outputs,
                metadata_byte_size * num_outputs,
                None,
                None,
                None,
            )
            .await?;

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
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
        recipient_script: TariScript,
        recipient_covenant: Covenant,
    ) -> Result<SenderTransactionProtocol, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Preparing to send transaction. Amount: {}. Unique id : {:?}. Fee per gram: {}. ",
            amount,
            unique_id,
            fee_per_gram,
        );
        let output_features_estimate = OutputFeatures::default();
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_metadata_size(
                output_features_estimate.consensus_encode_exact_size() +
                    recipient_script.consensus_encode_exact_size() +
                    recipient_covenant.consensus_encode_exact_size(),
            );

        let input_selection = self
            .select_utxos(
                amount,
                fee_per_gram,
                1,
                metadata_byte_size,
                None,
                unique_id.as_ref(),
                parent_public_key.as_ref(),
            )
            .await?;

        // TODO: improve this logic #LOGGED
        let recipient_output_features = match unique_id {
            Some(ref _unique_id) => match input_selection
                .utxos
                .iter()
                .find(|output| output.unblinded_output.features.unique_id.is_some())
            {
                Some(output) => output.unblinded_output.features.clone(),
                _ => OutputFeatures::default(),
            },
            _ => OutputFeatures::default(),
        };

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1, self.resources.consensus_constants.clone());
        builder
            .with_lock_height(lock_height.unwrap_or(0))
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
            )
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
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
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.rewind_data.clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let stp = builder
            .build::<HashDigest>(
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
    /// this blockheight will be cancelled.
    /// The key will be derived from the coinbase specific keychain using the blockheight as an index. The coinbase
    /// keychain is based on the wallets master_key and the "coinbase" branch.
    async fn get_coinbase_transaction(
        &mut self,
        tx_id: TxId,
        reward: MicroTari,
        fees: MicroTari,
        block_height: u64,
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
            .build_with_reward(&self.resources.consensus_constants, reward)?;

        let output = DbUnblindedOutput::rewindable_from_unblinded_output(
            unblinded_output,
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
            None,
        )?;

        // Clear any existing pending coinbase transactions for this blockheight if they exist
        match self
            .resources
            .db
            .clear_pending_coinbase_transaction_at_block_height(block_height)
        {
            Ok(_) => {
                debug!(
                    target: LOG_TARGET,
                    "An existing pending coinbase was cleared for block height {}", block_height
                )
            },
            Err(e) => match e {
                OutputManagerStorageError::DieselError(DieselError::NotFound) => {},
                _ => return Err(OutputManagerError::from(e)),
            },
        };

        // Clear any matching outputs for this commitment. Even if the older output is valid
        // we are losing no information as this output has the same commitment.
        match self.resources.db.remove_output_by_commitment(output.commitment.clone()) {
            Ok(_) => {},
            Err(OutputManagerStorageError::ValueNotFound) => {},
            Err(e) => return Err(e.into()),
        }

        self.resources
            .db
            .add_output_to_be_received(tx_id, output, Some(block_height))?;

        self.confirm_encumberance(tx_id)?;

        Ok(tx)
    }

    async fn create_pay_to_self_containing_outputs(
        &mut self,
        outputs: Vec<UnblindedOutputBuilder>,
        fee_per_gram: MicroTari,
        spending_unique_id: Option<&Vec<u8>>,
        spending_parent_public_key: Option<&PublicKey>,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        let total_value = MicroTari(outputs.iter().fold(0u64, |running, out| running + out.value.as_u64()));
        let nop_script = script![Nop];
        let weighting = self.resources.consensus_constants.transaction_weight();
        let metadata_byte_size = outputs.iter().fold(0usize, |total, output| {
            total +
                weighting.round_up_metadata_size({
                    output.features.consensus_encode_exact_size() +
                        output
                            .script
                            .as_ref()
                            .unwrap_or(&nop_script)
                            .consensus_encode_exact_size()
                })
        });
        let input_selection = self
            .select_utxos(
                total_value,
                fee_per_gram,
                outputs.len(),
                metadata_byte_size,
                None,
                spending_unique_id,
                spending_parent_public_key,
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
            .with_prevent_fee_gt_amount(false);

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
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

            let public_offset_commitment_private_key = PrivateKey::random(&mut OsRng);
            let public_offset_commitment_pub_key = PublicKey::from_secret_key(&public_offset_commitment_private_key);

            unblinded_output
                .update_recovery_byte_if_required(&self.resources.factories, Some(&self.resources.rewind_data))?;
            unblinded_output.sign_as_receiver(sender_offset_public_key, public_offset_commitment_pub_key)?;
            unblinded_output.sign_as_sender(&sender_offset_private_key)?;

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
            )?)
        }

        // let mut change_keys = None;
        //
        // let fee = Fee::calculate(fee_per_gram, 1, inputs.len(), 1);
        // let change_value = total.saturating_sub(fee);
        // if change_value > 0.into() {
        //     let (spending_key, script_private_key) = self
        //         .resources
        //         .master_key_manager
        //         .get_next_spend_and_script_key()
        //         .await?;
        //     change_keys = Some((spending_key.clone(), script_private_key.clone()));
        //     builder.with_change_secret(spending_key);
        //     builder.with_rewindable_outputs(&self.resources.rewind_data.clone());
        //     builder.with_change_script(
        //         script!(Nop),
        //         inputs!(PublicKey::from_secret_key(&script_private_key)),
        //         script_private_key,
        //     );
        // }

        let mut stp = builder
            .build::<HashDigest>(&self.resources.factories, None, u64::MAX)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        // if let Some((spending_key, script_private_key)) = change_keys {
        //     // let change_script_offset_public_key = stp.get_change_sender_offset_public_key()?.ok_or_else(|| {
        //     //     OutputManagerError::BuildError(
        //     //         "There should be a change script offset public key available".to_string(),
        //     //     )
        //     // })?;
        //
        //     let sender_offset_private_key = PrivateKey::random(&mut OsRng);
        //     let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
        //
        //     let public_offset_commitment_private_key = PrivateKey::random(&mut OsRng);
        //     let public_offset_commitment_pub_key = PublicKey::from_secret_key(&public_offset_commitment_private_key);
        //
        //     let mut output_builder = UnblindedOutputBuilder::new(stp.get_change_amount()?, spending_key)
        //         .with_script(script!(Nop))
        //         .with_input_data(inputs!(PublicKey::from_secret_key(&script_private_key)))
        //         .with_script_private_key(script_private_key);
        //
        //     output_builder.sign_as_receiver(sender_offset_public_key, public_offset_commitment_pub_key)?;
        //     output_builder.sign_as_sender(&sender_offset_private_key)?;
        //

        //     let change_output =
        //         DbUnblindedOutput::from_unblinded_output(output_builder.try_build()?, &self.resources.factories)?;
        //
        //     db_outputs.push(change_output);
        // }

        if let Some(unblinded_output) = stp.get_change_unblinded_output()? {
            db_outputs.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
                &self.resources.rewind_data,
                None,
                None,
            )?);
        }
        let tx_id = stp.get_tx_id()?;

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), db_outputs)?;
        stp.finalize(KernelFeatures::empty(), &self.resources.factories, None, u64::MAX)?;

        Ok((tx_id, stp.take_transaction()?))
    }

    #[allow(clippy::too_many_lines)]
    async fn create_pay_to_self_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(MicroTari, Transaction), OutputManagerError> {
        let script = script!(Nop);
        let covenant = Covenant::default();
        let output_features_estimate = OutputFeatures {
            unique_id: unique_id.clone(),
            ..Default::default()
        };
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_metadata_size(
                output_features_estimate.consensus_encode_exact_size() +
                    script.consensus_encode_exact_size() +
                    covenant.consensus_encode_exact_size(),
            );

        let input_selection = self
            .select_utxos(
                amount,
                fee_per_gram,
                1,
                metadata_byte_size,
                None,
                unique_id.as_ref(),
                parent_public_key.as_ref(),
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
            .with_message(message)
            .with_rewindable_outputs(self.resources.rewind_data.clone())
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_tx_id(tx_id);

        for uo in input_selection.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }

        let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
        let recovery_byte = self.calculate_recovery_byte(spending_key.clone(), amount.as_u64(), true)?;
        let output_features = OutputFeatures {
            recovery_byte,
            unique_id: unique_id.clone(),
            ..Default::default()
        };
        let metadata_signature = TransactionOutput::create_final_metadata_signature(
            TransactionOutputVersion::get_current_version(),
            amount,
            &spending_key.clone(),
            &script,
            &output_features,
            &sender_offset_private_key,
            &covenant,
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
            ),
            &self.resources.factories,
            &self.resources.rewind_data,
            None,
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

        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(
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
        stp.finalize(
            KernelFeatures::empty(),
            &factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )?;
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
        fee_per_gram: MicroTari,
        num_outputs: usize,
        output_metadata_byte_size: usize,
        strategy: Option<UTXOSelectionStrategy>,
        unique_id: Option<&Vec<u8>>,
        parent_public_key: Option<&PublicKey>,
    ) -> Result<UtxoSelection, OutputManagerError> {
        let token = match unique_id {
            Some(unique_id) => {
                debug!(target: LOG_TARGET, "Looking for {:?}", unique_id);
                // todo: new method to fetch by unique asset id
                let uo = self.resources.db.fetch_all_unspent_outputs()?;
                if let Some(token_id) = uo.into_iter().find(|x| match &x.unblinded_output.features.unique_id {
                    Some(token_unique_id) => {
                        debug!(target: LOG_TARGET, "Comparing with {:?}", token_unique_id);
                        token_unique_id == unique_id &&
                            x.unblinded_output.features.parent_public_key.as_ref() == parent_public_key
                    },
                    _ => false,
                }) {
                    Some(token_id)
                } else {
                    return Err(OutputManagerError::TokenUniqueIdNotFound);
                }
            },
            _ => None,
        };
        debug!(
            target: LOG_TARGET,
            "select_utxos amount: {}, token : {:?}, fee_per_gram: {}, num_outputs: {}, output_metadata_byte_size: {}, \
             strategy: {:?}",
            amount,
            token,
            fee_per_gram,
            num_outputs,
            output_metadata_byte_size,
            strategy
        );
        let mut utxos = Vec::new();

        let mut utxos_total_value = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);
        let fee_calc = self.get_fee_calc();
        if let Some(token) = token {
            utxos_total_value = token.unblinded_output.value;
            utxos.push(token);
        }

        // Attempt to get the chain tip height
        let chain_metadata = self.base_node_service.get_chain_metadata().await?;
        let (connected, tip_height) = match &chain_metadata {
            Some(metadata) => (true, Some(metadata.height_of_longest_chain())),
            None => (false, None),
        };

        // If no strategy was specified and no metadata is available, then make sure to use MaturitythenSmallest
        let strategy = match (strategy, connected) {
            (Some(s), _) => s,
            (None, false) => UTXOSelectionStrategy::MaturityThenSmallest,
            (None, true) => UTXOSelectionStrategy::Default, // use the selection heuristic next
        };

        // Heuristic for selection strategy: Default to MaturityThenSmallest, but if the amount is greater than
        // the largest UTXO, use Largest UTXOs first.
        // let strategy = match (strategy, uo.is_empty()) {
        //     (Some(s), _) => s,
        //     (None, true) => UTXOSelectionStrategy::Smallest,
        //     (None, false) => {
        //         let largest_utxo = &uo[uo.len() - 1];
        //         if amount > largest_utxo.unblinded_output.value {
        //             UTXOSelectionStrategy::Largest
        //         } else {
        //             UTXOSelectionStrategy::MaturityThenSmallest
        //         }
        //     },
        // };
        warn!(target: LOG_TARGET, "select_utxos selection strategy: {}", strategy);
        let uo = self
            .resources
            .db
            .fetch_unspent_outputs_for_spending(strategy, amount, tip_height)?;
        trace!(target: LOG_TARGET, "We found {} UTXOs to select from", uo.len());

        // Assumes that default Outputfeatures are used for change utxo
        let output_features_estimate = OutputFeatures::default();
        let default_metadata_size = fee_calc.weighting().round_up_metadata_size(
            output_features_estimate.consensus_encode_exact_size() + script![Nop].consensus_encode_exact_size(),
        );
        let mut requires_change_output = false;
        for o in uo {
            utxos_total_value += o.unblinded_output.value;
            utxos.push(o);
            // The assumption here is that the only output will be the payment output and change if required
            fee_without_change =
                fee_calc.calculate(fee_per_gram, 1, utxos.len(), num_outputs, output_metadata_byte_size);
            if utxos_total_value == amount + fee_without_change {
                break;
            }
            fee_with_change = fee_calc.calculate(
                fee_per_gram,
                1,
                utxos.len(),
                num_outputs + 1,
                output_metadata_byte_size + default_metadata_size,
            );
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

    pub fn fetch_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.get_invalid_outputs()?)
    }

    pub fn set_coinbase_abandoned(&self, tx_id: TxId, abandoned: bool) -> Result<(), OutputManagerError> {
        self.resources.db.set_coinbase_abandoned(tx_id, abandoned)?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn create_coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<(TxId, Transaction, MicroTari), OutputManagerError> {
        trace!(
            target: LOG_TARGET,
            "Select UTXOs and estimate coin split transaction fee."
        );
        let output_count = split_count;
        let script = script!(Nop);
        let covenant = Covenant::default();
        let output_features_estimate = OutputFeatures::default();
        let metadata_byte_size = self
            .resources
            .consensus_constants
            .transaction_weight()
            .round_up_metadata_size(
                output_features_estimate.consensus_encode_exact_size() +
                    script.consensus_encode_exact_size() +
                    covenant.consensus_encode_exact_size(),
            );

        let total_split_amount = amount_per_split * split_count as u64;
        let input_selection = self
            .select_utxos(
                total_split_amount,
                fee_per_gram,
                output_count,
                output_count * metadata_byte_size,
                Some(UTXOSelectionStrategy::Largest),
                None,
                None,
            )
            .await?;

        trace!(target: LOG_TARGET, "Construct coin split transaction.");
        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(0, self.resources.consensus_constants.clone());
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_rewindable_outputs(self.resources.rewind_data.clone());

        trace!(target: LOG_TARGET, "Add inputs to coin split transaction.");
        for uo in input_selection.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }

        let utxos_total_value = input_selection.total_value();
        trace!(target: LOG_TARGET, "Add outputs to coin split transaction.");
        let mut outputs: Vec<DbUnblindedOutput> = Vec::with_capacity(output_count);
        for _ in 0..output_count {
            let output_amount = amount_per_split;

            let (spending_key, script_private_key) = self.get_spend_and_script_keys().await?;
            let recovery_byte = self.calculate_recovery_byte(spending_key.clone(), output_amount.as_u64(), true)?;
            let output_features = OutputFeatures {
                recovery_byte,
                ..Default::default()
            };

            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
            let metadata_signature = TransactionOutput::create_final_metadata_signature(
                TransactionOutputVersion::get_current_version(),
                output_amount,
                &spending_key,
                &script,
                &output_features,
                &sender_offset_private_key,
                &covenant,
            )?;
            let utxo = DbUnblindedOutput::rewindable_from_unblinded_output(
                UnblindedOutput::new_current_version(
                    output_amount,
                    spending_key,
                    output_features,
                    script.clone(),
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                    sender_offset_public_key,
                    metadata_signature,
                    0,
                    covenant.clone(),
                ),
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
            )?;
            builder
                .with_output(utxo.unblinded_output.clone(), sender_offset_private_key)
                .map_err(|e| OutputManagerError::BuildError(e.message))?;
            outputs.push(utxo);
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

        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(
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
            "Encumber coin split transaction ({}) outputs.",
            tx_id
        );

        if input_selection.requires_change_output() {
            let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            outputs.push(DbUnblindedOutput::rewindable_from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
                &self.resources.rewind_data.clone(),
                None,
                None,
            )?);
        }

        self.resources
            .db
            .encumber_outputs(tx_id, input_selection.into_selected(), outputs)?;
        self.confirm_encumberance(tx_id)?;
        trace!(target: LOG_TARGET, "Finalize coin split transaction ({}).", tx_id);
        stp.finalize(
            KernelFeatures::empty(),
            &factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )?;
        let tx = stp.take_transaction()?;
        Ok((tx_id, tx, utxos_total_value))
    }

    async fn fetch_outputs_from_node(
        &mut self,
        hashes: Vec<HashOutput>,
    ) -> Result<Vec<TransactionOutput>, OutputManagerError> {
        // lets get the output from the blockchain
        let req = FetchMatchingUtxos { output_hashes: hashes };
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

    pub async fn create_claim_sha_atomic_swap_transaction(
        &mut self,
        output: TransactionOutput,
        pre_image: PublicKey,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, MicroTari, MicroTari, Transaction), OutputManagerError> {
        let spending_key = PrivateKey::from_bytes(
            CommsPublicKey::shared_secret(
                self.node_identity.as_ref().secret_key(),
                &output.sender_offset_public_key,
            )
            .as_bytes(),
        )?;
        let blinding_key = PrivateKey::from_bytes(&hash_secret_key(&spending_key))?;
        let rewind_key = PrivateKey::from_bytes(&hash_secret_key(&blinding_key))?;
        let rewound =
            output.full_rewind_range_proof(&self.resources.factories.range_proof, &rewind_key, &blinding_key)?;

        let rewound_output = UnblindedOutput::new(
            output.version,
            rewound.committed_value,
            rewound.blinding_factor.clone(),
            output.features,
            output.script,
            inputs!(pre_image),
            self.node_identity.as_ref().secret_key().clone(),
            output.sender_offset_public_key,
            output.metadata_signature,
            // Although the technically the script does have a script lock higher than 0, this does not apply to to us
            // as we are claiming the Hashed part which has a 0 time lock
            0,
            output.covenant,
        );
        let amount = rewound.committed_value;

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

        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(
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
        )?;
        outputs.push(change_output);

        trace!(target: LOG_TARGET, "Claiming HTLC with transaction ({}).", tx_id);
        self.resources.db.encumber_outputs(tx_id, Vec::new(), outputs)?;
        self.confirm_encumberance(tx_id)?;
        let fee = stp.get_fee_amount()?;
        trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", tx_id);
        stp.finalize(
            KernelFeatures::empty(),
            &factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )?;
        let tx = stp.take_transaction()?;

        Ok((tx_id, fee, amount - fee, tx))
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

        let factories = CryptoFactories::default();
        println!("he`");
        let mut stp = builder
            .build::<HashDigest>(
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
        )?;
        outputs.push(change_output);

        trace!(target: LOG_TARGET, "Claiming HTLC refund with transaction ({}).", tx_id);

        let fee = stp.get_fee_amount()?;

        stp.finalize(
            KernelFeatures::empty(),
            &factories,
            None,
            self.last_seen_tip_height.unwrap_or(u64::MAX),
        )?;

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

    /// Attempt to scan and then rewind all of the given transaction outputs into unblinded outputs based on known
    /// pubkeys
    fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<RecoveredOutput>, OutputManagerError> {
        let known_one_sided_payment_scripts: Vec<KnownOneSidedPaymentScript> =
            self.resources.db.get_all_known_one_sided_payment_scripts()?;

        let mut rewound_outputs: Vec<RecoveredOutput> = Vec::new();
        for output in outputs {
            let position = known_one_sided_payment_scripts
                .iter()
                .position(|known_one_sided_script| known_one_sided_script.script == output.script);
            if let Some(i) = position {
                let spending_key = PrivateKey::from_bytes(
                    CommsPublicKey::shared_secret(
                        &known_one_sided_payment_scripts[i].private_key,
                        &output.sender_offset_public_key,
                    )
                    .as_bytes(),
                )?;
                let rewind_blinding_key = PrivateKey::from_bytes(&hash_secret_key(&spending_key))?;
                let rewind_key = PrivateKey::from_bytes(&hash_secret_key(&rewind_blinding_key))?;
                let recovery_byte_key = PrivateKey::from_bytes(&hash_secret_key(&rewind_key))?;
                let rewound = output.full_rewind_range_proof(
                    &self.resources.factories.range_proof,
                    &rewind_key,
                    &rewind_blinding_key,
                );

                if let Ok(rewound_result) = rewound {
                    let rewound_output = UnblindedOutput::new(
                        output.version,
                        rewound_result.committed_value,
                        rewound_result.blinding_factor.clone(),
                        output.features,
                        known_one_sided_payment_scripts[i].script.clone(),
                        known_one_sided_payment_scripts[i].input.clone(),
                        known_one_sided_payment_scripts[i].private_key.clone(),
                        output.sender_offset_public_key,
                        output.metadata_signature,
                        known_one_sided_payment_scripts[i].script_lock_height,
                        output.covenant,
                    );

                    let db_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                        rewound_output.clone(),
                        &self.resources.factories,
                        &RewindData {
                            rewind_key,
                            rewind_blinding_key,
                            recovery_byte_key,
                            proof_message: [0u8; 21],
                        },
                        None,
                        Some(&output.proof),
                    )?;

                    let output_hex = output.commitment.to_hex();
                    let tx_id = TxId::new_random();

                    match self.resources.db.add_unspent_output_with_tx_id(tx_id, db_output) {
                        Ok(_) => {
                            rewound_outputs.push(RecoveredOutput {
                                output: rewound_output,
                                tx_id,
                            });
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
                    trace!(
                        target: LOG_TARGET,
                        "One-sided payment Output {} with value {} recovered",
                        output_hex,
                        rewound_result.committed_value,
                    );
                }
            }
        }
        Ok(rewound_outputs)
    }

    fn get_fee_calc(&self) -> Fee {
        Fee::new(*self.resources.consensus_constants.transaction_weight())
    }
}

/// Different UTXO selection strategies for choosing which UTXO's are used to fulfill a transaction
#[derive(Debug, PartialEq)]
pub enum UTXOSelectionStrategy {
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit
    // is removing small UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
    // Start from oldest maturity to reduce the likelihood of grabbing locked up UTXOs
    MaturityThenSmallest,
    // A strategy that selects the largest UTXOs first. Preferred when the amount is large
    Largest,
    // Heuristic for selection strategy: MaturityThenSmallest, but if the amount is greater than
    // the largest UTXO, use Largest UTXOs first
    Default,
}

impl Display for UTXOSelectionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UTXOSelectionStrategy::Smallest => write!(f, "Smallest"),
            UTXOSelectionStrategy::MaturityThenSmallest => write!(f, "MaturityThenSmallest"),
            UTXOSelectionStrategy::Largest => write!(f, "Largest"),
            UTXOSelectionStrategy::Default => write!(f, "Default"),
        }
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

fn hash_secret_key(key: &PrivateKey) -> Vec<u8> {
    HashDigest::new().chain(key.as_bytes()).finalize().to_vec()
}

#[derive(Debug, Clone)]
struct UtxoSelection {
    utxos: Vec<DbUnblindedOutput>,
    requires_change_output: bool,
    total_value: MicroTari,
    fee_without_change: MicroTari,
    fee_with_change: MicroTari,
}

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

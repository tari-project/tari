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

use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::{self, Display},
    sync::Arc,
    time::Duration,
};

use blake2::Digest;
use chrono::Utc;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use futures::{pin_mut, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use tari_crypto::{
    inputs,
    keys::{DiffieHellmanSharedSecret, PublicKey as PublicKeyTrait, SecretKey},
    script,
    script::TariScript,
    tari_utilities::{hex::Hex, ByteArray},
};
use tokio::sync::broadcast;

use tari_common_types::types::{PrivateKey, PublicKey};
use tari_comms::{
    connectivity::ConnectivityRequester,
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_core::{
    consensus::ConsensusConstants,
    transactions::{
        fee::Fee,
        tari_amount::MicroTari,
        transaction::{
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionInput,
            TransactionOutput,
            UnblindedOutput,
        },
        transaction_protocol::sender::TransactionSenderMessage,
        CoinbaseBuilder,
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerProtocolError, OutputManagerStorageError},
        handle::{OutputManagerEventSender, OutputManagerRequest, OutputManagerResponse},
        recovery::StandardUtxoRecoverer,
        resources::OutputManagerResources,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
            models::{DbUnblindedOutput, KnownOneSidedPaymentScript},
        },
        tasks::{TxoValidationTask, TxoValidationType},
        MasterKeyManager,
    },
    transaction_service::handle::TransactionServiceHandle,
    types::{HashDigest, ValidationRetryStrategy},
};
use tari_core::transactions::{transaction::UnblindedOutputBuilder, transaction_protocol::TxId};

const LOG_TARGET: &str = "wallet::output_manager_service";
const LOG_TARGET_STRESS: &str = "stress_test::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    resources: OutputManagerResources<TBackend>,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    base_node_update_publisher: broadcast::Sender<CommsPublicKey>,
    base_node_service: BaseNodeServiceHandle,
}

impl<TBackend> OutputManagerService<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        config: OutputManagerServiceConfig,
        transaction_service: TransactionServiceHandle,
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
        connectivity_manager: ConnectivityRequester,
        master_secret_key: CommsSecretKey,
    ) -> Result<OutputManagerService<TBackend>, OutputManagerError> {
        // Clear any encumberances for transactions that were being negotiated but did not complete to become official
        // Pending Transactions.
        db.clear_short_term_encumberances().await?;

        let master_key_manager = MasterKeyManager::new(master_secret_key, db.clone()).await?;

        let resources = OutputManagerResources {
            config,
            db,
            transaction_service,
            factories,
            base_node_public_key: None,
            event_publisher,
            master_key_manager: Arc::new(master_key_manager),
            consensus_constants,
            connectivity_manager,
            shutdown_signal,
        };

        let (base_node_update_publisher, _) =
            broadcast::channel(resources.config.base_node_update_publisher_channel_size);

        Ok(OutputManagerService {
            resources,
            request_stream: Some(request_stream),
            base_node_update_publisher,
            base_node_service,
        })
    }

    pub async fn start(mut self) -> Result<(), OutputManagerError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("OutputManagerService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown = self.resources.shutdown_signal.clone();

        info!(target: LOG_TARGET, "Output Manager Service started");
        loop {
            tokio::select! {
                Some(request_context) = request_stream.next() => {
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
    async fn handle_request(
        &mut self,
        request: OutputManagerRequest,
    ) -> Result<OutputManagerResponse, OutputManagerError> {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            OutputManagerRequest::AddOutput(uo) => self
                .add_output(None, *uo)
                .await
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::AddOutputWithTxId((tx_id, uo)) => self
                .add_output(Some(tx_id), *uo)
                .await
                .map(|_| OutputManagerResponse::OutputAdded),
            OutputManagerRequest::UpdateOutputMetadataSignature(uo) => self
                .update_output_metadata_signature(*uo)
                .await
                .map(|_| OutputManagerResponse::OutputMetadataSignatureUpdated),
            OutputManagerRequest::GetBalance => {
                let current_chain_tip = match self.base_node_service.get_chain_metadata().await {
                    Ok(metadata) => metadata.map(|m| m.height_of_longest_chain()),
                    Err(_) => None,
                };
                self.get_balance(current_chain_tip)
                    .await
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
                fee_per_gram,
                lock_height,
                message,
                script,
            } => self
                .prepare_transaction_to_send(tx_id, amount, unique_id, fee_per_gram, lock_height, message, script)
                .await
                .map(OutputManagerResponse::TransactionToSend),
            OutputManagerRequest::CreatePayToSelfTransaction {
                tx_id,
                amount,
                unique_id,
                fee_per_gram,
                lock_height,
                message,
            } => self
                .create_pay_to_self_transaction(tx_id, amount, unique_id, fee_per_gram, lock_height, message)
                .await
                .map(OutputManagerResponse::PayToSelfTransaction),
            OutputManagerRequest::FeeEstimate((amount, fee_per_gram, num_kernels, num_outputs)) => self
                .fee_estimate(amount, fee_per_gram, num_kernels, num_outputs)
                .await
                .map(OutputManagerResponse::FeeEstimate),
            OutputManagerRequest::ConfirmPendingTransaction(tx_id) => self
                .confirm_encumberance(tx_id)
                .await
                .map(|_| OutputManagerResponse::PendingTransactionConfirmed),
            OutputManagerRequest::ConfirmTransaction((tx_id, spent_outputs, received_outputs)) => self
                .confirm_transaction(tx_id, &spent_outputs, &received_outputs)
                .await
                .map(|_| OutputManagerResponse::TransactionConfirmed),
            OutputManagerRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .await
                .map(|_| OutputManagerResponse::TransactionCancelled),
            OutputManagerRequest::TimeoutTransactions(period) => self
                .timeout_pending_transactions(period)
                .await
                .map(|_| OutputManagerResponse::TransactionsTimedOut),
            OutputManagerRequest::GetPendingTransactions => self
                .fetch_pending_transaction_outputs()
                .await
                .map(OutputManagerResponse::PendingTransactions),
            OutputManagerRequest::GetSpentOutputs => {
                let outputs = self
                    .fetch_spent_outputs()
                    .await?
                    .into_iter()
                    .map(|v| v.into())
                    .collect();
                Ok(OutputManagerResponse::SpentOutputs(outputs))
            },
            OutputManagerRequest::GetUnspentOutputs => {
                let outputs = self
                    .fetch_unspent_outputs()
                    .await?
                    .into_iter()
                    .map(|v| v.into())
                    .collect();
                Ok(OutputManagerResponse::UnspentOutputs(outputs))
            },
            OutputManagerRequest::GetSeedWords => self
                .resources
                .master_key_manager
                .get_seed_words(&self.resources.config.seed_word_language)
                .await
                .map(OutputManagerResponse::SeedWords),
            OutputManagerRequest::SetBaseNodePublicKey(pk) => self
                .set_base_node_public_key(pk)
                .await
                .map(|_| OutputManagerResponse::BaseNodePublicKeySet),
            OutputManagerRequest::ValidateUtxos(validation_type, retries) => self
                .validate_outputs(validation_type, retries)
                .map(OutputManagerResponse::UtxoValidationStarted),
            OutputManagerRequest::GetInvalidOutputs => {
                let outputs = self
                    .fetch_invalid_outputs()
                    .await?
                    .into_iter()
                    .map(|v| v.into())
                    .collect();
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
                .await
                .map(|_| OutputManagerResponse::EncryptionApplied)
                .map_err(OutputManagerError::OutputManagerStorageError),
            OutputManagerRequest::RemoveEncryption => self
                .resources
                .db
                .remove_encryption()
                .await
                .map(|_| OutputManagerResponse::EncryptionRemoved)
                .map_err(OutputManagerError::OutputManagerStorageError),

            OutputManagerRequest::GetPublicRewindKeys => Ok(OutputManagerResponse::PublicRewindKeys(Box::new(
                self.resources.master_key_manager.get_rewind_public_keys(),
            ))),
            OutputManagerRequest::ScanForRecoverableOutputs(outputs) => StandardUtxoRecoverer::new(
                self.resources.master_key_manager.clone(),
                self.resources.factories.clone(),
                self.resources.db.clone(),
            )
            .scan_and_recover_outputs(outputs)
            .await
            .map(OutputManagerResponse::RewoundOutputs),
            OutputManagerRequest::ScanOutputs(outputs) => self
                .scan_outputs_for_one_sided_payments(outputs)
                .await
                .map(OutputManagerResponse::ScanOutputs),
            OutputManagerRequest::AddKnownOneSidedPaymentScript(known_script) => self
                .add_known_script(known_script)
                .await
                .map(|_| OutputManagerResponse::AddKnownOneSidedPaymentScript),
            OutputManagerRequest::ReinstateCancelledInboundTx(tx_id) => self
                .reinstate_cancelled_inbound_transaction(tx_id)
                .await
                .map(|_| OutputManagerResponse::ReinstatedCancelledInboundTx),
            OutputManagerRequest::CreateOutputWithFeatures {
                value,
                features,
                unique_id,
                parent_public_key,
            } => {
                let unblinded_output = self
                    .create_output_with_features(value, *features, unique_id, *parent_public_key)
                    .await?;
                Ok(OutputManagerResponse::CreateOutputWithFeatures {
                    output: Box::new(unblinded_output),
                })
            },
            OutputManagerRequest::CreatePayToSelfWithOutputs {
                amount: _,
                outputs,
                fee_per_gram,
            } => {
                let (tx_id, transaction) = self
                    .create_pay_to_self_containing_outputs(outputs, fee_per_gram)
                    .await?;
                Ok(OutputManagerResponse::CreatePayToSelfWithOutputs {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
        }
    }

    fn validate_outputs(
        &mut self,
        validation_type: TxoValidationType,
        retry_strategy: ValidationRetryStrategy,
    ) -> Result<u64, OutputManagerError> {
        match self.resources.base_node_public_key.as_ref() {
            None => Err(OutputManagerError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let id = OsRng.next_u64();

                let utxo_validation_task = TxoValidationTask::new(
                    id,
                    validation_type,
                    retry_strategy,
                    self.resources.clone(),
                    pk.clone(),
                    self.base_node_update_publisher.subscribe(),
                );

                tokio::spawn(async move {
                    match utxo_validation_task.execute().await {
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
                        },
                    }
                });

                Ok(id)
            },
        }
    }

    /// Add an unblinded output to the unspent outputs list
    pub async fn add_output(&mut self, tx_id: Option<TxId>, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value
        );
        let output = DbUnblindedOutput::from_unblinded_output(output, &self.resources.factories)?;
        match tx_id {
            None => self.resources.db.add_unspent_output(output).await?,
            Some(t) => self.resources.db.add_unspent_output_with_tx_id(t, output).await?,
        }
        Ok(())
    }

    /// Update an output's metadata signature, akin to 'finalize output'
    pub async fn update_output_metadata_signature(
        &mut self,
        output: TransactionOutput,
    ) -> Result<(), OutputManagerError> {
        self.resources.db.update_output_metadata_signature(output).await?;
        Ok(())
    }

    async fn create_output_with_features(
        &self,
        value: MicroTari,
        features: OutputFeatures,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
    ) -> Result<UnblindedOutputBuilder, OutputManagerError> {
        let (spending_key, script_private_key) = self
            .resources
            .master_key_manager
            .get_next_spend_and_script_key()
            .await?;
        let input_data = inputs!(PublicKey::from_secret_key(&script_private_key));
        let script = script!(Nop);

        Ok(UnblindedOutputBuilder::new(value, spending_key.clone())
            .with_features(features)
            .with_script(script.clone())
            .with_input_data(input_data)
            .with_script_private_key(script_private_key)
            .with_unique_id(unique_id)
            .with_parent_public_key(parent_public_key))
    }

    async fn get_balance(&self, current_chain_tip: Option<u64>) -> Result<Balance, OutputManagerError> {
        let balance = self.resources.db.get_balance(current_chain_tip).await?;
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

        let (spending_key, script_private_key) = self
            .resources
            .master_key_manager
            .get_next_spend_and_script_key()
            .await?;

        let output = DbUnblindedOutput::from_unblinded_output(
            UnblindedOutput::new(
                single_round_sender_data.amount,
                spending_key.clone(),
                single_round_sender_data.features.clone(),
                single_round_sender_data.script.clone(),
                // TODO: The input data should be variable; this will only work for a Nop script
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                single_round_sender_data.sender_offset_public_key.clone(),
                // Note: The commitment signature at this time is only partially built
                TransactionOutput::create_partial_metadata_signature(
                    &single_round_sender_data.amount,
                    &spending_key.clone(),
                    &single_round_sender_data.script.clone(),
                    &single_round_sender_data.features.clone(),
                    &single_round_sender_data.sender_offset_public_key.clone(),
                    &single_round_sender_data.public_commitment_nonce.clone(),
                )?,
            ),
            &self.resources.factories,
        )?;

        self.resources
            .db
            .accept_incoming_pending_transaction(single_round_sender_data.tx_id, output, None)
            .await?;

        self.confirm_encumberance(single_round_sender_data.tx_id).await?;

        let nonce = PrivateKey::random(&mut OsRng);

        let rtp = ReceiverTransactionProtocol::new_with_rewindable_output(
            sender_message.clone(),
            nonce,
            spending_key,
            single_round_sender_data.features.clone(),
            &self.resources.factories,
            self.resources.master_key_manager.rewind_data(),
        );

        Ok(rtp)
    }

    /// Confirm the reception of an expected transaction output. This will be called by the Transaction Service when it
    /// detects the output on the blockchain
    pub async fn confirm_received_transaction_output(
        &mut self,
        tx_id: TxId,
        received_output: &TransactionOutput,
    ) -> Result<(), OutputManagerError> {
        let pending_transaction = self.resources.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 {
            return Err(OutputManagerError::IncompleteTransaction(
                "unexpected number of outputs to be received, exactly one is expected",
            ));
        }

        if pending_transaction.outputs_to_be_received[0]
            .unblinded_output
            .as_transaction_input(&self.resources.factories.commitment)?
            .commitment !=
            received_output.commitment
        {
            return Err(OutputManagerError::IncompleteTransaction(
                "unexpected commitment received",
            ));
        }

        self.resources
            .db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id)
            .await?;

        debug!(
            target: LOG_TARGET,
            "Confirm received transaction outputs for TxId: {}", tx_id
        );

        Ok(())
    }

    /// Get a fee estimate for an amount of MicroTari, at a specified fee per gram and given number of kernels and
    /// outputs.
    async fn fee_estimate(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        num_kernels: u64,
        num_outputs: u64,
    ) -> Result<MicroTari, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Getting fee estimate. Amount: {}. Fee per gram: {}. Num kernels: {}. Num outputs: {}",
            amount,
            fee_per_gram,
            num_kernels,
            num_outputs
        );
        // TODO: Perhaps include unique id here
        let (utxos, _, _) = self
            .select_utxos(amount, fee_per_gram, num_outputs as usize, None, None)
            .await?;
        debug!(target: LOG_TARGET, "{} utxos selected.", utxos.len());

        let fee = Fee::calculate_with_minimum(fee_per_gram, num_kernels as usize, utxos.len(), num_outputs as usize);

        debug!(target: LOG_TARGET, "Fee calculated: {}", fee);
        Ok(fee)
    }

    /// Prepare a Sender Transaction Protocol for the amount and fee_per_gram specified. If required a change output
    /// will be produced.
    pub async fn prepare_transaction_to_send(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
        recipient_script: TariScript,
    ) -> Result<SenderTransactionProtocol, OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Preparing to send transaction. Amount: {}. Unique id : {:?}. Fee per gram: {}. ",
            amount,
            unique_id,
            fee_per_gram,
        );
        let (outputs, _, total) = self
            .select_utxos(amount, fee_per_gram, 1, None, unique_id.as_ref())
            .await?;

        let output_features = match unique_id {
            Some(ref _unique_id) => match outputs
                .clone()
                .into_iter()
                .find(|output| output.unblinded_output.features.unique_id.is_some())
            {
                Some(output) => output.unblinded_output.features,
                _ => Default::default(),
            },
            _ => Default::default(),
        };

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1);
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
                output_features,
                PrivateKey::random(&mut OsRng),
            )
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_tx_id(tx_id);

        for uo in outputs.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }
        debug!(
            target: LOG_TARGET,
            "Calculating fee for tx with: Fee per gram: {}. Num outputs: {}",
            amount,
            outputs.len()
        );
        let fee_without_change = Fee::calculate(fee_per_gram, 1, outputs.len(), 1);
        // If the input values > the amount to be sent + fee_without_change then we will need to include a change
        // output
        if total > amount + fee_without_change {
            let (spending_key, script_private_key) = self
                .resources
                .master_key_manager
                .get_next_spend_and_script_key()
                .await?;
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.master_key_manager.rewind_data().clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let mut change_output = Vec::<DbUnblindedOutput>::new();
        if total > amount + fee_without_change {
            let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            change_output.push(DbUnblindedOutput::from_unblinded_output(
                unblinded_output,
                &self.resources.factories,
            )?);
        }

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.resources
            .db
            .encumber_outputs(tx_id, outputs, change_output)
            .await?;

        debug!(target: LOG_TARGET, "Prepared transaction (TxId: {}) to send", tx_id);
        debug!(
            target: LOG_TARGET_STRESS,
            "Prepared transaction (TxId: {}) to send", tx_id
        );

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

        let (spending_key, script_key) = self
            .resources
            .master_key_manager
            .get_coinbase_spend_and_script_key_for_height(block_height)
            .await?;

        let nonce = PrivateKey::random(&mut OsRng);
        let (tx, unblinded_output) = CoinbaseBuilder::new(self.resources.factories.clone())
            .with_block_height(block_height)
            .with_fees(fees)
            .with_spend_key(spending_key.clone())
            .with_script_key(script_key.clone())
            .with_script(script!(Nop))
            .with_nonce(nonce)
            .with_rewind_data(self.resources.master_key_manager.rewind_data().clone())
            .build_with_reward(&self.resources.consensus_constants, reward)?;

        let output = DbUnblindedOutput::from_unblinded_output(unblinded_output, &self.resources.factories)?;

        // Clear any existing pending coinbase transactions for this blockheight
        self.resources
            .db
            .cancel_pending_transaction_at_block_height(block_height)
            .await?;

        // Clear any matching outputs for this commitment. Even if the older output is valid
        // we are losing no information as this output has the same commitment.
        match self
            .resources
            .db
            .remove_output_by_commitment(output.commitment.clone())
            .await
        {
            Ok(_) => {},
            Err(OutputManagerStorageError::ValueNotFound) => {},
            Err(e) => return Err(e.into()),
        }

        self.resources
            .db
            .accept_incoming_pending_transaction(tx_id, output, Some(block_height))
            .await?;

        self.confirm_encumberance(tx_id).await?;

        Ok(tx)
    }

    async fn create_pay_to_self_containing_outputs(
        &mut self,
        outputs: Vec<UnblindedOutputBuilder>,
        fee_per_gram: MicroTari,
    ) -> Result<(TxId, Transaction), OutputManagerError> {
        let (inputs, _, total) = self
            .select_utxos(0.into(), fee_per_gram, outputs.len(), None, None)
            .await?;
        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0);
        builder
            .with_lock_height(0)
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_prevent_fee_gt_amount(false);

        for uo in &inputs {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }
        let mut db_outputs = vec![];
        for mut unblinded_output in outputs {
            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

            let public_offset_commitment_private_key = PrivateKey::random(&mut OsRng);
            let public_offset_commitment_pub_key = PublicKey::from_secret_key(&public_offset_commitment_private_key);

            unblinded_output.sign_as_receiver(sender_offset_public_key, public_offset_commitment_pub_key)?;
            unblinded_output.sign_as_sender(&sender_offset_private_key)?;

            let ub = unblinded_output.try_build()?;
            builder
                .with_output(ub.clone(), sender_offset_private_key.clone())
                .map_err(|e| OutputManagerError::BuildError(e.message))?;
            db_outputs.push(DbUnblindedOutput::from_unblinded_output(ub, &self.resources.factories)?)
        }

        let mut change_keys = None;

        let fee = Fee::calculate(fee_per_gram, 1, inputs.len(), 1);
        let change_value = total.saturating_sub(fee);
        if change_value > 0.into() {
            let (spending_key, script_private_key) = self
                .resources
                .master_key_manager
                .get_next_spend_and_script_key()
                .await?;
            change_keys = Some((spending_key.clone(), script_private_key.clone()));
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.master_key_manager.rewind_data().clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let mut stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        if let Some((spending_key, script_private_key)) = change_keys {
            let _change_script_offset_public_key = stp.get_change_sender_offset_public_key()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change script offset public key available".to_string(),
                )
            })?;

            let sender_offset_private_key = PrivateKey::random(&mut OsRng);
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

            let public_offset_commitment_private_key = PrivateKey::random(&mut OsRng);
            let public_offset_commitment_pub_key = PublicKey::from_secret_key(&public_offset_commitment_private_key);

            let mut output_builder = UnblindedOutputBuilder::new(stp.get_change_amount()?, spending_key)
                .with_script(script!(Nop))
                .with_input_data(inputs!(PublicKey::from_secret_key(&script_private_key)))
                .with_script_private_key(script_private_key);

            output_builder.sign_as_receiver(sender_offset_public_key, public_offset_commitment_pub_key)?;
            output_builder.sign_as_sender(&sender_offset_private_key)?;

            let change_output =
                DbUnblindedOutput::from_unblinded_output(output_builder.try_build()?, &self.resources.factories)?;

            db_outputs.push(change_output);
        }
        let tx_id = stp.get_tx_id()?;

        self.resources.db.encumber_outputs(tx_id, inputs, db_outputs).await?;
        stp.finalize(KernelFeatures::empty(), &self.resources.factories)?;

        Ok((tx_id, stp.take_transaction()?))
    }

    async fn create_pay_to_self_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(MicroTari, Transaction), OutputManagerError> {
        let (inputs, _, total) = self
            .select_utxos(amount, fee_per_gram, 1, None, unique_id.as_ref())
            .await?;

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);
        let sender_offset_private_key = PrivateKey::random(&mut OsRng);

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount)
            .with_tx_id(tx_id);

        for uo in &inputs {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }

        let script = script!(Nop);
        let mut output_features = OutputFeatures::default();
        output_features.unique_id = unique_id.clone();
        let (spending_key, script_private_key) = self
            .resources
            .master_key_manager
            .get_next_spend_and_script_key()
            .await?;
        let metadata_signature = TransactionOutput::create_final_metadata_signature(
            &amount,
            &spending_key.clone(),
            &script,
            &output_features,
            &&sender_offset_private_key,
        )?;
        let utxo = DbUnblindedOutput::from_unblinded_output(
            UnblindedOutput::new(
                amount,
                spending_key.clone(),
                output_features,
                script,
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
                PublicKey::from_secret_key(&sender_offset_private_key),
                metadata_signature,
            ),
            &self.resources.factories,
        )?;
        builder
            .with_output(utxo.unblinded_output.clone(), sender_offset_private_key.clone())
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        let mut outputs = vec![utxo];

        let fee = Fee::calculate(fee_per_gram, 1, inputs.len(), 1);
        let change_value = total.saturating_sub(amount).saturating_sub(fee);
        if change_value > 0.into() {
            let (spending_key, script_private_key) = self
                .resources
                .master_key_manager
                .get_next_spend_and_script_key()
                .await?;
            builder.with_change_secret(spending_key);
            builder.with_rewindable_outputs(self.resources.master_key_manager.rewind_data().clone());
            builder.with_change_script(
                script!(Nop),
                inputs!(PublicKey::from_secret_key(&script_private_key)),
                script_private_key,
            );
        }

        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        if change_value > 0.into() {
            let unblinded_output = stp.get_change_unblinded_output()?.ok_or_else(|| {
                OutputManagerError::BuildError(
                    "There should be a change output metadata signature available".to_string(),
                )
            })?;
            let change_output = DbUnblindedOutput::from_unblinded_output(unblinded_output, &self.resources.factories)?;

            outputs.push(change_output);
        }

        trace!(
            target: LOG_TARGET,
            "Encumber send to self transaction ({}) outputs.",
            tx_id
        );
        self.resources.db.encumber_outputs(tx_id, inputs, outputs).await?;
        self.confirm_encumberance(tx_id).await?;
        let fee = stp.get_fee_amount()?;
        trace!(target: LOG_TARGET, "Finalize send-to-self transaction ({}).", tx_id);
        stp.finalize(KernelFeatures::empty(), &factories)?;
        let tx = stp.take_transaction()?;

        Ok((fee, tx))
    }

    /// Confirm that a transaction has finished being negotiated between parties so the short-term encumberance can be
    /// made official
    async fn confirm_encumberance(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        self.resources.db.confirm_encumbered_outputs(tx_id).await?;

        Ok(())
    }

    /// Confirm that a received or sent transaction and its outputs have been detected on the base chain. The inputs and
    /// outputs are checked to see that they match what the stored PendingTransaction contains. This will
    /// be called by the Transaction Service which monitors the base chain.
    async fn confirm_transaction(
        &mut self,
        tx_id: TxId,
        inputs: &[TransactionInput],
        outputs: &[TransactionOutput],
    ) -> Result<(), OutputManagerError> {
        let pending_transaction = self.resources.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Check that outputs to be spent can all be found in the provided transaction inputs
        for output_to_spend in pending_transaction.outputs_to_be_spent.iter() {
            let input_to_check = output_to_spend
                .unblinded_output
                .as_transaction_input(&self.resources.factories.commitment)?;

            if inputs.iter().all(|input| input.commitment != input_to_check.commitment) {
                return Err(OutputManagerError::IncompleteTransaction(
                    "outputs to spend are missing",
                ));
            }
        }

        // Check that outputs to be received can all be found in the provided transaction outputs
        for output_to_receive in pending_transaction.outputs_to_be_received.iter() {
            let output_to_check = output_to_receive
                .unblinded_output
                .as_transaction_input(&self.resources.factories.commitment)?;

            if outputs
                .iter()
                .all(|output| output.commitment != output_to_check.commitment)
            {
                return Err(OutputManagerError::IncompleteTransaction(
                    "outputs to receive are missing",
                ));
            }
        }

        self.resources
            .db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id)
            .await?;

        trace!(target: LOG_TARGET, "Confirm transaction (TxId: {})", tx_id);

        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Cancelling pending transaction outputs for TxId: {}", tx_id
        );
        Ok(self.resources.db.cancel_pending_transaction_outputs(tx_id).await?)
    }

    /// Restore the pending transaction encumberance and output for an inbound transaction that was previously
    /// cancelled.
    async fn reinstate_cancelled_inbound_transaction(&mut self, tx_id: TxId) -> Result<(), OutputManagerError> {
        self.resources.db.reinstate_inbound_output(tx_id).await?;

        self.resources
            .db
            .add_pending_transaction_outputs(PendingTransactionOutputs {
                tx_id,
                outputs_to_be_spent: Vec::new(),
                outputs_to_be_received: Vec::new(),
                timestamp: Utc::now().naive_utc(),
                coinbase_block_height: None,
            })
            .await?;

        self.confirm_encumberance(tx_id).await?;

        Ok(())
    }

    /// Go through the pending transaction and if any have existed longer than the specified duration, cancel them
    async fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        Ok(self.resources.db.timeout_pending_transaction_outputs(period).await?)
    }

    /// Select which unspent transaction outputs to use to send a transaction of the specified amount. Use the specified
    /// selection strategy to choose the outputs. It also determines if a change output is required.
    async fn select_utxos(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        output_count: usize,
        strategy: Option<UTXOSelectionStrategy>,
        unique_id: Option<&Vec<u8>>,
    ) -> Result<(Vec<DbUnblindedOutput>, bool, MicroTari), OutputManagerError> {
        let token = match unique_id {
            Some(unique_id) => {
                warn!(target: LOG_TARGET, "Looking for {:?}", unique_id);
                let uo = self.resources.db.fetch_all_unspent_outputs().await?;
                // for x in uo {
                //     warn!(target: LOG_TARGET, "{:?}", x.unblinded_output.unique_id);
                // }
                if let Some(token_id) = uo.into_iter().find(|x| match &x.unblinded_output.features.unique_id {
                    Some(token_unique_id) => {
                        warn!(target: LOG_TARGET, "Comparing with {:?}", token_unique_id);
                        token_unique_id == unique_id
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
            "select_utxos amount: {}, token : {:?}, fee_per_gram: {}, output_count: {}, strategy: {:?}",
            amount,
            token,
            fee_per_gram,
            output_count,
            strategy
        );
        let mut utxos = Vec::new();
        let mut utxos_total_value = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);

        if let Some(unblinded_token) = token {
            utxos.push(unblinded_token);
        }

        let uo = self.resources.db.fetch_spendable_outputs()?;

        // Attempt to get the chain tip height
        let chain_metadata = self.base_node_service.get_chain_metadata().await?;
        let (connected, tip_height) = match &chain_metadata {
            Some(metadata) => (true, metadata.height_of_longest_chain()),
            None => (false, 0),
        };

        // If no strategy was specified and no metadata is available, then make sure to use MaturitythenSmallest
        let strategy = match (strategy, connected) {
            (Some(s), _) => Some(s),
            (None, false) => Some(UTXOSelectionStrategy::MaturityThenSmallest),
            (None, true) => None, // use the selection heuristic next
        };

        // If we know the chain height then filter out unspendable UTXOs
        let num_utxos = uo.len();
        let uo = if connected {
            let mature_utxos = uo
                .into_iter()
                .filter(|u| u.unblinded_output.features.maturity <= tip_height)
                .collect::<Vec<DbUnblindedOutput>>();

            trace!(
                target: LOG_TARGET,
                "Some UTXOs have not matured yet at height {}, filtered {} UTXOs",
                tip_height,
                num_utxos - mature_utxos.len()
            );

            mature_utxos
        } else {
            uo
        };

        // Heuristic for selection strategy: Default to MaturityThenSmallest, but if the amount is greater than
        // the largest UTXO, use Largest UTXOs first.
        let strategy = match (strategy, uo.is_empty()) {
            (Some(s), _) => s,
            (None, true) => UTXOSelectionStrategy::Smallest,
            (None, false) => {
                let largest_utxo = &uo[uo.len() - 1];
                if amount > largest_utxo.unblinded_output.value {
                    UTXOSelectionStrategy::Largest
                } else {
                    UTXOSelectionStrategy::MaturityThenSmallest
                }
            },
        };
        warn!(target: LOG_TARGET, "select_utxos selection strategy: {}", strategy);

        let uo = match strategy {
            UTXOSelectionStrategy::Smallest => uo,
            UTXOSelectionStrategy::MaturityThenSmallest => {
                let mut uo = uo;
                uo.sort_by(|a, b| {
                    match a
                        .unblinded_output
                        .features
                        .maturity
                        .cmp(&b.unblinded_output.features.maturity)
                    {
                        Ordering::Equal => a.unblinded_output.value.cmp(&b.unblinded_output.value),
                        Ordering::Less => Ordering::Less,
                        Ordering::Greater => Ordering::Greater,
                    }
                });
                uo
            },
            UTXOSelectionStrategy::Largest => uo.into_iter().rev().collect(),
        };
        trace!(target: LOG_TARGET, "We found {} UTXOs to select from", uo.len());

        let mut require_change_output = false;
        for o in uo.iter() {
            utxos.push(o.clone());
            utxos_total_value += o.unblinded_output.value;
            // The assumption here is that the only output will be the payment output and change if required
            fee_without_change = Fee::calculate(fee_per_gram, 1, utxos.len(), output_count);
            if utxos_total_value == amount + fee_without_change {
                break;
            }
            fee_with_change = Fee::calculate(fee_per_gram, 1, utxos.len(), output_count + 1);
            if utxos_total_value > amount + fee_with_change {
                require_change_output = true;
                break;
            }
        }

        let perfect_utxo_selection = utxos_total_value == amount + fee_without_change;
        let enough_spendable = utxos_total_value > amount + fee_with_change;

        if !perfect_utxo_selection && !enough_spendable {
            let current_chain_tip = chain_metadata.map(|cm| cm.height_of_longest_chain());
            let balance = self.get_balance(current_chain_tip).await?;
            let pending_incoming = balance.pending_incoming_balance;
            if utxos_total_value + pending_incoming >= amount + fee_with_change {
                return Err(OutputManagerError::FundsPending);
            } else {
                return Err(OutputManagerError::NotEnoughFunds);
            }
        }

        Ok((utxos, require_change_output, utxos_total_value))
    }

    /// Set the base node public key to the list that will be used to check the status of UTXO's on the base chain. If
    /// this is the first time the base node public key is set do the UTXO queries.
    async fn set_base_node_public_key(
        &mut self,
        base_node_public_key: CommsPublicKey,
    ) -> Result<(), OutputManagerError> {
        info!(
            target: LOG_TARGET,
            "Setting base node public key {} for service", base_node_public_key
        );

        self.resources.base_node_public_key = Some(base_node_public_key.clone());
        if let Err(e) = self.base_node_update_publisher.send(base_node_public_key) {
            trace!(
                target: LOG_TARGET,
                "No subscribers to receive base node public key update: {:?}",
                e
            );
        }

        Ok(())
    }

    pub async fn fetch_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<TxId, PendingTransactionOutputs>, OutputManagerError> {
        Ok(self.resources.db.fetch_all_pending_transaction_outputs().await?)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_spent_outputs().await?)
    }

    pub async fn fetch_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_all_unspent_outputs().await?)
    }

    pub async fn fetch_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.get_invalid_outputs().await?)
    }

    async fn create_coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
    ) -> Result<(TxId, Transaction, MicroTari, MicroTari), OutputManagerError> {
        trace!(
            target: LOG_TARGET,
            "Select UTXOs and estimate coin split transaction fee."
        );
        let mut output_count = split_count;
        let total_split_amount = amount_per_split * split_count as u64;
        let (inputs, require_change_output, utxos_total_value) = self
            .select_utxos(
                total_split_amount,
                fee_per_gram,
                output_count,
                Some(UTXOSelectionStrategy::Largest),
                None,
            )
            .await?;
        let input_count = inputs.len();
        if require_change_output {
            output_count = split_count + 1
        };
        let fee = Fee::calculate(fee_per_gram, 1, input_count, output_count);

        trace!(target: LOG_TARGET, "Construct coin split transaction.");
        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(0);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_rewindable_outputs(self.resources.master_key_manager.rewind_data().clone());

        trace!(target: LOG_TARGET, "Add inputs to coin split transaction.");
        for uo in inputs.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.resources.factories.commitment)?,
                uo.unblinded_output.clone(),
            );
        }
        trace!(target: LOG_TARGET, "Add outputs to coin split transaction.");
        let mut outputs: Vec<DbUnblindedOutput> = Vec::with_capacity(output_count);
        let change_output = utxos_total_value
            .checked_sub(fee)
            .ok_or(OutputManagerError::NotEnoughFunds)?
            .checked_sub(total_split_amount)
            .ok_or(OutputManagerError::NotEnoughFunds)?;
        for i in 0..output_count {
            let output_amount = if i < split_count {
                amount_per_split
            } else {
                change_output
            };

            let (spending_key, script_private_key) = self
                .resources
                .master_key_manager
                .get_next_spend_and_script_key()
                .await?;
            let sender_offset_private_key = PrivateKey::random(&mut OsRng);

            let script = script!(Nop);
            let output_features = OutputFeatures::default();
            let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);
            let metadata_signature = TransactionOutput::create_final_metadata_signature(
                &output_amount,
                &spending_key.clone(),
                &script,
                &output_features,
                &sender_offset_private_key,
            )?;
            let utxo = DbUnblindedOutput::from_unblinded_output(
                UnblindedOutput::new(
                    output_amount,
                    spending_key.clone(),
                    output_features,
                    script,
                    inputs!(PublicKey::from_secret_key(&script_private_key)),
                    script_private_key,
                    sender_offset_public_key,
                    metadata_signature,
                ),
                &self.resources.factories,
            )?;
            outputs.push(utxo.clone());
            builder
                .with_output(utxo.unblinded_output, sender_offset_private_key)
                .map_err(|e| OutputManagerError::BuildError(e.message))?;
        }
        trace!(target: LOG_TARGET, "Build coin split transaction.");
        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = stp.get_tx_id()?;
        trace!(
            target: LOG_TARGET,
            "Encumber coin split transaction ({}) outputs.",
            tx_id
        );
        self.resources.db.encumber_outputs(tx_id, inputs, outputs).await?;
        self.confirm_encumberance(tx_id).await?;
        trace!(target: LOG_TARGET, "Finalize coin split transaction ({}).", tx_id);
        stp.finalize(KernelFeatures::empty(), &factories)?;
        let tx = stp.take_transaction()?;
        Ok((tx_id, tx, fee, utxos_total_value))
    }

    /// Persist a one-sided payment script for a Comms Public/Private key. These are the scripts that this wallet knows
    /// to look for when scanning for one-sided payments
    async fn add_known_script(&mut self, known_script: KnownOneSidedPaymentScript) -> Result<(), OutputManagerError> {
        debug!(target: LOG_TARGET, "Adding new script to output manager service");
        // It is not a problem if the script has already been persisted
        match self.resources.db.add_known_script(known_script).await {
            Ok(_) => (),
            Err(OutputManagerStorageError::DieselError(DieselError::DatabaseError(
                DatabaseErrorKind::UniqueViolation,
                _,
            ))) => (),
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    /// Attempt to scan and then rewind all of the given transaction outputs into unblinded outputs based on known
    /// pubkeys
    async fn scan_outputs_for_one_sided_payments(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        let known_one_sided_payment_scripts: Vec<KnownOneSidedPaymentScript> =
            self.resources.db.get_all_known_one_sided_payment_scripts().await?;

        let mut rewound_outputs: Vec<UnblindedOutput> = Vec::new();
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
                let rewind_key = PrivateKey::from_bytes(&hash_secret_key(&spending_key))?;
                let blinding_key = PrivateKey::from_bytes(&hash_secret_key(&rewind_key))?;
                let rewound =
                    output.full_rewind_range_proof(&self.resources.factories.range_proof, &rewind_key, &blinding_key);

                if let Ok(rewound_result) = rewound {
                    let rewound_output = UnblindedOutput::new(
                        rewound_result.committed_value,
                        rewound_result.blinding_factor.clone(),
                        output.features,
                        known_one_sided_payment_scripts[i].script.clone(),
                        known_one_sided_payment_scripts[i].input.clone(),
                        known_one_sided_payment_scripts[i].private_key.clone(),
                        output.sender_offset_public_key,
                        output.metadata_signature,
                    );
                    let db_output =
                        DbUnblindedOutput::from_unblinded_output(rewound_output.clone(), &self.resources.factories)?;

                    let output_hex = output.commitment.to_hex();
                    match self.resources.db.add_unspent_output(db_output).await {
                        Ok(_) => {
                            rewound_outputs.push(rewound_output);
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
}

/// Different UTXO selection strategies for choosing which UTXO's are used to fulfill a transaction
/// TODO Investigate and implement more optimal strategies
#[derive(Debug)]
pub enum UTXOSelectionStrategy {
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit
    // is removing small UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
    // Start from oldest maturity to reduce the likelihood of grabbing locked up UTXOs
    MaturityThenSmallest,
    // A strategy that selects the largest UTXOs first. Preferred when the amount is large
    Largest,
}

impl Display for UTXOSelectionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UTXOSelectionStrategy::Smallest => write!(f, "Smallest"),
            UTXOSelectionStrategy::MaturityThenSmallest => write!(f, "MaturityThenSmallest"),
            UTXOSelectionStrategy::Largest => write!(f, "Largest"),
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

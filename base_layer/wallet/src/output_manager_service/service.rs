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

use crate::{
    base_node_service::handle::BaseNodeServiceHandle,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerProtocolError},
        handle::{OutputManagerEventSender, OutputManagerRequest, OutputManagerResponse, PublicRewindKeys},
        protocols::txo_validation_protocol::{TxoValidationProtocol, TxoValidationType},
        storage::{
            database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
            models::DbUnblindedOutput,
        },
        TxId,
    },
    transaction_service::handle::TransactionServiceHandle,
    types::{HashDigest, KeyDigest, ValidationRetryStrategy},
};
use futures::{pin_mut, stream::FuturesUnordered, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::{self, Display},
    time::Duration,
};
use tari_comms::{connectivity::ConnectivityRequester, types::CommsPublicKey};
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
        transaction_protocol::{sender::TransactionSenderMessage, RewindData},
        types::{CryptoFactories, PrivateKey, PublicKey},
        CoinbaseBuilder,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
    range_proof::REWIND_USER_MESSAGE_LENGTH,
};
use tari_key_manager::{
    key_manager::KeyManager,
    mnemonic::{from_secret_key, MnemonicLanguage},
};
use tari_service_framework::reply_channel;
use tari_shutdown::ShutdownSignal;
use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
};

const LOG_TARGET: &str = "wallet::output_manager_service";
const LOG_TARGET_STRESS: &str = "stress_test::output_manager_service";

const KEY_MANAGER_COINBASE_BRANCH_KEY: &str = "coinbase";
const KEY_MANAGER_RECOVERY_VIEWONLY_BRANCH_KEY: &str = "recovery_viewonly";
const KEY_MANAGER_RECOVERY_BLINDING_BRANCH_KEY: &str = "recovery_blinding";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    resources: OutputManagerResources<TBackend>,
    key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    coinbase_key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
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
    ) -> Result<OutputManagerService<TBackend>, OutputManagerError>
    {
        // Check to see if there is any persisted state, otherwise start fresh
        let key_manager_state = match db.get_key_manager_state().await? {
            None => {
                let starting_state = KeyManagerState {
                    master_key: PrivateKey::random(&mut OsRng),
                    branch_seed: "".to_string(),
                    primary_key_index: 0,
                };
                db.set_key_manager_state(starting_state.clone()).await?;
                starting_state
            },
            Some(km) => km,
        };

        let coinbase_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.master_key.clone(),
            KEY_MANAGER_COINBASE_BRANCH_KEY.to_string(),
            0,
        );

        let key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.master_key.clone(),
            key_manager_state.branch_seed,
            key_manager_state.primary_key_index,
        );

        let rewind_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.master_key.clone(),
            KEY_MANAGER_RECOVERY_VIEWONLY_BRANCH_KEY.to_string(),
            0,
        );
        let rewind_key = rewind_key_manager.derive_key(0)?.k;

        let rewind_blinding_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.master_key,
            KEY_MANAGER_RECOVERY_BLINDING_BRANCH_KEY.to_string(),
            0,
        );
        let rewind_blinding_key = rewind_blinding_key_manager.derive_key(0)?.k;

        let rewind_data = RewindData {
            rewind_key,
            rewind_blinding_key,
            proof_message: [0u8; REWIND_USER_MESSAGE_LENGTH],
        };

        // Clear any encumberances for transactions that were being negotiated but did not complete to become official
        // Pending Transactions.
        db.clear_short_term_encumberances().await?;

        let resources = OutputManagerResources {
            config,
            db,
            transaction_service,
            factories,
            base_node_public_key: None,
            event_publisher,
            rewind_data,
            consensus_constants,
            connectivity_manager,
            shutdown_signal,
        };

        let (base_node_update_publisher, _) = broadcast::channel(50);

        Ok(OutputManagerService {
            resources,
            key_manager: Mutex::new(key_manager),
            coinbase_key_manager: Mutex::new(coinbase_key_manager),
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

        let mut txo_validation_handles: FuturesUnordered<JoinHandle<Result<u64, OutputManagerProtocolError>>> =
            FuturesUnordered::new();

        info!(target: LOG_TARGET, "Output Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request, &mut txo_validation_handles).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _ = reply_tx.send(response).map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
                },
                join_result = txo_validation_handles.select_next_some() => {
                   trace!(target: LOG_TARGET, "TXO Validation protocol has ended with result {:?}", join_result);
                   match join_result {
                        Ok(join_result_inner) => self.complete_utxo_validation_protocol(join_result_inner).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving TXO Validation protocol: {:?}", e),
                    };
                }
                _ = shutdown => {
                    info!(target: LOG_TARGET, "Output manager service shutting down because it received the shutdown signal");
                    break;
                }
                complete => {
                    info!(target: LOG_TARGET, "Output manager service shutting down");
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
        txo_validation_handles: &mut FuturesUnordered<JoinHandle<Result<u64, OutputManagerProtocolError>>>,
    ) -> Result<OutputManagerResponse, OutputManagerError>
    {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            OutputManagerRequest::AddOutput(uo) => {
                self.add_output(uo).await.map(|_| OutputManagerResponse::OutputAdded)
            },
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
            OutputManagerRequest::PrepareToSendTransaction((amount, fee_per_gram, lock_height, message)) => self
                .prepare_transaction_to_send(amount, fee_per_gram, lock_height, message)
                .await
                .map(OutputManagerResponse::TransactionToSend),
            OutputManagerRequest::CreatePayToSelfTransaction((amount, fee_per_gram, lock_height, message)) => self
                .create_pay_to_self_transaction(amount, fee_per_gram, lock_height, message)
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
            OutputManagerRequest::GetSeedWords => self.get_seed_words().await.map(OutputManagerResponse::SeedWords),
            OutputManagerRequest::SetBaseNodePublicKey(pk) => self
                .set_base_node_public_key(pk)
                .await
                .map(|_| OutputManagerResponse::BaseNodePublicKeySet),
            OutputManagerRequest::ValidateUtxos(validation_type, retries) => self
                .validate_outputs(validation_type, retries, txo_validation_handles)
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
                self.get_rewind_public_keys(),
            ))),
            OutputManagerRequest::RewindOutputs(outputs) => self
                .rewind_outputs(outputs)
                .await
                .map(OutputManagerResponse::RewindOutputs),
        }
    }

    fn validate_outputs(
        &mut self,
        validation_type: TxoValidationType,
        retry_strategy: ValidationRetryStrategy,
        txo_validation_handles: &mut FuturesUnordered<JoinHandle<Result<u64, OutputManagerProtocolError>>>,
    ) -> Result<u64, OutputManagerError>
    {
        match self.resources.base_node_public_key.as_ref() {
            None => Err(OutputManagerError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let id = OsRng.next_u64();

                let utxo_validation_protocol = TxoValidationProtocol::new(
                    id,
                    validation_type,
                    retry_strategy,
                    self.resources.clone(),
                    pk.clone(),
                    self.base_node_update_publisher.subscribe(),
                );

                let join_handle = tokio::spawn(utxo_validation_protocol.execute());
                txo_validation_handles.push(join_handle);

                Ok(id)
            },
        }
    }

    async fn complete_utxo_validation_protocol(&mut self, join_result: Result<u64, OutputManagerProtocolError>) {
        match join_result {
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
    }

    /// Add an unblinded output to the unspent outputs list
    pub async fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Add output of value {} to Output Manager", output.value
        );
        let output = DbUnblindedOutput::from_unblinded_output(output, &self.resources.factories)?;
        Ok(self.resources.db.add_unspent_output(output).await?)
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
    ) -> Result<ReceiverTransactionProtocol, OutputManagerError>
    {
        let (tx_id, amount) = match sender_message.single() {
            Some(data) => (data.tx_id, data.amount),
            _ => return Err(OutputManagerError::InvalidSenderMessage),
        };

        let key = self.get_next_spend_key().await?;
        self.resources
            .db
            .accept_incoming_pending_transaction(
                tx_id,
                amount,
                key.clone(),
                OutputFeatures::default(),
                &self.resources.factories,
                None,
            )
            .await?;

        self.confirm_encumberance(tx_id).await?;

        let nonce = PrivateKey::random(&mut OsRng);

        let rtp = ReceiverTransactionProtocol::new_with_rewindable_output(
            sender_message,
            nonce,
            key,
            OutputFeatures::default(),
            &self.resources.factories,
            &self.resources.rewind_data,
        );

        Ok(rtp)
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
    ) -> Result<Transaction, OutputManagerError>
    {
        self.resources
            .db
            .cancel_pending_transaction_at_block_height(block_height)
            .await?;

        let key = self.get_next_coinbase_key().await?;

        let nonce = PrivateKey::random(&mut OsRng);
        let (tx, _) = CoinbaseBuilder::new(self.resources.factories.clone())
            .with_block_height(block_height)
            .with_fees(fees)
            .with_spend_key(key.clone())
            .with_nonce(nonce)
            .with_rewind_data(self.resources.rewind_data.clone())
            .build_with_reward(&self.resources.consensus_constants, reward)?;

        self.resources
            .db
            .accept_incoming_pending_transaction(
                tx_id,
                reward + fees,
                key,
                OutputFeatures::create_coinbase(
                    block_height + self.resources.consensus_constants.coinbase_lock_height(),
                ),
                &self.resources.factories,
                Some(block_height),
            )
            .await?;

        self.confirm_encumberance(tx_id).await?;
        Ok(tx)
    }

    /// Confirm the reception of an expected transaction output. This will be called by the Transaction Service when it
    /// detects the output on the blockchain
    pub async fn confirm_received_transaction_output(
        &mut self,
        tx_id: u64,
        received_output: &TransactionOutput,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.resources.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 {
            return Err(OutputManagerError::IncompleteTransaction(
                "unexpected number of outputs to be received, exactly one is expected",
            ));
        }

        if pending_transaction.outputs_to_be_received[0]
            .unblinded_output
            .as_transaction_input(&self.resources.factories.commitment, OutputFeatures::default())
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
    ) -> Result<MicroTari, OutputManagerError>
    {
        debug!(
            target: LOG_TARGET,
            "Getting fee estimate. Amount: {}. Fee per gram: {}. Num kernels: {}. Num outputs: {}",
            amount,
            fee_per_gram,
            num_kernels,
            num_outputs
        );

        let (utxos, _) = self
            .select_utxos(amount, fee_per_gram, num_outputs as usize, None)
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
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<SenderTransactionProtocol, OutputManagerError>
    {
        debug!(
            target: LOG_TARGET,
            "Preparing to send transaction. Amount: {}. Fee per gram: {}. ", amount, fee_per_gram,
        );
        let (outputs, _) = self.select_utxos(amount, fee_per_gram, 1, None).await?;
        let total = outputs
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount)
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount);

        for uo in outputs.iter() {
            builder.with_input(
                uo.unblinded_output.as_transaction_input(
                    &self.resources.factories.commitment,
                    uo.unblinded_output.features.clone(),
                ),
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
        let mut change_key: Option<PrivateKey> = None;
        // If the input values > the amount to be sent + fee_without_change then we will need to include a change
        // output
        if total > amount + fee_without_change {
            let key = self.get_next_spend_key().await?;
            change_key = Some(key.clone());
            builder.with_rewindable_change_secret(key, self.resources.rewind_data.clone());
        }

        let stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let mut change_output = Vec::<DbUnblindedOutput>::new();
        if let Some(key) = change_key {
            change_output.push(DbUnblindedOutput::from_unblinded_output(
                UnblindedOutput::new(stp.get_amount_to_self()?, key, None),
                &self.resources.factories,
            )?);
        }

        let tx_id = stp.get_tx_id()?;
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

    async fn create_pay_to_self_transaction(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        lock_height: Option<u64>,
        message: String,
    ) -> Result<(TxId, MicroTari, Transaction), OutputManagerError>
    {
        let (inputs, _) = self.select_utxos(amount, fee_per_gram, 1, None).await?;
        let total = inputs.iter().map(|x| x.unblinded_output.value).sum::<MicroTari>();

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        // Create builder with no recipients (other than ourselves)
        let mut builder = SenderTransactionProtocol::builder(0);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount)
            .with_message(message)
            .with_prevent_fee_gt_amount(self.resources.config.prevent_fee_gt_amount);

        for uo in &inputs {
            builder.with_input(
                uo.unblinded_output.as_transaction_input(
                    &self.resources.factories.commitment,
                    uo.unblinded_output.features.clone(),
                ),
                uo.unblinded_output.clone(),
            );
        }

        let spend_key = self.get_next_spend_key().await?;
        let utxo = DbUnblindedOutput::rewindable_from_unblinded_output(
            UnblindedOutput::new(amount, spend_key, None),
            &self.resources.factories,
            &self.resources.rewind_data,
        )?;
        builder.with_output(utxo.unblinded_output.clone());

        let mut outputs = vec![utxo];
        let mut change_key = None;

        let fee = Fee::calculate(fee_per_gram, 1, inputs.len(), 1);
        let change_value = total.saturating_sub(amount).saturating_sub(fee);
        if change_value > 0.into() {
            let key = self.get_next_spend_key().await?;
            change_key = Some(key.clone());
            builder.with_rewindable_change_secret(key, self.resources.rewind_data.clone());
        }

        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(&self.resources.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        if let Some(key) = change_key {
            let change_amount = stp.get_change_amount()?;
            let change_output = DbUnblindedOutput::rewindable_from_unblinded_output(
                UnblindedOutput::new(change_amount, key, None),
                &self.resources.factories,
                &self.resources.rewind_data,
            )?;

            outputs.push(change_output);
        }

        let tx_id = stp.get_tx_id()?;
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

        Ok((tx_id, fee, tx))
    }

    /// Confirm that a transaction has finished being negotiated between parties so the short-term encumberance can be
    /// made official
    async fn confirm_encumberance(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        self.resources.db.confirm_encumbered_outputs(tx_id).await?;

        Ok(())
    }

    /// Confirm that a received or sent transaction and its outputs have been detected on the base chain. The inputs and
    /// outputs are checked to see that they match what the stored PendingTransaction contains. This will
    /// be called by the Transaction Service which monitors the base chain.
    async fn confirm_transaction(
        &mut self,
        tx_id: u64,
        inputs: &[TransactionInput],
        outputs: &[TransactionOutput],
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.resources.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Check that outputs to be spent can all be found in the provided transaction inputs
        for output_to_spend in pending_transaction.outputs_to_be_spent.iter() {
            let input_to_check = output_to_spend
                .unblinded_output
                .as_transaction_input(&self.resources.factories.commitment, OutputFeatures::default());

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
                .as_transaction_input(&self.resources.factories.commitment, OutputFeatures::default());

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
    pub async fn cancel_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        debug!(
            target: LOG_TARGET,
            "Cancelling pending transaction outputs for TxId: {}", tx_id
        );
        Ok(self.resources.db.cancel_pending_transaction_outputs(tx_id).await?)
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
    ) -> Result<(Vec<DbUnblindedOutput>, bool), OutputManagerError>
    {
        debug!(
            target: LOG_TARGET,
            "select_utxos amount: {}, fee_per_gram: {}, output_count: {}, strategy: {:?}",
            amount,
            fee_per_gram,
            output_count,
            strategy
        );
        let mut utxos = Vec::new();
        let mut total = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);

        let uo = self.resources.db.fetch_sorted_unspent_outputs().await?;

        // Attempt to get the chain tip height
        let chain_metadata = self.base_node_service.get_chain_metadata().await?;
        let (connected, tip_height) = match chain_metadata {
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
        debug!(target: LOG_TARGET, "select_utxos selection strategy: {}", strategy);

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
            total += o.unblinded_output.value;
            // The assumption here is that the only output will be the payment output and change if required
            fee_without_change = Fee::calculate(fee_per_gram, 1, utxos.len(), output_count);
            if total == amount + fee_without_change {
                break;
            }
            fee_with_change = Fee::calculate(fee_per_gram, 1, utxos.len(), output_count + 1);
            if total >= amount + fee_with_change {
                require_change_output = true;
                break;
            }
        }

        if (total != amount + fee_without_change) && (total < amount + fee_with_change) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        Ok((utxos, require_change_output))
    }

    /// Set the base node public key to the list that will be used to check the status of UTXO's on the base chain. If
    /// this is the first time the base node public key is set do the UTXO queries.
    async fn set_base_node_public_key(
        &mut self,
        base_node_public_key: CommsPublicKey,
    ) -> Result<(), OutputManagerError>
    {
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
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerError> {
        Ok(self.resources.db.fetch_all_pending_transaction_outputs().await?)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_spent_outputs().await?)
    }

    pub async fn fetch_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.resources.db.fetch_sorted_unspent_outputs().await?)
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
    ) -> Result<(u64, Transaction, MicroTari, MicroTari), OutputManagerError>
    {
        trace!(
            target: LOG_TARGET,
            "Select UTXOs and estimate coin split transaction fee."
        );
        let mut output_count = split_count;
        let total_split_amount = amount_per_split * split_count as u64;
        let (inputs, require_change_output) = self
            .select_utxos(
                total_split_amount,
                fee_per_gram,
                output_count,
                Some(UTXOSelectionStrategy::Largest),
            )
            .await?;
        let utxo_total_value = inputs
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
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
            .with_private_nonce(nonce.clone());
        trace!(target: LOG_TARGET, "Add inputs to coin split transaction.");
        for uo in inputs.iter() {
            builder.with_input(
                uo.unblinded_output.as_transaction_input(
                    &self.resources.factories.commitment,
                    uo.unblinded_output.clone().features,
                ),
                uo.unblinded_output.clone(),
            );
        }
        trace!(target: LOG_TARGET, "Add outputs to coin split transaction.");
        let mut outputs: Vec<DbUnblindedOutput> = Vec::with_capacity(output_count);
        let change_output = utxo_total_value
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

            let spend_key = self.get_next_spend_key().await?;
            let utxo = DbUnblindedOutput::from_unblinded_output(
                UnblindedOutput::new(output_amount, spend_key, None),
                &self.resources.factories,
            )?;
            outputs.push(utxo.clone());
            builder.with_output(utxo.unblinded_output);
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
        Ok((tx_id, tx, fee, utxo_total_value))
    }

    /// Return the Seed words for the current Master Key set in the Key Manager
    pub async fn get_seed_words(&self) -> Result<Vec<String>, OutputManagerError> {
        Ok(from_secret_key(
            self.key_manager.lock().await.master_key(),
            &MnemonicLanguage::English,
        )?)
    }

    /// Return the public rewind keys
    fn get_rewind_public_keys(&self) -> PublicRewindKeys {
        PublicRewindKeys {
            rewind_public_key: PublicKey::from_secret_key(&self.resources.rewind_data.rewind_key),
            rewind_blinding_public_key: PublicKey::from_secret_key(&self.resources.rewind_data.rewind_blinding_key),
        }
    }

    /// Attempt to rewind all of the given transaction outputs into unblinded outputs
    async fn rewind_outputs(
        &mut self,
        outputs: Vec<TransactionOutput>,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError>
    {
        let rewind_data = &self.resources.rewind_data;

        let rewound_outputs: Vec<UnblindedOutput> = outputs
            .into_iter()
            .filter_map(|output| {
                output
                    .full_rewind_range_proof(
                        &self.resources.factories.range_proof,
                        &rewind_data.rewind_key,
                        &rewind_data.rewind_blinding_key,
                    )
                    .ok()
            })
            .map(|output| UnblindedOutput::new(output.committed_value, output.blinding_factor, None))
            .collect();

        Ok(rewound_outputs)
    }

    async fn get_next_spend_key(&self) -> Result<PrivateKey, OutputManagerError> {
        let mut km = self.key_manager.lock().await;
        let key = km.next_key()?;
        self.resources.db.increment_key_index().await?;
        Ok(key.k)
    }

    async fn get_next_coinbase_key(&self) -> Result<PrivateKey, OutputManagerError> {
        let mut km = self.coinbase_key_manager.lock().await;
        let key = km.next_key()?;
        Ok(key.k)
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
        writeln!(f, "Pending incoming balance: {}", self.pending_incoming_balance)?;
        write!(f, "Pending outgoing balance: {}", self.pending_outgoing_balance)?;
        Ok(())
    }
}

/// This struct is a collection of the common resources that a async task in the service requires.
#[derive(Clone)]
pub struct OutputManagerResources<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    pub config: OutputManagerServiceConfig,
    pub db: OutputManagerDatabase<TBackend>,
    pub transaction_service: TransactionServiceHandle,
    pub factories: CryptoFactories,
    pub base_node_public_key: Option<CommsPublicKey>,
    pub event_publisher: OutputManagerEventSender,
    pub rewind_data: RewindData,
    pub consensus_constants: ConsensusConstants,
    pub connectivity_manager: ConnectivityRequester,
    pub shutdown_signal: ShutdownSignal,
}

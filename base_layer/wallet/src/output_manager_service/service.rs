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
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::OutputManagerError,
        handle::{OutputManagerEvent, OutputManagerRequest, OutputManagerResponse},
        storage::database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
        TxId,
    },
    types::{HashDigest, KeyDigest},
    util::futures::StateDelay,
};
use futures::{future::BoxFuture, pin_mut, stream::FuturesUnordered, FutureExt, SinkExt, Stream, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    fmt,
    sync::Mutex,
    time::Duration,
};
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, OutboundMessageRequester},
};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    transactions::{
        fee::Fee,
        tari_amount::MicroTari,
        transaction::{OutputFeatures, TransactionInput, TransactionOutput, UnblindedOutput},
        types::{CryptoFactories, PrivateKey},
        SenderTransactionProtocol,
    },
};
use tari_crypto::{keys::SecretKey as SecretKeyTrait, tari_utilities::hash::Hashable};
use tari_key_manager::{
    key_manager::KeyManager,
    mnemonic::{from_secret_key, MnemonicLanguage},
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::reply_channel;

const LOG_TARGET: &str = "wallet::output_manager_service";

/// This service will manage a wallet's available outputs and the key manager that produces the keys for these outputs.
/// The service will assemble transactions to be sent from the wallets available outputs and provide keys to receive
/// outputs. When the outputs are detected on the blockchain the Transaction service will call this Service to confirm
/// them to be moved to the spent and unspent output lists respectively.
pub struct OutputManagerService<TBackend, BNResponseStream>
where TBackend: OutputManagerBackend + 'static
{
    config: OutputManagerServiceConfig,
    key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    db: OutputManagerDatabase<TBackend>,
    outbound_message_service: OutboundMessageRequester,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    base_node_response_stream: Option<BNResponseStream>,
    factories: CryptoFactories,
    base_node_public_key: Option<CommsPublicKey>,
    pending_utxo_query_keys: HashSet<u64>,
    event_publisher: Publisher<OutputManagerEvent>,
}

impl<TBackend, BNResponseStream> OutputManagerService<TBackend, BNResponseStream>
where
    TBackend: OutputManagerBackend,
    BNResponseStream: Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
{
    pub async fn new(
        config: OutputManagerServiceConfig,
        outbound_message_service: OutboundMessageRequester,
        request_stream: reply_channel::Receiver<
            OutputManagerRequest,
            Result<OutputManagerResponse, OutputManagerError>,
        >,
        base_node_response_stream: BNResponseStream,
        db: OutputManagerDatabase<TBackend>,
        event_publisher: Publisher<OutputManagerEvent>,
        factories: CryptoFactories,
    ) -> Result<OutputManagerService<TBackend, BNResponseStream>, OutputManagerError>
    {
        // Check to see if there is any persisted state, otherwise start fresh
        let key_manager_state = match db.get_key_manager_state().await? {
            None => {
                let starting_state = KeyManagerState {
                    master_seed: PrivateKey::random(&mut OsRng),
                    branch_seed: "".to_string(),
                    primary_key_index: 0,
                };
                db.set_key_manager_state(starting_state.clone()).await?;
                starting_state
            },
            Some(km) => km,
        };

        Ok(OutputManagerService {
            config,
            outbound_message_service,
            key_manager: Mutex::new(KeyManager::<PrivateKey, KeyDigest>::from(
                key_manager_state.master_seed,
                key_manager_state.branch_seed,
                key_manager_state.primary_key_index,
            )),
            db,
            request_stream: Some(request_stream),
            base_node_response_stream: Some(base_node_response_stream),
            factories,
            base_node_public_key: None,
            pending_utxo_query_keys: HashSet::new(),
            event_publisher,
        })
    }

    pub async fn start(mut self) -> Result<(), OutputManagerError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("OutputManagerService initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let base_node_response_stream = self
            .base_node_response_stream
            .take()
            .expect("Output Manager Service initialized without base_node_response_stream")
            .fuse();
        pin_mut!(base_node_response_stream);

        let mut utxo_query_timeout_futures: FuturesUnordered<BoxFuture<'static, u64>> = FuturesUnordered::new();

        info!("Output Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request, &mut  utxo_query_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                 // Incoming messages from the Comms layer
                msg = base_node_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result = self.handle_base_node_response(inner_msg).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling base node service response from {}: {:?}", origin_public_key, resp);
                        Err(resp)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(OutputManagerEvent::Error(
                                    "Error handling Base Node Response message".to_string(),
                                ))
                                .await;
                    }
                }
                utxo_hash = utxo_query_timeout_futures.select_next_some() => {
                    let _ = self.handle_utxo_query_timeout(utxo_hash, &mut  utxo_query_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling UTXO query timeout : {:?}", resp);
                        Err(resp)
                    });
                }
                complete => {
                    info!(target: LOG_TARGET, "Output manager service shutting down");
                    break;
                }
            }
        }
        info!("Output Manager Service ended");
        Ok(())
    }

    /// This handler is called when the Service executor loops receives an API request
    async fn handle_request(
        &mut self,
        request: OutputManagerRequest,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<OutputManagerResponse, OutputManagerError>
    {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            OutputManagerRequest::AddOutput(uo) => {
                self.add_output(uo).await.map(|_| OutputManagerResponse::OutputAdded)
            },
            OutputManagerRequest::GetBalance => self.get_balance().await.map(OutputManagerResponse::Balance),
            OutputManagerRequest::GetRecipientKey((tx_id, amount)) => self
                .get_recipient_spending_key(tx_id, amount)
                .await
                .map(OutputManagerResponse::RecipientKeyGenerated),
            OutputManagerRequest::PrepareToSendTransaction((amount, fee_per_gram, lock_height, message)) => self
                .prepare_transaction_to_send(amount, fee_per_gram, lock_height, message)
                .await
                .map(OutputManagerResponse::TransactionToSend),
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
            OutputManagerRequest::GetSpentOutputs => self
                .fetch_spent_outputs()
                .await
                .map(OutputManagerResponse::SpentOutputs),
            OutputManagerRequest::GetUnspentOutputs => self
                .fetch_unspent_outputs()
                .await
                .map(OutputManagerResponse::UnspentOutputs),
            OutputManagerRequest::GetSeedWords => self.get_seed_words().map(OutputManagerResponse::SeedWords),
            OutputManagerRequest::GetCoinbaseKey((tx_id, amount, maturity_height)) => self
                .get_coinbase_spending_key(tx_id, amount, maturity_height)
                .await
                .map(OutputManagerResponse::RecipientKeyGenerated),
            OutputManagerRequest::SetBaseNodePublicKey(pk) => self
                .set_base_node_public_key(pk, utxo_query_timeout_futures)
                .await
                .map(|_| OutputManagerResponse::BaseNodePublicKeySet),
            OutputManagerRequest::SyncWithBaseNode => self
                .query_unspent_outputs_status(utxo_query_timeout_futures)
                .await
                .map(|_| OutputManagerResponse::StartedBaseNodeSync),
            OutputManagerRequest::GetInvalidOutputs => self
                .fetch_invalid_outputs()
                .await
                .map(|o| OutputManagerResponse::InvalidOutputs(o)),
        }
    }

    /// Handle an incoming basenode response message
    pub async fn handle_base_node_response(
        &mut self,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<(), OutputManagerError>
    {
        let request_key = response.request_key;

        let response: Vec<tari_core::transactions::proto::types::TransactionOutput> = match response.response {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => outputs.outputs,
            _ => {
                return Ok(());
            },
        };

        // Only process requests with a request_key that we are expecting.
        if !self.pending_utxo_query_keys.remove(&request_key) {
            debug!(
                target: LOG_TARGET,
                "Ignoring Base Node Response with unexpected request key, it was not meant for this service."
            );
            return Ok(());
        }

        // Construct a HashMap of all the unspent outputs
        let unspent_outputs: Vec<UnblindedOutput> = self.db.get_unspent_outputs().await?;

        let mut output_hashes = HashMap::new();
        for uo in unspent_outputs.iter() {
            let hash = uo.as_transaction_output(&self.factories)?.hash();
            output_hashes.insert(hash.clone(), uo.clone());
        }

        // Go through all the returned UTXOs and if they are in the hashmap remove them
        for output in response.iter() {
            let response_hash = TransactionOutput::try_from(output.clone())
                .map_err(|e| OutputManagerError::ConversionError(e))?
                .hash();

            let _ = output_hashes.remove(&response_hash);
        }

        // If there are any remaining Unspent Outputs we will move them to the invalid collection
        for (_k, v) in output_hashes {
            self.db.invalidate_output(v).await?;
        }

        debug!(
            target: LOG_TARGET,
            "Handled Base Node response for Query {}", request_key
        );

        self.event_publisher
            .send(OutputManagerEvent::ReceiveBaseNodeResponse(request_key))
            .await
            .map_err(|_| OutputManagerError::EventStreamError)?;

        Ok(())
    }

    /// Handle the timeout of a pending UTXO query.
    pub async fn handle_utxo_query_timeout(
        &mut self,
        query_key: u64,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<(), OutputManagerError>
    {
        if self.pending_utxo_query_keys.remove(&query_key) {
            debug!(target: LOG_TARGET, "UTXO Query {} timed out", query_key);
            self.query_unspent_outputs_status(utxo_query_timeout_futures).await?;

            self.event_publisher
                .send(OutputManagerEvent::BaseNodeSyncRequestTimedOut(query_key))
                .await
                .map_err(|_| OutputManagerError::EventStreamError)?;
        }
        Ok(())
    }

    /// Send queries to the base node to check the status of all unspent outputs. If the outputs are no longer
    /// available their status will be updated in the wallet.
    pub async fn query_unspent_outputs_status(
        &mut self,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<(), OutputManagerError>
    {
        match self.base_node_public_key.as_ref() {
            None => return Err(OutputManagerError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let unspent_outputs: Vec<UnblindedOutput> = self.db.get_unspent_outputs().await?;
                let mut output_hashes = Vec::new();
                for uo in unspent_outputs.iter() {
                    let hash = uo.as_transaction_output(&self.factories)?.hash();
                    output_hashes.push(hash.clone());
                }

                let request_key = OsRng.next_u64();

                let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs { outputs: output_hashes });
                let service_request = BaseNodeProto::BaseNodeServiceRequest {
                    request_key: request_key.clone(),
                    request: Some(request),
                };
                self.outbound_message_service
                    .send_direct(
                        pk.clone(),
                        OutboundEncryption::EncryptForPeer,
                        OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                    )
                    .await?;

                self.pending_utxo_query_keys.insert(request_key.clone());
                let state_timeout = StateDelay::new(
                    Duration::from_secs(self.config.base_node_query_timeout_in_secs),
                    request_key.clone(),
                );
                utxo_query_timeout_futures.push(state_timeout.delay().boxed());
                debug!(
                    target: LOG_TARGET,
                    "Output Manager Sync query ({}) sent to Base Node", request_key
                );
            },
        }
        Ok(())
    }

    /// Add an unblinded output to the unspent outputs list
    pub async fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        Ok(self.db.add_unspent_output(output).await?)
    }

    pub async fn get_balance(&self) -> Result<Balance, OutputManagerError> {
        Ok(self.db.get_balance().await?)
    }

    /// Request a spending key to be used to accept a transaction from a sender.
    pub async fn get_recipient_spending_key(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        let mut key = PrivateKey::default();
        {
            let mut km = acquire_lock!(self.key_manager);
            key = km.next_key()?.k;
        }

        self.db.increment_key_index().await?;
        self.db
            .accept_incoming_pending_transaction(tx_id, amount, key.clone(), OutputFeatures::default())
            .await?;

        Ok(key)
    }

    /// Request a spending key to be used to accept a coinbase output to be mined with the specified maturity height
    /// # Arguments:
    /// 'tx_id': the TxId that this coinbase transaction has been assigned
    /// 'amount': Amount of MicroTari the coinbase has as a value
    /// 'maturity_height': The block height at which this coinbase output becomes spendable
    pub async fn get_coinbase_spending_key(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<PrivateKey, OutputManagerError>
    {
        let mut key = PrivateKey::default();

        {
            let mut km = acquire_lock!(self.key_manager);
            key = km.next_key()?.k;
        }

        self.db.increment_key_index().await?;
        self.db
            .accept_incoming_pending_transaction(
                tx_id,
                amount,
                key.clone(),
                OutputFeatures::create_coinbase(maturity_height),
            )
            .await?;

        Ok(key)
    }

    /// Confirm the reception of an expected transaction output. This will be called by the Transaction Service when it
    /// detects the output on the blockchain
    pub async fn confirm_received_transaction_output(
        &mut self,
        tx_id: u64,
        received_output: &TransactionOutput,
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id.clone()).await?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 ||
            pending_transaction.outputs_to_be_received[0]
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default())
                .commitment !=
                received_output.commitment
        {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id.clone())
            .await?;

        Ok(())
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
        let outputs = self
            .select_outputs(amount, fee_per_gram, UTXOSelectionStrategy::Smallest)
            .await?;
        let total = outputs.iter().fold(MicroTari::from(0), |acc, x| acc + x.value);

        let offset = PrivateKey::random(&mut OsRng);
        let nonce = PrivateKey::random(&mut OsRng);

        let mut builder = SenderTransactionProtocol::builder(1);
        builder
            .with_lock_height(lock_height.unwrap_or(0))
            .with_fee_per_gram(fee_per_gram)
            .with_offset(offset.clone())
            .with_private_nonce(nonce.clone())
            .with_amount(0, amount)
            .with_message(message);

        for uo in outputs.iter() {
            builder.with_input(
                uo.as_transaction_input(&self.factories.commitment, uo.clone().features),
                uo.clone(),
            );
        }

        let fee_without_change = Fee::calculate(fee_per_gram, outputs.len(), 1);
        let mut change_key: Option<PrivateKey> = None;
        // If the input values > the amount to be sent + fees_without_change then we will need to include a change
        // output
        if total > amount + fee_without_change {
            let mut key = PrivateKey::default();
            {
                let mut km = acquire_lock!(self.key_manager);
                key = km.next_key()?.k;
            }
            self.db.increment_key_index().await?;
            change_key = Some(key.clone());
            builder.with_change_secret(key);
        }

        let stp = builder
            .build::<HashDigest>(&self.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;

        // If a change output was created add it to the pending_outputs list.
        let change_output = match change_key {
            Some(key) => Some(UnblindedOutput {
                value: stp.get_amount_to_self()?,
                spending_key: key,
                features: OutputFeatures::default(),
            }),
            None => None,
        };

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.db
            .encumber_outputs(stp.get_tx_id()?, outputs, change_output)
            .await?;

        Ok(stp)
    }

    /// Confirm that a received or sent transaction and its outputs have been detected on the base chain. The inputs and
    /// outputs are checked to see that they match what the stored PendingTransaction contains. This will
    /// be called by the Transaction Service which monitors the base chain.
    pub async fn confirm_transaction(
        &mut self,
        tx_id: u64,
        inputs: &[TransactionInput],
        outputs: &[TransactionOutput],
    ) -> Result<(), OutputManagerError>
    {
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id.clone()).await?;

        // Check that outputs to be spent can all be found in the provided transaction inputs
        let mut inputs_confirmed = true;
        for output_to_spend in pending_transaction.outputs_to_be_spent.iter() {
            let input_to_check = output_to_spend
                .clone()
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default());
            inputs_confirmed =
                inputs_confirmed && inputs.iter().any(|input| input.commitment == input_to_check.commitment);
        }

        // Check that outputs to be received can all be found in the provided transaction outputs
        let mut outputs_confirmed = true;
        for output_to_receive in pending_transaction.outputs_to_be_received.iter() {
            let output_to_check = output_to_receive
                .clone()
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default());
            outputs_confirmed = outputs_confirmed &&
                outputs
                    .iter()
                    .any(|output| output.commitment == output_to_check.commitment);
        }

        if !inputs_confirmed || !outputs_confirmed {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id)
            .await?;

        Ok(())
    }

    /// Cancel a pending transaction and place the encumbered outputs back into the unspent pool
    pub async fn cancel_transaction(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        Ok(self.db.cancel_pending_transaction_outputs(tx_id).await?)
    }

    /// Go through the pending transaction and if any have existed longer than the specified duration, cancel them
    pub async fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        Ok(self.db.timeout_pending_transaction_outputs(period).await?)
    }

    /// Select which outputs to use to send a transaction of the specified amount. Use the specified selection strategy
    /// to choose the outputs
    async fn select_outputs(
        &mut self,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        strategy: UTXOSelectionStrategy,
    ) -> Result<Vec<UnblindedOutput>, OutputManagerError>
    {
        let mut outputs = Vec::new();
        let mut total = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);

        let uo = self.db.fetch_sorted_unspent_outputs().await?;

        match strategy {
            UTXOSelectionStrategy::Smallest => {
                for o in uo.iter() {
                    outputs.push(o.clone());
                    total += o.value;
                    // I am assuming that the only output will be the payment output and change if required
                    fee_without_change = Fee::calculate(fee_per_gram, outputs.len(), 1);
                    fee_with_change = Fee::calculate(fee_per_gram, outputs.len(), 2);

                    if total == amount + fee_without_change || total >= amount + fee_with_change {
                        break;
                    }
                }
            },
        }

        if (total != amount + fee_without_change) && (total < amount + fee_with_change) {
            return Err(OutputManagerError::NotEnoughFunds);
        }

        Ok(outputs)
    }

    /// Set the base node public key to the list that will be used to check the status of UTXO's on the base chain. If
    /// this is the first time the base node public key is set do the UTXO queries.
    async fn set_base_node_public_key(
        &mut self,
        base_node_public_key: CommsPublicKey,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<(), OutputManagerError>
    {
        let startup_query = self.base_node_public_key.is_none();

        self.base_node_public_key = Some(base_node_public_key);

        if startup_query {
            self.query_unspent_outputs_status(utxo_query_timeout_futures).await?;
        }
        Ok(())
    }

    pub async fn fetch_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerError> {
        Ok(self.db.fetch_all_pending_transaction_outputs().await?)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_spent_outputs().await?)
    }

    pub async fn fetch_unspent_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_sorted_unspent_outputs().await?)
    }

    pub async fn fetch_invalid_outputs(&self) -> Result<Vec<UnblindedOutput>, OutputManagerError> {
        Ok(self.db.get_invalid_outputs().await?)
    }

    /// Return the Seed words for the current Master Key set in the Key Manager
    pub fn get_seed_words(&self) -> Result<Vec<String>, OutputManagerError> {
        Ok(from_secret_key(
            &acquire_lock!(self.key_manager).master_key,
            &MnemonicLanguage::English,
        )?)
    }
}

/// Different UTXO selection strategies for choosing which UTXO's are used to fulfill a transaction
/// TODO Investigate and implement more optimal strategies
pub enum UTXOSelectionStrategy {
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit is removing small
    // UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
}

/// This struct holds the detailed balance of the Output Manager Service.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// The current balance that is available to spend
    pub available_balance: MicroTari,
    /// The current balance of funds that are due to be received but have not yet been confirmed
    pub pending_incoming_balance: MicroTari,
    /// The current balance of funds encumbered in pending outbound transactions that have not been confirmed
    pub pending_outgoing_balance: MicroTari,
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Available balance: {}", self.available_balance)?;
        writeln!(f, "Pending incoming balance: {}", self.pending_incoming_balance)?;
        write!(f, "Pending outgoing balance: {}", self.pending_outgoing_balance)?;
        Ok(())
    }
}

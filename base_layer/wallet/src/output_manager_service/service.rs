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
        storage::{
            database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
            models::DbUnblindedOutput,
        },
        TxId,
    },
    transaction_service::handle::TransactionServiceHandle,
    types::{HashDigest, KeyDigest},
    util::futures::StateDelay,
};
use futures::{future::BoxFuture, pin_mut, stream::FuturesUnordered, FutureExt, SinkExt, Stream, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{cmp, cmp::Ordering, collections::HashMap, convert::TryFrom, fmt, sync::Mutex, time::Duration};
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundMessageRequester};
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
        transaction::{
            KernelFeatures,
            OutputFeatures,
            Transaction,
            TransactionInput,
            TransactionOutput,
            UnblindedOutput,
        },
        types::{CryptoFactories, PrivateKey},
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    keys::SecretKey as SecretKeyTrait,
    tari_utilities::{hash::Hashable, hex::Hex},
};
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
    transaction_service: TransactionServiceHandle,
    request_stream:
        Option<reply_channel::Receiver<OutputManagerRequest, Result<OutputManagerResponse, OutputManagerError>>>,
    base_node_response_stream: Option<BNResponseStream>,
    factories: CryptoFactories,
    base_node_public_key: Option<CommsPublicKey>,
    pending_utxo_query_keys: HashMap<u64, Vec<Vec<u8>>>,
    pending_revalidation_query_keys: HashMap<u64, Vec<Vec<u8>>>,
    event_publisher: Publisher<OutputManagerEvent>,
}

impl<TBackend, BNResponseStream> OutputManagerService<TBackend, BNResponseStream>
where
    TBackend: OutputManagerBackend,
    BNResponseStream: Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
{
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        config: OutputManagerServiceConfig,
        outbound_message_service: OutboundMessageRequester,
        transaction_service: TransactionServiceHandle,
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

        // Clear any encumberances for transactions that were being negotiated but did not complete to become official
        // Pending Transactions.
        db.clear_short_term_encumberances().await?;

        Ok(OutputManagerService {
            config,
            outbound_message_service,
            transaction_service,
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
            pending_utxo_query_keys: HashMap::new(),
            pending_revalidation_query_keys: HashMap::new(),
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

        info!(target: LOG_TARGET, "Output Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request, &mut utxo_query_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                 // Incoming messages from the Comms layer
                msg = base_node_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.clone().into_origin_and_inner();
                    trace!(target: LOG_TARGET, "Handling Base Node Response, Trace: {}", msg.dht_header.message_tag);
                    let result = self.handle_base_node_response(inner_msg).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling base node service response from {}: {:?}, Trace: {}", origin_public_key, resp, msg.dht_header.message_tag);
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
                    trace!(target: LOG_TARGET, "Handling Base Node Sync Timeout");
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
            trace!(target: LOG_TARGET, "Select Loop end");
        }
        info!(target: LOG_TARGET, "Output Manager Service ended");
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
            OutputManagerRequest::GetSeedWords => self.get_seed_words().map(OutputManagerResponse::SeedWords),
            OutputManagerRequest::SetBaseNodePublicKey(pk) => self
                .set_base_node_public_key(pk, utxo_query_timeout_futures)
                .await
                .map(|_| OutputManagerResponse::BaseNodePublicKeySet),
            OutputManagerRequest::SyncWithBaseNode => self
                .query_unspent_outputs_status(utxo_query_timeout_futures)
                .await
                .map(OutputManagerResponse::StartedBaseNodeSync),
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
        }
    }

    /// Handle an incoming basenode response message
    async fn handle_base_node_response(
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

        let mut unspent_query_handled = false;
        let mut invalid_query_handled = false;

        // Check if the received key is in the pending UTXO query list to be handled
        if let Some(queried_hashes) = self.pending_utxo_query_keys.remove(&request_key) {
            trace!(
                target: LOG_TARGET,
                "Handling a Base Node Response for a Unspent Outputs request ({})",
                request_key
            );

            // Construct a HashMap of all the unspent outputs
            let unspent_outputs: Vec<DbUnblindedOutput> = self.db.get_unspent_outputs().await?;

            let mut output_hashes = HashMap::new();
            for uo in unspent_outputs.iter() {
                let hash = uo.hash.clone();
                if queried_hashes.iter().any(|h| &hash == h) {
                    output_hashes.insert(hash.clone(), uo.clone());
                }
            }

            // Go through all the returned UTXOs and if they are in the hashmap remove them
            for output in response.iter() {
                let response_hash = TransactionOutput::try_from(output.clone())
                    .map_err(OutputManagerError::ConversionError)?
                    .hash();

                let _ = output_hashes.remove(&response_hash);
            }

            // If there are any remaining Unspent Outputs we will move them to the invalid collection
            for (_k, v) in output_hashes {
                // Get the transaction these belonged to so we can display the kernel signature of the transaction
                // this output belonged to.

                warn!(
                    target: LOG_TARGET,
                    "Output with value {} not returned from Base Node query and is thus being invalidated",
                    v.unblinded_output.value
                );
                // If the output that is being invalidated has an associated TxId then get the kernel signature of
                // the transaction and display for easier debugging
                if let Some(tx_id) = self.db.invalidate_output(v).await? {
                    if let Ok(transaction) = self.transaction_service.get_completed_transaction(tx_id).await {
                        info!(
                            target: LOG_TARGET,
                            "Invalidated Output is from Transaction (TxId: {}) with message: {} and Kernel Signature: \
                             {}",
                            transaction.tx_id,
                            transaction.message,
                            transaction.transaction.body.kernels()[0]
                                .excess_sig
                                .get_signature()
                                .to_hex()
                        )
                    }
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Invalidated Output does not have an associated TxId so it is likely a Coinbase output lost \
                         to a Re-Org"
                    );
                }
            }
            unspent_query_handled = true;
            debug!(
                target: LOG_TARGET,
                "Handled Base Node response for Unspent Outputs Query {}", request_key
            );
        };

        // Check if the received key is in the Invalid UTXO query list waiting to be handled
        if self.pending_revalidation_query_keys.remove(&request_key).is_some() {
            trace!(
                target: LOG_TARGET,
                "Handling a Base Node Response for a Invalid Outputs request ({})",
                request_key
            );
            let invalid_outputs = self.db.get_invalid_outputs().await?;

            for output in response.iter() {
                let response_hash = TransactionOutput::try_from(output.clone())
                    .map_err(OutputManagerError::ConversionError)?
                    .hash();

                if let Some(output) = invalid_outputs.iter().find(|o| o.hash == response_hash) {
                    if self
                        .db
                        .revalidate_output(output.unblinded_output.spending_key.clone())
                        .await
                        .is_ok()
                    {
                        trace!(
                            target: LOG_TARGET,
                            "Output with value {} has been restored to a valid spendable output",
                            output.unblinded_output.value
                        );
                    }
                }
            }
            invalid_query_handled = true;
            debug!(
                target: LOG_TARGET,
                "Handled Base Node response for Invalid Outputs Query {}", request_key
            );
        }

        if unspent_query_handled || invalid_query_handled {
            let _ = self
                .event_publisher
                .send(OutputManagerEvent::ReceiveBaseNodeResponse(request_key))
                .await
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event, usually because there are no subscribers: {:?}",
                        e
                    );
                    e
                });
        }

        Ok(())
    }

    /// Handle the timeout of a pending UTXO query.
    pub async fn handle_utxo_query_timeout(
        &mut self,
        query_key: u64,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<(), OutputManagerError>
    {
        if let Some(hashes) = self.pending_utxo_query_keys.remove(&query_key) {
            warn!(target: LOG_TARGET, "UTXO Unspent Outputs Query {} timed out", query_key);
            self.query_outputs_status(utxo_query_timeout_futures, hashes, UtxoQueryType::UnspentOutputs)
                .await?;
        }

        if let Some(hashes) = self.pending_revalidation_query_keys.remove(&query_key) {
            warn!(target: LOG_TARGET, "UTXO Invalid Outputs Query {} timed out", query_key);
            self.query_outputs_status(utxo_query_timeout_futures, hashes, UtxoQueryType::InvalidOutputs)
                .await?;
        }

        let _ = self
            .event_publisher
            .send(OutputManagerEvent::BaseNodeSyncRequestTimedOut(query_key))
            .await
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });
        Ok(())
    }

    pub async fn query_unspent_outputs_status(
        &mut self,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<u64, OutputManagerError>
    {
        let unspent_output_hashes = self
            .db
            .get_unspent_outputs()
            .await?
            .iter()
            .map(|uo| uo.hash.clone())
            .collect();

        let key = self
            .query_outputs_status(
                utxo_query_timeout_futures,
                unspent_output_hashes,
                UtxoQueryType::UnspentOutputs,
            )
            .await?;

        Ok(key)
    }

    pub async fn query_invalid_outputs_status(
        &mut self,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<u64, OutputManagerError>
    {
        let invalid_output_hashes: Vec<_> = self
            .db
            .get_invalid_outputs()
            .await?
            .into_iter()
            .map(|uo| uo.hash)
            .collect();

        let key = if !invalid_output_hashes.is_empty() {
            self.query_outputs_status(
                utxo_query_timeout_futures,
                invalid_output_hashes,
                UtxoQueryType::InvalidOutputs,
            )
            .await?
        } else {
            0
        };

        Ok(key)
    }

    /// Send queries to the base node to check the status of all specified outputs.
    async fn query_outputs_status(
        &mut self,
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
        mut outputs_to_query: Vec<Vec<u8>>,
        query_type: UtxoQueryType,
    ) -> Result<u64, OutputManagerError>
    {
        match self.base_node_public_key.as_ref() {
            None => Err(OutputManagerError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let mut first_request_key = 0;

                // Determine how many rounds of base node request we need to query all the outputs in batches of
                // max_utxo_query_size
                let rounds =
                    ((outputs_to_query.len() as f32) / (self.config.max_utxo_query_size as f32 + 0.1)) as usize + 1;

                for r in 0..rounds {
                    let mut output_hashes = Vec::new();
                    for uo_hash in
                        outputs_to_query.drain(..cmp::min(self.config.max_utxo_query_size, outputs_to_query.len()))
                    {
                        output_hashes.push(uo_hash);
                    }
                    let request_key = OsRng.next_u64();
                    if first_request_key == 0 {
                        first_request_key = request_key;
                    }

                    let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs {
                        outputs: output_hashes.clone(),
                    });

                    let service_request = BaseNodeProto::BaseNodeServiceRequest {
                        request_key,
                        request: Some(request),
                    };

                    let send_message_response = self
                        .outbound_message_service
                        .send_direct(
                            pk.clone(),
                            OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                        )
                        .await?;

                    // Here we are going to spawn a non-blocking task that will monitor and log the progress of the
                    // send process.
                    tokio::spawn(async move {
                        match send_message_response.resolve().await {
                            Err(err) => warn!(
                                target: LOG_TARGET,
                                "Failed to send Output Manager UTXO query ({}) to Base Node: {}", request_key, err
                            ),
                            Ok(send_states) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "Output Manager UTXO query ({}) queued for sending with Message {}",
                                    request_key,
                                    send_states[0].tag,
                                );
                                let message_tag = send_states[0].tag;
                                if send_states.wait_single().await {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Output Manager UTXO query ({}) successfully sent to Base Node with Message {}",
                                        request_key,
                                        message_tag,
                                    )
                                } else {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Failed to send Output Manager UTXO query ({}) to Base Node with Message {}",
                                        request_key,
                                        message_tag,
                                    );
                                }
                            },
                        }
                    });

                    match query_type {
                        UtxoQueryType::UnspentOutputs => {
                            self.pending_utxo_query_keys.insert(request_key, output_hashes);
                        },
                        UtxoQueryType::InvalidOutputs => {
                            self.pending_revalidation_query_keys.insert(request_key, output_hashes);
                        },
                    }

                    let state_timeout = StateDelay::new(self.config.base_node_query_timeout, request_key);
                    utxo_query_timeout_futures.push(state_timeout.delay().boxed());

                    debug!(
                        target: LOG_TARGET,
                        "Output Manager {} query ({}) sent to Base Node, part {} of {} requests",
                        query_type,
                        request_key,
                        r + 1,
                        rounds
                    );
                }
                // We are just going to return the first request key for use by the front end. It is very unlikely that
                // a mobile wallet will ever have this query split up
                Ok(first_request_key)
            },
        }
    }

    /// Add an unblinded output to the unspent outputs list
    pub async fn add_output(&mut self, output: UnblindedOutput) -> Result<(), OutputManagerError> {
        let output = DbUnblindedOutput::from_unblinded_output(output, &self.factories)?;
        Ok(self.db.add_unspent_output(output).await?)
    }

    async fn get_balance(&self) -> Result<Balance, OutputManagerError> {
        let balance = self.db.get_balance().await?;
        trace!(target: LOG_TARGET, "Balance: {:?}", balance);
        Ok(balance)
    }

    /// Request a spending key to be used to accept a transaction from a sender.
    async fn get_recipient_spending_key(
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
            .accept_incoming_pending_transaction(tx_id, amount, key.clone(), OutputFeatures::default(), &self.factories)
            .await?;

        self.confirm_encumberance(tx_id).await?;
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
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Assumption: We are only allowing a single output per receiver in the current transaction protocols.
        if pending_transaction.outputs_to_be_received.len() != 1 ||
            pending_transaction.outputs_to_be_received[0]
                .unblinded_output
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default())
                .commitment !=
                received_output.commitment
        {
            return Err(OutputManagerError::IncompleteTransaction);
        }

        self.db
            .confirm_pending_transaction_outputs(pending_transaction.tx_id)
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
            .with_message(message);

        for uo in outputs.iter() {
            builder.with_input(
                uo.unblinded_output
                    .as_transaction_input(&self.factories.commitment, uo.unblinded_output.clone().features),
                uo.unblinded_output.clone(),
            );
        }

        let fee_without_change = Fee::calculate(fee_per_gram, 1, outputs.len(), 1);
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
        let mut change_output = Vec::<DbUnblindedOutput>::new();
        if let Some(key) = change_key {
            change_output.push(DbUnblindedOutput::from_unblinded_output(
                UnblindedOutput {
                    value: stp.get_amount_to_self()?,
                    spending_key: key,
                    features: OutputFeatures::default(),
                },
                &self.factories,
            )?);
        }

        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        self.db
            .encumber_outputs(stp.get_tx_id()?, outputs, change_output)
            .await?;

        Ok(stp)
    }

    /// Confirm that a transaction has finished being negotiated between parties so the short-term encumberance can be
    /// made official
    async fn confirm_encumberance(&mut self, tx_id: u64) -> Result<(), OutputManagerError> {
        self.db.confirm_encumbered_outputs(tx_id).await?;

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
        let pending_transaction = self.db.fetch_pending_transaction_outputs(tx_id).await?;

        // Check that outputs to be spent can all be found in the provided transaction inputs
        let mut inputs_confirmed = true;
        for output_to_spend in pending_transaction.outputs_to_be_spent.iter() {
            let input_to_check = output_to_spend
                .unblinded_output
                .clone()
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default());
            inputs_confirmed =
                inputs_confirmed && inputs.iter().any(|input| input.commitment == input_to_check.commitment);
        }

        // Check that outputs to be received can all be found in the provided transaction outputs
        let mut outputs_confirmed = true;
        for output_to_receive in pending_transaction.outputs_to_be_received.iter() {
            let output_to_check = output_to_receive
                .unblinded_output
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
        trace!(
            target: LOG_TARGET,
            "Cancelling pending transaction outputs for TxId: {}",
            tx_id
        );
        Ok(self.db.cancel_pending_transaction_outputs(tx_id).await?)
    }

    /// Go through the pending transaction and if any have existed longer than the specified duration, cancel them
    async fn timeout_pending_transactions(&mut self, period: Duration) -> Result<(), OutputManagerError> {
        Ok(self.db.timeout_pending_transaction_outputs(period).await?)
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
        let mut utxos = Vec::new();
        let mut total = MicroTari::from(0);
        let mut fee_without_change = MicroTari::from(0);
        let mut fee_with_change = MicroTari::from(0);

        let uo = self.db.fetch_sorted_unspent_outputs().await?;

        // Heuristic for selecting strategy: Default to MaturityThenSmallest, but if amount >
        // alpha * largest UTXO, use Largest
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

        let uo = match strategy {
            UTXOSelectionStrategy::Smallest => uo,
            // TODO: We should pass in the current height and group
            // all funds less than the current height as maturity 0
            UTXOSelectionStrategy::MaturityThenSmallest => {
                let mut new_uo = uo;
                new_uo.sort_by(|a, b| {
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
                new_uo
            },
            UTXOSelectionStrategy::Largest => uo.into_iter().rev().collect(),
        };

        let mut require_change_output = false;
        for o in uo.iter() {
            utxos.push(o.clone());
            total += o.unblinded_output.value;
            // I am assuming that the only output will be the payment output and change if required
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
        utxo_query_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, u64>>,
    ) -> Result<(), OutputManagerError>
    {
        let startup_query = self.base_node_public_key.is_none();

        self.base_node_public_key = Some(base_node_public_key);

        if startup_query {
            self.query_unspent_outputs_status(utxo_query_timeout_futures).await?;
            self.query_invalid_outputs_status(utxo_query_timeout_futures).await?;
        }
        Ok(())
    }

    pub async fn fetch_pending_transaction_outputs(
        &self,
    ) -> Result<HashMap<u64, PendingTransactionOutputs>, OutputManagerError> {
        Ok(self.db.fetch_all_pending_transaction_outputs().await?)
    }

    pub async fn fetch_spent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_spent_outputs().await?)
    }

    pub async fn fetch_unspent_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.db.fetch_sorted_unspent_outputs().await?)
    }

    pub async fn fetch_invalid_outputs(&self) -> Result<Vec<DbUnblindedOutput>, OutputManagerError> {
        Ok(self.db.get_invalid_outputs().await?)
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
        let utxo_total = inputs
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
                uo.unblinded_output
                    .as_transaction_input(&self.factories.commitment, uo.unblinded_output.clone().features),
                uo.unblinded_output.clone(),
            );
        }
        trace!(target: LOG_TARGET, "Add outputs to coin split transaction.");
        let mut outputs: Vec<DbUnblindedOutput> = Vec::with_capacity(output_count);
        let change_output = utxo_total
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

            let mut spend_key = PrivateKey::default();
            {
                let mut km = acquire_lock!(self.key_manager);
                spend_key = km.next_key()?.k;
            }
            self.db.increment_key_index().await?;
            let utxo = DbUnblindedOutput::from_unblinded_output(
                UnblindedOutput::new(output_amount, spend_key, None),
                &self.factories,
            )?;
            outputs.push(utxo.clone());
            builder.with_output(utxo.unblinded_output);
        }
        trace!(target: LOG_TARGET, "Build coin split transaction.");
        let factories = CryptoFactories::default();
        let mut stp = builder
            .build::<HashDigest>(&self.factories)
            .map_err(|e| OutputManagerError::BuildError(e.message))?;
        // The Transaction Protocol built successfully so we will pull the unspent outputs out of the unspent list and
        // store them until the transaction times out OR is confirmed
        let tx_id = stp.get_tx_id()?;
        trace!(
            target: LOG_TARGET,
            "Encumber coin split transaction ({}) outputs.",
            tx_id
        );
        self.db.encumber_outputs(tx_id, inputs, outputs).await?;
        self.confirm_encumberance(tx_id).await?;
        trace!(target: LOG_TARGET, "Finalize coin split transaction ({}).", tx_id);
        stp.finalize(KernelFeatures::empty(), &factories)?;
        let tx = stp.get_transaction().map(Clone::clone)?;
        Ok((tx_id, tx, fee, utxo_total))
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
    // Start from the smallest UTXOs and work your way up until the amount is covered. Main benefit
    // is removing small UTXOs from the blockchain, con is that it costs more in fees
    Smallest,
    // Start from oldest maturity to reduce the likelihood of grabbing locked up UTXOs
    MaturityThenSmallest,
    // A strategy that selects the largest UTXOs first. Preferred when the amount is large
    Largest,
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

enum UtxoQueryType {
    UnspentOutputs,
    InvalidOutputs,
}

impl fmt::Display for UtxoQueryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtxoQueryType::UnspentOutputs => write!(f, "Unspent Outputs"),
            UtxoQueryType::InvalidOutputs => write!(f, "Invalid Outputs"),
        }
    }
}

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
    output_manager_service::{handle::OutputManagerHandle, TxId},
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceRequest, TransactionServiceResponse},
        storage::database::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            PendingCoinbaseTransaction,
            TransactionBackend,
            TransactionDatabase,
            TransactionStatus,
        },
    },
    util::futures::StateDelay,
};
use chrono::Utc;
use futures::{
    channel::oneshot,
    future::{BoxFuture, FutureExt},
    pin_mut,
    stream::FuturesUnordered,
    SinkExt,
    Stream,
    StreamExt,
};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
};
use tari_broadcast_channel::Publisher;
use tari_comms::{
    message::MessageTag,
    peer_manager::NodeIdentity,
    protocol::messaging::{MessagingEvent, MessagingEventReceiver},
    types::CommsPublicKey,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageResponse},
};
#[cfg(feature = "test_harness")]
use tari_core::transactions::{tari_amount::T, types::BlindingFactor};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    mempool::{
        proto::mempool as MempoolProto,
        service::{MempoolResponse, MempoolServiceResponse},
        TxStorageResponse,
    },
    transactions::{
        proto as TransactionProto,
        tari_amount::MicroTari,
        transaction::{KernelFeatures, OutputFeatures, OutputFlags, Transaction, TransactionOutput},
        transaction_protocol::{
            proto,
            recipient::{RecipientSignedMessage, RecipientState},
            sender::TransactionSenderMessage,
        },
        types::{CryptoFactories, PrivateKey},
        ReceiverTransactionProtocol,
    },
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey, tari_utilities::hash::Hashable};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::Receiver};

const LOG_TARGET: &str = "base_layer::wallet::transaction_service::service";

/// Contains the generated TxId and SpendingKey for a Pending Coinbase transaction
#[derive(Debug)]
pub struct PendingCoinbaseSpendingKey {
    pub tx_id: TxId,
    pub spending_key: PrivateKey,
}

/// TransactionService allows for the management of multiple inbound and outbound transaction protocols
/// which are uniquely identified by a tx_id. The TransactionService generates and accepts the various protocol
/// messages and applies them to the appropriate protocol instances based on the tx_id.
/// The TransactionService allows for the sending of transactions to single receivers, when the appropriate recipient
/// response is handled the transaction is completed and moved to the completed_transaction buffer.
/// The TransactionService will accept inbound transactions and generate a reply. Received transactions will remain
/// in the pending_inbound_transactions buffer.
/// TODO Allow for inbound transactions that are detected on the blockchain to be marked as complete in the
/// OutputManagerService
/// TODO Detect Completed Transactions on the blockchain before marking them as completed in OutputManagerService
/// # Fields
/// `pending_outbound_transactions` - List of transaction protocols sent by this client and waiting response from the
/// recipient
/// `pending_inbound_transactions` - List of transaction protocols that have been received and responded to.
/// `completed_transaction` - List of sent transactions that have been responded to and are completed.

pub struct TransactionService<TTxStream, TTxReplyStream, TTxFinalizedStream, MReplyStream, BNResponseStream, TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    config: TransactionServiceConfig,
    db: TransactionDatabase<TBackend>,
    outbound_message_service: OutboundMessageRequester,
    message_event_receiver: Option<MessagingEventReceiver>,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    mempool_response_stream: Option<MReplyStream>,
    base_node_response_stream: Option<BNResponseStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: Publisher<TransactionEvent>,
    node_identity: Arc<NodeIdentity>,
    factories: CryptoFactories,
    base_node_public_key: Option<CommsPublicKey>,
    pending_outbound_message_results: HashMap<MessageTag, OutboundTransaction>,
}

impl<TTxStream, TTxReplyStream, TTxFinalizedStream, MReplyStream, BNResponseStream, TBackend>
    TransactionService<TTxStream, TTxReplyStream, TTxFinalizedStream, MReplyStream, BNResponseStream, TBackend>
where
    TTxStream: Stream<Item = DomainMessage<proto::TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<proto::RecipientSignedMessage>>,
    TTxFinalizedStream: Stream<Item = DomainMessage<proto::TransactionFinalizedMessage>>,
    MReplyStream: Stream<Item = DomainMessage<MempoolProto::MempoolServiceResponse>>,
    BNResponseStream: Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    TBackend: TransactionBackend + Clone + 'static,
{
    pub fn new(
        config: TransactionServiceConfig,
        db: TransactionDatabase<TBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        transaction_finalized_stream: TTxFinalizedStream,
        mempool_response_stream: MReplyStream,
        base_node_response_stream: BNResponseStream,
        output_manager_service: OutputManagerHandle,
        outbound_message_service: OutboundMessageRequester,
        message_event_receiver: MessagingEventReceiver,
        event_publisher: Publisher<TransactionEvent>,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
    ) -> Self
    {
        TransactionService {
            config,
            db,
            outbound_message_service,
            message_event_receiver: Some(message_event_receiver),
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            transaction_finalized_stream: Some(transaction_finalized_stream),
            mempool_response_stream: Some(mempool_response_stream),
            base_node_response_stream: Some(base_node_response_stream),
            request_stream: Some(request_stream),
            event_publisher,
            node_identity,
            factories,
            base_node_public_key: None,
            pending_outbound_message_results: HashMap::new(),
        }
    }

    #[warn(unreachable_code)]
    pub async fn start(mut self) -> Result<(), TransactionServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Transaction Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);
        let transaction_stream = self
            .transaction_stream
            .take()
            .expect("Transaction Service initialized without transaction_stream")
            .fuse();
        pin_mut!(transaction_stream);
        let transaction_reply_stream = self
            .transaction_reply_stream
            .take()
            .expect("Transaction Service initialized without transaction_reply_stream")
            .fuse();
        pin_mut!(transaction_reply_stream);
        let transaction_finalized_stream = self
            .transaction_finalized_stream
            .take()
            .expect("Transaction Service initialized without transaction_finalized_stream")
            .fuse();
        pin_mut!(transaction_finalized_stream);
        let mempool_response_stream = self
            .mempool_response_stream
            .take()
            .expect("Transaction Service initialized without mempool_response_stream")
            .fuse();
        pin_mut!(mempool_response_stream);
        let base_node_response_stream = self
            .base_node_response_stream
            .take()
            .expect("Transaction Service initialized without base_node_response_stream")
            .fuse();
        pin_mut!(base_node_response_stream);
        let message_event_receiver = self
            .message_event_receiver
            .take()
            .expect("Transaction Service initialized without message_event_subscription")
            .fuse();
        pin_mut!(message_event_receiver);

        let mut discovery_process_futures: FuturesUnordered<
            BoxFuture<'static, Result<(MessageTag, OutboundTransaction), TransactionServiceError>>,
        > = FuturesUnordered::new();

        let mut broadcast_timeout_futures: FuturesUnordered<BoxFuture<'static, TxId>> = FuturesUnordered::new();
        let mut mined_request_timeout_futures: FuturesUnordered<BoxFuture<'static, TxId>> = FuturesUnordered::new();

        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request, &mut discovery_process_futures, &mut  broadcast_timeout_futures, &mut  mined_request_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = transaction_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result  = self.accept_transaction(origin_public_key, inner_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming Transaction message: {:?} for NodeID: {}", err, self.node_identity.node_id().short_str());
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(TransactionEvent::Error(
                                    "Error handling Transaction Sender message".to_string(),
                                ))
                                .await;
                    }
                },
                 // Incoming messages from the Comms layer
                msg = transaction_reply_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result = self.accept_recipient_reply(origin_public_key, inner_msg, &mut broadcast_timeout_futures).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming Transaction Reply message: {:?} for NodeId: {}", err, self.node_identity.node_id().short_str());
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(TransactionEvent::Error(
                                    "Error handling Transaction Recipient Reply message".to_string(),
                                ))
                                .await;
                    }
                },
               // Incoming messages from the Comms layer
                msg = transaction_finalized_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result = self.accept_finalized_transaction(origin_public_key, inner_msg, &mut broadcast_timeout_futures).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming Transaction Finalized message: {:?} for NodeID: {}", err , self.node_identity.node_id().short_str());
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher
                                .send(TransactionEvent::Error(
                                    "Error handling Transaction Finalized message".to_string(),
                                ))
                                .await;
                    }
                },
                // Incoming messages from the Comms layer
                msg = mempool_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let _ = self.handle_mempool_response(inner_msg, &mut mined_request_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling mempool service response: {:?}", resp);
                        Err(resp)
                    });
                }
                // Incoming messages from the Comms layer
                msg = base_node_response_stream.select_next_some() => {
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let _ = self.handle_base_node_response(inner_msg).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling base node service response from {}: {:?} for NodeID: {}", origin_public_key, resp, self.node_identity.node_id().short_str());
                        Err(resp)
                    });
                }
                response = discovery_process_futures.select_next_some() => {
                    match response {
                        Ok((message_tag, outbound_tx)) => {
                            self.db
                                .add_pending_outbound_transaction(outbound_tx.tx_id, outbound_tx.clone())
                                .await?;
                            self.pending_outbound_message_results.insert(message_tag, outbound_tx.clone());
                            let _ = self.event_publisher
                                .send(TransactionEvent::TransactionSendDiscoveryComplete(outbound_tx.tx_id, true))
                                .await;
                            info!(
                                target: LOG_TARGET,
                                "Discovery process completed for TxId: {} with Message Tag: {} now waiting for MessageSent event",
                                outbound_tx.tx_id,
                                message_tag,
                            );
                        },
                        Err(TransactionServiceError::DiscoveryProcessFailed(tx_id)) => {
                            if let Err(e) = self.output_manager_service.cancel_transaction(tx_id).await {
                                error!(target: LOG_TARGET, "Failed to Cancel TX_ID: {} after failed sending attempt", tx_id);
                            }
                            error!(target: LOG_TARGET, "Discovery and Send failed for TX_ID: {}", tx_id);
                            let _ = self.event_publisher
                                .send(TransactionEvent::TransactionSendDiscoveryComplete(tx_id, false))
                                .await;
                        }
                        Err(e) => error!(target: LOG_TARGET, "Discovery and Send failed with Error: {:?}", e),
                    }
                },
                message_event = message_event_receiver.select_next_some() => {
                   match message_event {
                   Ok(event) => {
                       let _ = self.handle_message_event((*event).clone()).await.or_else(|resp| {
                            error!(target: LOG_TARGET, "Error handling outbound message event: {:?}", resp);
                            Err(resp)
                        });
                    },
                    Err(e) => error!(target: LOG_TARGET, "Error handling Outbound Message Event: {:?}", e),
                   }
                }
                tx_id = broadcast_timeout_futures.select_next_some() => {
                    let _ = self.handle_mempool_broadcast_timeout(tx_id, &mut  broadcast_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling mempool broadcast timeout : {:?}", resp);
                        Err(resp)
                    });
                }
                tx_id = mined_request_timeout_futures.select_next_some() => {
                    let _ = self.handle_transaction_mined_request_timeout(tx_id, &mut  mined_request_timeout_futures).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling transaction mined? request timeout : {:?}", resp);
                        Err(resp)
                    });
                }
                complete => {
                    info!(target: LOG_TARGET, "Transaction service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: TransactionServiceRequest,
        discovery_process_futures: &mut FuturesUnordered<
            BoxFuture<'static, Result<(MessageTag, OutboundTransaction), TransactionServiceError>>,
        >,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Handling Service Request: {:?}", request);
        match request {
            TransactionServiceRequest::SendTransaction((dest_pubkey, amount, fee_per_gram, message)) => self
                .send_transaction(dest_pubkey, amount, fee_per_gram, message, discovery_process_futures)
                .await
                .map(|_| TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::GetPendingInboundTransactions => Ok(
                TransactionServiceResponse::PendingInboundTransactions(self.get_pending_inbound_transactions().await?),
            ),
            TransactionServiceRequest::GetPendingOutboundTransactions => {
                Ok(TransactionServiceResponse::PendingOutboundTransactions(
                    self.get_pending_outbound_transactions().await?,
                ))
            },
            TransactionServiceRequest::GetCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.get_completed_transactions().await?),
            ),
            TransactionServiceRequest::RequestCoinbaseSpendingKey((amount, maturity_height)) => Ok(
                TransactionServiceResponse::CoinbaseKey(self.request_coinbase_key(amount, maturity_height).await?),
            ),
            TransactionServiceRequest::CompleteCoinbaseTransaction((tx_id, completed_transaction)) => {
                self.submit_completed_coinbase_transaction(tx_id, completed_transaction)
                    .await?;
                Ok(TransactionServiceResponse::CompletedCoinbaseTransactionReceived)
            },
            TransactionServiceRequest::CancelPendingCoinbaseTransaction(tx_id) => {
                self.cancel_pending_coinbase_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::CoinbaseTransactionCancelled)
            },
            TransactionServiceRequest::SetBaseNodePublicKey(public_key) => self
                .set_base_node_public_key(public_key, broadcast_timeout_futures, mined_request_timeout_futures)
                .await
                .map(|_| TransactionServiceResponse::BaseNodePublicKeySet),
            TransactionServiceRequest::ImportUtxo(value, source_public_key, message) => self
                .add_utxo_import_transaction(value, source_public_key, message)
                .await
                .map(TransactionServiceResponse::UtxoImported),
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::CompletePendingOutboundTransaction(completed_transaction) => {
                self.complete_pending_outbound_transaction(completed_transaction)
                    .await?;
                Ok(TransactionServiceResponse::CompletedPendingTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::FinalizePendingInboundTransaction(tx_id) => {
                self.finalize_received_test_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::FinalizedPendingInboundTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::AcceptTestTransaction((tx_id, amount, source_pubkey)) => {
                self.receive_test_transaction(tx_id, amount, source_pubkey).await?;
                Ok(TransactionServiceResponse::AcceptedTestTransaction)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::BroadcastTransaction(tx_id) => {
                self.broadcast_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::TransactionBroadcast)
            },
            #[cfg(feature = "test_harness")]
            TransactionServiceRequest::MineTransaction(tx_id) => {
                self.mine_transaction(tx_id).await?;
                Ok(TransactionServiceResponse::TransactionMined)
            },
        }
    }

    async fn handle_message_event(&mut self, message_event: MessagingEvent) -> Result<(), TransactionServiceError> {
        let (message_tag, result) = match message_event {
            MessagingEvent::MessageSent(message_tag) => (message_tag, true),
            MessagingEvent::SendMessageFailed(outbound_message, _reason) => (outbound_message.tag, false),
            _ => return Ok(()),
        };
        match self.pending_outbound_message_results.remove(&message_tag) {
            None => (),
            Some(outbound_tx) => {
                // If the message was successfully sent then add it to the pending transaction list
                if result {
                    info!(
                        target: LOG_TARGET,
                        "Pending Outbound Transaction TxId: {:?} was successfully sent with Message Tag: {:?}",
                        outbound_tx.tx_id,
                        message_tag
                    );
                } else {
                    error!(
                        target: LOG_TARGET,
                        "Pending Outbound Transaction TxId: {:?} with Message Tag {:?} could not be sent",
                        message_tag,
                        outbound_tx.tx_id
                    );

                    self.db.remove_pending_outbound_transaction(outbound_tx.tx_id).await?;
                }

                let _ = self
                    .event_publisher
                    .send(TransactionEvent::TransactionSendResult(outbound_tx.tx_id, result))
                    .await;
            },
        }

        Ok(())
    }

    /// Sends a new transaction to a recipient
    /// # Arguments
    /// 'dest_pubkey': The Comms pubkey of the recipient node
    /// 'amount': The amount of Tari to send to the recipient
    /// 'fee_per_gram': The amount of fee per transaction gram to be included in transaction
    pub async fn send_transaction(
        &mut self,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        fee_per_gram: MicroTari,
        message: String,
        discovery_process_futures: &mut FuturesUnordered<
            BoxFuture<'static, Result<(MessageTag, OutboundTransaction), TransactionServiceError>>,
        >,
    ) -> Result<(), TransactionServiceError>
    {
        let mut sender_protocol = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None, message.clone())
            .await?;

        if !sender_protocol.is_single_round_message_ready() {
            return Err(TransactionServiceError::InvalidStateError);
        }

        let msg = sender_protocol.build_single_round_message()?;
        let tx_id = msg.tx_id;
        let proto_message = proto::TransactionSenderMessage::single(msg.into());

        match self
            .outbound_message_service
            .send_direct(
                dest_pubkey.clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message),
            )
            .await?
        {
            SendMessageResponse::Queued(tags) => match tags.len() {
                0 => error!(
                    target: LOG_TARGET,
                    "Queuing Transaction TX_ID: {} for send was unsuccessful and no message was sent", tx_id
                ),
                1 => {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send successfully queued for send with Message Tag: {:?}",
                        tx_id,
                        tags[0],
                    );

                    let outbound_tx = OutboundTransaction {
                        tx_id,
                        destination_public_key: dest_pubkey.clone(),
                        amount,
                        fee: sender_protocol.get_fee_amount()?,
                        sender_protocol,
                        status: TransactionStatus::Pending,
                        message,
                        timestamp: Utc::now().naive_utc(),
                    };

                    self.db
                        .add_pending_outbound_transaction(outbound_tx.tx_id, outbound_tx.clone())
                        .await?;

                    self.pending_outbound_message_results
                        .insert(tags[0].clone(), outbound_tx);
                },
                _ => error!(
                    target: LOG_TARGET,
                    "Send process for TX_ID: {} was unsuccessful due to more than 1 MessageTag being returned", tx_id
                ),
            },
            SendMessageResponse::Failed => return Err(TransactionServiceError::OutboundSendFailure),
            SendMessageResponse::PendingDiscovery(r) => {
                // The sending of the message resulted in a long running Discovery process being performed by the Comms
                // layer. This can take minutes so we will spawn a task to wait for the result and then act
                // appropriately on it
                let tx_id_clone = tx_id;
                let outbound_tx_clone = OutboundTransaction {
                    tx_id,
                    destination_public_key: dest_pubkey.clone(),
                    amount,
                    fee: sender_protocol.get_fee_amount()?,
                    sender_protocol: sender_protocol.clone(),
                    status: TransactionStatus::Pending,
                    message: message.clone(),
                    timestamp: Utc::now().naive_utc(),
                };

                info!(
                    target: LOG_TARGET,
                    "Send Transaction request for TxID: {:?} to recipient with public_key {} requires that a \
                     Discovery Process be conducted",
                    tx_id,
                    dest_pubkey
                );

                let discovery_future = async move {
                    transaction_send_discovery_process_completion(r, tx_id_clone, outbound_tx_clone).await
                };
                discovery_process_futures.push(discovery_future.boxed());

                return Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id));
            },
        }

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} queued to be sent to {}", tx_id, dest_pubkey
        );

        Ok(())
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_recipient_reply(
        &mut self,
        source_pubkey: CommsPublicKey,
        recipient_reply: proto::RecipientSignedMessage,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let mut outbound_tx = self.db.get_pending_outbound_transaction(recipient_reply.tx_id).await?;

        let tx_id = recipient_reply.tx_id;
        if !outbound_tx.sender_protocol.check_tx_id(tx_id.clone()) ||
            !outbound_tx.sender_protocol.is_collecting_single_signature()
        {
            return Err(TransactionServiceError::InvalidStateError);
        }

        outbound_tx
            .sender_protocol
            .add_single_recipient_info(recipient_reply, &self.factories.range_proof)?;
        outbound_tx
            .sender_protocol
            .finalize(KernelFeatures::empty(), &self.factories)?;
        let tx = outbound_tx.sender_protocol.get_transaction()?;

        let completed_transaction = CompletedTransaction {
            tx_id,
            source_public_key: self.node_identity.public_key().clone(),
            destination_public_key: outbound_tx.destination_public_key,
            amount: outbound_tx.amount,
            fee: outbound_tx.fee,
            transaction: tx.clone(),
            status: TransactionStatus::Completed,
            message: outbound_tx.message.clone(),
            timestamp: Utc::now().naive_utc(),
        };
        self.db
            .complete_outbound_transaction(tx_id.clone(), completed_transaction.clone())
            .await?;
        info!(
            target: LOG_TARGET,
            "Transaction Recipient Reply for TX_ID = {} received", tx_id,
        );

        let finalized_transaction_message = proto::TransactionFinalizedMessage {
            tx_id,
            transaction: Some(tx.clone().into()),
        };

        self.outbound_message_service
            .send_direct(
                source_pubkey.clone(),
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::TransactionFinalized, finalized_transaction_message),
            )
            .await?;

        // Logging this error here instead of propogating it up to the select! catchall which generates the Error Event.
        let _ = self
            .broadcast_completed_transaction_to_mempool(tx_id, broadcast_timeout_futures)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting completed transaction to mempool: {:?}", e
                );
                e
            });

        self.event_publisher
            .send(TransactionEvent::ReceivedTransactionReply(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

        Ok(())
    }

    /// Accept a new transaction from a sender by handling a public SenderMessage. The reply is generated and sent.
    /// # Arguments
    /// 'source_pubkey' - The pubkey from which the message was sent and to which the reply will be sent.
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    pub async fn accept_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        sender_message: proto::TransactionSenderMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let sender_message: TransactionSenderMessage = sender_message
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = sender_message.clone() {
            let amount = data.amount;

            let spending_key = self
                .output_manager_service
                .get_recipient_spending_key(data.tx_id, data.amount)
                .await?;
            let nonce = PrivateKey::random(&mut OsRng);

            let rtp = ReceiverTransactionProtocol::new(
                sender_message,
                nonce,
                spending_key,
                OutputFeatures::default(),
                &self.factories,
            );
            let recipient_reply = rtp.get_signed_data()?.clone();

            // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed
            // transactions
            if self.db.transaction_exists(recipient_reply.tx_id).await? {
                return Err(TransactionServiceError::RepeatedMessageError);
            }

            let tx_id = recipient_reply.tx_id;
            let proto_message: proto::RecipientSignedMessage = recipient_reply.into();
            self.outbound_message_service
                .send_direct(
                    source_pubkey.clone(),
                    OutboundEncryption::EncryptForPeer,
                    OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, proto_message),
                )
                .await?;

            // Otherwise add it to our pending transaction list and return reply
            let inbound_transaction = InboundTransaction {
                tx_id,
                source_public_key: source_pubkey.clone(),
                amount,
                receiver_protocol: rtp.clone(),
                status: TransactionStatus::Pending,
                message: data.message,
                timestamp: Utc::now().naive_utc(),
            };
            self.db
                .add_pending_inbound_transaction(tx_id, inbound_transaction.clone())
                .await?;

            info!(
                target: LOG_TARGET,
                "Transaction with TX_ID = {} received from {}. Reply Sent", tx_id, source_pubkey,
            );

            self.event_publisher
                .send(TransactionEvent::ReceivedTransaction(tx_id))
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
        }
        Ok(())
    }

    /// Accept a new transaction from a sender by handling a public SenderMessage. The reply is generated and sent.
    /// # Arguments
    /// 'source_pubkey' - The pubkey from which the message was sent and to which the reply will be sent.
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    pub async fn accept_finalized_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        finalized_transaction: proto::TransactionFinalizedMessage,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let tx_id = finalized_transaction.tx_id;
        let transaction: Transaction = finalized_transaction
            .transaction
            .ok_or_else(|| {
                TransactionServiceError::InvalidMessageError(
                    "Finalized Transaction missing Transaction field".to_string(),
                )
            })?
            .try_into()
            .map_err(|_| {
                TransactionServiceError::InvalidMessageError(
                    "Cannot convert Transaction field from TransactionFinalized message".to_string(),
                )
            })?;
        info!(
            target: LOG_TARGET,
            "Finalized Transaction with TX_ID = {} received from {}",
            tx_id,
            source_pubkey.clone()
        );

        let inbound_tx = self
            .db
            .get_pending_inbound_transaction(tx_id.clone())
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Finalized transaction TxId does not exist in Pending Inbound Transactions"
                );
                e
            })?;

        if inbound_tx.source_public_key != source_pubkey {
            error!(
                target: LOG_TARGET,
                "Finalized transaction Source Public Key does not correspond to stored value"
            );
            return Err(TransactionServiceError::InvalidSourcePublicKey);
        }

        let rtp_output = match inbound_tx.receiver_protocol.state {
            RecipientState::Finalized(s) => s.output.clone(),
            RecipientState::Failed(_) => return Err(TransactionServiceError::InvalidStateError),
        };

        let finalized_outputs = transaction.body.outputs();

        if finalized_outputs.iter().find(|o| o == &&rtp_output).is_none() {
            error!(
                target: LOG_TARGET,
                "Finalized transaction not contain the Receiver's output"
            );
            return Err(TransactionServiceError::ReceiverOutputNotFound);
        }

        let completed_transaction = CompletedTransaction {
            tx_id,
            source_public_key: source_pubkey.clone(),
            destination_public_key: self.node_identity.public_key().clone(),
            amount: inbound_tx.amount,
            fee: transaction.body.get_total_fee(),
            transaction: transaction.clone(),
            status: TransactionStatus::Completed,
            message: inbound_tx.message.clone(),
            timestamp: inbound_tx.timestamp,
        };

        self.db
            .complete_inbound_transaction(tx_id.clone(), completed_transaction.clone())
            .await?;

        self.event_publisher
            .send(TransactionEvent::ReceivedFinalizedTransaction(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

        info!(
            target: LOG_TARGET,
            "Inbound Transaction with TX_ID = {} from {} moved to Completed Transactions",
            tx_id,
            source_pubkey.clone()
        );

        // Logging this error here instead of propogating it up to the select! catchall which generates the Error Event.
        let _ = self
            .broadcast_completed_transaction_to_mempool(tx_id, broadcast_timeout_futures)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting completed transaction to mempool: {:?}", e
                );
                e
            });

        Ok(())
    }

    /// Request a tx_id and spending_key for a coinbase output to be mined
    pub async fn request_coinbase_key(
        &mut self,
        amount: MicroTari,
        maturity_height: u64,
    ) -> Result<PendingCoinbaseSpendingKey, TransactionServiceError>
    {
        let tx_id: TxId = OsRng.next_u64();

        let spending_key = self
            .output_manager_service
            .get_coinbase_spending_key(tx_id.clone(), amount.clone(), maturity_height)
            .await?;

        self.db
            .add_pending_coinbase_transaction(tx_id.clone(), PendingCoinbaseTransaction {
                tx_id,
                amount,
                commitment: self.factories.commitment.commit_value(&spending_key, u64::from(amount)),
                timestamp: Utc::now().naive_utc(),
            })
            .await?;

        Ok(PendingCoinbaseSpendingKey { tx_id, spending_key })
    }

    /// Once the miner has constructed the completed Coinbase transaction they will submit it to the Transaction Service
    /// which will monitor the chain to see when it has been mined.
    pub async fn submit_completed_coinbase_transaction(
        &mut self,
        tx_id: TxId,
        completed_transaction: Transaction,
    ) -> Result<(), TransactionServiceError>
    {
        let coinbase_tx = self
            .db
            .get_pending_coinbase_transaction(tx_id.clone())
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Finalized coinbase transaction TxId does not exist in Pending Inbound Transactions"
                );
                e
            })?;

        if !completed_transaction.body.inputs().is_empty() ||
            completed_transaction.body.outputs().len() != 1 ||
            completed_transaction.body.kernels().len() != 1
        {
            error!(
                target: LOG_TARGET,
                "Provided Completed Transaction for Coinbase Transaction does not contain just a single output"
            );
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        if coinbase_tx.commitment != completed_transaction.body.outputs()[0].commitment ||
            completed_transaction.body.outputs()[0].features.flags != OutputFlags::COINBASE_OUTPUT ||
            completed_transaction.body.kernels()[0].features != KernelFeatures::COINBASE_KERNEL
        {
            error!(
                target: LOG_TARGET,
                "Provided Completed Transaction commitment for Coinbase Transaction does not match the stored \
                 commitment for this TxId"
            );
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        self.db
            .complete_coinbase_transaction(tx_id, CompletedTransaction {
                tx_id,
                source_public_key: self.node_identity.public_key().clone(),
                destination_public_key: self.node_identity.public_key().clone(),
                amount: coinbase_tx.amount,
                fee: MicroTari::from(0),
                transaction: completed_transaction,
                status: TransactionStatus::Completed,
                message: "Coinbase Transaction".to_string(),
                timestamp: Utc::now().naive_utc(),
            })
            .await?;

        Ok(())
    }

    /// If a specific coinbase transaction will not be mined then the Miner can cancel it
    pub async fn cancel_pending_coinbase_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let _ = self
            .db
            .get_pending_coinbase_transaction(tx_id.clone())
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Finalized coinbase transaction TxId does not exist in Pending Inbound Transactions"
                );
                e
            })?;

        self.output_manager_service.cancel_transaction(tx_id).await?;

        self.db.cancel_coinbase_transaction(tx_id).await?;

        Ok(())
    }

    pub async fn get_pending_inbound_transactions(
        &self,
    ) -> Result<HashMap<u64, InboundTransaction>, TransactionServiceError> {
        Ok(self.db.get_pending_inbound_transactions().await?)
    }

    pub async fn get_pending_outbound_transactions(
        &self,
    ) -> Result<HashMap<u64, OutboundTransaction>, TransactionServiceError> {
        Ok(self.db.get_pending_outbound_transactions().await?)
    }

    pub async fn get_completed_transactions(
        &self,
    ) -> Result<HashMap<u64, CompletedTransaction>, TransactionServiceError> {
        Ok(self.db.get_completed_transactions().await?)
    }

    /// Add a base node public key to the list that will be used to broadcast transactions and monitor the base chain
    /// for the presence of spendable outputs. If this is the first time the base node public key is set do the initial
    /// mempool broadcast
    async fn set_base_node_public_key(
        &mut self,
        base_node_public_key: CommsPublicKey,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let startup_broadcast = self.base_node_public_key.is_none();

        self.base_node_public_key = Some(base_node_public_key);

        if startup_broadcast {
            let _ = self
                .broadcast_all_completed_transactions_to_mempool(broadcast_timeout_futures)
                .await
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Error broadcasting all completed transactions: {:?}", resp
                    );
                    Err(resp)
                });

            let _ = self
                .monitor_all_completed_transactions_for_mining(mined_request_timeout_futures)
                .await
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Error querying base_node for all completed transactions: {:?}", resp
                    );
                    Err(resp)
                });
        }
        Ok(())
    }

    /// Broadcast the specified Completed Transaction to the Base Node. After sending the transaction send a Mempool
    /// request to check that the transaction has been received. The final step is to set a timeout future to check on
    /// the status of the transaction in the future.
    pub async fn broadcast_completed_transaction_to_mempool(
        &mut self,
        tx_id: TxId,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id.clone()).await?;

        if completed_tx.status != TransactionStatus::Completed || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                info!(
                    target: LOG_TARGET,
                    "Attempting to Broadcast TxId: {} to Mempool", completed_tx.tx_id
                );
                // Broadcast Transaction
                self.outbound_message_service
                    .send_direct(
                        pk.clone(),
                        OutboundEncryption::EncryptForPeer,
                        OutboundDomainMessage::new(
                            TariMessageType::NewTransaction,
                            TransactionProto::types::Transaction::from(completed_tx.transaction.clone()),
                        ),
                    )
                    .await?;

                // Send  Mempool Request
                let tx_excess_sig = completed_tx.transaction.body.kernels()[0].excess_sig.clone();
                let mempool_request = MempoolProto::MempoolServiceRequest {
                    request_key: completed_tx.tx_id,
                    request: Some(MempoolProto::mempool_service_request::Request::GetTxStateWithExcessSig(
                        tx_excess_sig.into(),
                    )),
                };
                self.outbound_message_service
                    .send_direct(
                        pk.clone(),
                        OutboundEncryption::EncryptForPeer,
                        OutboundDomainMessage::new(TariMessageType::MempoolRequest, mempool_request),
                    )
                    .await?;
                // Start Timeout
                let state_timeout = StateDelay::new(self.config.mempool_broadcast_timeout.clone(), completed_tx.tx_id);

                broadcast_timeout_futures.push(state_timeout.delay().boxed());
            },
        }

        Ok(())
    }

    /// Go through all completed transactions that have not yet been broadcast and broadcast all of them to the base
    /// node followed by mempool requests to confirm that they have been received
    async fn broadcast_all_completed_transactions_to_mempool(
        &mut self,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Completed {
                self.broadcast_completed_transaction_to_mempool(completed_tx.tx_id.clone(), broadcast_timeout_futures)
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle the timeout of a pending transaction broadcast request. This will check if the transaction's status has
    /// been updated by received MempoolRepsonse during the course of this timeout. If it has not been updated the
    /// transaction is broadcast again
    pub async fn handle_mempool_broadcast_timeout(
        &mut self,
        tx_id: TxId,
        broadcast_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id.clone()).await?;

        if completed_tx.status == TransactionStatus::Completed {
            info!(target: LOG_TARGET, "Mempool broadcast timed out for TX_ID: {}", tx_id);

            self.broadcast_completed_transaction_to_mempool(tx_id, broadcast_timeout_futures)
                .await?;

            self.event_publisher
                .send(TransactionEvent::MempoolBroadcastTimedOut(tx_id))
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
        }

        Ok(())
    }

    /// Handle an incoming mempool response message
    pub async fn handle_mempool_response(
        &mut self,
        response: MempoolProto::MempoolServiceResponse,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let response = MempoolServiceResponse::try_from(response).unwrap();
        let tx_id = response.request_key;
        match response.response {
            MempoolResponse::Stats(_) => Err(TransactionServiceError::InvalidMessageError(
                "Mempool Response of invalid type".to_string(),
            )),
            MempoolResponse::TxStorage(ts) => match ts {
                TxStorageResponse::NotStored => {
                    debug!(
                        target: LOG_TARGET,
                        "Mempool response received for TxId: {:?} but requested transaction was not found in mempool",
                        tx_id
                    );
                    Ok(())
                },
                // Any other variant of this enum means the transaction has been received by the base_node and is in one
                // of the various mempools
                _ => {
                    let completed_tx = self.db.get_completed_transaction(response.request_key.clone()).await?;
                    // If this transaction is still in the Completed State it should be upgraded to the Broadcast state
                    if completed_tx.status == TransactionStatus::Completed {
                        info!(
                            target: LOG_TARGET,
                            "Completed Transaction with TxId: {} detected as Broadcast to Base Node Mempool", tx_id
                        );
                        self.db.broadcast_completed_transaction(tx_id.clone()).await?;
                        // Start monitoring the base node to see if this Tx has been mined
                        self.send_transaction_mined_request(tx_id.clone(), mined_request_timeout_futures)
                            .await?;

                        self.event_publisher
                            .send(TransactionEvent::TransactionBroadcast(tx_id))
                            .await
                            .map_err(|_| TransactionServiceError::EventStreamError)?;
                    }

                    Ok(())
                },
            },
        }
    }

    /// Send a request to the Base Node to see if the specified transaction has been mined yet. This function will send
    /// the request and store a timeout future to check in on the status of the transaction in the future.
    async fn send_transaction_mined_request(
        &mut self,
        tx_id: TxId,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id.clone()).await?;

        if completed_tx.status != TransactionStatus::Broadcast || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let mut hashes = Vec::new();
                for o in completed_tx.transaction.body.outputs() {
                    hashes.push(o.hash());
                }

                info!(
                    target: LOG_TARGET,
                    "Sending Transaction Mined? request for TxId: {} to Base Node with {} outputs",
                    tx_id,
                    hashes.len(),
                );

                let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs { outputs: hashes });
                let service_request = BaseNodeProto::BaseNodeServiceRequest {
                    request_key: tx_id,
                    request: Some(request),
                };
                self.outbound_message_service
                    .send_direct(
                        pk.clone(),
                        OutboundEncryption::EncryptForPeer,
                        OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                    )
                    .await?;
                // Start Timeout
                let state_timeout =
                    StateDelay::new(self.config.base_node_mined_timeout.clone(), completed_tx.tx_id.clone());

                mined_request_timeout_futures.push(state_timeout.delay().boxed());
            },
        }
        Ok(())
    }

    /// Handle the timeout of a pending transaction mined? request. This will check if the transaction's status has
    /// been updated by received BaseNodeRepsonse during the course of this timeout. If it has not been updated the
    /// transaction is broadcast again
    pub async fn handle_transaction_mined_request_timeout(
        &mut self,
        tx_id: TxId,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id.clone()).await?;

        if completed_tx.status == TransactionStatus::Broadcast {
            info!(
                target: LOG_TARGET,
                "Transaction Mined? request timed out for TX_ID: {}", tx_id
            );

            self.send_transaction_mined_request(tx_id, mined_request_timeout_futures)
                .await?;

            self.event_publisher
                .send(TransactionEvent::TransactionMinedRequestTimedOut(tx_id))
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
        }

        Ok(())
    }

    /// Handle an incoming basenode response message
    pub async fn handle_base_node_response(
        &mut self,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<(), TransactionServiceError>
    {
        let tx_id = response.request_key;
        let response = match response.response {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => Ok(outputs.outputs),
            _ => Err(TransactionServiceError::InvalidStateError),
        }?;

        let completed_tx = match self.db.get_completed_transaction(tx_id.clone()).await {
            Ok(tx) => tx,
            Err(_) => {
                debug!(
                    target: LOG_TARGET,
                    "Base Node Response received with unexpected key {:?}", tx_id
                );
                return Ok(());
            },
        };
        // If this transaction is still in the Broadcast State it should be upgraded to the Mined state
        if completed_tx.status == TransactionStatus::Broadcast {
            // Confirm that all outputs were reported as mined for the transaction
            if response.len() != completed_tx.transaction.body.outputs().len() {
                info!(
                    target: LOG_TARGET,
                    "Base node response received for TxId: {:?} with {} outputs but expected {} outputs, Tx not mined \
                     yet",
                    tx_id,
                    response.len(),
                    completed_tx.transaction.body.outputs().len(),
                );
            } else {
                let mut check = true;

                for output in response.iter() {
                    let transaction_output = TransactionOutput::try_from(output.clone())
                        .map_err(TransactionServiceError::ConversionError)?;

                    check = check &&
                        completed_tx
                            .transaction
                            .body
                            .outputs()
                            .iter()
                            .any(|item| item == &transaction_output);
                }
                // If all outputs are present then mark this transaction as mined.
                if check {
                    self.output_manager_service
                        .confirm_transaction(
                            tx_id.clone(),
                            completed_tx.transaction.body.inputs().clone(),
                            completed_tx.transaction.body.outputs().clone(),
                        )
                        .await?;

                    self.db.mine_completed_transaction(tx_id).await?;

                    self.event_publisher
                        .send(TransactionEvent::TransactionMined(tx_id))
                        .await
                        .map_err(|_| TransactionServiceError::EventStreamError)?;

                    info!("Transaction (TxId: {:?}) detected as mined on the Base Layer", tx_id);
                }
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Base node response received for TxId: {:?} but this transaction is not in the Broadcast state", tx_id
            );
        }

        Ok(())
    }

    /// Go through all completed transactions that have  been broadcast and start querying the base_node to see if they
    /// have been mined
    async fn monitor_all_completed_transactions_for_mining(
        &mut self,
        mined_request_timeout_futures: &mut FuturesUnordered<BoxFuture<'static, TxId>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Broadcast {
                self.send_transaction_mined_request(completed_tx.tx_id.clone(), mined_request_timeout_futures)
                    .await?;
            }
        }

        Ok(())
    }

    /// Add a completed transaction to the Transaction Manager to record directly importing a spendable UTXO.
    pub async fn add_utxo_import_transaction(
        &mut self,
        value: MicroTari,
        source_public_key: CommsPublicKey,
        message: String,
    ) -> Result<TxId, TransactionServiceError>
    {
        let tx_id = OsRng.next_u64();
        self.db
            .add_utxo_import_transaction(
                tx_id.clone(),
                value,
                source_public_key,
                self.node_identity.public_key().clone(),
                message,
            )
            .await?;
        Ok(tx_id)
    }

    /// This function is only available for testing by the client of LibWallet. It simulates a receiver accepting and
    /// replying to a Pending Outbound Transaction. This results in that transaction being "completed" and it's status
    /// set to `Broadcast` which indicated it is in a base_layer mempool.
    #[cfg(feature = "test_harness")]
    pub async fn complete_pending_outbound_transaction(
        &mut self,
        completed_tx: CompletedTransaction,
    ) -> Result<(), TransactionServiceError>
    {
        self.db
            .complete_outbound_transaction(completed_tx.tx_id.clone(), completed_tx.clone())
            .await?;
        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is broadcast in a mempool on the base layer. The function will update the status of
    /// the completed transaction.
    #[cfg(feature = "test_harness")]
    pub async fn broadcast_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        completed_txs.get(&tx_id.clone()).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to broadcast.".to_string())
        })?;

        self.db.broadcast_completed_transaction(tx_id).await?;

        self.event_publisher
            .send(TransactionEvent::TransactionBroadcast(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is detected as mined on the base layer. The function will update the status of the
    /// completed transaction AND complete the transaction on the Output Manager Service which will update the status of
    /// the outputs
    #[cfg(feature = "test_harness")]
    pub async fn mine_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        let _found_tx = completed_txs.get(&tx_id.clone()).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to mine.".to_string())
        })?;

        let pending_tx_outputs = self.output_manager_service.get_pending_transactions().await?;
        let pending_tx = pending_tx_outputs.get(&tx_id.clone()).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Pending TX to complete.".to_string())
        })?;

        self.output_manager_service
            .confirm_transaction(
                tx_id.clone(),
                pending_tx
                    .outputs_to_be_spent
                    .iter()
                    .map(|o| {
                        o.as_transaction_input(&self.factories.commitment, OutputFeatures::default())
                            .clone()
                    })
                    .collect(),
                pending_tx
                    .outputs_to_be_received
                    .iter()
                    .map(|o| {
                        o.as_transaction_output(&self.factories)
                            .expect("Failed to convert to Transaction Output")
                            .clone()
                    })
                    .collect(),
            )
            .await?;

        self.db.mine_completed_transaction(tx_id).await?;

        self.event_publisher
            .send(TransactionEvent::TransactionMined(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function simulates an external
    /// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
    #[cfg(feature = "test_harness")]
    pub async fn receive_test_transaction(
        &mut self,
        tx_id: TxId,
        amount: MicroTari,
        source_public_key: CommsPublicKey,
    ) -> Result<(), TransactionServiceError>
    {
        use crate::output_manager_service::{
            config::OutputManagerServiceConfig,
            service::OutputManagerService,
            storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
        };
        use futures::{channel::mpsc, stream};
        use tari_broadcast_channel::bounded;

        let (_sender, receiver) = reply_channel::unbounded();
        let (tx, _rx) = mpsc::channel(20);
        let (oms_event_publisher, _oms_event_subscriber) = bounded(100);

        let mut fake_oms = OutputManagerService::new(
            OutputManagerServiceConfig::default(),
            OutboundMessageRequester::new(tx),
            receiver,
            stream::empty(),
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
            oms_event_publisher,
            self.factories.clone(),
        )
        .await?;

        use crate::testnet_utils::make_input;
        let (_ti, uo) = make_input(&mut OsRng, amount + 1 * T, &self.factories);

        fake_oms.add_output(uo).await?;

        let mut stp = fake_oms
            .prepare_transaction_to_send(amount, MicroTari::from(100), None, "".to_string())
            .await?;

        let msg = stp.build_single_round_message()?;
        let proto_msg = proto::TransactionSenderMessage::single(msg.into());
        let sender_message: TransactionSenderMessage = proto_msg
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let spending_key = self
            .output_manager_service
            .get_recipient_spending_key(tx_id.clone(), amount.clone())
            .await?;
        let nonce = PrivateKey::random(&mut OsRng);
        let rtp = ReceiverTransactionProtocol::new(
            sender_message,
            nonce,
            spending_key.clone(),
            OutputFeatures::default(),
            &self.factories,
        );

        let inbound_transaction = InboundTransaction {
            tx_id,
            source_public_key,
            amount,
            receiver_protocol: rtp,
            status: TransactionStatus::Pending,
            message: "".to_string(),
            timestamp: Utc::now().naive_utc(),
        };

        self.db
            .add_pending_inbound_transaction(tx_id.clone(), inbound_transaction.clone())
            .await?;

        self.event_publisher
            .send(TransactionEvent::ReceivedTransaction(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;

        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function simulates an external
    /// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
    #[cfg(feature = "test_harness")]
    pub async fn finalize_received_test_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let inbound_txs = self.db.get_pending_inbound_transactions().await?;

        let found_tx = inbound_txs.get(&tx_id.clone()).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Pending Inbound TX to finalize.".to_string())
        })?;

        let completed_transaction = CompletedTransaction {
            tx_id,
            source_public_key: found_tx.source_public_key.clone(),
            destination_public_key: self.node_identity.public_key().clone(),
            amount: found_tx.amount,
            fee: MicroTari::from(2000), // a placeholder fee for this test function
            transaction: Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
            status: TransactionStatus::Completed,
            message: found_tx.message.clone(),
            timestamp: found_tx.timestamp,
        };

        self.db
            .complete_inbound_transaction(tx_id.clone(), completed_transaction.clone())
            .await?;
        self.event_publisher
            .send(TransactionEvent::ReceivedFinalizedTransaction(tx_id))
            .await
            .map_err(|_| TransactionServiceError::EventStreamError)?;
        Ok(())
    }
}

// Asynchronous Tasks

async fn transaction_send_discovery_process_completion(
    response_channel: oneshot::Receiver<SendMessageResponse>,
    tx_id: TxId,
    outbound_tx: OutboundTransaction,
) -> Result<(MessageTag, OutboundTransaction), TransactionServiceError>
{
    let mut message_tag: Option<MessageTag> = None;
    match response_channel.await {
        Ok(response) => match response {
            SendMessageResponse::Queued(tags) => match tags.len() {
                0 => error!(
                    target: LOG_TARGET,
                    "Send Discovery process for TX_ID: {} was unsuccessful and no message was sent", tx_id
                ),
                1 => {
                    message_tag = Some(tags[0].clone());

                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send Discovery process successful with Message Tag: {:?}",
                        tx_id,
                        message_tag,
                    );
                },
                _ => error!(
                    target: LOG_TARGET,
                    "Send Discovery process for TX_ID: {} was unsuccessful due to more than 1 MessageTag being \
                     returned",
                    tx_id
                ),
            },
            _ => {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) Send Discovery process failed", tx_id
                );
            },
        },
        Err(_) => {
            error!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) Send Response One-shot channel dropped", tx_id
            );
        },
    }

    return if let Some(mt) = message_tag {
        let updated_outbound_tx = OutboundTransaction {
            timestamp: Utc::now().naive_utc(),
            ..outbound_tx.clone()
        };
        Ok((mt, updated_outbound_tx))
    } else {
        Err(TransactionServiceError::DiscoveryProcessFailed(tx_id))
    };
}

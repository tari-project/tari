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
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::{TransactionEvent, TransactionEventSender, TransactionServiceRequest, TransactionServiceResponse},
        protocols::{
            transaction_broadcast_protocol::TransactionBroadcastProtocol,
            transaction_chain_monitoring_protocol::TransactionChainMonitoringProtocol,
            transaction_send_protocol::{TransactionProtocolStage, TransactionSendProtocol},
        },
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
};
use chrono::Utc;
use futures::{
    channel::{mpsc, mpsc::Sender, oneshot},
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
use tari_comms::{
    peer_manager::{NodeId, NodeIdentity},
    types::CommsPublicKey,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester},
};
#[cfg(feature = "test_harness")]
use tari_core::transactions::{tari_amount::uT, types::BlindingFactor};
use tari_core::{
    base_node::proto::base_node as BaseNodeProto,
    mempool::{proto::mempool as MempoolProto, service::MempoolServiceResponse},
    transactions::{
        tari_amount::MicroTari,
        transaction::{KernelFeatures, OutputFeatures, OutputFlags, Transaction},
        transaction_protocol::{
            proto,
            recipient::{RecipientSignedMessage, RecipientState},
            sender::TransactionSenderMessage,
        },
        types::{CryptoFactories, PrivateKey},
        ReceiverTransactionProtocol,
    },
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::Receiver};
use tokio::task::JoinHandle;

const LOG_TARGET: &str = "wallet::transaction_service::service";

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
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    mempool_response_stream: Option<MReplyStream>,
    base_node_response_stream: Option<BNResponseStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: TransactionEventSender,
    node_identity: Arc<NodeIdentity>,
    factories: CryptoFactories,
    base_node_public_key: Option<CommsPublicKey>,
    service_resources: TransactionServiceResources<TBackend>,
    pending_transaction_reply_senders: HashMap<TxId, Sender<(CommsPublicKey, RecipientSignedMessage)>>,
    mempool_response_senders: HashMap<u64, Sender<MempoolServiceResponse>>,
    base_node_response_senders: HashMap<u64, Sender<BaseNodeProto::BaseNodeServiceResponse>>,
    send_transaction_cancellation_senders: HashMap<u64, oneshot::Sender<()>>,
}

#[allow(clippy::too_many_arguments)]
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
        event_publisher: TransactionEventSender,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
    ) -> Self
    {
        // Collect the resources that all protocols will need so that they can be neatly cloned as the protocols are
        // spawned.
        let service_resources = TransactionServiceResources {
            db: db.clone(),
            output_manager_service: output_manager_service.clone(),
            outbound_message_service: outbound_message_service.clone(),
            event_publisher: event_publisher.clone(),
            node_identity: node_identity.clone(),
            factories: factories.clone(),
        };
        TransactionService {
            config,
            db,
            outbound_message_service,
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
            service_resources,
            pending_transaction_reply_senders: HashMap::new(),
            mempool_response_senders: HashMap::new(),
            base_node_response_senders: HashMap::new(),
            send_transaction_cancellation_senders: HashMap::new(),
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

        let mut send_transaction_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut transaction_broadcast_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        let mut transaction_chain_monitoring_protocol_handles: FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        > = FuturesUnordered::new();

        info!(target: LOG_TARGET, "Transaction Service started");
        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request, &mut send_transaction_protocol_handles,  &mut transaction_broadcast_protocol_handles, &mut transaction_chain_monitoring_protocol_handles).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = transaction_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Transaction Message");
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result  = self.accept_transaction(origin_public_key, inner_msg).await;

                    match result {
                        Err(TransactionServiceError::RepeatedMessageError) => {
                            trace!(target: LOG_TARGET, "A repeated Transaction message was received");
                        }
                        Err(e) => {
                            error!(target: LOG_TARGET, "Failed to handle incoming Transaction message: {:?} for NodeID: {}", e, self.node_identity.node_id().short_str());
                            let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error(format!("Error handling Transaction Sender message: {:?}", e).to_string())));
                        }
                        _ => (),
                    }
                },
                 // Incoming messages from the Comms layer
                msg = transaction_reply_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Transaction Reply Message");
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result = self.accept_recipient_reply(origin_public_key, inner_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming Transaction Reply message: {:?} for NodeId: {}", err, self.node_identity.node_id().short_str());
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling Transaction Recipient Reply message".to_string(),)));
                    }
                },
               // Incoming messages from the Comms layer
                msg = transaction_finalized_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Transaction Finalized Message");
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let result = self.accept_finalized_transaction(origin_public_key, inner_msg, &mut transaction_broadcast_protocol_handles).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming Transaction Finalized message: {:?} for NodeID: {}", err , self.node_identity.node_id().short_str());
                        Err(err)
                    });

                    if result.is_err() {
                        let _ = self.event_publisher.send(Arc::new(TransactionEvent::Error("Error handling Transaction Finalized message".to_string(),)));
                    }
                },
                // Incoming messages from the Comms layer
                msg = mempool_response_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Mempool Response");
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let _ = self.handle_mempool_response(inner_msg).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling mempool service response: {:?}", resp);
                        Err(resp)
                    });
                }
                // Incoming messages from the Comms layer
                msg = base_node_response_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Base Node Response");
                    let (origin_public_key, inner_msg) = msg.into_origin_and_inner();
                    let _ = self.handle_base_node_response(inner_msg).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling base node service response from {}: {:?} for NodeID: {}", origin_public_key, resp, self.node_identity.node_id().short_str());
                        Err(resp)
                    });
                }
                join_result = send_transaction_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Send Protocol for Transaction has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_send_transaction_protocol(join_result_inner, &mut transaction_broadcast_protocol_handles).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Join Handle: {:?}", e),
                    };
                }
                join_result = transaction_broadcast_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Transaction Broadcast protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_broadcast_protocol(join_result_inner, &mut transaction_chain_monitoring_protocol_handles).await,
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Join Handle: {:?}", e),
                    };
                }
                join_result = transaction_chain_monitoring_protocol_handles.select_next_some() => {
                    trace!(target: LOG_TARGET, "Transaction chain monitoring protocol has ended with result {:?}", join_result);
                    match join_result {
                        Ok(join_result_inner) => self.complete_transaction_chain_monitoring_protocol(join_result_inner),
                        Err(e) => error!(target: LOG_TARGET, "Error resolving Join Handle: {:?}", e),
                    };
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
        send_transaction_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
        chain_monitoring_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Handling Service Request: {}", request);
        match request {
            TransactionServiceRequest::SendTransaction((dest_pubkey, amount, fee_per_gram, message)) => self
                .send_transaction(
                    dest_pubkey,
                    amount,
                    fee_per_gram,
                    message,
                    send_transaction_join_handles,
                )
                .await
                .map(TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::CancelTransaction(tx_id) => self
                .cancel_transaction(tx_id)
                .await
                .map(|_| TransactionServiceResponse::TransactionCancelled),
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
                .set_base_node_public_key(
                    public_key,
                    transaction_broadcast_join_handles,
                    chain_monitoring_join_handles,
                    send_transaction_join_handles,
                )
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
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<TxId, TransactionServiceError>
    {
        let sender_protocol = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None, message.clone())
            .await?;

        let tx_id = sender_protocol.get_tx_id()?;

        let (tx_reply_sender, tx_reply_receiver) = mpsc::channel(100);
        let (cancellation_sender, cancellation_receiver) = oneshot::channel();
        self.pending_transaction_reply_senders.insert(tx_id, tx_reply_sender);
        self.send_transaction_cancellation_senders
            .insert(tx_id, cancellation_sender);
        let protocol = TransactionSendProtocol::new(
            tx_id,
            self.service_resources.clone(),
            tx_reply_receiver,
            cancellation_receiver,
            dest_pubkey,
            amount,
            message,
            sender_protocol,
            TransactionProtocolStage::Initial,
        );

        let join_handle = tokio::spawn(protocol.execute());
        join_handles.push(join_handle);

        Ok(tx_id)
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_recipient_reply(
        &mut self,
        source_pubkey: CommsPublicKey,
        recipient_reply: proto::RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let tx_id = recipient_reply.tx_id;

        let sender = match self.pending_transaction_reply_senders.get_mut(&tx_id) {
            None => return Err(TransactionServiceError::TransactionDoesNotExistError),
            Some(s) => s,
        };

        sender
            .send((source_pubkey, recipient_reply))
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Handle the final clean up after a Send Transaction protocol completes
    async fn complete_send_transaction_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    )
    {
        match join_result {
            Ok(id) => {
                let _ = self.pending_transaction_reply_senders.remove(&id);
                let _ = self.send_transaction_cancellation_senders.remove(&id);
                let _ = self
                    .broadcast_completed_transaction_to_mempool(id, transaction_broadcast_join_handles)
                    .await
                    .or_else(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Error starting Broadcast Protocol after completed Send Transaction Protocol : {:?}", resp
                        );
                        Err(resp)
                    });
                trace!(
                    target: LOG_TARGET,
                    "Send Transaction Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.pending_transaction_reply_senders.remove(&id);
                let _ = self.send_transaction_cancellation_senders.remove(&id);
                error!(
                    target: LOG_TARGET,
                    "Error completing Send Transaction Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Cancel a pending outbound transaction
    async fn cancel_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        self.db.cancel_pending_transaction(tx_id).await.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Pending Transaction does not exist and could not be cancelled: {:?}", e
            );
            e
        })?;

        self.output_manager_service.cancel_transaction(tx_id).await?;

        if let Some(cancellation_sender) = self.send_transaction_cancellation_senders.remove(&tx_id) {
            let _ = cancellation_sender.send(());
        }
        let _ = self.pending_transaction_reply_senders.remove(&tx_id);

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });

        info!(target: LOG_TARGET, "Pending Transaction (TxId: {}) cancelled", tx_id);

        Ok(())
    }

    async fn restart_all_send_transaction_protocols(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let outbound_txs = self.db.get_pending_outbound_transactions().await?;
        for (tx_id, tx) in outbound_txs {
            if !self.pending_transaction_reply_senders.contains_key(&tx_id) {
                debug!(
                    target: LOG_TARGET,
                    "Restarting listening for Reply for Pending Outbound Transaction TxId: {}", tx_id
                );
                let (tx_reply_sender, tx_reply_receiver) = mpsc::channel(100);
                let (cancellation_sender, cancellation_receiver) = oneshot::channel();
                self.pending_transaction_reply_senders.insert(tx_id, tx_reply_sender);
                self.send_transaction_cancellation_senders
                    .insert(tx_id, cancellation_sender);
                let protocol = TransactionSendProtocol::new(
                    tx_id,
                    self.service_resources.clone(),
                    tx_reply_receiver,
                    cancellation_receiver,
                    tx.destination_public_key,
                    tx.amount,
                    tx.message,
                    tx.sender_protocol,
                    TransactionProtocolStage::WaitForReply,
                );

                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            }
        }

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
            trace!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) received from {}",
                data.tx_id,
                source_pubkey
            );
            // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed
            // transactions
            if self.db.transaction_exists(data.tx_id).await? {
                trace!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) already present in database.",
                    data.tx_id
                );
                return Err(TransactionServiceError::RepeatedMessageError);
            }

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

            let tx_id = recipient_reply.tx_id;
            let proto_message: proto::RecipientSignedMessage = recipient_reply.into();
            self.outbound_message_service
                .send_direct(
                    source_pubkey.clone(),
                    OutboundEncryption::None,
                    OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, proto_message.clone()),
                )
                .await?;

            self.outbound_message_service
                .propagate(
                    NodeDestination::NodeId(Box::new(NodeId::from_key(&source_pubkey)?)),
                    OutboundEncryption::EncryptFor(Box::new(source_pubkey.clone())),
                    vec![],
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
                message: data.message.clone(),
                timestamp: Utc::now().naive_utc(),
            };
            self.db
                .add_pending_inbound_transaction(tx_id, inbound_transaction.clone())
                .await?;

            info!(
                target: LOG_TARGET,
                "Transaction with TX_ID = {} received from {}. Reply Sent", tx_id, source_pubkey,
            );
            info!(
                target: LOG_TARGET,
                "Transaction (TX_ID: {}) - Amount: {} - Message: {}", tx_id, amount, data.message
            );

            let _ = self
                .event_publisher
                .send(Arc::new(TransactionEvent::ReceivedTransaction(tx_id)))
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

    /// Accept a new transaction from a sender by handling a public SenderMessage. The reply is generated and sent.
    /// # Arguments
    /// 'source_pubkey' - The pubkey from which the message was sent and to which the reply will be sent.
    /// 'sender_message' - Message from a sender containing the setup of the transaction being sent to you
    pub async fn accept_finalized_transaction(
        &mut self,
        source_pubkey: CommsPublicKey,
        finalized_transaction: proto::TransactionFinalizedMessage,
        transaction_broadcast_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
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

        let inbound_tx = self.db.get_pending_inbound_transaction(tx_id).await.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Finalized transaction TxId does not exist in Pending Inbound Transactions, could be a repeat Store \
                 and Forward message"
            );
            e
        })?;

        info!(
            target: LOG_TARGET,
            "Finalized Transaction with TX_ID = {} received from {}",
            tx_id,
            source_pubkey.clone()
        );

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
            .complete_inbound_transaction(tx_id, completed_transaction.clone())
            .await?;

        info!(
            target: LOG_TARGET,
            "Inbound Transaction with TX_ID = {} from {} moved to Completed Transactions",
            tx_id,
            source_pubkey.clone()
        );

        // Logging this error here instead of propogating it up to the select! catchall which generates the Error Event.
        let _ = self
            .broadcast_completed_transaction_to_mempool(tx_id, transaction_broadcast_join_handles)
            .await
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Error broadcasting completed transaction to mempool: {:?}", e
                );
                e
            });

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(tx_id)))
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
            .get_coinbase_spending_key(tx_id, amount.clone(), maturity_height)
            .await?;

        self.db
            .add_pending_coinbase_transaction(tx_id, PendingCoinbaseTransaction {
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
        let coinbase_tx = self.db.get_pending_coinbase_transaction(tx_id).await.map_err(|e| {
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
        let _ = self.db.get_pending_coinbase_transaction(tx_id).await.map_err(|e| {
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
        broadcast_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        chain_monitoring_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
        send_transaction_join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let startup_broadcast = self.base_node_public_key.is_none();

        self.base_node_public_key = Some(base_node_public_key);

        if startup_broadcast {
            let _ = self
                .broadcast_all_completed_transactions_to_mempool(broadcast_join_handles)
                .await
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Error broadcasting all completed transactions: {:?}", resp
                    );
                    Err(resp)
                });

            let _ = self
                .start_chain_monitoring_for_all_broadcast_transactions(chain_monitoring_join_handles)
                .await
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Error querying base_node for all completed transactions: {:?}", resp
                    );
                    Err(resp)
                });

            let _ = self
                .restart_all_send_transaction_protocols(send_transaction_join_handles)
                .await
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Error restarting protocols for all pending outbound transactions: {:?}", resp
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
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id).await?;

        if completed_tx.status != TransactionStatus::Completed || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }
        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(100);
                let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(100);
                self.mempool_response_senders.insert(tx_id, mempool_response_sender);
                self.base_node_response_senders.insert(tx_id, base_node_response_sender);
                let protocol = TransactionBroadcastProtocol::new(
                    tx_id,
                    self.service_resources.clone(),
                    self.config.mempool_broadcast_timeout,
                    pk,
                    mempool_response_receiver,
                    base_node_response_receiver,
                );
                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            },
        }

        Ok(())
    }

    /// Go through all completed transactions that have not yet been broadcast and broadcast all of them to the base
    /// node followed by mempool requests to confirm that they have been received
    async fn broadcast_all_completed_transactions_to_mempool(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(target: LOG_TARGET, "Attempting to Broadcast all Completed Transactions");
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Completed &&
                !self.mempool_response_senders.contains_key(&completed_tx.tx_id)
            {
                self.broadcast_completed_transaction_to_mempool(completed_tx.tx_id, join_handles)
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle an incoming mempool response message
    pub async fn handle_mempool_response(
        &mut self,
        response: MempoolProto::MempoolServiceResponse,
    ) -> Result<(), TransactionServiceError>
    {
        let response = MempoolServiceResponse::try_from(response).unwrap();
        trace!(target: LOG_TARGET, "Received Mempool Response: {:?}", response);

        let tx_id = response.request_key;

        let sender = match self.mempool_response_senders.get_mut(&tx_id) {
            None => return Err(TransactionServiceError::UnexpectedMempoolResponse),
            Some(s) => s,
        };
        sender
            .send(response)
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Handle the final clean up after a Transaction Broadcast protocol completes
    async fn complete_transaction_broadcast_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
        transaction_chain_monitoring_join_handles: &mut FuturesUnordered<
            JoinHandle<Result<u64, TransactionServiceProtocolError>>,
        >,
    )
    {
        match join_result {
            Ok(id) => {
                // Cleanup any registered senders
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                trace!(
                    target: LOG_TARGET,
                    "Transaction Broadcast Protocol for TxId: {} completed successfully",
                    id
                );
                let _ = self
                    .start_transaction_chain_monitoring_protocol(id, transaction_chain_monitoring_join_handles)
                    .await
                    .or_else(|resp| {
                        match resp {
                            TransactionServiceError::InvalidCompletedTransaction => trace!(
                                target: LOG_TARGET,
                                "Not starting Chain monitoring protocol as transaction cannot be found, either \
                                 cancelled or already mined."
                            ),
                            _ => error!(
                                target: LOG_TARGET,
                                "Error starting Chain Monitoring Protocol after completed Broadcast Protocol : {:?}",
                                resp
                            ),
                        }
                        Err(resp)
                    });
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                error!(
                    target: LOG_TARGET,
                    "Error completing Transaction Broadcast Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Send a request to the Base Node to see if the specified transaction has been mined yet. This function will send
    /// the request and store a timeout future to check in on the status of the transaction in the future.
    async fn start_transaction_chain_monitoring_protocol(
        &mut self,
        tx_id: TxId,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        let completed_tx = self.db.get_completed_transaction(tx_id).await?;

        if completed_tx.status != TransactionStatus::Broadcast || completed_tx.transaction.body.kernels().is_empty() {
            return Err(TransactionServiceError::InvalidCompletedTransaction);
        }

        match self.base_node_public_key.clone() {
            None => return Err(TransactionServiceError::NoBaseNodeKeysProvided),
            Some(pk) => {
                let protocol_id = OsRng.next_u64();

                let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(100);
                let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(100);
                self.mempool_response_senders
                    .insert(protocol_id, mempool_response_sender);
                self.base_node_response_senders
                    .insert(protocol_id, base_node_response_sender);
                let protocol = TransactionChainMonitoringProtocol::new(
                    protocol_id,
                    completed_tx.tx_id,
                    self.service_resources.clone(),
                    self.config.base_node_mined_timeout,
                    pk,
                    mempool_response_receiver,
                    base_node_response_receiver,
                );
                let join_handle = tokio::spawn(protocol.execute());
                join_handles.push(join_handle);
            },
        }
        Ok(())
    }

    /// Handle the final clean up after a Transaction Chain Monitoring protocol completes
    fn complete_transaction_chain_monitoring_protocol(
        &mut self,
        join_result: Result<u64, TransactionServiceProtocolError>,
    )
    {
        match join_result {
            Ok(id) => {
                // Cleanup any registered senders
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                trace!(
                    target: LOG_TARGET,
                    "Transaction chain monitoring Protocol for TxId: {} completed successfully",
                    id
                );
            },
            Err(TransactionServiceProtocolError { id, error }) => {
                let _ = self.mempool_response_senders.remove(&id);
                let _ = self.base_node_response_senders.remove(&id);
                error!(
                    target: LOG_TARGET,
                    "Error completing Transaction chain monitoring Protocol (Id: {}): {:?}", id, error
                );
                let _ = self
                    .event_publisher
                    .send(Arc::new(TransactionEvent::Error(format!("{:?}", error))));
            },
        }
    }

    /// Handle an incoming basenode response message
    pub async fn handle_base_node_response(
        &mut self,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<(), TransactionServiceError>
    {
        let sender = match self.base_node_response_senders.get_mut(&response.request_key) {
            None => {
                trace!(
                    target: LOG_TARGET,
                    "Received Base Node response with unexpected key: {}. Not for this service",
                    response.request_key
                );
                return Ok(());
            },
            Some(s) => s,
        };
        sender
            .send(response.clone())
            .await
            .map_err(|_| TransactionServiceError::ProtocolChannelError)?;

        Ok(())
    }

    /// Go through all completed transactions that have been broadcast and start querying the base_node to see if they
    /// have been mined
    async fn start_chain_monitoring_for_all_broadcast_transactions(
        &mut self,
        join_handles: &mut FuturesUnordered<JoinHandle<Result<u64, TransactionServiceProtocolError>>>,
    ) -> Result<(), TransactionServiceError>
    {
        trace!(
            target: LOG_TARGET,
            "Starting Chain monitoring for all Broadcast Transactions"
        );
        let completed_txs = self.db.get_completed_transactions().await?;
        for completed_tx in completed_txs.values() {
            if completed_tx.status == TransactionStatus::Broadcast {
                self.start_transaction_chain_monitoring_protocol(completed_tx.tx_id, join_handles)
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
                tx_id,
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
            .complete_outbound_transaction(completed_tx.tx_id, completed_tx.clone())
            .await?;
        Ok(())
    }

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is broadcast in a mempool on the base layer. The function will update the status of
    /// the completed transaction.
    #[cfg(feature = "test_harness")]
    pub async fn broadcast_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        completed_txs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to broadcast.".to_string())
        })?;

        self.db.broadcast_completed_transaction(tx_id).await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionBroadcast(tx_id)))
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

    /// This function is only available for testing by the client of LibWallet. This function will simulate the process
    /// when a completed transaction is detected as mined on the base layer. The function will update the status of the
    /// completed transaction AND complete the transaction on the Output Manager Service which will update the status of
    /// the outputs
    #[cfg(feature = "test_harness")]
    pub async fn mine_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let completed_txs = self.db.get_completed_transactions().await?;
        let _found_tx = completed_txs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Completed TX to mine.".to_string())
        })?;

        let pending_tx_outputs = self.output_manager_service.get_pending_transactions().await?;
        let pending_tx = pending_tx_outputs.get(&tx_id).ok_or_else(|| {
            TransactionServiceError::TestHarnessError("Could not find Pending TX to complete.".to_string())
        })?;

        self.output_manager_service
            .confirm_transaction(
                tx_id,
                pending_tx
                    .outputs_to_be_spent
                    .iter()
                    .map(|o| o.as_transaction_input(&self.factories.commitment, OutputFeatures::default()))
                    .collect(),
                pending_tx
                    .outputs_to_be_received
                    .iter()
                    .map(|o| {
                        o.as_transaction_output(&self.factories)
                            .expect("Failed to convert to Transaction Output")
                    })
                    .collect(),
            )
            .await?;

        self.db.mine_completed_transaction(tx_id).await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionMined(tx_id)))
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
        use futures::stream;
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
        let (_ti, uo) = make_input(&mut OsRng, amount + 1000 * uT, &self.factories);

        fake_oms.add_output(uo).await?;

        let mut stp = fake_oms
            .prepare_transaction_to_send(amount, MicroTari::from(25), None, "".to_string())
            .await?;

        let msg = stp.build_single_round_message()?;
        let proto_msg = proto::TransactionSenderMessage::single(msg.into());
        let sender_message: TransactionSenderMessage = proto_msg
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let spending_key = self
            .output_manager_service
            .get_recipient_spending_key(tx_id, amount.clone())
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
            .add_pending_inbound_transaction(tx_id, inbound_transaction.clone())
            .await?;

        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedTransaction(tx_id)))
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

    /// This function is only available for testing by the client of LibWallet. This function simulates an external
    /// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
    #[cfg(feature = "test_harness")]
    pub async fn finalize_received_test_transaction(&mut self, tx_id: TxId) -> Result<(), TransactionServiceError> {
        let inbound_txs = self.db.get_pending_inbound_transactions().await?;

        let found_tx = inbound_txs.get(&tx_id).ok_or_else(|| {
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
            .complete_inbound_transaction(tx_id, completed_transaction.clone())
            .await?;
        let _ = self
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(tx_id)))
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
}

/// This struct is a collection of the common resources that a protocol in the service requires.
#[derive(Clone)]
pub struct TransactionServiceResources<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub db: TransactionDatabase<TBackend>,
    pub output_manager_service: OutputManagerHandle,
    pub outbound_message_service: OutboundMessageRequester,
    pub event_publisher: TransactionEventSender,
    pub node_identity: Arc<NodeIdentity>,
    pub factories: CryptoFactories,
}

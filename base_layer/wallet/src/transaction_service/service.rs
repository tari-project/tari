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
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tari_broadcast_channel::Publisher;
use tari_comms::{peer_manager::NodeIdentity, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageResponse},
};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, OutputFlags, Transaction},
    transaction_protocol::{
        proto,
        recipient::{RecipientSignedMessage, RecipientState},
        sender::TransactionSenderMessage,
    },
    types::{CryptoFactories, PrivateKey},
    ReceiverTransactionProtocol,
};
#[cfg(feature = "test_harness")]
use tari_core::transactions::{tari_amount::T, types::BlindingFactor};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::Receiver};

const LOG_TARGET: &'static str = "base_layer::wallet::transaction_service::service";

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

pub struct TransactionService<TTxStream, TTxReplyStream, TTxFinalizedStream, TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    db: TransactionDatabase<TBackend>,
    outbound_message_service: OutboundMessageRequester,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    transaction_finalized_stream: Option<TTxFinalizedStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: Publisher<TransactionEvent>,
    node_identity: Arc<NodeIdentity>,
    factories: CryptoFactories,
}

impl<TTxStream, TTxReplyStream, TTxFinalizedStream, TBackend>
    TransactionService<TTxStream, TTxReplyStream, TTxFinalizedStream, TBackend>
where
    TTxStream: Stream<Item = DomainMessage<proto::TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<proto::RecipientSignedMessage>>,
    TTxFinalizedStream: Stream<Item = DomainMessage<proto::TransactionFinalizedMessage>>,
    TBackend: TransactionBackend + Clone + 'static,
{
    pub fn new(
        db: TransactionDatabase<TBackend>,
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        transaction_finalized_stream: TTxFinalizedStream,
        output_manager_service: OutputManagerHandle,
        outbound_message_service: OutboundMessageRequester,
        event_publisher: Publisher<TransactionEvent>,
        node_identity: Arc<NodeIdentity>,
        factories: CryptoFactories,
    ) -> Self
    {
        TransactionService {
            db,
            outbound_message_service,
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            transaction_finalized_stream: Some(transaction_finalized_stream),
            request_stream: Some(request_stream),
            event_publisher,
            node_identity,
            factories,
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

        let mut discovery_process_futures: FuturesUnordered<BoxFuture<'static, Result<TxId, TransactionServiceError>>> =
            FuturesUnordered::new();

        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request, &mut discovery_process_futures).await.or_else(|resp| {
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
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
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
                    let result = self.accept_recipient_reply(origin_public_key, inner_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
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
                    let result = self.accept_finalized_transaction(origin_public_key, inner_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming message: {:?}", err);
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
                            response = discovery_process_futures.select_next_some() => {
                                match response {
                                    Ok(tx_id) => {
                                        let _ = self.event_publisher
                                            .send(TransactionEvent::TransactionSendDiscoveryComplete(tx_id, true))
                                            .await;
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

                complete => {
                    info!(target: LOG_TARGET, "Text message service shutting down");
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
        discovery_process_futures: &mut FuturesUnordered<BoxFuture<'static, Result<TxId, TransactionServiceError>>>,
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
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
        discovery_process_futures: &mut FuturesUnordered<BoxFuture<'static, Result<TxId, TransactionServiceError>>>,
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
            SendMessageResponse::Queued(_) => (),
            SendMessageResponse::Failed => return Err(TransactionServiceError::OutboundSendFailure),
            SendMessageResponse::PendingDiscovery(r) => {
                // The sending of the message resulted in a long running Discovery process being performed by the Comms
                // layer. This can take minutes so we will spawn a task to wait for the result and then act
                // appropriately on it
                let db_clone = self.db.clone();
                let tx_id_clone = tx_id.clone();
                let outbound_tx_clone = OutboundTransaction {
                    tx_id,
                    destination_public_key: dest_pubkey.clone(),
                    amount,
                    fee: sender_protocol.get_fee_amount()?,
                    sender_protocol: sender_protocol.clone(),
                    message: message.clone(),
                    timestamp: Utc::now().naive_utc(),
                };
                let discovery_future = async move {
                    transaction_send_discovery_process_completion(r, db_clone, tx_id_clone, outbound_tx_clone).await
                };
                discovery_process_futures.push(discovery_future.boxed());
                return Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id.clone()));
            },
        }

        self.db
            .add_pending_outbound_transaction(tx_id, OutboundTransaction {
                tx_id,
                destination_public_key: dest_pubkey.clone(),
                amount,
                fee: sender_protocol.get_fee_amount()?,
                sender_protocol,
                message,
                timestamp: Utc::now().naive_utc(),
            })
            .await?;

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} sent to {}", tx_id, dest_pubkey
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
    ) -> Result<(), TransactionServiceError>
    {
        let recipient_reply: RecipientSignedMessage = recipient_reply
            .try_into()
            .map_err(TransactionServiceError::InvalidMessageError)?;

        let mut outbound_tx = self
            .db
            .get_pending_outbound_transaction(recipient_reply.tx_id.clone())
            .await?;

        let tx_id = recipient_reply.tx_id.clone();
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

        // TODO Broadcast this to the chain
        let completed_transaction = CompletedTransaction {
            tx_id: tx_id.clone(),
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
            let amount = data.amount.clone();

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
            if self.db.transaction_exists(&recipient_reply.tx_id).await? {
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
    ) -> Result<(), TransactionServiceError>
    {
        let tx_id = finalized_transaction.tx_id.clone();
        let transaction: Transaction = finalized_transaction
            .transaction
            .ok_or(TransactionServiceError::InvalidMessageError(
                "Finalized Transaction missing Transaction field".to_string(),
            ))?
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
            tx_id: tx_id.clone(),
            source_public_key: source_pubkey.clone(),
            destination_public_key: self.node_identity.public_key().clone(),
            amount: inbound_tx.amount,
            fee: transaction.body.get_total_fee(),
            transaction: transaction.clone(),
            status: TransactionStatus::Completed,
            message: inbound_tx.message.clone(),
            timestamp: inbound_tx.timestamp.clone(),
        };

        self.db
            .complete_inbound_transaction(tx_id.clone(), completed_transaction.clone())
            .await?;

        // TODO Actually Broadcast this Transaction to a base node

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
                commitment: self
                    .factories
                    .commitment
                    .commit_value(&spending_key, u64::from(amount.clone())),
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

        if completed_transaction.body.inputs().len() != 0 ||
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
        let _found_tx = completed_txs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Completed TX to broadcast.".to_string(),
            ))?;

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
        use tari_core::transactions::transaction::TransactionOutput;

        let completed_txs = self.db.get_completed_transactions().await?;
        let _found_tx = completed_txs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Completed TX to mine.".to_string(),
            ))?;

        let pending_tx_outputs = self.output_manager_service.get_pending_transactions().await?;
        let pending_tx = pending_tx_outputs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Pending TX to complete.".to_string(),
            ))?;

        let outputs_to_be_spent = pending_tx
            .outputs_to_be_spent
            .clone()
            .iter()
            .map(|o| o.as_transaction_input(&self.factories.commitment, OutputFeatures::default()))
            .collect();

        let mut outputs_to_be_received = Vec::new();

        for o in pending_tx.outputs_to_be_received.clone() {
            outputs_to_be_received.push(o.as_transaction_output(&self.factories)?)
        }
        outputs_to_be_received.push(TransactionOutput::default());

        self.output_manager_service
            .confirm_sent_transaction(tx_id.clone(), outputs_to_be_spent, outputs_to_be_received)
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
            service::OutputManagerService,
            storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
        };

        let (_sender, receiver) = reply_channel::unbounded();

        let mut fake_oms = OutputManagerService::new(
            receiver,
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
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

        let found_tx = inbound_txs
            .get(&tx_id.clone())
            .ok_or(TransactionServiceError::TestHarnessError(
                "Could not find Pending Inbound TX to finalize.".to_string(),
            ))?;

        let completed_transaction = CompletedTransaction {
            tx_id: tx_id.clone(),
            source_public_key: found_tx.source_public_key.clone(),
            destination_public_key: self.node_identity.public_key().clone(),
            amount: found_tx.amount,
            fee: MicroTari::from(2000), // a placeholder fee for this test function
            transaction: Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
            status: TransactionStatus::Completed,
            message: found_tx.message.clone(),
            timestamp: found_tx.timestamp.clone(),
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

async fn transaction_send_discovery_process_completion<TBackend: TransactionBackend + Clone + 'static>(
    response_channel: oneshot::Receiver<SendMessageResponse>,
    db: TransactionDatabase<TBackend>,
    tx_id: TxId,
    outbound_tx: OutboundTransaction,
) -> Result<TxId, TransactionServiceError>
{
    let mut success = false;
    match response_channel.await {
        Ok(response) => match response {
            SendMessageResponse::Queued(tags) => {
                if tags.len() == 0 {
                    error!(
                        target: LOG_TARGET,
                        "Send Discovery process for TX_ID: {} was unsuccessful and no message was sent", tx_id
                    );
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send Discovery process successful? {}",
                        tx_id,
                        tags.len()
                    );
                    success = true;
                }
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

    if success {
        let updated_outbound_tx = OutboundTransaction {
            timestamp: Utc::now().naive_utc(),
            ..outbound_tx
        };
        if let Err(_) = db
            .add_pending_outbound_transaction(tx_id, updated_outbound_tx.clone())
            .await
        {
            success = false;
        }
        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} sent to {} after Discovery process completed",
            tx_id,
            updated_outbound_tx.destination_public_key.clone()
        );
    }

    if !success {
        return Err(TransactionServiceError::DiscoveryProcessFailed(tx_id));
    }

    Ok(tx_id)
}

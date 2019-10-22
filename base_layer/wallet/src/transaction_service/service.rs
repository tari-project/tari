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
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::{
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceRequest, TransactionServiceResponse},
    },
    types::TransactionRng,
};
use futures::{pin_mut, SinkExt, Stream, StreamExt};
use log::*;
use std::collections::HashMap;
use tari_broadcast_channel::Publisher;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    envelope::NodeDestination,
    outbound::{BroadcastStrategy, OutboundEncryption, OutboundMessageRequester},
};
use tari_core::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, Transaction},
    transaction_protocol::{recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
    types::{PrivateKey, COMMITMENT_FACTORY, PROVER},
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_crypto::keys::SecretKey;
use tari_p2p::{
    domain_message::DomainMessage,
    tari_message::{BlockchainMessage, TariMessageType},
};
use tari_service_framework::{reply_channel, reply_channel::Receiver};

const LOG_TARGET: &'static str = "base_layer::wallet::transaction_service::service";

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

pub struct TransactionService<TTxStream, TTxReplyStream> {
    pending_outbound_transactions: HashMap<u64, SenderTransactionProtocol>,
    pending_inbound_transactions: HashMap<u64, ReceiverTransactionProtocol>,
    completed_transactions: HashMap<u64, Transaction>,
    outbound_message_service: OutboundMessageRequester,
    output_manager_service: OutputManagerHandle,
    transaction_stream: Option<TTxStream>,
    transaction_reply_stream: Option<TTxReplyStream>,
    request_stream: Option<
        reply_channel::Receiver<TransactionServiceRequest, Result<TransactionServiceResponse, TransactionServiceError>>,
    >,
    event_publisher: Publisher<TransactionEvent>,
}

impl<TTxStream, TTxReplyStream> TransactionService<TTxStream, TTxReplyStream>
where
    TTxStream: Stream<Item = DomainMessage<TransactionSenderMessage>>,
    TTxReplyStream: Stream<Item = DomainMessage<RecipientSignedMessage>>,
{
    pub fn new(
        request_stream: Receiver<
            TransactionServiceRequest,
            Result<TransactionServiceResponse, TransactionServiceError>,
        >,
        transaction_stream: TTxStream,
        transaction_reply_stream: TTxReplyStream,
        output_manager_service: OutputManagerHandle,
        outbound_message_service: OutboundMessageRequester,
        event_publisher: Publisher<TransactionEvent>,
    ) -> Self
    {
        TransactionService {
            pending_outbound_transactions: HashMap::new(),
            pending_inbound_transactions: HashMap::new(),
            completed_transactions: HashMap::new(),
            outbound_message_service,
            output_manager_service,
            transaction_stream: Some(transaction_stream),
            transaction_reply_stream: Some(transaction_reply_stream),
            request_stream: Some(request_stream),
            event_publisher,
        }
    }

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
        loop {
            futures::select! {
                //Incoming request
                request_context = request_stream.select_next_some() => {
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },
                // Incoming messages from the Comms layer
                msg = transaction_stream.select_next_some() => {
                    let result  = self.accept_transaction(msg.origin_pubkey, msg.inner).await.or_else(|err| {
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
                    let result = self.accept_recipient_reply(msg.inner).await.or_else(|err| {
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
    ) -> Result<TransactionServiceResponse, TransactionServiceError>
    {
        match request {
            TransactionServiceRequest::SendTransaction((dest_pubkey, amount, fee_per_gram)) => self
                .send_transaction(dest_pubkey, amount, fee_per_gram)
                .await
                .map(|_| TransactionServiceResponse::TransactionSent),
            TransactionServiceRequest::GetPendingInboundTransactions => Ok(
                TransactionServiceResponse::PendingInboundTransactions(self.get_pending_inbound_transactions()),
            ),
            TransactionServiceRequest::GetPendingOutboundTransactions => Ok(
                TransactionServiceResponse::PendingOutboundTransactions(self.get_pending_outbound_transactions()),
            ),
            TransactionServiceRequest::GetCompletedTransactions => Ok(
                TransactionServiceResponse::CompletedTransactions(self.get_completed_transactions()),
            ),
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
    ) -> Result<(), TransactionServiceError>
    {
        let mut stp = self
            .output_manager_service
            .prepare_transaction_to_send(amount, fee_per_gram, None)
            .await?;

        if !stp.is_single_round_message_ready() {
            return Err(TransactionServiceError::InvalidStateError);
        }

        let msg = stp.build_single_round_message()?;
        self.outbound_message_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(dest_pubkey.clone()),
                NodeDestination::Unspecified,
                OutboundEncryption::EncryptForDestination,
                TariMessageType::new(BlockchainMessage::Transaction),
                TransactionSenderMessage::Single(Box::new(msg.clone())),
            )
            .await?;

        self.pending_outbound_transactions.insert(msg.tx_id.clone(), stp);

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} sent to {}",
            msg.tx_id.clone(),
            dest_pubkey
        );

        Ok(())
    }

    /// Accept the public reply from a recipient and apply the reply to the relevant transaction protocol
    /// # Arguments
    /// 'recipient_reply' - The public response from a recipient with data required to complete the transaction
    pub async fn accept_recipient_reply(
        &mut self,
        recipient_reply: RecipientSignedMessage,
    ) -> Result<(), TransactionServiceError>
    {
        let mut marked_for_removal = None;

        for (tx_id, stp) in self.pending_outbound_transactions.iter_mut() {
            let recp_tx_id = recipient_reply.tx_id.clone();
            if stp.check_tx_id(recp_tx_id) && stp.is_collecting_single_signature() {
                stp.add_single_recipient_info(recipient_reply, &PROVER)?;
                stp.finalize(KernelFeatures::empty(), &PROVER, &COMMITMENT_FACTORY)?;
                let tx = stp.get_transaction()?;
                self.completed_transactions.insert(recp_tx_id, tx.clone());
                // TODO Broadcast this to the chain
                // TODO Only confirm this transaction once it is detected on chain. For now just confirming it directly.
                self.output_manager_service
                    .confirm_sent_transaction(recp_tx_id, tx.body.inputs().clone(), tx.body.outputs().clone())
                    .await?;

                marked_for_removal = Some(tx_id.clone());
                break;
            }
        }

        if marked_for_removal.is_none() {
            return Err(TransactionServiceError::TransactionDoesNotExistError);
        }

        if let Some(tx_id) = marked_for_removal {
            self.pending_outbound_transactions.remove(&tx_id);
            info!(
                target: LOG_TARGET,
                "Transaction Recipient Reply for TX_ID = {} received", tx_id,
            );
            self.event_publisher
                .send(TransactionEvent::ReceivedTransactionReply)
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
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
        sender_message: TransactionSenderMessage,
    ) -> Result<(), TransactionServiceError>
    {
        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = sender_message.clone() {
            let spending_key = self
                .output_manager_service
                .get_recipient_spending_key(data.tx_id, data.amount)
                .await?;
            let mut rng = TransactionRng::new().unwrap();
            let nonce = PrivateKey::random(&mut rng);

            let rtp = ReceiverTransactionProtocol::new(
                sender_message,
                nonce,
                spending_key,
                OutputFeatures::default(),
                &PROVER,
                &COMMITMENT_FACTORY,
            );
            let recipient_reply = rtp.get_signed_data()?.clone();

            // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed
            // transactions
            if self.pending_outbound_transactions.contains_key(&recipient_reply.tx_id) ||
                self.pending_inbound_transactions.contains_key(&recipient_reply.tx_id) ||
                self.completed_transactions.contains_key(&recipient_reply.tx_id)
            {
                return Err(TransactionServiceError::RepeatedMessageError);
            }

            self.outbound_message_service
                .send_message(
                    BroadcastStrategy::DirectPublicKey(source_pubkey.clone()),
                    NodeDestination::Unspecified,
                    OutboundEncryption::EncryptForDestination,
                    TariMessageType::new(BlockchainMessage::TransactionReply),
                    recipient_reply.clone(),
                )
                .await?;

            // Otherwise add it to our pending transaction list and return reply
            self.pending_inbound_transactions
                .insert(recipient_reply.tx_id.clone(), rtp);

            info!(
                target: LOG_TARGET,
                "Transaction with TX_ID = {} received from {}. Reply Sent",
                recipient_reply.tx_id.clone(),
                source_pubkey.clone()
            );

            self.event_publisher
                .send(TransactionEvent::ReceivedTransaction)
                .await
                .map_err(|_| TransactionServiceError::EventStreamError)?;
        }
        Ok(())
    }

    pub fn get_pending_inbound_transactions(&self) -> HashMap<u64, ReceiverTransactionProtocol> {
        self.pending_inbound_transactions.clone()
    }

    pub fn get_pending_outbound_transactions(&self) -> HashMap<u64, SenderTransactionProtocol> {
        self.pending_outbound_transactions.clone()
    }

    pub fn get_completed_transactions(&self) -> HashMap<u64, Transaction> {
        self.completed_transactions.clone()
    }
}

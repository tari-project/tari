// Copyright 2020. The Tari Project
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

use std::sync::Arc;

use chrono::Utc;
use futures::{channel::mpsc::Receiver, StreamExt};
use log::*;

use crate::transaction_service::{
    error::{TransactionServiceError, TransactionServiceProtocolError},
    handle::TransactionEvent,
    service::TransactionServiceResources,
    storage::database::{CompletedTransaction, OutboundTransaction, TransactionBackend, TransactionStatus},
};
use tari_comms::{
    protocol::messaging::{MessagingEvent, MessagingEventReceiver},
    types::CommsPublicKey,
};
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, TransactionError},
    transaction_protocol::{proto, recipient::RecipientSignedMessage},
    SenderTransactionProtocol,
};
use tari_p2p::tari_message::TariMessageType;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::send_protocol";

pub struct TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    resources: TransactionServiceResources<TBackend>,
    transaction_reply_receiver: Option<Receiver<(CommsPublicKey, RecipientSignedMessage)>>,
    message_send_event_receiver: MessagingEventReceiver,
    dest_pubkey: CommsPublicKey,
    amount: MicroTari,
    message: String,
    sender_protocol: SenderTransactionProtocol,
}

impl<TBackend> TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        resources: TransactionServiceResources<TBackend>,
        transaction_reply_receiver: Receiver<(CommsPublicKey, RecipientSignedMessage)>,
        message_send_event_receiver: MessagingEventReceiver,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        message: String,
        sender_protocol: SenderTransactionProtocol,
    ) -> Self
    {
        Self {
            id,
            resources,
            transaction_reply_receiver: Some(transaction_reply_receiver),
            message_send_event_receiver,
            dest_pubkey,
            amount,
            message,
            sender_protocol,
        }
    }

    /// Execute the Transaction Send Protocol as an async task.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        if !self.sender_protocol.is_single_round_message_ready() {
            error!(target: LOG_TARGET, "Sender Transaction Protocol is in an invalid state");
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::InvalidStateError,
            ));
        }

        let msg = self
            .sender_protocol
            .build_single_round_message()
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        let tx_id = msg.tx_id;

        if tx_id != self.id {
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::InvalidStateError,
            ));
        }

        let proto_message = proto::TransactionSenderMessage::single(msg.into());

        let message_tag = match self
            .resources
            .outbound_message_service
            .send_direct(
                self.dest_pubkey.clone(),
                OutboundEncryption::None,
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message),
            )
            .await
        {
            Ok(result) => match result.resolve_ok().await {
                None => {
                    let _ = self.resources.event_publisher.send(Arc::new(
                        TransactionEvent::TransactionSendDiscoveryComplete(tx_id, false),
                    ));
                    error!(target: LOG_TARGET, "Transaction Send message failed to send");
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::OutboundSendFailure,
                    ));
                },
                Some(tags) if tags.len() == 1 => {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send Discovery process successful with Message Tag: {:?}",
                        tx_id,
                        tags[0],
                    );
                    tags[0]
                },
                Some(_tags) => {
                    error!(
                        target: LOG_TARGET,
                        "Send Discovery process for TX_ID: {} was unsuccessful and no message was sent", tx_id
                    );
                    let _ = self.resources.event_publisher.send(Arc::new(
                        TransactionEvent::TransactionSendDiscoveryComplete(tx_id, false),
                    ));
                    if let Err(e) = self.resources.output_manager_service.cancel_transaction(tx_id).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}", tx_id, e
                        );
                    };
                    error!(target: LOG_TARGET, "Transaction Send message failed to send");
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::OutboundSendFailure,
                    ));
                },
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Transaction Send failed: {:?}", e);
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::OutboundSendFailure,
                ));
            },
        };

        info!(
            target: LOG_TARGET,
            "Transaction with TX_ID = {} queued to be sent to {}", tx_id, self.dest_pubkey
        );

        // Wait for the Message Event to tell us this has been sent
        loop {
            let (received_tag, success) = match self.message_send_event_receiver.next().await {
                Some(read_item) => match read_item {
                    Ok(event) => match &*event {
                        MessagingEvent::MessageSent(message_tag) => (message_tag.clone(), true),
                        MessagingEvent::SendMessageFailed(outbound_message, _reason) => (outbound_message.tag, false),
                        _ => continue,
                    },
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "Error reading from message send event stream: {:?}", e
                        );
                        return Err(TransactionServiceProtocolError::new(
                            self.id,
                            TransactionServiceError::from(e),
                        ));
                    },
                },
                None => {
                    error!(target: LOG_TARGET, "Error reading from message send event stream");
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::OutboundSendFailure,
                    ));
                },
            };

            if received_tag != message_tag {
                continue;
            }

            if success {
                self.resources
                    .output_manager_service
                    .confirm_pending_transaction(tx_id)
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                let fee = self
                    .sender_protocol
                    .get_fee_amount()
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
                let outbound_tx = OutboundTransaction {
                    tx_id,
                    destination_public_key: self.dest_pubkey.clone(),
                    amount: self.amount,
                    fee,
                    sender_protocol: self.sender_protocol.clone(),
                    status: TransactionStatus::Pending,
                    message: self.message.clone(),
                    timestamp: Utc::now().naive_utc(),
                };

                self.resources
                    .db
                    .add_pending_outbound_transaction(outbound_tx.tx_id, outbound_tx)
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                info!(
                    target: LOG_TARGET,
                    "Pending Outbound Transaction TxId: {:?} was successfully sent with Message Tag: {:?}",
                    tx_id,
                    message_tag
                );
            } else {
                error!(
                    target: LOG_TARGET,
                    "Pending Outbound Transaction TxId: {:?} with Message Tag {:?} could not be sent",
                    tx_id,
                    message_tag,
                );
                if let Err(e) = self.resources.output_manager_service.cancel_transaction(tx_id).await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}", tx_id, e
                    );
                }
            }

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionSendResult(tx_id, success)));

            if !success {
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::OutboundSendFailure,
                ));
            }

            break;
        }

        // Wait for Transaction Reply
        let mut receiver = self
            .transaction_reply_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut outbound_tx = self
            .resources
            .db
            .get_pending_outbound_transaction(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        if !outbound_tx.sender_protocol.is_collecting_single_signature() {
            error!(target: LOG_TARGET, "Pending Transaction not in correct state");
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::InvalidStateError,
            ));
        }

        let mut source_pubkey;
        let mut recipient_reply;
        loop {
            match receiver.next().await {
                Some((spk, rr)) => {
                    source_pubkey = spk;
                    recipient_reply = rr;
                },
                None => {
                    error!(target: LOG_TARGET, "Transaction Reply Channel has closes");
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::ProtocolChannelError,
                    ));
                },
            }

            if outbound_tx.destination_public_key != source_pubkey {
                error!(
                    target: LOG_TARGET,
                    "Transaction Reply did not come from the expected Public Key"
                );
            } else if !outbound_tx.sender_protocol.check_tx_id(recipient_reply.tx_id.clone()) {
                error!(target: LOG_TARGET, "Transaction Reply does not have the correct TxId");
            } else {
                break;
            }
        }

        outbound_tx
            .sender_protocol
            .add_single_recipient_info(recipient_reply, &self.resources.factories.range_proof)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        let finalize_result = outbound_tx
            .sender_protocol
            .finalize(KernelFeatures::empty(), &self.resources.factories)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        if !finalize_result {
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::TransactionError(TransactionError::ValidationError(
                    "Transaction could not be finalized".to_string(),
                )),
            ));
        }

        let tx = outbound_tx
            .sender_protocol
            .get_transaction()
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        let completed_transaction = CompletedTransaction {
            tx_id,
            source_public_key: self.resources.node_identity.public_key().clone(),
            destination_public_key: outbound_tx.destination_public_key.clone(),
            amount: outbound_tx.amount,
            fee: outbound_tx.fee,
            transaction: tx.clone(),
            status: TransactionStatus::Completed,
            message: outbound_tx.message.clone(),
            timestamp: Utc::now().naive_utc(),
        };

        self.resources
            .db
            .complete_outbound_transaction(tx_id.clone(), completed_transaction.clone())
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        info!(
            target: LOG_TARGET,
            "Transaction Recipient Reply for TX_ID = {} received", tx_id,
        );

        let finalized_transaction_message = proto::TransactionFinalizedMessage {
            tx_id,
            transaction: Some(tx.clone().into()),
        };

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::ReceivedTransactionReply(tx_id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event, usually because there are no subscribers: {:?}",
                    e
                );
                e
            });

        self.resources
            .outbound_message_service
            .send_direct(
                outbound_tx.destination_public_key.clone(),
                OutboundEncryption::None,
                OutboundDomainMessage::new(TariMessageType::TransactionFinalized, finalized_transaction_message),
            )
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        Ok(self.id)
    }
}

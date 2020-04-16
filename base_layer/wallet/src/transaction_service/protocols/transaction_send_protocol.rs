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
use futures::{channel::mpsc::Receiver, FutureExt, StreamExt};
use log::*;

use crate::transaction_service::{
    error::{TransactionServiceError, TransactionServiceProtocolError},
    handle::TransactionEvent,
    service::TransactionServiceResources,
    storage::database::{CompletedTransaction, OutboundTransaction, TransactionBackend, TransactionStatus},
};
use futures::channel::oneshot;
use tari_comms::{
    protocol::messaging::{MessagingEvent, MessagingEventReceiver},
    types::CommsPublicKey,
};
use tari_comms_dht::{domain_message::OutboundDomainMessage, envelope::NodeDestination, outbound::OutboundEncryption};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, TransactionError},
    transaction_protocol::{proto, recipient::RecipientSignedMessage},
    SenderTransactionProtocol,
};
use tari_p2p::tari_message::TariMessageType;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::send_protocol";

#[derive(PartialEq)]
pub enum TransactionProtocolStage {
    Initial,
    WaitForReply,
}

pub struct TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    resources: TransactionServiceResources<TBackend>,
    transaction_reply_receiver: Option<Receiver<(CommsPublicKey, RecipientSignedMessage)>>,
    cancellation_receiver: Option<oneshot::Receiver<()>>,
    message_send_event_receiver: Option<MessagingEventReceiver>,
    dest_pubkey: CommsPublicKey,
    amount: MicroTari,
    message: String,
    sender_protocol: SenderTransactionProtocol,
    stage: TransactionProtocolStage,
}

impl<TBackend> TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        resources: TransactionServiceResources<TBackend>,
        transaction_reply_receiver: Receiver<(CommsPublicKey, RecipientSignedMessage)>,
        cancellation_receiver: oneshot::Receiver<()>,
        message_send_event_receiver: MessagingEventReceiver,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        message: String,
        sender_protocol: SenderTransactionProtocol,
        stage: TransactionProtocolStage,
    ) -> Self
    {
        Self {
            id,
            resources,
            transaction_reply_receiver: Some(transaction_reply_receiver),
            cancellation_receiver: Some(cancellation_receiver),
            message_send_event_receiver: Some(message_send_event_receiver),
            dest_pubkey,
            amount,
            message,
            sender_protocol,
            stage,
        }
    }

    /// Execute the Transaction Send Protocol as an async task.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        if self.stage == TransactionProtocolStage::Initial {
            self.send_transaction().await?;
        }

        // Waiting  for Transaction Reply
        let tx_id = self.id;
        let mut receiver = self
            .transaction_reply_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut cancellation_receiver = self
            .cancellation_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

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
        #[allow(unused_assignments)]
        let mut reply = None;
        loop {
            #[allow(unused_assignments)]
            let mut rr_tx_id = 0;
            futures::select! {
                (spk, rr) = receiver.select_next_some() => {
                    source_pubkey = spk;
                    rr_tx_id = rr.tx_id;
                    reply = Some(rr);
                },
                _ = cancellation_receiver => {
                    info!(target: LOG_TARGET, "Cancelling Transaction Send Protocol for TxId: {}", self.id);
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::TransactionCancelled,
                    ));
                }
            }

            if outbound_tx.destination_public_key != source_pubkey {
                error!(
                    target: LOG_TARGET,
                    "Transaction Reply did not come from the expected Public Key"
                );
            } else if !outbound_tx.sender_protocol.check_tx_id(rr_tx_id) {
                error!(target: LOG_TARGET, "Transaction Reply does not have the correct TxId");
            } else {
                break;
            }
        }

        let recipient_reply = reply.ok_or(TransactionServiceProtocolError::new(
            self.id,
            TransactionServiceError::TransactionCancelled,
        ))?;

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

    /// Contains all the logic to initially send the transaction. This will only be done on the first time this Protocol
    /// is executed.
    async fn send_transaction(&mut self) -> Result<(), TransactionServiceProtocolError> {
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
        let mut direct_send_success = false;
        match self
            .resources
            .outbound_message_service
            .send_direct(
                self.dest_pubkey.clone(),
                OutboundEncryption::None,
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message.clone()),
            )
            .await
        {
            Ok(result) => match result.resolve_ok().await {
                None => {
                    let _ = self
                        .resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, false)));
                    error!(target: LOG_TARGET, "Transaction Send directly to recipient failed");
                },
                Some(send_states) if send_states.len() == 1 => {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Direct Send to {} successful with Message Tag: {:?}",
                        tx_id,
                        self.dest_pubkey,
                        send_states[0].tag,
                    );
                    direct_send_success = true;
                    let message_tag = send_states[0].tag;
                    let event_publisher = self.resources.event_publisher.clone();
                    // Launch a task to monitor if the message gets sent
                    if let Some(mut message_event_receiver) = self.message_send_event_receiver.take() {
                        tokio::spawn(async move {
                            loop {
                                let (received_tag, success) = match message_event_receiver.next().await {
                                    Some(read_item) => match read_item {
                                        Ok(event) => match &*event {
                                            MessagingEvent::MessageSent(message_tag) => (message_tag.clone(), true),
                                            MessagingEvent::SendMessageFailed(outbound_message, _reason) => {
                                                (outbound_message.tag, false)
                                            },
                                            _ => continue,
                                        },
                                        Err(e) => {
                                            error!(
                                                target: LOG_TARGET,
                                                "Error reading from message send event stream: {:?}", e
                                            );
                                            break;
                                        },
                                    },
                                    None => {
                                        error!(target: LOG_TARGET, "Error reading from message send event stream");
                                        break;
                                    },
                                };
                                if received_tag != message_tag {
                                    continue;
                                }
                                let _ = event_publisher
                                    .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, success)));
                                break;
                            }
                        });
                    }
                },
                Some(_tags) => {
                    error!(
                        target: LOG_TARGET,
                        "Direct Send process for TX_ID: {} was unsuccessful and no message was sent", tx_id
                    );
                    let _ = self
                        .resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, false)));
                    error!(target: LOG_TARGET, "Transaction Send message failed to send");
                },
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Direct Transaction Send failed: {:?}", e);
                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, false)));
            },
        };

        let mut store_and_forward_send_success = false;
        match self
            .resources
            .outbound_message_service
            .propagate(
                NodeDestination::from(self.dest_pubkey.clone()),
                OutboundEncryption::EncryptFor(Box::new(self.dest_pubkey.clone())),
                vec![],
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message),
            )
            .await
        {
            Ok(result) => match result.resolve_ok().await {
                None => {
                    error!(
                        target: LOG_TARGET,
                        "Transaction Send (TxId: {}) to neighbours for Store and Forward failed", self.id
                    );
                },
                Some(tags) if !tags.is_empty() => {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send to Neighbours for Store and Forward successful with Message \
                         Tags: {:?}",
                        tx_id,
                        tags,
                    );
                    store_and_forward_send_success = true;
                },
                Some(_) => {
                    error!(
                        target: LOG_TARGET,
                        "Transaction Send to Neighbours for Store and Forward for TX_ID: {} was unsuccessful and no \
                         messages were sent",
                        tx_id
                    );
                },
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Transaction Send (TxId: {}) to neighbours for Store and Forward failed: {:?}", self.id, e
                );
            },
        };

        if !direct_send_success && !store_and_forward_send_success {
            if let Err(e) = self.resources.output_manager_service.cancel_transaction(tx_id).await {
                error!(
                    target: LOG_TARGET,
                    "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}", tx_id, e
                );
            };
            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                    tx_id, false,
                )));
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::OutboundSendFailure,
            ));
        }

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
            "Pending Outbound Transaction TxId: {:?} added. Waiting for Reply or Cancellation", tx_id,
        );

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                tx_id, true,
            )));

        Ok(())
    }
}

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
    storage::database::{
        CompletedTransaction,
        OutboundTransaction,
        TransactionBackend,
        TransactionDirection,
        TransactionStatus,
    },
};
use futures::channel::oneshot;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{MessageSendStates, OutboundEncryption, SendMessageResponse},
};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, Transaction, TransactionError},
    transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::SingleRoundSenderData},
    SenderTransactionProtocol,
};
use tari_p2p::tari_message::TariMessageType;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::send_protocol";

#[derive(Debug, PartialEq)]
pub enum TransactionSendProtocolStage {
    Initial,
    WaitForReply,
}

pub struct TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    dest_pubkey: CommsPublicKey,
    amount: MicroTari,
    message: String,
    sender_protocol: SenderTransactionProtocol,
    stage: TransactionSendProtocolStage,
    resources: TransactionServiceResources<TBackend>,
    transaction_reply_receiver: Option<Receiver<(CommsPublicKey, RecipientSignedMessage)>>,
    cancellation_receiver: Option<oneshot::Receiver<()>>,
}

#[allow(clippy::too_many_arguments)]
impl<TBackend> TransactionSendProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        resources: TransactionServiceResources<TBackend>,
        transaction_reply_receiver: Receiver<(CommsPublicKey, RecipientSignedMessage)>,
        cancellation_receiver: oneshot::Receiver<()>,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        message: String,
        sender_protocol: SenderTransactionProtocol,
        stage: TransactionSendProtocolStage,
    ) -> Self
    {
        Self {
            id,
            resources,
            transaction_reply_receiver: Some(transaction_reply_receiver),
            cancellation_receiver: Some(cancellation_receiver),
            dest_pubkey,
            amount,
            message,
            sender_protocol,
            stage,
        }
    }

    /// Execute the Transaction Send Protocol as an async task.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Starting Transaction Send protocol for TxId: {} at Stage {:?}", self.id, self.stage
        );

        match self.stage {
            TransactionSendProtocolStage::Initial => {
                self.initial_send_transaction().await?;
                self.wait_for_reply().await?;
            },
            TransactionSendProtocolStage::WaitForReply => {
                self.wait_for_reply().await?;
            },
        }

        Ok(self.id)
    }

    async fn initial_send_transaction(&mut self) -> Result<(), TransactionServiceProtocolError> {
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

        let SendResult {
            direct_send_result,
            store_and_forward_send_result,
        } = self.send_transaction(msg).await?;

        if !direct_send_result && !store_and_forward_send_result {
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::OutboundSendFailure,
            ));
        }

        self.resources
            .output_manager_service
            .confirm_pending_transaction(self.id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        let fee = self
            .sender_protocol
            .get_fee_amount()
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        let outbound_tx = OutboundTransaction::new(
            tx_id,
            self.dest_pubkey.clone(),
            self.amount,
            fee,
            self.sender_protocol.clone(),
            TransactionStatus::Pending,
            self.message.clone(),
            Utc::now().naive_utc(),
            direct_send_result,
        );

        self.resources
            .db
            .add_pending_outbound_transaction(outbound_tx.tx_id, outbound_tx)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        info!(
            target: LOG_TARGET,
            "Pending Outbound Transaction TxId: {:?} added. Waiting for Reply or Cancellation", self.id,
        );

        Ok(())
    }

    async fn wait_for_reply(&mut self) -> Result<(), TransactionServiceProtocolError> {
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

        #[allow(unused_assignments)]
        let mut reply = None;
        loop {
            futures::select! {
                (spk, rr) = receiver.select_next_some() => {
                    let rr_tx_id = rr.tx_id;
                    reply = Some(rr);

                    if outbound_tx.destination_public_key != spk {
                        warn!(
                            target: LOG_TARGET,
                            "Transaction Reply did not come from the expected Public Key"
                        );
                    } else if !outbound_tx.sender_protocol.check_tx_id(rr_tx_id) {
                        warn!(target: LOG_TARGET, "Transaction Reply does not have the correct TxId");
                    } else {
                        break;
                    }
                },
                _ = cancellation_receiver => {
                    info!(target: LOG_TARGET, "Cancelling Transaction Send Protocol for TxId: {}", self.id);
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::TransactionCancelled,
                    ));
                }
            }
        }

        let recipient_reply = reply.ok_or_else(|| {
            TransactionServiceProtocolError::new(self.id, TransactionServiceError::TransactionCancelled)
        })?;

        outbound_tx
            .sender_protocol
            .add_single_recipient_info(recipient_reply, &self.resources.factories.range_proof)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        let finalize_result = outbound_tx
            .sender_protocol
            .finalize(KernelFeatures::empty(), &self.resources.factories)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        if !finalize_result {
            error!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) could not be finalized. Failure error: {:?}",
                self.id,
                self.sender_protocol.failure_reason()
            );
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

        let completed_transaction = CompletedTransaction::new(
            tx_id,
            self.resources.node_identity.public_key().clone(),
            outbound_tx.destination_public_key.clone(),
            outbound_tx.amount,
            outbound_tx.fee,
            tx.clone(),
            TransactionStatus::Completed,
            outbound_tx.message.clone(),
            Utc::now().naive_utc(),
            TransactionDirection::Outbound,
        );

        self.resources
            .db
            .complete_outbound_transaction(tx_id, completed_transaction.clone())
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        info!(
            target: LOG_TARGET,
            "Transaction Recipient Reply for TX_ID = {} received", tx_id,
        );

        self.send_transaction_finalized_message(tx.clone()).await?;

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

        Ok(())
    }

    /// Attempt to send the transaction to the recipient both directly and via Store-and-forward. If both fail to send
    /// the transaction will be cancelled.
    /// # Arguments
    /// `msg`: The transaction data message to be sent     
    async fn send_transaction(
        &mut self,
        msg: SingleRoundSenderData,
    ) -> Result<SendResult, TransactionServiceProtocolError>
    {
        let proto_message = proto::TransactionSenderMessage::single(msg.clone().into());
        let mut store_and_forward_send_result = false;
        let mut direct_send_result = false;

        info!(
            target: LOG_TARGET,
            "Attempting to Send Transaction (TxId: {}) to recipient with Node Id: {}", self.id, self.dest_pubkey,
        );

        match self
            .resources
            .outbound_message_service
            .send_direct(
                self.dest_pubkey.clone(),
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message.clone()),
            )
            .await
        {
            Ok(result) => match result {
                SendMessageResponse::Queued(send_states) => {
                    if self.wait_on_dial(send_states, "Transaction").await {
                        direct_send_result = true;
                    } else {
                        store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                    }
                },
                SendMessageResponse::Failed(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Transaction Send Direct for TxID {} failed: {}", self.id, err
                    );
                    store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                },
                SendMessageResponse::PendingDiscovery(rx) => {
                    store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                    // now wait for discovery to complete
                    match rx.await {
                        Ok(send_msg_response) => {
                            if let SendMessageResponse::Queued(send_states) = send_msg_response {
                                debug!(
                                    target: LOG_TARGET,
                                    "Discovery of {} completed for TxID: {}", self.dest_pubkey, self.id
                                );
                                direct_send_result = self.wait_on_dial(send_states, "Transaction").await;
                            }
                        },
                        Err(e) => {
                            warn!(
                                target: LOG_TARGET,
                                "Error waiting for Discovery while sending message to TxId: {} {:?}", self.id, e
                            );
                        },
                    }
                },
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Direct Transaction Send failed: {:?}", e);
            },
        }

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionDirectSendResult(
                self.id,
                direct_send_result,
            )));
        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                self.id,
                store_and_forward_send_result,
            )));

        if !direct_send_result && !store_and_forward_send_result {
            error!(
                target: LOG_TARGET,
                "Failed to Send Transaction (TxId: {}) both Directly or via Store and Forward. Pending Transaction \
                 will be cancelled",
                self.id
            );
            if let Err(e) = self.resources.output_manager_service.cancel_transaction(self.id).await {
                warn!(
                    target: LOG_TARGET,
                    "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}", self.id, e
                );
            };
        }

        Ok(SendResult {
            direct_send_result,
            store_and_forward_send_result,
        })
    }

    /// This function contains the logic to wait on a dial and send of a queued message
    async fn wait_on_dial(&self, send_states: MessageSendStates, message: &str) -> bool {
        if send_states.len() == 1 {
            debug!(
                target: LOG_TARGET,
                "{} (TxId: {}) Direct Send to {} queued with Message {}",
                message,
                self.id,
                self.dest_pubkey,
                send_states[0].tag,
            );
            let (sent, failed) = send_states
                .wait_n_timeout(self.resources.config.direct_send_timeout, 1)
                .await;
            if !sent.is_empty() {
                info!(
                    target: LOG_TARGET,
                    "Direct Send process for {} TX_ID: {} was successful with Message: {}", message, self.id, sent[0]
                );
                true
            } else {
                if failed.is_empty() {
                    warn!(
                        target: LOG_TARGET,
                        "Direct Send process for {} TX_ID: {} timed out", message, self.id
                    );
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Direct Send process for {} TX_ID: {} and Message {} was unsuccessful and no message was sent",
                        message,
                        self.id,
                        failed[0]
                    );
                }
                false
            }
        } else {
            warn!(
                target: LOG_TARGET,
                "{} Send Direct for TxID: {} failed", message, self.id
            );
            false
        }
    }

    /// Contains all the logic to send the transaction to the recipient via store and forward
    /// # Arguments
    /// `msg`: The transaction data message to be sent
    /// 'send_events': A bool indicating whether we should send events during the operation or not.
    async fn send_transaction_store_and_forward(
        &mut self,
        msg: SingleRoundSenderData,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        let proto_message = proto::TransactionSenderMessage::single(msg.into());
        match self
            .resources
            .outbound_message_service
            .broadcast(
                NodeDestination::NodeId(Box::new(NodeId::from_key(&self.dest_pubkey).map_err(|e| {
                    TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
                })?)),
                OutboundEncryption::EncryptFor(Box::new(self.dest_pubkey.clone())),
                vec![],
                OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message),
            )
            .await
        {
            Ok(send_states) if !send_states.is_empty() => {
                let (successful_sends, failed_sends) = send_states
                    .wait_n_timeout(self.resources.config.broadcast_send_timeout, 1)
                    .await;
                if !successful_sends.is_empty() {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Send to Neighbours for Store and Forward successful with Message \
                         Tags: {:?}",
                        self.id,
                        successful_sends[0],
                    );
                    Ok(true)
                } else if !failed_sends.is_empty() {
                    warn!(
                        target: LOG_TARGET,
                        "Transaction Send to Neighbours for Store and Forward for TX_ID: {} was unsuccessful and no \
                         messages were sent",
                        self.id
                    );
                    Ok(false)
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Transaction Send to Neighbours for Store and Forward for TX_ID: {} timed out and was \
                         unsuccessful. Some message might still be sent.",
                        self.id
                    );
                    Ok(false)
                }
            },
            Ok(_) => {
                warn!(
                    target: LOG_TARGET,
                    "Transaction Send to Neighbours for Store and Forward for TX_ID: {} was unsuccessful and no \
                     messages were sent",
                    self.id
                );
                Ok(false)
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Transaction Send (TxId: {}) to neighbours for Store and Forward failed: {:?}", self.id, e
                );
                Ok(false)
            },
        }
    }

    async fn send_transaction_finalized_message(
        &mut self,
        transaction: Transaction,
    ) -> Result<(), TransactionServiceProtocolError>
    {
        let finalized_transaction_message = proto::TransactionFinalizedMessage {
            tx_id: self.id,
            transaction: Some(transaction.clone().into()),
        };
        let mut store_and_forward_send_result = false;
        let mut direct_send_result = false;
        match self
            .resources
            .outbound_message_service
            .send_direct(
                self.dest_pubkey.clone(),
                OutboundDomainMessage::new(
                    TariMessageType::TransactionFinalized,
                    finalized_transaction_message.clone(),
                ),
            )
            .await
        {
            Ok(result) => match result {
                SendMessageResponse::Queued(send_states) => {
                    if self.wait_on_dial(send_states, "Finalized Transaction").await {
                        direct_send_result = true;
                    } else {
                        store_and_forward_send_result = self
                            .send_transaction_finalized_message_store_and_forward(finalized_transaction_message.clone())
                            .await?;
                    }
                },
                SendMessageResponse::Failed(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Finalized Transaction Send Direct for TxID {} failed: {}", self.id, err
                    );
                    store_and_forward_send_result = self
                        .send_transaction_finalized_message_store_and_forward(finalized_transaction_message.clone())
                        .await?;
                },
                SendMessageResponse::PendingDiscovery(rx) => {
                    store_and_forward_send_result = self
                        .send_transaction_finalized_message_store_and_forward(finalized_transaction_message.clone())
                        .await?;
                    // now wait for discovery to complete
                    match rx.await {
                        Ok(send_msg_response) => {
                            if let SendMessageResponse::Queued(send_states) = send_msg_response {
                                debug!(
                                    target: LOG_TARGET,
                                    "Discovery of {} completed for TxID: {}", self.dest_pubkey, self.id
                                );
                                direct_send_result = self.wait_on_dial(send_states, "Finalized Transaction").await;
                            }
                        },
                        Err(e) => {
                            warn!(
                                target: LOG_TARGET,
                                "Error waiting for Discovery while sending message to TxId: {} {:?}", self.id, e
                            );
                        },
                    }
                },
            },
            Err(e) => {
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::from(e),
                ))
            },
        }
        if !direct_send_result && !store_and_forward_send_result {
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::OutboundSendFailure,
            ));
        }
        Ok(())
    }

    async fn send_transaction_finalized_message_store_and_forward(
        &mut self,
        msg: proto::TransactionFinalizedMessage,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        match self
            .resources
            .outbound_message_service
            .broadcast(
                NodeDestination::NodeId(Box::new(NodeId::from_key(&self.dest_pubkey).map_err(|e| {
                    TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
                })?)),
                OutboundEncryption::EncryptFor(Box::new(self.dest_pubkey.clone())),
                vec![],
                OutboundDomainMessage::new(TariMessageType::TransactionFinalized, msg.clone()),
            )
            .await
        {
            Ok(send_states) => {
                info!(
                    target: LOG_TARGET,
                    "Sending Finalized Transaction (TxId: {}) to Neighbours for Store and Forward successful with \
                     Message Tags: {:?}",
                    self.id,
                    send_states.to_tags(),
                );
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Sending Finalized Transaction (TxId: {}) to neighbours for Store and Forward failed: {:?}",
                    self.id,
                    e
                );
            },
        };

        Ok(true)
    }
}

struct SendResult {
    direct_send_result: bool,
    store_and_forward_send_result: bool,
}

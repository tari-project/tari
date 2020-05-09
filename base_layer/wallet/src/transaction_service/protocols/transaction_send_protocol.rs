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
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{domain_message::OutboundDomainMessage, envelope::NodeDestination, outbound::OutboundEncryption};
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, TransactionError},
    transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::SingleRoundSenderData},
    SenderTransactionProtocol,
};
use tari_p2p::{services::liveness::LivenessEvent, tari_message::TariMessageType};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::send_protocol";

#[derive(Debug, PartialEq)]
pub enum TransactionProtocolStage {
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
    stage: TransactionProtocolStage,
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
        stage: TransactionProtocolStage,
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
            "Starting Transaction Send protocol for TxId: {} at Stage {:?}",
            self.id, self.stage
        );

        // Only Send the transaction if the protocol stage is Initial. If the protocol is started in a later stage
        // ignore this
        if self.stage == TransactionProtocolStage::Initial {
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

            self.send_transaction(msg, true).await?;

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

        // Add receiver to Liveness Service to monitor for liveness
        let mut liveness_event_stream = self.resources.liveness_service.get_event_stream_fused();
        let destination_node_id = NodeId::from_key(&self.dest_pubkey)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        self.resources
            .liveness_service
            .add_node_id(destination_node_id.clone())
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        #[allow(unused_assignments)]
        let mut reply = None;
        loop {
            futures::select! {
                (spk, rr) = receiver.select_next_some() => {
                    let rr_tx_id = rr.tx_id;
                    reply = Some(rr);

                    if outbound_tx.destination_public_key != spk {
                        error!(
                            target: LOG_TARGET,
                            "Transaction Reply did not come from the expected Public Key"
                        );
                    } else if !outbound_tx.sender_protocol.check_tx_id(rr_tx_id) {
                        error!(target: LOG_TARGET, "Transaction Reply does not have the correct TxId");
                    } else {
                        break;
                    }
                },
                liveness_event = liveness_event_stream.select_next_some() => {
                    if let Ok(event) = liveness_event {
                        if let LivenessEvent::ReceivedPong(pong_event) = (*event).clone() {
                            if pong_event.node_id == destination_node_id{
                                debug!(target: LOG_TARGET, "Pong message received from counter-party before Transaction Reply is received, resending transaction.");
                                let msg = self.sender_protocol.get_single_round_message().map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
                                self.send_transaction(msg, false).await?;
                            }
                        }
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

        // TODO Actually monitor the send status of this message
        self.resources
            .outbound_message_service
            .send_direct(
                outbound_tx.destination_public_key.clone(),
                OutboundEncryption::None,
                OutboundDomainMessage::new(
                    TariMessageType::TransactionFinalized,
                    finalized_transaction_message.clone(),
                ),
            )
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        // TODO Monitor the final send result of this process
        match self
            .resources
            .outbound_message_service
            .broadcast(
                NodeDestination::NodeId(Box::new(NodeId::from_key(&self.dest_pubkey).map_err(|e| {
                    TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
                })?)),
                OutboundEncryption::EncryptFor(Box::new(self.dest_pubkey.clone())),
                vec![],
                OutboundDomainMessage::new(
                    TariMessageType::TransactionFinalized,
                    finalized_transaction_message.clone(),
                ),
            )
            .await
        {
            Ok(result) => match result.resolve_ok().await {
                None => {
                    error!(
                        target: LOG_TARGET,
                        "Sending Finalized Transaction (TxId: {}) to neighbours for Store and Forward failed", self.id
                    );
                },
                Some(tags) if !tags.is_empty() => {
                    info!(
                        target: LOG_TARGET,
                        "Sending Finalized Transaction (TxId: {}) to Neighbours for Store and Forward successful with \
                         Message Tags: {:?}",
                        tx_id,
                        tags,
                    );
                },
                Some(_) => {
                    error!(
                        target: LOG_TARGET,
                        "Sending Finalized Transaction to Neighbours for Store and Forward for TX_ID: {} was \
                         unsuccessful and no messages were sent",
                        tx_id
                    );
                },
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Sending Finalized Transaction (TxId: {}) to neighbours for Store and Forward failed: {:?}",
                    self.id,
                    e
                );
            },
        };

        Ok(self.id)
    }

    /// Contains all the logic to initially send the transaction to the recipient directly and via SAF
    async fn send_transaction(
        &mut self,
        msg: SingleRoundSenderData,
        send_events: bool,
    ) -> Result<(), TransactionServiceProtocolError>
    {
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
                Some(send_states) if send_states.len() == 1 => {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) Direct Send to {} successful with Message Tag: {:?}",
                        self.id,
                        self.dest_pubkey,
                        send_states[0].tag,
                    );
                    direct_send_success = true;

                    let event_publisher = self.resources.event_publisher.clone();
                    let tx_id = self.id;
                    // Launch a task to monitor if the message gets sent
                    tokio::spawn(async move {
                        match send_states.wait_single().await {
                            true => {
                                info!(
                                    target: LOG_TARGET,
                                    "Direct Send process for TX_ID: {} was successful", tx_id
                                );
                                let _ = event_publisher
                                    .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, true)));
                            },
                            false => {
                                error!(
                                    target: LOG_TARGET,
                                    "Direct Send process for TX_ID: {} was unsuccessful and no message was sent", tx_id
                                );
                                if send_events {
                                    let _ = event_publisher
                                        .send(Arc::new(TransactionEvent::TransactionDirectSendResult(tx_id, false)));
                                }
                            },
                        }
                    });
                },
                _ => {
                    if send_events {
                        let _ = self
                            .resources
                            .event_publisher
                            .send(Arc::new(TransactionEvent::TransactionDirectSendResult(self.id, false)));
                    }
                    error!(
                        target: LOG_TARGET,
                        "Transaction Send Direct for TxID: {} failed", self.id
                    );
                },
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Direct Transaction Send failed: {:?}", e);
                if send_events {
                    let _ = self
                        .resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionDirectSendResult(self.id, false)));
                }
            },
        };

        // TODO Actually monitor the send status of this message
        let mut store_and_forward_send_success = false;
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
                        self.id,
                        tags,
                    );
                    store_and_forward_send_success = true;
                },
                Some(_) => {
                    error!(
                        target: LOG_TARGET,
                        "Transaction Send to Neighbours for Store and Forward for TX_ID: {} was unsuccessful and no \
                         messages were sent",
                        self.id
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
            error!(
                target: LOG_TARGET,
                "Failed to Send Transaction (TxId: {}) both Directly or via Store and Forward. Pending Transaction \
                 will be cancelled",
                self.id
            );
            if let Err(e) = self.resources.output_manager_service.cancel_transaction(self.id).await {
                error!(
                    target: LOG_TARGET,
                    "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}", self.id, e
                );
            };
            if send_events {
                let _ =
                    self.resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                            self.id, false,
                        )));
            }
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::OutboundSendFailure,
            ));
        }
        if send_events {
            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                    self.id, true,
                )));
        }
        Ok(())
    }
}

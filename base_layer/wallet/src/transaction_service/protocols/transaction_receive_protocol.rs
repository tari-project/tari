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

use crate::{
    output_manager_service::TxId,
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::TransactionEvent,
        service::TransactionServiceResources,
        storage::database::{
            CompletedTransaction,
            InboundTransaction,
            TransactionBackend,
            TransactionDirection,
            TransactionStatus,
        },
    },
};
use chrono::Utc;
use futures::{
    channel::{mpsc, oneshot},
    future::FutureExt,
    StreamExt,
};
use log::*;
use rand::rngs::OsRng;
use std::sync::Arc;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{MessageSendStates, OutboundEncryption, SendMessageResponse},
};
use tari_core::transactions::{
    transaction::Transaction,
    transaction_protocol::{proto, recipient::RecipientState, sender::TransactionSenderMessage},
    types::PrivateKey,
    OutputFeatures,
    ReceiverTransactionProtocol,
};
use tari_crypto::keys::SecretKey;
use tari_p2p::tari_message::TariMessageType;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::receive_protocol";

#[derive(Debug, PartialEq)]
pub enum TransactionReceiveProtocolStage {
    Initial,
    WaitForFinalize,
}

pub struct TransactionReceiveProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    source_pubkey: CommsPublicKey,
    sender_message: TransactionSenderMessage,
    stage: TransactionReceiveProtocolStage,
    resources: TransactionServiceResources<TBackend>,
    transaction_finalize_receiver: Option<mpsc::Receiver<(CommsPublicKey, TxId, Transaction)>>,
    cancellation_receiver: Option<oneshot::Receiver<()>>,
}

impl<TBackend> TransactionReceiveProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        source_pubkey: CommsPublicKey,
        sender_message: TransactionSenderMessage,
        stage: TransactionReceiveProtocolStage,
        resources: TransactionServiceResources<TBackend>,
        transaction_finalize_receiver: mpsc::Receiver<(CommsPublicKey, TxId, Transaction)>,
        cancellation_receiver: oneshot::Receiver<()>,
    ) -> Self
    {
        Self {
            id,
            source_pubkey,
            sender_message,
            stage,
            resources,
            transaction_finalize_receiver: Some(transaction_finalize_receiver),
            cancellation_receiver: Some(cancellation_receiver),
        }
    }

    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Starting Transaction Receive protocol for TxId: {} at Stage {:?}", self.id, self.stage
        );

        match self.stage {
            TransactionReceiveProtocolStage::Initial => {
                self.accept_transaction().await?;
                self.wait_for_finalization().await?;
            },
            TransactionReceiveProtocolStage::WaitForFinalize => {
                self.wait_for_finalization().await?;
            },
        }

        Ok(self.id)
    }

    async fn accept_transaction(&mut self) -> Result<(), TransactionServiceProtocolError> {
        // Currently we will only reply to a Single sender transaction protocol
        if let TransactionSenderMessage::Single(data) = self.sender_message.clone() {
            // Check this is not a repeat message i.e. tx_id doesn't already exist in our pending or completed
            // transactions
            if self
                .resources
                .db
                .transaction_exists(data.tx_id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?
            {
                trace!(
                    target: LOG_TARGET,
                    "Received Transaction (TxId: {}) already present in database.",
                    data.tx_id,
                );
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::RepeatedMessageError,
                ));
            }

            let amount = data.amount;

            let spending_key = self
                .resources
                .output_manager_service
                .get_recipient_spending_key(data.tx_id, data.amount)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
            let nonce = PrivateKey::random(&mut OsRng);

            let rtp = ReceiverTransactionProtocol::new(
                self.sender_message.clone(),
                nonce,
                spending_key,
                OutputFeatures::default(),
                &self.resources.factories,
            );
            let recipient_reply = rtp
                .get_signed_data()
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?
                .clone();

            let mut store_and_forward_send_result = false;
            let mut direct_send_result = false;

            let tx_id = recipient_reply.tx_id;
            let proto_message: proto::RecipientSignedMessage = recipient_reply.into();
            match self
                .resources
                .outbound_message_service
                .send_direct(
                    self.source_pubkey.clone(),
                    OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, proto_message.clone()),
                )
                .await
            {
                Ok(result) => match result {
                    SendMessageResponse::Queued(send_states) => {
                        if self.wait_on_dial(send_states).await {
                            direct_send_result = true;
                        } else {
                            store_and_forward_send_result = self
                                .send_transaction_reply_store_and_forward(
                                    tx_id,
                                    self.source_pubkey.clone(),
                                    proto_message.clone(),
                                )
                                .await
                                .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;
                        }
                    },
                    SendMessageResponse::Failed(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Transaction Reply Send Direct for TxID {} failed: {}", self.id, err
                        );
                        store_and_forward_send_result = self
                            .send_transaction_reply_store_and_forward(
                                tx_id,
                                self.source_pubkey.clone(),
                                proto_message.clone(),
                            )
                            .await
                            .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;
                    },
                    SendMessageResponse::PendingDiscovery(rx) => {
                        store_and_forward_send_result = self
                            .send_transaction_reply_store_and_forward(
                                tx_id,
                                self.source_pubkey.clone(),
                                proto_message.clone(),
                            )
                            .await
                            .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;
                        // now wait for discovery to complete
                        match rx.await {
                            Ok(send_msg_response) => {
                                if let SendMessageResponse::Queued(send_states) = send_msg_response {
                                    debug!(
                                        target: LOG_TARGET,
                                        "Discovery of {} completed for TxID: {}", self.source_pubkey, self.id
                                    );
                                    direct_send_result = self.wait_on_dial(send_states).await;
                                }
                            },
                            Err(e) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Error waiting for Discovery while sending message to TxId: {} {:?}", self.id, e
                                );
                            },
                        }
                    },
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "Direct Transaction Reply Send failed: {:?}", e);
                },
            }

            // Otherwise add it to our pending transaction list and return reply
            let inbound_transaction = InboundTransaction::new(
                tx_id,
                self.source_pubkey.clone(),
                amount,
                rtp.clone(),
                TransactionStatus::Pending,
                data.message.clone(),
                Utc::now().naive_utc(),
            );
            self.resources
                .db
                .add_pending_inbound_transaction(tx_id, inbound_transaction.clone())
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            if !direct_send_result && !store_and_forward_send_result {
                error!(
                    target: LOG_TARGET,
                    "Transaction with TX_ID = {} received from {}. Reply could not be sent!", tx_id, self.source_pubkey,
                );
            } else {
                info!(
                    target: LOG_TARGET,
                    "Transaction with TX_ID = {} received from {}. Reply Sent", tx_id, self.source_pubkey,
                );
            }

            trace!(
                target: LOG_TARGET,
                "Transaction (TX_ID: {}) - Amount: {} - Message: {}",
                tx_id,
                amount,
                data.message,
            );

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::ReceivedTransaction(tx_id)))
                .map_err(|e| {
                    trace!(target: LOG_TARGET, "Error sending event due to no subscribers: {:?}", e);
                    e
                });
            Ok(())
        } else {
            Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::InvalidStateError,
            ))
        }
    }

    async fn send_transaction_reply_store_and_forward(
        &mut self,
        tx_id: TxId,
        source_pubkey: CommsPublicKey,
        msg: proto::RecipientSignedMessage,
    ) -> Result<bool, TransactionServiceError>
    {
        match self
            .resources
            .outbound_message_service
            .broadcast(
                NodeDestination::NodeId(Box::new(NodeId::from_key(&source_pubkey)?)),
                OutboundEncryption::EncryptFor(Box::new(source_pubkey.clone())),
                vec![],
                OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, msg),
            )
            .await
        {
            Ok(send_states) => {
                info!(
                    target: LOG_TARGET,
                    "Sending Transaction Reply (TxId: {}) to Neighbours for Store and Forward successful with Message \
                     Tags: {:?}",
                    tx_id,
                    send_states.to_tags(),
                );
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Sending Transaction Reply (TxId: {}) to neighbours for Store and Forward failed: {:?}", tx_id, e,
                );
            },
        };

        Ok(true)
    }

    /// This function contains the logic to wait on a dial and send of a queued message
    async fn wait_on_dial(&self, send_states: MessageSendStates) -> bool {
        if send_states.len() == 1 {
            debug!(
                target: LOG_TARGET,
                "Transaction Reply (TxId: {}) Direct Send to {} queued with Message {}",
                self.id,
                self.source_pubkey,
                send_states[0].tag,
            );
            let (sent, failed) = send_states
                .wait_n_timeout(self.resources.config.direct_send_timeout, 1)
                .await;
            if !sent.is_empty() {
                info!(
                    target: LOG_TARGET,
                    "Direct Send process of Transaction Reply TX_ID: {} was successful with Message: {}",
                    self.id,
                    sent[0]
                );
                true
            } else {
                if failed.is_empty() {
                    warn!(
                        target: LOG_TARGET,
                        "Direct Send process for Transaction Reply TX_ID: {} timed out", self.id
                    );
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Direct Send process for Transaction Reply TX_ID: {} and Message {} was unsuccessful and no \
                         message was sent",
                        self.id,
                        failed[0]
                    );
                }
                false
            }
        } else {
            warn!(
                target: LOG_TARGET,
                "Transaction Reply Send Direct for TxID: {} failed", self.id
            );
            false
        }
    }

    async fn wait_for_finalization(&mut self) -> Result<(), TransactionServiceProtocolError> {
        let mut receiver = self
            .transaction_finalize_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut cancellation_receiver = self
            .cancellation_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

        let inbound_tx = match self.resources.db.get_pending_inbound_transaction(self.id).await {
            Ok(tx) => tx,
            Err(_e) => {
                debug!(
                    target: LOG_TARGET,
                    "TxId for received Finalized Transaction does not exist in Pending Inbound Transactions, could be \
                     a repeat Store and Forward message"
                );
                return Ok(());
            },
        };

        #[allow(unused_assignments)]
        let mut incoming_finalized_transaction = None;
        loop {
            loop {
                futures::select! {
                    (spk, tx_id, tx) = receiver.select_next_some() => {
                        incoming_finalized_transaction = Some(tx);
                        if inbound_tx.source_public_key != spk {
                            warn!(
                                target: LOG_TARGET,
                                "Finalized Transaction did not come from the expected Public Key"
                            );
                        } else if tx_id != inbound_tx.tx_id || tx_id != self.id {
                            debug!(target: LOG_TARGET, "Finalized Transaction does not have the correct TxId");
                        } else {
                            break;
                        }
                    },
                     _ = cancellation_receiver => {
                        info!(target: LOG_TARGET, "Cancelling Transaction Receive Protocol for TxId: {}", self.id);
                        return Err(TransactionServiceProtocolError::new(
                            self.id,
                            TransactionServiceError::TransactionCancelled,
                        ));
                    }
                }
            }

            let finalized_transaction: Transaction = incoming_finalized_transaction.ok_or_else(|| {
                TransactionServiceProtocolError::new(self.id, TransactionServiceError::TransactionCancelled)
            })?;

            info!(
                target: LOG_TARGET,
                "Finalized Transaction with TX_ID = {} received from {}",
                self.id,
                self.source_pubkey.clone()
            );

            let rtp_output = match inbound_tx.receiver_protocol.state.clone() {
                RecipientState::Finalized(s) => s.output,
                RecipientState::Failed(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "Finalized Transaction TxId: {} is not in the correct state to be completed", self.id
                    );
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::InvalidStateError,
                    ));
                },
            };

            let finalized_outputs = finalized_transaction.body.outputs();

            if finalized_outputs.iter().find(|o| o == &&rtp_output).is_none() {
                warn!(
                    target: LOG_TARGET,
                    "Finalized Transaction does not contain the Receiver's output"
                );
                continue;
            }

            let completed_transaction = CompletedTransaction::new(
                self.id,
                self.source_pubkey.clone(),
                self.resources.node_identity.public_key().clone(),
                inbound_tx.amount,
                finalized_transaction.body.get_total_fee(),
                finalized_transaction.clone(),
                TransactionStatus::Completed,
                inbound_tx.message.clone(),
                inbound_tx.timestamp,
                TransactionDirection::Inbound,
                None,
            );

            self.resources
                .db
                .complete_inbound_transaction(self.id, completed_transaction.clone())
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            info!(
                target: LOG_TARGET,
                "Inbound Transaction with TX_ID = {} from {} moved to Completed Transactions",
                self.id,
                self.source_pubkey.clone()
            );

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(self.id)))
                .map_err(|e| {
                    trace!(target: LOG_TARGET, "Error sending event, no subscribers: {:?}", e);
                    e
                });
            break;
        }
        Ok(())
    }
}

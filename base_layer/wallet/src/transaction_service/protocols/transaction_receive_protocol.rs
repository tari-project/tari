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
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, InboundTransaction, TransactionDirection, TransactionStatus},
        },
        tasks::send_transaction_reply::send_transaction_reply,
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
use tari_comms::types::CommsPublicKey;

use tari_core::transactions::{
    transaction::Transaction,
    transaction_protocol::{recipient::RecipientState, sender::TransactionSenderMessage},
    types::PrivateKey,
    OutputFeatures,
    ReceiverTransactionProtocol,
};
use tari_crypto::keys::SecretKey;
use tokio::time::delay_for;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::receive_protocol";
const LOG_TARGET_STRESS: &str = "stress_test::receive_protocol";

#[derive(Debug, PartialEq)]
pub enum TransactionReceiveProtocolStage {
    Initial,
    WaitForFinalize,
}

pub struct TransactionReceiveProtocol<TBackend>
where TBackend: TransactionBackend + 'static
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
where TBackend: TransactionBackend + 'static
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

            let inbound_transaction = InboundTransaction::new(
                data.tx_id,
                self.source_pubkey.clone(),
                amount,
                rtp,
                TransactionStatus::Pending,
                data.message.clone(),
                Utc::now().naive_utc(),
            );

            let send_result = send_transaction_reply(
                inbound_transaction.clone(),
                self.resources.outbound_message_service.clone(),
                self.resources.config.direct_send_timeout,
            )
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;

            self.resources
                .db
                .add_pending_inbound_transaction(inbound_transaction.tx_id, inbound_transaction)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            self.resources
                .db
                .increment_send_count(self.id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            if !send_result {
                error!(
                    target: LOG_TARGET,
                    "Transaction with TX_ID = {} received from {}. Reply could not be sent!",
                    data.tx_id,
                    self.source_pubkey,
                );
            } else {
                info!(
                    target: LOG_TARGET,
                    "Transaction with TX_ID = {} received from {}. Reply Sent", data.tx_id, self.source_pubkey,
                );
            }

            trace!(
                target: LOG_TARGET,
                "Transaction (TX_ID: {}) - Amount: {} - Message: {}",
                data.tx_id,
                amount,
                data.message,
            );

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::ReceivedTransaction(data.tx_id)))
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

        // Determine the time remaining before this transaction times out
        let elapsed_time = Utc::now()
            .naive_utc()
            .signed_duration_since(inbound_tx.timestamp)
            .to_std()
            .map_err(|_| {
                TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::ConversionError("duration::OutOfRangeError".to_string()),
                )
            })?;

        let timeout_duration = match self
            .resources
            .config
            .pending_transaction_cancellation_timeout
            .checked_sub(elapsed_time)
        {
            None => {
                // This will cancel the transaction and exit this protocol
                return self.timeout_transaction().await;
            },
            Some(t) => t,
        };
        let mut timeout_delay = delay_for(timeout_duration).fuse();

        // check to see if a resend is due
        let resend = match inbound_tx.last_send_timestamp {
            None => true,
            Some(timestamp) => {
                let elapsed_time = Utc::now()
                    .naive_utc()
                    .signed_duration_since(timestamp)
                    .to_std()
                    .map_err(|_| {
                        TransactionServiceProtocolError::new(
                            self.id,
                            TransactionServiceError::ConversionError("duration::OutOfRangeError".to_string()),
                        )
                    })?;
                elapsed_time > self.resources.config.transaction_resend_period
            },
        };

        if resend {
            if let Err(e) = send_transaction_reply(
                inbound_tx.clone(),
                self.resources.outbound_message_service.clone(),
                self.resources.config.direct_send_timeout,
            )
            .await
            {
                warn!(
                    target: LOG_TARGET,
                    "Error resending Transaction Reply (TxId: {}): {:?}", self.id, e
                );
            }
            self.resources
                .db
                .increment_send_count(self.id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
        }

        let mut shutdown = self.resources.shutdown_signal.clone();

        #[allow(unused_assignments)]
        let mut incoming_finalized_transaction = None;
        loop {
            loop {
                let mut resend_timeout = delay_for(self.resources.config.transaction_resend_period).fuse();
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
                    result = cancellation_receiver => {
                        if result.is_ok() {
                            info!(target: LOG_TARGET, "Cancelling Transaction Receive Protocol for TxId: {}", self.id);
                            return Err(TransactionServiceProtocolError::new(
                                self.id,
                                TransactionServiceError::TransactionCancelled,
                            ));
                        }
                    },
                    () = resend_timeout => {
                        match send_transaction_reply(
                            inbound_tx.clone(),
                            self.resources.outbound_message_service.clone(),
                            self.resources.config.direct_send_timeout,
                        )
                        .await {
                            Ok(_) => self.resources
                                        .db
                                        .increment_send_count(self.id)
                                        .await
                                        .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?,
                            Err(e) => warn!(
                                            target: LOG_TARGET,
                                            "Error resending Transaction Reply (TxId: {}): {:?}", self.id, e
                                        ),
                        }
                    },
                    () = timeout_delay => {
                        return self.timeout_transaction().await;
                    }
                    _ = shutdown => {
                        info!(target: LOG_TARGET, "Transaction Receive Protocol (id: {}) shutting down because it received the shutdown signal", self.id);
                        return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
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
            debug!(
                target: LOG_TARGET_STRESS,
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

    async fn timeout_transaction(&mut self) -> Result<(), TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Cancelling Transaction Receive Protocol (TxId: {}) due to timeout after no counterparty response", self.id
        );

        self.resources
            .db
            .cancel_pending_transaction(self.id)
            .await
            .map_err(|e| {
                warn!(
                    target: LOG_TARGET,
                    "Pending Transaction does not exist and could not be cancelled: {:?}", e
                );
                TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
            })?;

        self.resources
            .output_manager_service
            .cancel_transaction(self.id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionCancelled(self.id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {:?}",
                    e
                );
                TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::BroadcastSendError(format!("{:?}", e)),
                )
            });

        info!(
            target: LOG_TARGET,
            "Pending Transaction (TxId: {}) timed out after no response from counterparty", self.id
        );

        Err(TransactionServiceProtocolError::new(
            self.id,
            TransactionServiceError::Timeout,
        ))
    }
}

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
use futures::FutureExt;
use log::*;
use tari_common_types::{
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::{HashOutput, PublicKey},
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, SendMessageResponse},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::KernelFeatures,
        transaction_protocol::{
            proto::protocol as proto,
            recipient::RecipientSignedMessage,
            sender::SingleRoundSenderData,
        },
        SenderTransactionProtocol,
    },
};
use tari_crypto::script;
use tari_p2p::tari_message::TariMessageType;
use tokio::{
    sync::{mpsc::Receiver, oneshot},
    time::sleep,
};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    transaction_service::{
        config::TransactionRoutingMechanism,
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::{TransactionEvent, TransactionServiceResponse},
        service::TransactionServiceResources,
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, OutboundTransaction, TxCancellationReason},
        },
        tasks::{
            send_finalized_transaction::send_finalized_transaction_message,
            send_transaction_cancelled::send_transaction_cancelled_message,
            wait_on_dial::wait_on_dial,
        },
        utc::utc_duration_since,
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::send_protocol";

#[derive(Debug, PartialEq)]
pub enum TransactionSendProtocolStage {
    Initial,
    WaitForReply,
}

pub struct TransactionSendProtocol<TBackend, TWalletConnectivity> {
    id: TxId,
    dest_pubkey: CommsPublicKey,
    amount: MicroTari,
    unique_id: Option<Vec<u8>>,
    parent_public_key: Option<PublicKey>,
    fee_per_gram: MicroTari,
    message: String,
    service_request_reply_channel: Option<oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>>,
    stage: TransactionSendProtocolStage,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity>,
    transaction_reply_receiver: Option<Receiver<(CommsPublicKey, RecipientSignedMessage)>>,
    cancellation_receiver: Option<oneshot::Receiver<()>>,
    prev_header: Option<HashOutput>,
    height: Option<u64>,
}

impl<TBackend, TWalletConnectivity> TransactionSendProtocol<TBackend, TWalletConnectivity>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        id: TxId,
        resources: TransactionServiceResources<TBackend, TWalletConnectivity>,
        transaction_reply_receiver: Receiver<(CommsPublicKey, RecipientSignedMessage)>,
        cancellation_receiver: oneshot::Receiver<()>,
        dest_pubkey: CommsPublicKey,
        amount: MicroTari,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        fee_per_gram: MicroTari,
        message: String,
        service_request_reply_channel: Option<
            oneshot::Sender<Result<TransactionServiceResponse, TransactionServiceError>>,
        >,
        stage: TransactionSendProtocolStage,
        prev_header: Option<HashOutput>,
        height: Option<u64>,
    ) -> Self {
        Self {
            id,
            resources,
            transaction_reply_receiver: Some(transaction_reply_receiver),
            cancellation_receiver: Some(cancellation_receiver),
            dest_pubkey,
            amount,
            unique_id,
            parent_public_key,
            fee_per_gram,
            message,
            service_request_reply_channel,
            stage,
            prev_header,
            height,
        }
    }

    /// Execute the Transaction Send Protocol as an async task.
    pub async fn execute(mut self) -> Result<TxId, TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Starting Transaction Send protocol for TxId: {} at Stage {:?}", self.id, self.stage
        );

        match self.stage {
            TransactionSendProtocolStage::Initial => {
                let sender_protocol = self.prepare_transaction().await?;
                self.initial_send_transaction(sender_protocol).await?;
                self.wait_for_reply().await?;
            },
            TransactionSendProtocolStage::WaitForReply => {
                self.wait_for_reply().await?;
            },
        }

        Ok(self.id)
    }

    async fn prepare_transaction(&mut self) -> Result<SenderTransactionProtocol, TransactionServiceProtocolError> {
        let service_reply_channel = match self.service_request_reply_channel.take() {
            Some(src) => src,
            None => {
                error!(
                    target: LOG_TARGET,
                    "Service Reply Channel not provided for new Send Transaction Protocol"
                );
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::ProtocolChannelError,
                ));
            },
        };

        match self
            .resources
            .output_manager_service
            .prepare_transaction_to_send(
                self.id,
                self.amount,
                self.unique_id.clone(),
                self.parent_public_key.clone(),
                self.fee_per_gram,
                None,
                self.message.clone(),
                script!(Nop),
                Covenant::default(),
            )
            .await
        {
            Ok(sp) => {
                let _ = service_reply_channel
                    .send(Ok(TransactionServiceResponse::TransactionSent(self.id)))
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send service reply");
                        e
                    });
                Ok(sp)
            },
            Err(e) => {
                let error_string = e.to_string();
                let _ = service_reply_channel
                    .send(Err(TransactionServiceError::from(e)))
                    .map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send service reply");
                        e
                    });
                Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::ServiceError(error_string),
                ))
            },
        }
    }

    async fn initial_send_transaction(
        &mut self,
        mut sender_protocol: SenderTransactionProtocol,
    ) -> Result<(), TransactionServiceProtocolError> {
        if !sender_protocol.is_single_round_message_ready() {
            error!(target: LOG_TARGET, "Sender Transaction Protocol is in an invalid state");
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::InvalidStateError,
            ));
        }

        let msg = sender_protocol
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

        if direct_send_result || store_and_forward_send_result {
            self.resources
                .output_manager_service
                .confirm_pending_transaction(self.id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            let fee = sender_protocol
                .get_fee_amount()
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
            let outbound_tx = OutboundTransaction::new(
                tx_id,
                self.dest_pubkey.clone(),
                self.amount,
                fee,
                sender_protocol.clone(),
                TransactionStatus::Pending,
                self.message.clone(),
                Utc::now().naive_utc(),
                direct_send_result,
            );
            info!(
                target: LOG_TARGET,
                "Pending Outbound Transaction TxId: {:?} added. Waiting for Reply or Cancellation", self.id,
            );
            self.resources
                .db
                .add_pending_outbound_transaction(outbound_tx.tx_id, outbound_tx)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            self.resources
                .db
                .increment_send_count(self.id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
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
            return Err(TransactionServiceProtocolError::new(
                self.id,
                TransactionServiceError::OutboundSendFailure,
            ));
        }

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

        // Determine the time remaining before this transaction times out
        let elapsed_time = utc_duration_since(&outbound_tx.timestamp)
            .map_err(|e| TransactionServiceProtocolError::new(self.id, e.into()))?;

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
        let timeout_delay = sleep(timeout_duration).fuse();
        tokio::pin!(timeout_delay);

        // check to see if a resend is due
        let resend = match outbound_tx.last_send_timestamp {
            None => true,
            Some(timestamp) => {
                let elapsed_time = utc_duration_since(&timestamp)
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, e.into()))?;
                elapsed_time > self.resources.config.transaction_resend_period
            },
        };

        if resend {
            if let Err(e) = self
                .send_transaction(
                    outbound_tx
                        .sender_protocol
                        .get_single_round_message()
                        .map_err(|e| TransactionServiceProtocolError::new(self.id, e.into()))?,
                )
                .await
            {
                warn!(
                    target: LOG_TARGET,
                    "Error resending Transaction (TxId: {}): {:?}", self.id, e
                );
            }
            self.resources
                .db
                .increment_send_count(self.id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, e.into()))?;
        }

        let mut shutdown = self.resources.shutdown_signal.clone();
        #[allow(unused_assignments)]
        let mut reply = None;
        loop {
            let resend_timeout = sleep(self.resources.config.transaction_resend_period).fuse();
            tokio::select! {
                Some((spk, rr)) = receiver.recv() => {
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
                result = &mut cancellation_receiver => {
                    if result.is_ok() {
                        info!(target: LOG_TARGET, "Cancelling Transaction Send Protocol (TxId: {})", self.id);
                        let _ = send_transaction_cancelled_message(self.id,self.dest_pubkey.clone(), self.resources.outbound_message_service.clone(), ).await.map_err(|e| {
                            warn!(
                                target: LOG_TARGET,
                                "Error sending Transaction Cancelled (TxId: {}) message: {:?}", self.id, e
                            )
                        });
                        self.resources
                            .db
                            .increment_send_count(self.id)
                            .await
                            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
                        return Err(TransactionServiceProtocolError::new(
                            self.id,
                            TransactionServiceError::TransactionCancelled,
                        ));
                    }
                },
                () = resend_timeout => {
                    if let Err(e) = self.send_transaction(outbound_tx.sender_protocol.get_single_round_message().map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?).await {
                        warn!(
                            target: LOG_TARGET,
                            "Error resending Transaction (TxId: {}): {:?}", self.id, e
                        );
                    } else {
                        self.resources
                            .db
                            .increment_send_count(self.id)
                            .await
                            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
                    }
                },
                () = &mut timeout_delay => {
                    return self.timeout_transaction().await;
                }
                _ = shutdown.wait() => {
                    info!(target: LOG_TARGET, "Transaction Send Protocol (id: {}) shutting down because it received the shutdown signal", self.id);
                    return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
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

        outbound_tx
            .sender_protocol
            .finalize(
                KernelFeatures::empty(),
                &self.resources.factories,
                self.prev_header.clone(),
                self.height.unwrap_or(u64::MAX),
            )
            .map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) could not be finalized. Failure error: {:?}", self.id, e,
                );
                TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
            })?;

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
            None,
            None,
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

        send_finalized_transaction_message(
            tx_id,
            tx.clone(),
            self.dest_pubkey.clone(),
            self.resources.outbound_message_service.clone(),
            self.resources.config.direct_send_timeout,
            self.resources.config.transaction_routing_mechanism,
        )
        .await
        .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;

        self.resources
            .db
            .increment_send_count(tx_id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

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

    /// Attempt to send the transaction to the recipient either directly, via Store-and-forward or both as per config
    /// setting. If the selected sending mechanism fail to send the transaction will be cancelled.
    /// # Argumentswallet_sync_with_base_node
    /// `msg`: The transaction data message to be sent
    async fn send_transaction(
        &mut self,
        msg: SingleRoundSenderData,
    ) -> Result<SendResult, TransactionServiceProtocolError> {
        let mut result = SendResult {
            direct_send_result: false,
            store_and_forward_send_result: false,
        };

        match self.resources.config.transaction_routing_mechanism {
            TransactionRoutingMechanism::DirectOnly | TransactionRoutingMechanism::DirectAndStoreAndForward => {
                result = self.send_transaction_direct(msg.clone()).await?;
            },
            TransactionRoutingMechanism::StoreAndForwardOnly => {
                result.store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
            },
        };

        Ok(result)
    }

    /// Attempt to send the transaction to the recipient both directly and via Store-and-forward. If both fail to send
    /// the transaction will be cancelled.
    /// # Argumentswallet_sync_with_base_node
    /// `msg`: The transaction data message to be sent
    async fn send_transaction_direct(
        &mut self,
        msg: SingleRoundSenderData,
    ) -> Result<SendResult, TransactionServiceProtocolError> {
        let proto_message = proto::TransactionSenderMessage::single(msg.clone().into());
        let mut store_and_forward_send_result = false;
        let mut direct_send_result = false;

        info!(
            target: LOG_TARGET,
            "Attempting to Send Transaction (TxId: {}) to recipient with Public Key: {}", self.id, self.dest_pubkey,
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
                    if wait_on_dial(
                        send_states,
                        self.id,
                        self.dest_pubkey.clone(),
                        "Transaction",
                        self.resources.config.direct_send_timeout,
                    )
                    .await
                    {
                        direct_send_result = true;
                    }
                    // Send a Store and Forward (SAF) regardless. Empirical testing determined
                    // that in some cases a direct send would be reported as true, even though the wallet
                    // was offline. Possibly due to the Tor connection remaining active for a few
                    // minutes after wallet shutdown.
                    info!(
                        target: LOG_TARGET,
                        "Direct Send result was {}. Sending SAF for TxId: {} to recipient with Public Key: {}",
                        direct_send_result,
                        self.id,
                        self.dest_pubkey,
                    );
                    store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                },
                SendMessageResponse::Failed(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Transaction Send Direct for TxID {} failed: {}", self.id, err
                    );
                    store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                },
                SendMessageResponse::PendingDiscovery(rx) => {
                    let _ = self
                        .resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::TransactionDiscoveryInProgress(self.id)));
                    store_and_forward_send_result = self.send_transaction_store_and_forward(msg.clone()).await?;
                    // now wait for discovery to complete
                    match rx.await {
                        Ok(SendMessageResponse::Queued(send_states)) => {
                            debug!(
                                target: LOG_TARGET,
                                "Discovery of {} completed for TxID: {}", self.dest_pubkey, self.id
                            );
                            direct_send_result = wait_on_dial(
                                send_states,
                                self.id,
                                self.dest_pubkey.clone(),
                                "Transaction",
                                self.resources.config.direct_send_timeout,
                            )
                            .await;
                        },
                        Ok(SendMessageResponse::Failed(e)) => warn!(
                            target: LOG_TARGET,
                            "Failed to send message ({}) for TxId: {}", e, self.id
                        ),
                        Ok(SendMessageResponse::PendingDiscovery(_)) => unreachable!(),
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

        Ok(SendResult {
            direct_send_result,
            store_and_forward_send_result,
        })
    }

    /// Contains all the logic to send the transaction to the recipient via store and forward
    /// # Arguments
    /// `msg`: The transaction data message to be sent
    /// 'send_events': A bool indicating whether we should send events during the operation or not.
    async fn send_transaction_store_and_forward(
        &mut self,
        msg: SingleRoundSenderData,
    ) -> Result<bool, TransactionServiceProtocolError> {
        if self.resources.config.transaction_routing_mechanism == TransactionRoutingMechanism::DirectOnly {
            return Ok(false);
        }
        let proto_message = proto::TransactionSenderMessage::single(msg.into());
        match self
            .resources
            .outbound_message_service
            .closest_broadcast(
                NodeId::from_public_key(&self.dest_pubkey),
                OutboundEncryption::encrypt_for(self.dest_pubkey.clone()),
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

    async fn timeout_transaction(&mut self) -> Result<(), TransactionServiceProtocolError> {
        info!(
            target: LOG_TARGET,
            "Cancelling Transaction Send Protocol (TxId: {}) due to timeout after no counterparty response", self.id
        );
        let _ = send_transaction_cancelled_message(
            self.id,
            self.dest_pubkey.clone(),
            self.resources.outbound_message_service.clone(),
        )
        .await
        .map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Error sending Transaction Cancelled (TxId: {}) message: {:?}", self.id, e
            )
        });
        self.resources
            .db
            .increment_send_count(self.id)
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

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
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                self.id,
                TxCancellationReason::Timeout,
            )))
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

struct SendResult {
    direct_send_result: bool,
    store_and_forward_send_result: bool,
}

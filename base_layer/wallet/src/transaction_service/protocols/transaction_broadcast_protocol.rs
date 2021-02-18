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
            models::{CompletedTransaction, TransactionStatus},
        },
    },
};
use futures::{FutureExt, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletRpcClient,
    },
    transactions::{transaction::Transaction, types::Signature},
};
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::broadcast_protocol";

pub struct TransactionBroadcastProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    tx_id: TxId,
    mode: TxBroadcastMode,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    timeout_update_receiver: Option<broadcast::Receiver<Duration>>,
    base_node_update_receiver: Option<broadcast::Receiver<CommsPublicKey>>,
    first_rejection: bool,
}

impl<TBackend> TransactionBroadcastProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub fn new(
        tx_id: TxId,
        resources: TransactionServiceResources<TBackend>,
        timeout: Duration,
        base_node_public_key: CommsPublicKey,
        timeout_update_receiver: broadcast::Receiver<Duration>,
        base_node_update_receiver: broadcast::Receiver<CommsPublicKey>,
    ) -> Self
    {
        Self {
            tx_id,
            mode: TxBroadcastMode::TransactionSubmission,
            resources,
            timeout,
            base_node_public_key,
            timeout_update_receiver: Some(timeout_update_receiver),
            base_node_update_receiver: Some(base_node_update_receiver),
            first_rejection: false,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut timeout_update_receiver = self
            .timeout_update_receiver
            .take()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidStateError)
            })?
            .fuse();

        let mut base_node_update_receiver = self
            .base_node_update_receiver
            .take()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidStateError)
            })?
            .fuse();

        let mut shutdown = self.resources.shutdown_signal.clone();
        // Main protocol loop
        loop {
            let base_node_node_id = NodeId::from_key(&self.base_node_public_key.clone())
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;
            let mut connection: Option<PeerConnection> = None;

            let delay = delay_for(self.timeout);

            debug!(
                target: LOG_TARGET,
                "Connecting to Base Node (Public Key: {})", self.base_node_public_key,
            );
            futures::select! {
                dial_result = self.resources.connectivity_manager.dial_peer(base_node_node_id.clone()).fuse() => {
                    match dial_result {
                        Ok(base_node_connection) => {
                            connection = Some(base_node_connection);
                        },
                        Err(e) => {
                            info!(target: LOG_TARGET, "Problem connecting to base node: {} for Transaction Broadcast Protocol (TxId: {})", e, self.tx_id);
                            let _ = self
                            .resources
                            .event_publisher
                            .send(Arc::new(TransactionEvent::TransactionBaseNodeConnectionProblem(
                                self.tx_id,
                            )))
                            .map_err(|e| {
                                trace!(
                                    target: LOG_TARGET,
                                    "Error sending event because there are no subscribers: {:?}",
                                    e
                                );
                                e
                            });
                        },
                    }
                },
                updated_timeout = timeout_update_receiver.select_next_some() => {
                    match updated_timeout {
                        Ok(to) => {
                            self.timeout = to;
                             info!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol (TxId: {}) timeout updated to {:?}", self.tx_id, self.timeout
                            );
                        },
                        Err(e) => {
                            trace!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol (TxId: {}) event 'updated_timeout' triggered with error: {:?}",
                                self.tx_id,
                                e,
                            );
                        }
                    }
                },
                new_base_node = base_node_update_receiver.select_next_some() => {
                    match new_base_node {
                        Ok(bn) => {
                            self.base_node_public_key = bn;
                             info!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol (TxId: {}) Base Node Public key updated to {:?}", self.tx_id, self.base_node_public_key
                            );
                            self.first_rejection = false;
                            continue;
                        },
                        Err(e) => {
                            trace!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol (TxId: {}) event 'base_node_update' triggered with error: {:?}",
                                self.tx_id,
                                e,
                            );
                        }
                    }
                }
                _ = shutdown => {
                    info!(target: LOG_TARGET, "Transaction Broadcast Protocol (TxId: {}) shutting down because it received the shutdown signal", self.tx_id);
                    return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                },
            }

            let mut base_node_connection = match connection {
                None => {
                    futures::select! {
                        _ = delay.fuse() => {
                            continue;
                        },
                        _ = shutdown => {
                            info!(target: LOG_TARGET, "Transaction Broadcast Protocol (TxId: {}) shutting down because it received the shutdown signal", self.tx_id);
                            return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                        },
                    }
                },
                Some(c) => c,
            };

            let completed_tx = match self.resources.db.get_completed_transaction(self.tx_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Cannot find Completed Transaction (TxId: {}) referred to by this Broadcast protocol: {:?}",
                        self.tx_id,
                        e
                    );
                    return Err(TransactionServiceProtocolError::new(
                        self.tx_id,
                        TransactionServiceError::TransactionDoesNotExistError,
                    ));
                },
            };

            if !(completed_tx.status == TransactionStatus::Completed ||
                completed_tx.status == TransactionStatus::Broadcast ||
                completed_tx.status == TransactionStatus::MinedUnconfirmed)
            {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) no longer in Completed state and will stop being broadcast", self.tx_id
                );
                return Ok(self.tx_id);
            }

            let mut client = match base_node_connection
                .connect_rpc_using_builder(
                    BaseNodeWalletRpcClient::builder()
                        .with_deadline(self.resources.config.broadcast_monitoring_timeout),
                )
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(target: LOG_TARGET, "Problem establishing RPC connection: {}", e);
                    delay.await;
                    continue;
                },
            };

            let delay = delay_for(self.timeout);
            loop {
                futures::select! {
                    new_base_node = base_node_update_receiver.select_next_some() => {
                        match new_base_node {
                            Ok(bn) => {
                                self.base_node_public_key = bn;
                                 info!(
                                    target: LOG_TARGET,
                                    "Transaction Broadcast protocol (TxId: {}) Base Node Public key updated to {:?}", self.tx_id, self.base_node_public_key
                                );
                                self.first_rejection = false;
                                continue;
                            },
                            Err(e) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "Transaction Broadcast protocol (TxId: {}) event 'base_node_update' triggered with error: {:?}",
                                    self.tx_id,
                                    e,
                                );
                            }
                        }
                    },
                    result = self.query_or_submit_transaction(completed_tx.clone(), &mut client).fuse() => {
                        match self.mode {
                            TxBroadcastMode::TransactionSubmission => {
                                if result? {
                                    self.mode = TxBroadcastMode::TransactionQuery;
                                }
                            },
                            TxBroadcastMode::TransactionQuery => {
                                if result? {
                                    // We are done!
                                    self.resources
                                        .output_manager_service
                                        .confirm_transaction(
                                            completed_tx.tx_id,
                                            completed_tx.transaction.body.inputs().clone(),
                                            completed_tx.transaction.body.outputs().clone(),
                                        )
                                        .await
                                        .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;

                                    self.resources
                                        .db
                                        .confirm_broadcast_transaction(completed_tx.tx_id)
                                        .await
                                        .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;

                                   let _ = self
                                        .resources
                                        .event_publisher
                                        .send(Arc::new(TransactionEvent::TransactionMined(self.tx_id)))
                                        .map_err(|e| {
                                            trace!(
                                                target: LOG_TARGET,
                                                "Error sending event because there are no subscribers: {:?}",
                                                e
                                            );
                                            e
                                        });

                                    return Ok(self.tx_id)
                                }
                            },
                        }
                        // Wait out the remainder of the delay before proceeding with next loop
                        delay.await;
                        break;
                    },
                    updated_timeout = timeout_update_receiver.select_next_some() => {
                        if let Ok(to) = updated_timeout {
                            self.timeout = to;
                             info!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol (TxId: {}) timeout updated to {:?}", self.tx_id, self.timeout
                            );
                            break;
                        } else {
                            trace!(
                                target: LOG_TARGET,
                                "Transaction Broadcast protocol event 'updated_timeout' triggered (TxId: {}) ({:?})",
                                self.tx_id,
                                updated_timeout,
                            );
                        }
                    },
                    _ = shutdown => {
                        info!(target: LOG_TARGET, "Transaction Broadcast Protocol (TxId: {}) shutting down because it received the shutdown signal", self.tx_id);
                        return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                    },
                }
            }
        }
    }

    /// Attempt to submit the transaction to the base node via RPC.
    /// # Returns:
    /// `Ok(true)` => Transaction was successfully submitted to UnconfirmedPool
    /// `Ok(false)` => There was a problem with the RPC call and this should be retried
    /// `Err(_)` => The transaction was rejected by the base node and the protocol should end.
    async fn submit_transaction(
        &mut self,
        tx: Transaction,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        let response = match client.submit_transaction(tx.into()).await {
            Ok(r) => match TxSubmissionResponse::try_from(r) {
                Ok(r) => r,
                Err(_) => {
                    trace!(target: LOG_TARGET, "Could not convert proto TxSubmission Response");
                    return Ok(false);
                },
            },
            Err(e) => {
                info!(
                    target: LOG_TARGET,
                    "Submit Transaction RPC Call to Base Node failed: {}", e
                );
                return Ok(false);
            },
        };

        if !response.is_synced {
            info!(
                target: LOG_TARGET,
                "Base Node reports not being synced, submission will be retried."
            );
            return Ok(false);
        }

        if !response.accepted && response.rejection_reason != TxSubmissionRejectionReason::AlreadyMined {
            error!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) rejected by Base Node for reason: {}", self.tx_id, response.rejection_reason
            );

            self.cancel_transaction().await;

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionCancelled(self.tx_id)))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event because there are no subscribers: {:?}",
                        e
                    );
                    e
                });

            let reason = match response.rejection_reason {
                TxSubmissionRejectionReason::None | TxSubmissionRejectionReason::ValidationFailed => {
                    TransactionServiceError::MempoolRejectionInvalidTransaction
                },
                TxSubmissionRejectionReason::DoubleSpend => TransactionServiceError::MempoolRejectionDoubleSpend,
                TxSubmissionRejectionReason::Orphan => TransactionServiceError::MempoolRejectionOrphan,
                TxSubmissionRejectionReason::TimeLocked => TransactionServiceError::MempoolRejectionTimeLocked,
                _ => TransactionServiceError::UnexpectedBaseNodeResponse,
            };
            return Err(TransactionServiceProtocolError::new(self.tx_id, reason));
        } else if response.rejection_reason == TxSubmissionRejectionReason::AlreadyMined {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) is Already Mined according to Base Node.", self.tx_id
            );
            self.resources
                .db
                .mine_completed_transaction(self.tx_id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;
        } else {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) successfully submitted to UnconfirmedPool", self.tx_id
            );
            self.resources
                .db
                .broadcast_completed_transaction(self.tx_id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;
            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionBroadcast(self.tx_id)))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event, usually because there are no subscribers: {:?}",
                        e
                    );
                    e
                });
        }

        Ok(true)
    }

    /// Attempt to query the location of the transaction from the base node via RPC.
    /// # Returns:
    /// `Ok(true)` => Transaction was successfully mined and confirmed
    /// `Ok(false)` => There was a problem with the RPC call or the transaction is not mined but still in the mempool
    /// and this should be retried `Err(_)` => The transaction was rejected by the base node and the protocol should
    /// end.
    async fn transaction_query(
        &mut self,
        signature: Signature,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        let response = match client.transaction_query(signature.into()).await {
            Ok(r) => match TxQueryResponse::try_from(r) {
                Ok(r) => r,
                Err(_) => {
                    trace!(target: LOG_TARGET, "Could not convert proto TxQueryResponse");
                    return Ok(false);
                },
            },
            Err(e) => {
                info!(
                    target: LOG_TARGET,
                    "Transaction Query RPC Call to Base Node failed: {}", e
                );
                return Ok(false);
            },
        };

        if !(response.is_synced ||
            (response.location == TxLocation::Mined &&
                response.confirmations >= self.resources.config.num_confirmations_required as u64))
        {
            info!(
                target: LOG_TARGET,
                "Base Node reports not being synced, submission will be retried."
            );
            return Ok(false);
        }

        // Mined?
        if response.location == TxLocation::Mined {
            if response.confirmations >= self.resources.config.num_confirmations_required as u64 {
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) detected as mined and CONFIRMED with {} confirmations",
                    self.tx_id,
                    response.confirmations
                );
                return Ok(true);
            }
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) detected as mined but UNCONFIRMED with {} confirmations",
                self.tx_id,
                response.confirmations
            );
            self.resources
                .db
                .mine_completed_transaction(self.tx_id)
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionMinedUnconfirmed(
                    self.tx_id,
                    response.confirmations,
                )))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event because there are no subscribers: {:?}",
                        e
                    );
                    e
                });
        } else if response.location != TxLocation::InMempool {
            if !self.first_rejection {
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) not found in mempool, attempting to resubmit transaction", self.tx_id
                );
                self.mode = TxBroadcastMode::TransactionSubmission;
                self.first_rejection = true;
            } else {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) has been rejected by the mempool after second submission attempt, \
                     cancelling transaction",
                    self.tx_id
                );
                self.cancel_transaction().await;

                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionCancelled(self.tx_id)))
                    .map_err(|e| {
                        trace!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {:?}",
                            e
                        );
                        e
                    });
                return Err(TransactionServiceProtocolError::new(
                    self.tx_id,
                    TransactionServiceError::MempoolRejection,
                ));
            }
        } else {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) found in mempool.", self.tx_id
            );
        }

        Ok(false)
    }

    async fn query_or_submit_transaction(
        &mut self,
        completed_transaction: CompletedTransaction,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        if self.mode == TxBroadcastMode::TransactionSubmission {
            info!(
                target: LOG_TARGET,
                "Submitting Transaction (TxId: {}) to Base Node", self.tx_id
            );
            self.submit_transaction(completed_transaction.transaction, client).await
        } else {
            info!(
                target: LOG_TARGET,
                "Querying Transaction (TxId: {}) status on Base Node", self.tx_id
            );
            let signature = completed_transaction
                .transaction
                .first_kernel_excess_sig()
                .ok_or_else(|| {
                    TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidTransaction)
                })?;
            self.transaction_query(signature.clone(), client).await
        }
    }

    async fn cancel_transaction(&mut self) {
        if let Err(e) = self
            .resources
            .output_manager_service
            .cancel_transaction(self.tx_id)
            .await
        {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel outputs for TxId: {} after failed sending attempt with error {:?}", self.tx_id, e
            );
        }
        if let Err(e) = self.resources.db.cancel_completed_transaction(self.tx_id).await {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel TxId: {} after failed sending attempt with error {:?}", self.tx_id, e
            );
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TxBroadcastMode {
    TransactionSubmission,
    TransactionQuery,
}

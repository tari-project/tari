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

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::FutureExt;
use log::*;
use tari_common_types::{
    transaction::{TransactionStatus, TxId},
    types::Signature,
};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletRpcClient,
    },
    transactions::transaction_components::Transaction,
};
use tari_crypto::tari_utilities::hex::Hex;
use tokio::{sync::watch, time::sleep};

use crate::{
    connectivity_service::WalletConnectivityInterface,
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::TransactionEvent,
        service::TransactionServiceResources,
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, TxCancellationReason},
        },
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::broadcast_protocol";

pub struct TransactionBroadcastProtocol<TBackend, TWalletConnectivity> {
    tx_id: TxId,
    mode: TxBroadcastMode,
    resources: TransactionServiceResources<TBackend, TWalletConnectivity>,
    timeout_update_receiver: watch::Receiver<Duration>,
    last_rejection: Option<Instant>,
}

impl<TBackend, TWalletConnectivity> TransactionBroadcastProtocol<TBackend, TWalletConnectivity>
where
    TBackend: TransactionBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        tx_id: TxId,
        resources: TransactionServiceResources<TBackend, TWalletConnectivity>,
        timeout_update_receiver: watch::Receiver<Duration>,
    ) -> Self {
        Self {
            tx_id,
            mode: TxBroadcastMode::TransactionSubmission,
            resources,
            timeout_update_receiver,
            last_rejection: None,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<TxId, TransactionServiceProtocolError> {
        let mut shutdown = self.resources.shutdown_signal.clone();
        let mut current_base_node_watcher = self.resources.connectivity.get_current_base_node_watcher();
        let mut timeout_update_receiver = self.timeout_update_receiver.clone();

        // Main protocol loop
        loop {
            let mut client = self
                .resources
                .connectivity
                .obtain_base_node_wallet_rpc_client()
                .await
                .ok_or_else(|| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))?;

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

            loop {
                tokio::select! {
                    _ = current_base_node_watcher.changed() => {
                            if let Some(peer) = &*current_base_node_watcher.borrow() {
                                info!(
                                    target: LOG_TARGET,
                                    "Transaction Broadcast protocol (TxId: {}) Base Node Public key updated to {} (NodeID: {})", self.tx_id, peer.public_key, peer.node_id
                                );
                            }
                            self.last_rejection = None;
                            continue;
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
                                    debug!(target: LOG_TARGET, "Transaction broadcast, transaction validation protocol will continue from here");
                                    return Ok(self.tx_id)
                                }
                            },
                        }
                        // Wait out the remainder of the delay before proceeding with next loop
                        drop(client);
                        let delay = *timeout_update_receiver.borrow();
                        sleep(delay).await;
                        break;
                    },
                    _ = timeout_update_receiver.changed() => {
                         info!(
                            target: LOG_TARGET,
                            "Transaction Broadcast protocol (TxId: {}) timeout updated to {:?}", self.tx_id, timeout_update_receiver.borrow()
                        );
                        break;
                    },
                    _ = shutdown.wait() => {
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
    ) -> Result<bool, TransactionServiceProtocolError> {
        let response = match client
            .submit_transaction(tx.try_into().map_err(|e| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidMessageError(e))
            })?)
            .await
        {
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

            let (reason_error, reason) = match response.rejection_reason {
                TxSubmissionRejectionReason::None | TxSubmissionRejectionReason::ValidationFailed => (
                    TransactionServiceError::MempoolRejectionInvalidTransaction,
                    TxCancellationReason::InvalidTransaction,
                ),
                TxSubmissionRejectionReason::DoubleSpend => (
                    TransactionServiceError::MempoolRejectionDoubleSpend,
                    TxCancellationReason::DoubleSpend,
                ),
                TxSubmissionRejectionReason::Orphan => (
                    TransactionServiceError::MempoolRejectionOrphan,
                    TxCancellationReason::Orphan,
                ),
                TxSubmissionRejectionReason::TimeLocked => (
                    TransactionServiceError::MempoolRejectionTimeLocked,
                    TxCancellationReason::TimeLocked,
                ),
                _ => (
                    TransactionServiceError::UnexpectedBaseNodeResponse,
                    TxCancellationReason::Unknown,
                ),
            };

            self.cancel_transaction(reason).await;

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionCancelled(self.tx_id, reason)))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event because there are no subscribers: {:?}",
                        e
                    );
                    e
                });

            return Err(TransactionServiceProtocolError::new(self.tx_id, reason_error));
        } else if response.rejection_reason == TxSubmissionRejectionReason::AlreadyMined {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) is Already Mined according to Base Node. Will be completed by transaction \
                 validation protocol.",
                self.tx_id
            );
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
    ) -> Result<bool, TransactionServiceProtocolError> {
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
            info!(
                target: LOG_TARGET,
                "Broadcast transaction detected as mined, will be managed by transaction validation protocol"
            );
            Ok(true)
        } else if response.location != TxLocation::InMempool {
            if self.last_rejection.is_none() ||
                self.last_rejection.unwrap().elapsed() >
                    self.resources.config.transaction_mempool_resubmission_window
            {
                info!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) not found in mempool, attempting to resubmit transaction", self.tx_id
                );
                self.mode = TxBroadcastMode::TransactionSubmission;
                self.last_rejection = Some(Instant::now());
                Ok(false)
            } else {
                error!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) has been rejected by the mempool after second submission attempt, \
                     cancelling transaction",
                    self.tx_id
                );
                self.cancel_transaction(TxCancellationReason::InvalidTransaction).await;

                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionCancelled(
                        self.tx_id,
                        TxCancellationReason::InvalidTransaction,
                    )))
                    .map_err(|e| {
                        trace!(
                            target: LOG_TARGET,
                            "Error sending event because there are no subscribers: {:?}",
                            e
                        );
                        e
                    });
                Err(TransactionServiceProtocolError::new(
                    self.tx_id,
                    TransactionServiceError::MempoolRejection,
                ))
            }
        } else {
            info!(
                target: LOG_TARGET,
                "Transaction (TxId: {}) found in mempool.", self.tx_id
            );
            Ok(true)
        }
    }

    async fn query_or_submit_transaction(
        &mut self,
        completed_transaction: CompletedTransaction,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, TransactionServiceProtocolError> {
        let signature = completed_transaction
            .transaction
            .first_kernel_excess_sig()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidTransaction)
            })?;
        if self.mode == TxBroadcastMode::TransactionSubmission {
            info!(
                target: LOG_TARGET,
                "Submitting Transaction (TxId: {}) with signature '{}' to Base Node",
                self.tx_id,
                signature.clone().get_signature().to_hex(),
            );
            self.submit_transaction(completed_transaction.transaction, client).await
        } else {
            info!(
                target: LOG_TARGET,
                "Querying Transaction (TxId: {}) status on Base Node", self.tx_id
            );
            self.transaction_query(signature.clone(), client).await
        }
    }

    async fn cancel_transaction(&mut self, reason: TxCancellationReason) {
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
        if let Err(e) = self.resources.db.reject_completed_transaction(self.tx_id, reason).await {
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

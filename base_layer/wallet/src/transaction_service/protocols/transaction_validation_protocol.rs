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
    transaction_service::{
        error::{TransactionServiceError, TransactionServiceProtocolError},
        handle::TransactionEvent,
        service::TransactionServiceResources,
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, TransactionStatus},
        },
    },
    types::ValidationRetryStrategy,
};
use futures::{FutureExt, StreamExt};
use log::*;
use std::{cmp, convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryBatchResponse},
        rpc::BaseNodeWalletRpcClient,
    },
    proto::{base_node::Signatures as SignaturesProto, types::Signature as SignatureProto},
};
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::validation_protocol";

pub struct TransactionValidationProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    id: u64,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    base_node_update_receiver: Option<broadcast::Receiver<CommsPublicKey>>,
    timeout_update_receiver: Option<broadcast::Receiver<Duration>>,
    retry_strategy: ValidationRetryStrategy,
    base_node_synced: bool,
}

/// This protocol will check all of the mined transactions (both valid and invalid) in the db to see if they are present
/// on the current base node. # Behaviour
/// - If a valid transaction is not present the protocol will mark the transaction as invalid
/// - If an invalid transaction is present on th ebase node it will be marked as valid
/// - If a Confirmed mined transaction is present but no longer confirmed its status will change to MinedUnconfirmed
impl<TBackend> TransactionValidationProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub fn new(
        id: u64,
        resources: TransactionServiceResources<TBackend>,
        base_node_public_key: CommsPublicKey,
        timeout: Duration,
        base_node_update_receiver: broadcast::Receiver<CommsPublicKey>,
        timeout_update_receiver: broadcast::Receiver<Duration>,
        retry_strategy: ValidationRetryStrategy,
    ) -> Self
    {
        Self {
            id,
            resources,
            timeout,
            base_node_public_key,
            base_node_update_receiver: Some(base_node_update_receiver),
            timeout_update_receiver: Some(timeout_update_receiver),
            retry_strategy,
            base_node_synced: true,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut timeout_update_receiver = self
            .timeout_update_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

        let mut base_node_update_receiver = self
            .base_node_update_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

        let mut shutdown = self.resources.shutdown_signal.clone();

        let total_retries_str = match self.retry_strategy {
            ValidationRetryStrategy::Limited(n) => format!("{}", n),
            ValidationRetryStrategy::UntilSuccess => "∞".to_string(),
        };

        info!(
            "Starting Transaction Validation Protocol (Id: {}) with {} retries",
            self.id, total_retries_str
        );

        let mut batches = self
            .get_transaction_batches()
            .await
            .map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;
        let mut retries = 0;

        // Main protocol loop
        'main: loop {
            if let ValidationRetryStrategy::Limited(max_retries) = self.retry_strategy {
                if retries > max_retries {
                    info!(
                        target: LOG_TARGET,
                        "Maximum attempts exceeded for Transaction Validation Protocol (Id: {})", self.id
                    );
                    // If this retry is not because of a !base_node_synced then we emit this error event, if the retries
                    // are due to a base node NOT being synced then we rely on the TransactionValidationDelayed event
                    // because we were actually able to connect
                    if self.base_node_synced {
                        let _ = self
                            .resources
                            .event_publisher
                            .send(Arc::new(TransactionEvent::TransactionValidationFailure(self.id)))
                            .map_err(|e| {
                                trace!(
                                    target: LOG_TARGET,
                                    "Error sending event because there are no subscribers: {:?}",
                                    e
                                );
                                e
                            });
                    }
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::MaximumAttemptsExceeded,
                    ));
                }
            }
            // Assume base node is synced until we achieve a connection and it tells us it is not synced
            self.base_node_synced = true;

            let base_node_node_id = NodeId::from_key(&self.base_node_public_key.clone())
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;
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
                            info!(target: LOG_TARGET, "Problem connecting to base node: {} for Transaction Validation Protocol", e);
                        },
                    }
                },
                new_base_node = base_node_update_receiver.select_next_some() => {

                    match new_base_node {
                        Ok(_) => {
                            info!(target: LOG_TARGET, "Aborting Transaction Validation Protocol as new Base node is set");
                             let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionValidationAborted(self.id)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event because there are no subscribers: {:?}",
                                        e
                                    );
                                    e
                                });
                                return Ok(self.id);
                        },
                        Err(e) => {
                            trace!(
                                target: LOG_TARGET,
                                "Transaction Validation protocol event 'base_node_update' triggered with error: {:?}",

                                e,
                            );
                        }
                    }
                }
                updated_timeout = timeout_update_receiver.select_next_some() => {
                    match updated_timeout {
                        Ok(to) => {
                            self.timeout = to;
                             info!(
                                target: LOG_TARGET,
                                "Transaction Validation protocol timeout updated to {:?}",  self.timeout
                            );
                        },
                        Err(e) => {
                            trace!(
                                target: LOG_TARGET,
                                "Transaction Validation protocol event 'updated_timeout' triggered with error: {:?}",

                                e,
                            );
                        }
                    }
                },
                _ = shutdown => {
                    info!(target: LOG_TARGET, "Transaction Validation Protocol shutting down because it received the shutdown signal");
                    return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
                },
            }

            let mut base_node_connection = match connection {
                None => {
                    futures::select! {
                        _ = delay.fuse() => {
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionValidationTimedOut(self.id)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event {:?}, because there are no subscribers.",
                                        e.0
                                    );
                                    e
                                });
                            retries += 1;
                            continue;
                        },
                        _ = shutdown => {
                            info!(target: LOG_TARGET, "Transaction Validation Protocol shutting down because it received the shutdown signal");
                            return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
                        },
                    }
                },
                Some(c) => c,
            };

            let mut client = match base_node_connection
                .connect_rpc_using_builder(BaseNodeWalletRpcClient::builder().with_deadline(self.timeout))
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(target: LOG_TARGET, "Problem establishing RPC connection: {}", e);
                    delay.await;
                    continue;
                },
            };

            debug!(target: LOG_TARGET, "RPC client connected");

            'per_tx: loop {
                let batch = if let Some(b) = batches.pop() {
                    b
                } else {
                    break 'main;
                };
                let delay = delay_for(self.timeout);
                futures::select! {
                    new_base_node = base_node_update_receiver.select_next_some() => {
                        match new_base_node {
                            Ok(_) => {
                               info!(target: LOG_TARGET, "Aborting Transaction Validation Protocol as new Base node is set");
                                let _ = self
                                    .resources
                                    .event_publisher
                                    .send(Arc::new(TransactionEvent::TransactionValidationAborted(self.id)))
                                    .map_err(|e| {
                                        trace!(
                                            target: LOG_TARGET,
                                            "Error sending event because there are no subscribers: {:?}",
                                            e
                                        );
                                        e
                                    });
                                return Ok(self.id);
                            },
                            Err(e) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "Transaction Validation protocol event 'base_node_update' triggered with error: {:?}",

                                    e,
                                );
                            }
                        }
                    },
                    result = self.transaction_query_batch(batch.clone(), &mut client).fuse() => {
                        match result {
                            Ok(synced) => {
                                self.base_node_synced = synced;
                                if !synced {
                                    info!(target: LOG_TARGET, "Base Node reports not being synced, will retry.");
                                        let _ = self
                                        .resources
                                        .event_publisher
                                        .send(Arc::new(TransactionEvent::TransactionValidationDelayed(self.id)))
                                        .map_err(|e| {
                                            trace!(
                                                target: LOG_TARGET,
                                                "Error sending event because there are no subscribers: {:?}",
                                                e
                                            );
                                            e
                                        });
                                    delay.await;
                                    retries += 1;
                                    batches = self.get_transaction_batches().await.map_err(|e| TransactionServiceProtocolError::new(self.id, e))?;
                                    break 'per_tx;
                                }
                            },
                            Err(TransactionServiceError::RpcError(e)) => {
                                warn!(target: LOG_TARGET, "Error with RPC Client: {}. Retrying RPC client connection.", e);
                                let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionValidationTimedOut(self.id)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event {:?}, because there are no subscribers.",
                                        e.0
                                    );
                                    e
                                });
                                delay.await;
                                batches.push(batch);
                                retries += 1;
                                break 'per_tx;
                            }
                            Err(e) => {
                                let _ = self
                                    .resources
                                    .event_publisher
                                    .send(Arc::new(TransactionEvent::TransactionValidationFailure(self.id)))
                                    .map_err(|e| {
                                        trace!(
                                            target: LOG_TARGET,
                                            "Error sending event because there are no subscribers: {:?}",
                                            e
                                        );
                                        e
                                    });
                                return Err(TransactionServiceProtocolError::new(self.id,e));
                            },
                        }
                    },
                    updated_timeout = timeout_update_receiver.select_next_some() => {
                        match updated_timeout {
                            Ok(to) => {
                                self.timeout = to;
                                 info!(
                                    target: LOG_TARGET,
                                    "Transaction Validation protocol timeout updated to {:?}",  self.timeout
                                );
                            },
                            Err(e) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "Transaction Validation protocol event 'updated_timeout' triggered with error: {:?}",

                                    e,
                                );
                            }
                        }
                    },
                    _ = shutdown => {
                        info!(target: LOG_TARGET, "Transaction Validation Protocol shutting down because it received the shutdown signal");
                        return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
                    },
                }
            }
        }

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(TransactionEvent::TransactionValidationSuccess(self.id)))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event because there are no subscribers: {:?}",
                    e
                );
                e
            });

        Ok(self.id)
    }

    /// Attempt to query the location of the transaction from the base node via RPC.
    /// # Returns:
    /// `Ok(true)` => Transaction was successfully mined and confirmed
    /// `Ok(false)` => There was a problem with the RPC call or the transaction is not mined but still in the mempool
    /// and this should be retried `Err(_)` => The transaction was rejected by the base node and the protocol should
    /// end.
    async fn transaction_query_batch(
        &mut self,
        batch: Vec<CompletedTransaction>,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, TransactionServiceError>
    {
        let mut batch_signatures = Vec::new();
        for tx in batch.iter() {
            let signature = tx
                .transaction
                .first_kernel_excess_sig()
                .ok_or_else(|| TransactionServiceError::InvalidTransaction)?;
            batch_signatures.push(SignatureProto::from(signature.clone()));
        }

        let batch_response = client
            .transaction_batch_query(SignaturesProto { sigs: batch_signatures })
            .await?;

        if !batch_response.is_synced {
            return Ok(false);
        }

        for response_proto in batch_response.responses {
            let response = TxQueryBatchResponse::try_from(response_proto)
                .map_err(TransactionServiceError::ProtobufConversionError)?;

            if let Some(queried_tx) = batch.iter().find(|tx| {
                if let Some(sig) = tx.transaction.first_kernel_excess_sig() {
                    sig == &response.signature
                } else {
                    false
                }
            }) {
                // Mined?
                if response.location == TxLocation::Mined {
                    if !queried_tx.valid {
                        info!(
                            target: LOG_TARGET,
                            "Transaction (TxId: {}) is VALID according to base node, status will be updated",
                            queried_tx.tx_id
                        );
                        if let Err(e) = self
                            .resources
                            .db
                            .set_completed_transaction_validity(queried_tx.tx_id, true)
                            .await
                        {
                            warn!(
                                target: LOG_TARGET,
                                "Error setting transaction (TxId: {}) validity: {}", queried_tx.tx_id, e
                            );
                        }
                    }
                    if response.confirmations >= self.resources.config.num_confirmations_required as u64 {
                        if queried_tx.status == TransactionStatus::MinedUnconfirmed {
                            info!(
                                target: LOG_TARGET,
                                "Transaction (TxId: {}) is MINED and CONFIRMED according to base node, status will be \
                                 updated",
                                queried_tx.tx_id
                            );
                            if let Err(e) = self
                                .resources
                                .db
                                .confirm_broadcast_or_coinbase_transaction(queried_tx.tx_id)
                                .await
                            {
                                warn!(
                                    target: LOG_TARGET,
                                    "Error confirming mined transaction (TxId: {}): {}", queried_tx.tx_id, e
                                );
                            }
                            if let Err(e) = self
                                .resources
                                .output_manager_service
                                .confirm_transaction(
                                    queried_tx.tx_id,
                                    queried_tx.transaction.body.inputs().clone(),
                                    queried_tx.transaction.body.outputs().clone(),
                                )
                                .await
                            {
                                debug!(
                                    target: LOG_TARGET,
                                    "Error confirming outputs transaction (TxId: {}) that was validated with new base \
                                     node: {}. Usually means this transaction was confirmed in the past",
                                    queried_tx.tx_id,
                                    e
                                );
                            }
                        }
                    } else if queried_tx.status == TransactionStatus::MinedConfirmed {
                        info!(
                            target: LOG_TARGET,
                            "Transaction (TxId: {}) is MINED but UNCONFIRMED according to base node, status will be \
                             updated",
                            queried_tx.tx_id
                        );
                        if let Err(e) = self.resources.db.unconfirm_mined_transaction(queried_tx.tx_id).await {
                            warn!(
                                target: LOG_TARGET,
                                "Error unconfirming mined transaction (TxId: {}): {}", queried_tx.tx_id, e
                            );
                        }
                    }
                } else if queried_tx.valid {
                    info!(
                        target: LOG_TARGET,
                        "Transaction (TxId: {}) is INVALID according to base node, status will be updated",
                        queried_tx.tx_id
                    );
                    if let Err(e) = self
                        .resources
                        .db
                        .set_completed_transaction_validity(queried_tx.tx_id, false)
                        .await
                    {
                        warn!(
                            target: LOG_TARGET,
                            "Error setting transaction (TxId: {}) validity: {}", queried_tx.tx_id, e
                        );
                    }
                }
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Could not find transaction corresponding to returned query response"
                );
            }
        }
        Ok(true)
    }

    /// Get completed transactions from db and sort the mined transactions into batches
    async fn get_transaction_batches(&self) -> Result<Vec<Vec<CompletedTransaction>>, TransactionServiceError> {
        let mut completed_txs: Vec<CompletedTransaction> = self
            .resources
            .db
            .get_completed_transactions()
            .await?
            .values()
            .filter(|tx| {
                tx.status == TransactionStatus::MinedUnconfirmed || tx.status == TransactionStatus::MinedConfirmed
            })
            .cloned()
            .collect();
        // Determine how many rounds of base node request we need to query all the transactions in batches of
        // max_tx_query_batch_size
        let num_batches =
            ((completed_txs.len() as f32) / (self.resources.config.max_tx_query_batch_size as f32 + 0.1)) as usize + 1;

        let mut batches: Vec<Vec<CompletedTransaction>> = Vec::new();
        for _b in 0..num_batches {
            let mut batch = Vec::new();
            for tx in
                completed_txs.drain(..cmp::min(self.resources.config.max_tx_query_batch_size, completed_txs.len()))
            {
                batch.push(tx);
            }
            if !batch.is_empty() {
                batches.push(batch);
            }
        }
        Ok(batches)
    }
}

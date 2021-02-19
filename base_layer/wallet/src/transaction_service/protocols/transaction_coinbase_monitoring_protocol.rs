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
        storage::{database::TransactionBackend, models::CompletedTransaction},
    },
};
use futures::{FutureExt, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse},
        rpc::BaseNodeWalletRpcClient,
    },
    transactions::types::Signature,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::coinbase_monitoring";

/// This protocol defines the process of monitoring a mempool and base node to detect when a Broadcast transaction is
/// Mined or leaves the mempool in which case it should be cancelled

pub struct TransactionCoinbaseMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    tx_id: TxId,
    block_height: u64,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    base_node_update_receiver: Option<broadcast::Receiver<CommsPublicKey>>,
    timeout_update_receiver: Option<broadcast::Receiver<Duration>>,
    first_rejection: bool,
}

impl<TBackend> TransactionCoinbaseMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tx_id: TxId,
        block_height: u64,
        resources: TransactionServiceResources<TBackend>,
        timeout: Duration,
        base_node_public_key: CommsPublicKey,
        base_node_update_receiver: broadcast::Receiver<CommsPublicKey>,
        timeout_update_receiver: broadcast::Receiver<Duration>,
    ) -> Self
    {
        Self {
            tx_id,
            block_height,
            resources,
            timeout,
            base_node_public_key,
            base_node_update_receiver: Some(base_node_update_receiver),
            timeout_update_receiver: Some(timeout_update_receiver),
            first_rejection: false,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut base_node_update_receiver = self
            .base_node_update_receiver
            .take()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidStateError)
            })?
            .fuse();

        let mut timeout_update_receiver = self
            .timeout_update_receiver
            .take()
            .ok_or_else(|| {
                TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::InvalidStateError)
            })?
            .fuse();

        trace!(
            target: LOG_TARGET,
            "Starting coinbase monitoring protocol for transaction (TxId: {})",
            self.tx_id
        );

        // This is the main loop of the protocol and following the following steps
        // 1) Check transaction being monitored is still in the Coinbase state and needs to be monitored
        // 2) Make a transaction_query RPC call to the base node
        // 3) Wait for both Base Node responses OR a Timeout
        //      a) If the chain tip moves beyond this block height and require confirmations AND the coinbase kernel is
        //         not in the blockchain cancel this transaction
        //      b) If the coinbase kernel is in the blockchain the protocol can end with success
        //      c) IF timeout is reached, start again
        let mut shutdown = self.resources.shutdown_signal.clone();
        loop {
            let completed_tx = match self.resources.db.get_completed_transaction(self.tx_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Cannot find Completed Transaction (TxId: {}) referred to by this Coinbase Monitoring \
                         Protocol: {:?}",
                        self.tx_id,
                        e
                    );
                    return Err(TransactionServiceProtocolError::new(
                        self.tx_id,
                        TransactionServiceError::TransactionDoesNotExistError,
                    ));
                },
            };
            debug!(
                target: LOG_TARGET,
                "Coinbase transaction (TxId: {}) has status '{}' and is cancelled ({}) and is valid ({}).",
                self.tx_id,
                completed_tx.status,
                completed_tx.cancelled,
                completed_tx.valid,
            );

            let mut hashes = Vec::new();
            for o in completed_tx.transaction.body.outputs() {
                hashes.push(o.hash());
            }

            info!(
                target: LOG_TARGET,
                "Sending Transaction Mined? request for Coinbase Tx with TxId: {} and Kernel Signature {} to Base Node",
                self.tx_id,
                completed_tx.transaction.body.kernels()[0]
                    .excess_sig
                    .get_signature()
                    .to_hex(),
            );

            // Get a base node RPC connection
            let base_node_node_id = NodeId::from_key(&self.base_node_public_key.clone())
                .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;
            let mut connection: Option<PeerConnection> = None;
            debug!(
                target: LOG_TARGET,
                "Connecting to Base Node (Public Key: {}) for transaction (TxId: {})",
                self.base_node_public_key,
                self.tx_id,
            );
            futures::select! {
                dial_result = self.resources.connectivity_manager.dial_peer(base_node_node_id.clone()).fuse() => {
                    match dial_result {
                        Ok(base_node_connection) => {
                            connection = Some(base_node_connection);
                        },
                        Err(e) => {
                            warn!(
                                target: LOG_TARGET,
                                "Problem connecting to base node for Coinbase Monitoring Protocol (TxId: {}): {}",
                                self.tx_id,
                                e,
                            );
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
                                "Coinbase Monitoring protocol (TxId: {}) timeout updated to {:?}",
                                self.tx_id,
                                self.timeout
                            );
                        },
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "Coinbase Monitoring protocol (TxId: {}) event 'updated_timeout' triggered with \
                                 error: {:?}",
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
                                "Coinbase Monitoring protocol (TxId: {}) Base Node Public key updated to {:?}",
                                self.tx_id,
                                self.base_node_public_key
                            );
                            self.first_rejection = false;
                            continue;
                        },
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "Coinbase Monitoring protocol (TxId: {}) event 'base_node_update' triggered with \
                                 error: {:?}",
                                self.tx_id,
                                e,
                            );
                        }
                    }
                }
                _ = shutdown => {
                    info!(
                        target: LOG_TARGET,
                        "Coinbase Monitoring protocol (TxId: {}) shutting down because it received the shutdown \
                         signal (at 1)",
                        self.tx_id
                    );
                    return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                },
            }

            let delay = delay_for(self.timeout);
            let mut base_node_connection = match connection {
                None => {
                    futures::select! {
                        _ = delay.fuse() => {
                            continue;
                        },
                        _ = shutdown => {
                            info!(
                                target: LOG_TARGET,
                                "Coinbase Monitoring Protocol (TxId: {}) shutting down because it received the \
                                 shutdown signal (at 2)",
                                self.tx_id
                            );
                            return Err(TransactionServiceProtocolError::new(
                                self.tx_id,
                                TransactionServiceError::Shutdown
                            ))
                        },
                    }
                },
                Some(c) => c,
            };
            let mut client = match base_node_connection
                .connect_rpc_using_builder(
                    BaseNodeWalletRpcClient::builder().with_deadline(self.resources.config.chain_monitoring_timeout),
                )
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Problem establishing RPC connection (TxId: {}): {}", self.tx_id, e
                    );
                    delay.await;
                    continue;
                },
            };

            let signature: Signature;
            if !completed_tx.transaction.body.kernels().is_empty() {
                signature = completed_tx.transaction.body.kernels()[0].clone().excess_sig;
            } else {
                error!(
                    target: LOG_TARGET,
                    "Malformed transaction (TxId: {}); signature does not exist", self.tx_id,
                );
                return Err(TransactionServiceProtocolError::new(
                    self.tx_id,
                    TransactionServiceError::InvalidCompletedTransaction,
                ));
            }
            let delay = delay_for(self.timeout).fuse();
            loop {
                futures::select! {
                    new_base_node = base_node_update_receiver.select_next_some() => {
                        match new_base_node {
                            Ok(bn) => {
                                self.base_node_public_key = bn;
                                 info!(
                                    target: LOG_TARGET,
                                    "Coinbase Monitoring protocol (TxId: {}) Base Node Public key updated to {:?}",
                                    self.tx_id,
                                    self.base_node_public_key
                                );
                                self.first_rejection = false;
                                continue;
                            },
                            Err(e) => {
                                error!(
                                    target: LOG_TARGET,
                                    "Coinbase Monitoring protocol (TxId: {}) event 'base_node_update' triggered with \
                                     error: {:?}",
                                    self.tx_id,
                                    e,
                                );
                            }
                        }
                    }
                    result = self.query_coinbase_transaction(
                        signature.clone(), completed_tx.clone(), &mut client
                    ).fuse() => {
                        let (coinbase_kernel_found, metadata) = match result {
                            Ok(r) => r,
                            _ => (false, None),
                        };
                        if coinbase_kernel_found {
                            // We are done!
                            info!(
                                target: LOG_TARGET,
                                "Coinbase monitoring protocol for transaction (TxId: {}) completed successfully.",
                                self.tx_id,
                            );
                            return Ok(self.tx_id);
                        }
                        match metadata {
                            Some(tip) => {
                                // If the tip has moved beyond this Coinbase transaction's blockheight and required
                                // number of confirmations and it wasn't mined then it should be cancelled
                                if tip > self.block_height + self.resources.config.num_confirmations_required {
                                    warn!(
                                        target: LOG_TARGET,
                                        "Chain tip has moved ahead of this Coinbase transaction's block height and \
                                         required number of confirmations without it being mined. Cancelling Coinbase \
                                         transaction (TxId: {}).",
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
                                                "Error sending event, usually because there are no subscribers: {:?}",
                                                e
                                            );
                                            e
                                        });
                                    return Err(TransactionServiceProtocolError::new(
                                        self.tx_id,
                                        TransactionServiceError::ChainTipHigherThanCoinbaseHeight,
                                    ));
                                };
                            },
                            _ => {},
                        }
                        info!(
                            target: LOG_TARGET,
                            "Coinbase transaction (TxId: {}) not mined yet, still waiting.", self.tx_id,
                        );
                        // Wait out the remainder of the delay before proceeding with next loop
                        delay.await;
                        break;
                    },
                    updated_timeout = timeout_update_receiver.select_next_some() => {
                        if let Ok(to) = updated_timeout {
                            self.timeout = to;
                             info!(
                                target: LOG_TARGET,
                                "Coinbase monitoring protocol (TxId: {}) timeout updated to {:?}",
                                self.tx_id,
                                self.timeout
                            );
                            break;
                        } else {
                            trace!(
                                target: LOG_TARGET,
                                "Coinbase monitoring protocol event 'updated_timeout' triggered (TxId: {}) ({:?})",
                                self.tx_id,
                                updated_timeout,
                            );
                        }
                    },
                    _ = shutdown => {
                        info!(
                            target: LOG_TARGET,
                            "Coinbase Monitoring Protocol (TxId: {}) shutting down because it received the shutdown \
                             signal (at 3)",
                            self.tx_id
                        );
                        return Err(TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::Shutdown))
                    },
                }
                info!(
                    target: LOG_TARGET,
                    "Coinbase monitoring process timed out for transaction (TxId: {})", self.tx_id
                );

                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionMinedRequestTimedOut(self.tx_id)))
                    .map_err(|e| {
                        trace!(
                            target: LOG_TARGET,
                            "Error sending event, usually because there are no subscribers: {:?}",
                            e
                        );
                        e
                    });
            }
        }
    }

    /// Attempt to query the location of the transaction from the base node via RPC.
    /// # Returns:
    /// `Ok((true, Some(u64)))`  => Transaction was successfully mined and confirmed.
    /// `Ok((false, Some(u64)))` => Either the transaction is mined but does not have the required number of
    ///                             confirmations yet, or it is not mined and still in the mempool, or it is not mined
    ///                             and not found in the mempool.
    /// `Ok((false, None))`      => There was a problem with the RPC call.
    async fn query_coinbase_transaction(
        &mut self,
        signature: Signature,
        completed_tx: CompletedTransaction,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<(bool, Option<u64>), TransactionServiceProtocolError>
    {
        trace!(
            target: LOG_TARGET,
            "Querying status for coinbase transaction (TxId: {})",
            self.tx_id,
        );
        let response = match client.transaction_query(signature.into()).await {
            Ok(r) => match TxQueryResponse::try_from(r) {
                Ok(r) => r,
                Err(_) => {
                    trace!(
                        target: LOG_TARGET,
                        "Could not convert proto TxQueryResponse for coinbase transaction (TxId: {})",
                        self.tx_id,
                    );
                    return Ok((false, None));
                },
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Coinbase transaction Query RPC Call to Base Node failed for coinbase transaction (TxId: {}): {}",
                    self.tx_id,
                    e
                );
                return Ok((false, None));
            },
        };

        if !(response.is_synced ||
            response.location == TxLocation::Mined &&
                response.confirmations >= self.resources.config.num_confirmations_required)
        {
            info!(
                target: LOG_TARGET,
                "Base Node reports not being synced, coinbase monitoring will be retried."
            );
            return Ok((false, Some(response.height_of_longest_chain)));
        }

        // Mined?
        if response.location == TxLocation::Mined {
            if response.confirmations >= self.resources.config.num_confirmations_required {
                info!(
                    target: LOG_TARGET,
                    "Coinbase transaction (TxId: {}) detected as mined and CONFIRMED with {} confirmations",
                    self.tx_id,
                    response.confirmations
                );
                self.resources
                    .output_manager_service
                    .confirm_transaction(
                        self.tx_id,
                        completed_tx.transaction.body.inputs().clone(),
                        completed_tx.transaction.body.outputs().clone(),
                    )
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.tx_id, TransactionServiceError::from(e)))?;

                self.resources
                    .db
                    .confirm_broadcast_or_coinbase_transaction(self.tx_id)
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
                return Ok((true, Some(response.height_of_longest_chain)));
            }
            info!(
                target: LOG_TARGET,
                "Coinbase transaction (TxId: {}) detected as mined but UNCONFIRMED with {} confirmations",
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
        } else if response.location == TxLocation::InMempool {
            debug!(
                target: LOG_TARGET,
                "Coinbase transaction (TxId: {}) found in mempool, still waiting.", self.tx_id
            );
        } else {
            debug!(
                target: LOG_TARGET,
                "Coinbase transaction (TxId: {}) not found in mempool, still waiting.", self.tx_id
            );
        }

        Ok((false, Some(response.height_of_longest_chain)))
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
                "Failed to Cancel outputs for Coinbase transaction (TxId: {}) with error: {:?}", self.tx_id, e
            );
        }
        if let Err(e) = self.resources.db.cancel_completed_transaction(self.tx_id).await {
            warn!(
                target: LOG_TARGET,
                "Failed to Cancel Coinbase transaction (TxId: {}) with error: {:?}", self.tx_id, e
            );
        }
    }
}

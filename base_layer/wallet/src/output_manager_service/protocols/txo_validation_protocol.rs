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
    output_manager_service::{
        error::{OutputManagerError, OutputManagerProtocolError},
        handle::OutputManagerEvent,
        service::OutputManagerResources,
        storage::{database::OutputManagerBackend, models::DbUnblindedOutput},
    },
    types::ValidationRetryStrategy,
};
use futures::{FutureExt, StreamExt};
use log::*;
use std::{cmp, collections::HashMap, convert::TryFrom, fmt, sync::Arc, time::Duration};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, PeerConnection};
use tari_core::{
    base_node::rpc::BaseNodeWalletRpcClient,
    proto::base_node::FetchMatchingUtxos,
    transactions::transaction::TransactionOutput,
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::output_manager_service::protocols::utxo_validation_protocol";

const MAX_RETRY_DELAY: Duration = Duration::from_secs(300);

pub struct TxoValidationProtocol<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    id: u64,
    validation_type: TxoValidationType,
    retry_strategy: ValidationRetryStrategy,
    resources: OutputManagerResources<TBackend>,
    base_node_public_key: CommsPublicKey,
    retry_delay: Duration,
    base_node_update_receiver: Option<broadcast::Receiver<CommsPublicKey>>,
    base_node_synced: bool,
}

/// This protocol defines the process of submitting our current UTXO set to the Base Node to validate it.
impl<TBackend> TxoValidationProtocol<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        validation_type: TxoValidationType,
        retry_strategy: ValidationRetryStrategy,
        resources: OutputManagerResources<TBackend>,
        base_node_public_key: CommsPublicKey,
        base_node_update_receiver: broadcast::Receiver<CommsPublicKey>,
    ) -> Self
    {
        let retry_delay = resources.config.base_node_query_timeout;
        Self {
            id,
            validation_type,
            retry_strategy,
            resources,
            base_node_public_key,
            retry_delay,
            base_node_update_receiver: Some(base_node_update_receiver),
            base_node_synced: true,
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, OutputManagerProtocolError> {
        let mut base_node_update_receiver = self
            .base_node_update_receiver
            .take()
            .ok_or_else(|| {
                OutputManagerProtocolError::new(
                    self.id,
                    OutputManagerError::ServiceError("A Base Node Update receiver was not provided".to_string()),
                )
            })?
            .fuse();

        let mut shutdown = self.resources.shutdown_signal.clone();

        let total_retries_str = match self.retry_strategy {
            ValidationRetryStrategy::Limited(n) => format!("{}", n),
            ValidationRetryStrategy::UntilSuccess => "∞".to_string(),
        };

        info!(
            target: LOG_TARGET,
            "Starting TXO validation protocol (Id: {}) for {} with {} retries",
            self.id,
            self.validation_type,
            total_retries_str
        );

        let mut output_batches_to_query: Vec<Vec<Vec<u8>>> = self.get_output_batches().await?;

        if output_batches_to_query.is_empty() {
            debug!(
                target: LOG_TARGET,
                "TXO validation protocol (Id: {}) has no outputs to validate", self.id,
            );
            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(
                    self.id,
                    self.validation_type,
                )))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event {:?}, because there are no subscribers.",
                        e.0
                    );
                    e
                });
            return Ok(self.id);
        }

        let mut retries = 0;
        let batch_total = output_batches_to_query.len();

        'main: loop {
            if let ValidationRetryStrategy::Limited(max_retries) = self.retry_strategy {
                if retries > max_retries {
                    info!(
                        target: LOG_TARGET,
                        "Maximum attempts exceeded for TXO Validation Protocol (Id: {})", self.id
                    );
                    // If this retry is not because of a !base_node_synced then we emit this error event, if the retries
                    // are due to a base node NOT being synced then we rely on the TxoValidationDelayed event
                    // because we were actually able to connect
                    if self.base_node_synced {
                        let _ = self
                            .resources
                            .event_publisher
                            .send(Arc::new(OutputManagerEvent::TxoValidationFailure(
                                self.id,
                                self.validation_type,
                            )))
                            .map_err(|e| {
                                trace!(
                                    target: LOG_TARGET,
                                    "Error sending event because there are no subscribers: {:?}",
                                    e
                                );
                                e
                            });
                    }
                    return Err(OutputManagerProtocolError::new(
                        self.id,
                        OutputManagerError::MaximumAttemptsExceeded,
                    ));
                }
            }
            // Assume base node is synced until we achieve a connection and it tells us it is not synced
            self.base_node_synced = true;

            let base_node_node_id = NodeId::from_key(&self.base_node_public_key.clone())
                .map_err(|e| OutputManagerProtocolError::new(self.id, OutputManagerError::from(e)))?;
            let mut connection: Option<PeerConnection> = None;

            let delay = delay_for(self.resources.config.peer_dial_retry_timeout);

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
                            info!(target: LOG_TARGET, "Problem connecting to base node: {} for Output TXO Validation Validation Protocol: {}", e, self.id);
                        },
                    }
                },
                new_base_node = base_node_update_receiver.select_next_some() => {
                    match new_base_node {
                        Ok(_) => {
                             info!(
                                target: LOG_TARGET,
                                "TXO Validation protocol aborted due to Base Node Public key change"
                             );
                             let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(OutputManagerEvent::TxoValidationAborted(self.id, self.validation_type)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event {:?}, because there are no subscribers.",
                                        e.0
                                    );
                                    e
                                });
                            return Ok(self.id);
                        },
                        Err(e) => {
                            trace!(
                                target: LOG_TARGET,
                                "TXO Validation protocol event 'base_node_update' triggered with error: {:?}",

                                e,
                            );
                        }
                    }
                }
                _ = shutdown => {
                    info!(target: LOG_TARGET, "TXO Validation Protocol  (Id: {}) shutting down because it received the shutdown signal", self.id);
                    return Err(OutputManagerProtocolError::new(self.id, OutputManagerError::Shutdown));
                },
            }

            let mut base_node_connection = match connection {
                None => {
                    futures::select! {
                        _ = delay.fuse() => {
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(OutputManagerEvent::TxoValidationTimedOut(self.id, self.validation_type)))
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
                            info!(target: LOG_TARGET, "TXO Validation Protocol  (Id: {}) shutting down because it received the shutdown signal", self.id);
                            return Err(OutputManagerProtocolError::new(self.id, OutputManagerError::Shutdown));
                        },
                    }
                },
                Some(c) => c,
            };

            let mut client = match base_node_connection
                .connect_rpc_using_builder(
                    BaseNodeWalletRpcClient::builder().with_deadline(self.resources.config.base_node_query_timeout),
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
            let mut batch_num = 0;
            debug!(target: LOG_TARGET, "RPC client connected");
            'per_batch: loop {
                let batch = if let Some(b) = output_batches_to_query.pop() {
                    batch_num += 1;
                    b
                } else {
                    break 'main;
                };
                info!(
                    target: LOG_TARGET,
                    "Output Manager TXO Validation protocol (Id: {}) sending batch query {} of {}",
                    self.id,
                    batch_num,
                    batch_total
                );
                let delay = delay_for(self.retry_delay);
                futures::select! {
                    new_base_node = base_node_update_receiver.select_next_some() => {
                        match new_base_node {
                            Ok(_bn) => {
                             info!(target: LOG_TARGET, "TXO Validation protocol aborted due to Base Node Public key change" );
                             let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(OutputManagerEvent::TxoValidationAborted(self.id, self.validation_type)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event {:?}, because there are no subscribers.",
                                        e.0
                                    );
                                    e
                                });
                                return Ok(self.id);
                            },
                            Err(e) => {
                                trace!(
                                    target: LOG_TARGET,
                                    "TXO Validation protocol event 'base_node_update' triggered with error: {:?}",
                                    e,
                                );
                            }
                        }
                    },
                    result = self.send_query_batch(batch.clone(), &mut client).fuse() => {
                        match result {
                            Ok(synced) => {
                                self.base_node_synced = synced;
                                if !synced {
                                    info!(target: LOG_TARGET, "Base Node reports not being synced, will retry.");
                                    let _ = self
                                        .resources
                                        .event_publisher
                                        .send(Arc::new(OutputManagerEvent::TxoValidationDelayed(self.id, self.validation_type)))
                                        .map_err(|e| {
                                            trace!(
                                                target: LOG_TARGET,
                                                "Error sending event {:?}, because there are no subscribers.",
                                                e.0
                                            );
                                            e
                                        });
                                    delay.await;
                                    self.update_retry_delay(false);
                                    output_batches_to_query = self.get_output_batches().await?;
                                    retries += 1;
                                    break 'per_batch;
                                }
                                self.update_retry_delay(true);
                            },
                            Err(OutputManagerProtocolError{id: _, error: OutputManagerError::RpcError(e)}) => {
                                warn!(target: LOG_TARGET, "Error with RPC Client: {}. Retrying RPC client connection.", e);
                                delay.await;
                                self.update_retry_delay(false);
                                output_batches_to_query.push(batch);
                                retries += 1;
                                break 'per_batch;
                            }
                            Err(e) => {
                                let _ = self
                                    .resources
                                    .event_publisher
                                    .send(Arc::new(OutputManagerEvent::TxoValidationFailure(self.id, self.validation_type)))
                                    .map_err(|e| {
                                        trace!(
                                            target: LOG_TARGET,
                                            "Error sending event because there are no subscribers: {:?}",
                                            e
                                        );
                                        e
                                    });
                                return Err(e);
                            },
                        }
                    },
                    _ = shutdown => {
                        info!(target: LOG_TARGET, "TXO Validation Protocol (Id: {}) shutting down because it received the shutdown signal", self.id);
                        return Err(OutputManagerProtocolError::new(self.id, OutputManagerError::Shutdown));
                    },
                }
            }
        }

        let _ = self
            .resources
            .event_publisher
            .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(
                self.id,
                self.validation_type,
            )))
            .map_err(|e| {
                trace!(
                    target: LOG_TARGET,
                    "Error sending event {:?}, because there are no subscribers.",
                    e.0
                );
                e
            });
        Ok(self.id)
    }

    async fn send_query_batch(
        &mut self,
        batch: Vec<Vec<u8>>,
        client: &mut BaseNodeWalletRpcClient,
    ) -> Result<bool, OutputManagerProtocolError>
    {
        let request = FetchMatchingUtxos {
            output_hashes: batch.clone(),
        };

        let batch_response = client
            .fetch_matching_utxos(request)
            .await
            .map_err(|e| OutputManagerProtocolError::new(self.id, OutputManagerError::from(e)))?;

        if !batch_response.is_synced {
            return Ok(false);
        }

        let mut returned_outputs = Vec::new();
        for output_proto in batch_response.outputs.iter() {
            let output = TransactionOutput::try_from(output_proto.clone()).map_err(|_| {
                OutputManagerProtocolError::new(
                    self.id,
                    OutputManagerError::ConversionError("Could not convert protobuf TransactionOutput".to_string()),
                )
            })?;
            returned_outputs.push(output);
        }

        // complete validation
        match self.validation_type {
            TxoValidationType::Unspent => {
                // Construct a HashMap of all the unspent outputs
                let unspent_outputs: Vec<DbUnblindedOutput> =
                    self.resources.db.get_unspent_outputs().await.map_err(|e| {
                        OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                    })?;

                // We only want to check outputs that we were expecting and are still valid
                let mut output_hashes = HashMap::new();
                for uo in unspent_outputs.iter() {
                    let hash = uo.hash.clone();
                    if batch.iter().any(|h| &hash == h) {
                        output_hashes.insert(hash, uo.clone());
                    }
                }

                // Go through all the returned UTXOs and if they are in the hashmap remove them
                for output in returned_outputs.iter() {
                    let response_hash = output.hash();

                    let _ = output_hashes.remove(&response_hash);
                }

                // If there are any remaining Unspent Outputs we will move them to the invalid collection
                for (_k, v) in output_hashes {
                    // Get the transaction these belonged to so we can display the kernel signature of the transaction
                    // this output belonged to.

                    warn!(
                        target: LOG_TARGET,
                        "Output with value {} not returned from Base Node query and is thus being invalidated",
                        v.unblinded_output.value,
                    );
                    // If the output that is being invalidated has an associated TxId then get the kernel signature of
                    // the transaction and display for easier debugging
                    if let Some(tx_id) = self.resources.db.invalidate_output(v).await.map_err(|e| {
                        OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                    })? {
                        if let Ok(transaction) = self
                            .resources
                            .transaction_service
                            .get_completed_transaction(tx_id)
                            .await
                        {
                            info!(
                                target: LOG_TARGET,
                                "Invalidated Output is from Transaction (TxId: {}) with message: {} and Kernel \
                                 Signature: {}",
                                transaction.tx_id,
                                transaction.message,
                                transaction.transaction.body.kernels()[0]
                                    .excess_sig
                                    .get_signature()
                                    .to_hex()
                            )
                        }
                    } else {
                        info!(
                            target: LOG_TARGET,
                            "Invalidated Output does not have an associated TxId, it is likely a Coinbase output lost \
                             to a Re-Org"
                        );
                    }
                }
            },
            TxoValidationType::Invalid => {
                let invalid_outputs = self.resources.db.get_invalid_outputs().await.map_err(|e| {
                    OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                })?;

                for output in returned_outputs.iter() {
                    let response_hash = output.hash();

                    if let Some(output) = invalid_outputs.iter().find(|o| o.hash == response_hash) {
                        if self
                            .resources
                            .db
                            .revalidate_output(output.commitment.clone())
                            .await
                            .is_ok()
                        {
                            info!(
                                target: LOG_TARGET,
                                "Output with value {} has been restored to a valid spendable output",
                                output.unblinded_output.value
                            );
                        }
                    }
                }
            },
            TxoValidationType::Spent => {
                // Go through the response outputs and check if they are currently Spent, if they are then they can be
                // marked as Unspent because they exist in the UTXO set. Hooray!
                for output in returned_outputs.iter() {
                    match self
                        .resources
                        .db
                        .update_spent_output_to_unspent(output.clone().commitment)
                        .await
                    {
                        Ok(uo) => info!(
                            target: LOG_TARGET,
                            "Spent output with value {} restored to Unspent output", uo.unblinded_output.value
                        ),
                        Err(e) => debug!(target: LOG_TARGET, "Unable to restore Spent output to Unspent: {}", e),
                    }
                }
            },
        }
        debug!(
            target: LOG_TARGET,
            "Completed validation query for one batch of output hashes"
        );

        Ok(true)
    }

    async fn get_output_batches(&self) -> Result<Vec<Vec<Vec<u8>>>, OutputManagerProtocolError> {
        let mut outputs: Vec<Vec<u8>> = match self.validation_type {
            TxoValidationType::Unspent => self
                .resources
                .db
                .get_unspent_outputs()
                .await
                .map_err(|e| {
                    OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                })?
                .iter()
                .map(|uo| uo.hash.clone())
                .collect(),
            TxoValidationType::Spent => self
                .resources
                .db
                .get_spent_outputs()
                .await
                .map_err(|e| {
                    OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                })?
                .iter()
                .map(|uo| uo.hash.clone())
                .collect(),
            TxoValidationType::Invalid => self
                .resources
                .db
                .get_invalid_outputs()
                .await
                .map_err(|e| {
                    OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                })?
                .into_iter()
                .map(|uo| uo.hash)
                .collect(),
        };

        // Determine how many rounds of base node request we need to query all the transactions in batches of
        // max_tx_query_batch_size
        let num_batches =
            ((outputs.len() as f32) / (self.resources.config.max_utxo_query_size as f32 + 0.1)) as usize + 1;

        let mut batches: Vec<Vec<Vec<u8>>> = Vec::new();
        for _b in 0..num_batches {
            let mut batch = Vec::new();
            for o in outputs.drain(..cmp::min(self.resources.config.max_utxo_query_size, outputs.len())) {
                batch.push(o);
            }
            if !batch.is_empty() {
                batches.push(batch);
            }
        }
        Ok(batches)
    }

    // exponential back-off with max and min delays
    fn update_retry_delay(&mut self, synced: bool) {
        let new_delay = if synced {
            self.resources.config.base_node_query_timeout
        } else {
            let delay = self.retry_delay;
            cmp::min(delay * 2, MAX_RETRY_DELAY)
        };

        self.retry_delay = new_delay;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxoValidationType {
    Unspent,
    Spent,
    Invalid,
}

impl fmt::Display for TxoValidationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxoValidationType::Unspent => write!(f, "Unspent Outputs Validation"),
            TxoValidationType::Spent => write!(f, "Spent Outputs Validation"),
            TxoValidationType::Invalid => write!(f, "Invalid Outputs Validation"),
        }
    }
}

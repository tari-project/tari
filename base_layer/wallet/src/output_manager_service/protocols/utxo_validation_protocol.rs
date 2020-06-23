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

use crate::output_manager_service::{
    error::{OutputManagerError, OutputManagerProtocolError},
    handle::OutputManagerEvent,
    service::OutputManagerResources,
    storage::{database::OutputManagerBackend, models::DbUnblindedOutput},
};
use futures::{FutureExt, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{cmp, collections::HashMap, convert::TryFrom, fmt, sync::Arc, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    transactions::transaction::TransactionOutput,
};
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};
use tari_p2p::tari_message::TariMessageType;
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::output_manager_service::protocols::utxo_validation_protocol";

pub struct UtxoValidationProtocol<TBackend>
where TBackend: OutputManagerBackend + Clone + 'static
{
    id: u64,
    validation_type: UtxoValidationType,
    retry_strategy: UtxoValidationRetry,
    resources: OutputManagerResources<TBackend>,
    base_node_public_key: CommsPublicKey,
    timeout: Duration,
    base_node_response_receiver: Option<broadcast::Receiver<Arc<BaseNodeProto::BaseNodeServiceResponse>>>,
    pending_queries: HashMap<u64, Vec<Vec<u8>>>,
}

/// This protocol defines the process of submitting our current UTXO set to the Base Node to validate it.
impl<TBackend> UtxoValidationProtocol<TBackend>
where TBackend: OutputManagerBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        validation_type: UtxoValidationType,
        retry_strategy: UtxoValidationRetry,
        resources: OutputManagerResources<TBackend>,
        base_node_public_key: CommsPublicKey,
        timeout: Duration,
        base_node_response_receiver: broadcast::Receiver<Arc<BaseNodeProto::BaseNodeServiceResponse>>,
    ) -> Self
    {
        Self {
            id,
            validation_type,
            retry_strategy,
            resources,
            base_node_public_key,
            timeout,
            base_node_response_receiver: Some(base_node_response_receiver),
            pending_queries: Default::default(),
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, OutputManagerProtocolError> {
        let mut base_node_response_receiver = self
            .base_node_response_receiver
            .take()
            .ok_or_else(|| {
                OutputManagerProtocolError::new(
                    self.id,
                    OutputManagerError::ServiceError("No base node response channel provided".to_string()),
                )
            })?
            .fuse();

        trace!(
            target: LOG_TARGET,
            "Starting UTXO validation protocol (Id: {}) for {}",
            self.id,
            self.validation_type,
        );

        let outputs_to_query: Vec<Vec<u8>> = match self.validation_type {
            UtxoValidationType::Unspent => self
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
            UtxoValidationType::Invalid => self
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

        if outputs_to_query.is_empty() {
            trace!(
                target: LOG_TARGET,
                "UTXO validation protocol (Id: {}) has no outputs to validate",
                self.id,
            );
            return Ok(self.id);
        }

        let total_retries_str = match self.retry_strategy {
            UtxoValidationRetry::Limited(n) => format!("{}", n),
            UtxoValidationRetry::UntilSuccess => "∞".to_string(),
        };

        let mut retries = 0;
        loop {
            self.send_queries(outputs_to_query.clone()).await?;

            let mut delay = delay_for(self.timeout).fuse();

            loop {
                futures::select! {
                    base_node_response = base_node_response_receiver.select_next_some() => {
                        match base_node_response {
                            Ok(response) => if self.handle_base_node_response(response).await? {
                            error!(target: LOG_TARGET, "Response handled with success for {} and pending_queries len: {}", self.id, self.pending_queries.len());
                                if self.pending_queries.is_empty() {
                                    let _ = self
                                        .resources
                                        .event_publisher
                                        .send(OutputManagerEvent::UtxoValidationSuccess(self.id))
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
                            },
                            Err(e) => trace!(target: LOG_TARGET, "Error reading broadcast base_node_response: {:?}", e),
                        }

                    },
                    () = delay => {
                        break;
                    },
                }
            }

            info!(
                target: LOG_TARGET,
                "UTXO Validation protocol (Id: {}) attempt {} out of {} timed out.",
                self.id,
                retries + 1,
                total_retries_str
            );

            let _ = self
                .resources
                .event_publisher
                .send(OutputManagerEvent::UtxoValidationTimedOut(self.id))
                .map_err(|e| {
                    trace!(
                        target: LOG_TARGET,
                        "Error sending event {:?}, because there are no subscribers.",
                        e.0
                    );
                    e
                });

            retries += 1;
            match self.retry_strategy {
                UtxoValidationRetry::Limited(n) => {
                    if retries >= n {
                        break;
                    }
                },
                UtxoValidationRetry::UntilSuccess => (),
            }

            self.pending_queries.clear();
        }

        info!(
            target: LOG_TARGET,
            "Maximum attempts exceeded for UTXO Validation Protocol (Id: {})", self.id
        );
        Err(OutputManagerProtocolError::new(
            self.id,
            OutputManagerError::MaximumAttemptsExceeded,
        ))
    }

    async fn send_queries(&mut self, mut outputs_to_query: Vec<Vec<u8>>) -> Result<(), OutputManagerProtocolError> {
        // Determine how many rounds of base node request we need to query all the outputs in batches of
        // max_utxo_query_size
        let rounds =
            ((outputs_to_query.len() as f32) / (self.resources.config.max_utxo_query_size as f32 + 0.1)) as usize + 1;

        for r in 0..rounds {
            let mut output_hashes = Vec::new();
            for uo_hash in
                outputs_to_query.drain(..cmp::min(self.resources.config.max_utxo_query_size, outputs_to_query.len()))
            {
                output_hashes.push(uo_hash);
            }

            let request_key = if r == 0 { self.id } else { OsRng.next_u64() };

            let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs {
                outputs: output_hashes.clone(),
            });

            let service_request = BaseNodeProto::BaseNodeServiceRequest {
                request_key,
                request: Some(request),
            };

            let send_message_response = self
                .resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                )
                .await
                .map_err(|e| OutputManagerProtocolError::new(self.id, OutputManagerError::from(e)))?;

            // Here we are going to spawn a non-blocking task that will monitor and log the progress of the
            // send process.
            tokio::spawn(async move {
                match send_message_response.resolve().await {
                    Err(e) => trace!(
                        target: LOG_TARGET,
                        "Failed to send Output Manager UTXO query ({}) to Base Node: {}",
                        request_key,
                        e
                    ),
                    Ok(send_states) => {
                        trace!(
                            target: LOG_TARGET,
                            "Output Manager UTXO query ({}) queued for sending with Message {}",
                            request_key,
                            send_states[0].tag,
                        );
                        let message_tag = send_states[0].tag;
                        if send_states.wait_single().await {
                            trace!(
                                target: LOG_TARGET,
                                "Output Manager UTXO query ({}) successfully sent to Base Node with Message {}",
                                request_key,
                                message_tag,
                            )
                        } else {
                            trace!(
                                target: LOG_TARGET,
                                "Failed to send Output Manager UTXO query ({}) to Base Node with Message {}",
                                request_key,
                                message_tag,
                            );
                        }
                    },
                }
            });

            self.pending_queries.insert(request_key, output_hashes);

            info!(
                target: LOG_TARGET,
                "Output Manager {} query (Id: {}) sent to Base Node, part {} of {} requests",
                self.validation_type,
                request_key,
                r + 1,
                rounds
            );
        }

        Ok(())
    }

    async fn handle_base_node_response(
        &mut self,
        response: Arc<BaseNodeProto::BaseNodeServiceResponse>,
    ) -> Result<bool, OutputManagerProtocolError>
    {
        let request_key = response.request_key;

        let queried_hashes = if let Some(hashes) = self.pending_queries.remove(&request_key) {
            hashes
        } else {
            trace!(
                target: LOG_TARGET,
                "Base Node Response (Id: {}) not expected for UTXO Validation protocol {}",
                request_key,
                self.id
            );
            return Ok(false);
        };

        trace!(
            target: LOG_TARGET,
            "Handling a Base Node Response for {} request (Id: {}) for UTXO Validation protocol {}",
            self.validation_type,
            request_key,
            self.id
        );

        let response: Vec<tari_core::transactions::proto::types::TransactionOutput> = match (*response).clone().response
        {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => outputs.outputs,
            _ => {
                return Err(OutputManagerProtocolError::new(
                    self.id,
                    OutputManagerError::InvalidResponseError("Base Node Response of unexpected variant".to_string()),
                ));
            },
        };

        match self.validation_type {
            UtxoValidationType::Unspent => {
                // Construct a HashMap of all the unspent outputs
                let unspent_outputs: Vec<DbUnblindedOutput> =
                    self.resources.db.get_unspent_outputs().await.map_err(|e| {
                        OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                    })?;

                // We only want to check outputs that we were expecting and are still valid
                let mut output_hashes = HashMap::new();
                for uo in unspent_outputs.iter() {
                    let hash = uo.hash.clone();
                    if queried_hashes.iter().any(|h| &hash == h) {
                        output_hashes.insert(hash, uo.clone());
                    }
                }

                // Go through all the returned UTXOs and if they are in the hashmap remove them
                for output in response.iter() {
                    let response_hash = TransactionOutput::try_from(output.clone())
                        .map_err(|_| {
                            OutputManagerProtocolError::new(
                                self.id,
                                OutputManagerError::ConversionError(
                                    "Could not convert protobuf TransactionOutput".to_string(),
                                ),
                            )
                        })?
                        .hash();

                    let _ = output_hashes.remove(&response_hash);
                }

                // If there are any remaining Unspent Outputs we will move them to the invalid collection
                for (_k, v) in output_hashes {
                    // Get the transaction these belonged to so we can display the kernel signature of the transaction
                    // this output belonged to.

                    warn!(
                        target: LOG_TARGET,
                        "Output with value {} not returned from Base Node query ({}) and is thus being invalidated",
                        v.unblinded_output.value,
                        request_key,
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
                            "Invalidated Output does not have an associated TxId so it is likely a Coinbase output \
                             lost to a Re-Org"
                        );
                    }
                }
                debug!(
                    target: LOG_TARGET,
                    "Handled Base Node response (Id: {}) for Unspent Outputs Query {}", request_key, self.id
                );
            },
            UtxoValidationType::Invalid => {
                let invalid_outputs = self.resources.db.get_invalid_outputs().await.map_err(|e| {
                    OutputManagerProtocolError::new(self.id, OutputManagerError::OutputManagerStorageError(e))
                })?;

                for output in response.iter() {
                    let response_hash = TransactionOutput::try_from(output.clone())
                        .map_err(|_| {
                            OutputManagerProtocolError::new(
                                self.id,
                                OutputManagerError::ConversionError("Could not convert Transaction Output".to_string()),
                            )
                        })?
                        .hash();

                    if let Some(output) = invalid_outputs.iter().find(|o| o.hash == response_hash) {
                        if self
                            .resources
                            .db
                            .revalidate_output(output.unblinded_output.spending_key.clone())
                            .await
                            .is_ok()
                        {
                            trace!(
                                target: LOG_TARGET,
                                "Output with value {} has been restored to a valid spendable output",
                                output.unblinded_output.value
                            );
                        }
                    }
                }

                debug!(
                    target: LOG_TARGET,
                    "Handled Base Node response (Id: {}) for Invalidated Outputs Query {}", request_key, self.id
                );
            },
        }
        Ok(true)
    }
}

pub enum UtxoValidationType {
    Unspent,
    Invalid,
}

impl fmt::Display for UtxoValidationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtxoValidationType::Unspent => write!(f, "Unspent Outputs Validation"),
            UtxoValidationType::Invalid => write!(f, "Invalid Outputs Validation"),
        }
    }
}

// 0 means keep retying until success
#[derive(Debug)]
pub enum UtxoValidationRetry {
    Limited(u8),
    UntilSuccess,
}

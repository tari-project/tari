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
        storage::{database::TransactionBackend, models::TransactionStatus},
    },
};
use futures::{channel::mpsc::Receiver, FutureExt, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_core::{
    base_node::{
        proto,
        proto::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    transactions::transaction::TransactionOutput,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_p2p::tari_message::TariMessageType;
use tokio::{sync::broadcast, time::delay_for};
const LOG_TARGET: &str = "wallet::transaction_service::protocols::chain_monitoring_protocol";

/// This protocol defines the process of monitoring a mempool and base node to detect when a Broadcast transaction is
/// Mined or leaves the mempool in which case it should be cancelled

pub struct TransactionCoinbaseMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    id: u64,
    tx_id: TxId,
    block_height: u64,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    base_node_response_receiver: Option<Receiver<proto::BaseNodeServiceResponse>>,
    timeout_update_receiver: Option<broadcast::Receiver<Duration>>,
}

impl<TBackend> TransactionCoinbaseMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        tx_id: TxId,
        block_height: u64,
        resources: TransactionServiceResources<TBackend>,
        timeout: Duration,
        base_node_public_key: CommsPublicKey,
        base_node_response_receiver: Receiver<proto::BaseNodeServiceResponse>,
        timeout_update_receiver: broadcast::Receiver<Duration>,
    ) -> Self
    {
        Self {
            id,
            tx_id,
            block_height,
            resources,
            timeout,
            base_node_public_key,
            base_node_response_receiver: Some(base_node_response_receiver),
            timeout_update_receiver: Some(timeout_update_receiver),
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut base_node_response_receiver = self
            .base_node_response_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut timeout_update_receiver = self
            .timeout_update_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

        trace!(
            target: LOG_TARGET,
            "Starting coinbase monitoring protocol for TxId: {} with Protocol ID: {}",
            self.tx_id,
            self.id
        );

        // This is the main loop of the protocol and following the following steps
        // 1) Check transaction being monitored is still in the Coinbase state and needs to be monitored
        // 2) Send a GetchainMetadata and a FetchUtxo request to the base node
        // 3) Wait for both Base Node responses OR a Timeout
        //      a) If the chain tip moves beyond this block height AND output is not in the Utxo set cancel this Tx
        //      b) If the output is in the Utxo set the protocol can end with success
        //      c) Timeout is reached > Start again
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
                        self.id,
                        TransactionServiceError::TransactionDoesNotExistError,
                    ));
                },
            };

            if completed_tx.status != TransactionStatus::Coinbase {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) no longer in Coinbase state and will stop being monitored for being Mined",
                    self.tx_id
                );
                return Ok(self.id);
            }

            let mut hashes = Vec::new();
            for o in completed_tx.transaction.body.outputs() {
                hashes.push(o.hash());
            }

            info!(
                target: LOG_TARGET,
                "Sending Transaction Mined? request for Coinbase Tx with TxId: {} and Kernel Signature {} to Base Node",
                completed_tx.tx_id,
                completed_tx.transaction.body.kernels()[0]
                    .excess_sig
                    .get_signature()
                    .to_hex(),
            );

            // Send Base Node GetChainMetadata query
            let request = BaseNodeRequestProto::GetChainMetadata(true);
            let service_request = proto::BaseNodeServiceRequest {
                request_key: self.id,
                request: Some(request),
            };
            self.resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                )
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            let request = BaseNodeRequestProto::FetchMatchingUtxos(proto::HashOutputs { outputs: hashes });
            let service_request = proto::BaseNodeServiceRequest {
                request_key: self.id,
                request: Some(request),
            };
            self.resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                )
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            let mut delay = delay_for(self.timeout).fuse();
            let mut shutdown = self.resources.shutdown_signal.clone();
            let mut chain_metadata_response_received: Option<bool> = None;
            let mut fetch_utxo_response_received = false;
            // Loop until both a Mempool response AND a Base node response is received OR the Timeout expires.
            loop {
                futures::select! {
                    base_node_response = base_node_response_receiver.select_next_some() => {
                        match self
                        .handle_base_node_response(completed_tx.tx_id, base_node_response)
                        .await? {
                            BaseNodeResponseType::ChainMetadata(result) =>
                                chain_metadata_response_received = Some(result),
                            BaseNodeResponseType::FetchUtxo(result) => {
                                if result {
                                    // Tx is mined!
                                    return Ok(self.id);
                                }
                                fetch_utxo_response_received = true;
                            },
                            _ => (),
                        }

                    },
                    updated_timeout = timeout_update_receiver.select_next_some() => {
                        if let Ok(to) = updated_timeout {
                            self.timeout = to;
                             info!(
                                target: LOG_TARGET,
                                "Coinbase monitoring protocol (Id: {}) timeout updated to {:?}", self.id, self.timeout
                            );
                            break;
                        }
                    },
                    () = delay => {
                        break;
                    },
                    _ = shutdown => {
                        info!(target: LOG_TARGET, "Transaction Coinbase Monitoring Protocol (id: {}) shutting down because it received the shutdown signal", self.id);
                        return Err(TransactionServiceProtocolError::new(self.id, TransactionServiceError::Shutdown))
                    },
                }

                if fetch_utxo_response_received {
                    if let Some(result) = chain_metadata_response_received {
                        // If the tip has moved beyond this Coinbase transaction's blockheight and it wasn't mined then
                        // it should be cancelled

                        if !result {
                            error!(
                                target: LOG_TARGET,
                                "Chain tip has moved ahead of this Coinbase transaction's block height without it \
                                 being mine. Cancelling Coinbase transaction (TxId: {})",
                                completed_tx.tx_id
                            );
                            if let Err(e) = self
                                .resources
                                .output_manager_service
                                .cancel_transaction(completed_tx.tx_id)
                                .await
                            {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel outputs for Coinbase TX_ID: {} with error: {:?}",
                                    completed_tx.tx_id,
                                    e
                                );
                            }
                            if let Err(e) = self.resources.db.cancel_completed_transaction(completed_tx.tx_id).await {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel Coinbase TX_ID: {} with error: {:?}", completed_tx.tx_id, e
                                );
                            }
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionCancelled(completed_tx.tx_id)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event, usually because there are no subscribers: {:?}",
                                        e
                                    );
                                    e
                                });
                            return Err(TransactionServiceProtocolError::new(
                                self.id,
                                TransactionServiceError::ChainTipHigherThanCoinbaseHeight,
                            ));
                        }

                        break;
                    }
                }
            }

            if chain_metadata_response_received.is_some() && fetch_utxo_response_received {
                debug!(
                    target: LOG_TARGET,
                    "Both Base node responses received. TxId: {:?} not mined yet.", completed_tx.tx_id,
                );
                // Finish out the rest of this period before moving onto next round
                delay.await;
            }

            info!(
                target: LOG_TARGET,
                "Coinbase monitoring process timed out for Transaction TX_ID: {}", completed_tx.tx_id
            );

            let _ = self
                .resources
                .event_publisher
                .send(Arc::new(TransactionEvent::TransactionMinedRequestTimedOut(
                    completed_tx.tx_id,
                )))
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

    async fn handle_base_node_response(
        &mut self,
        tx_id: TxId,
        response: proto::BaseNodeServiceResponse,
    ) -> Result<BaseNodeResponseType, TransactionServiceProtocolError>
    {
        let mut returned_ouputs: Vec<tari_core::proto::types::TransactionOutput> = Vec::new();
        match response.response {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => returned_ouputs = outputs.outputs,
            Some(BaseNodeResponseProto::ChainMetadata(metadata)) => {
                if let Some(tip) = metadata.height_of_longest_chain {
                    return if tip > self.block_height {
                        Ok(BaseNodeResponseType::ChainMetadata(false))
                    } else {
                        Ok(BaseNodeResponseType::ChainMetadata(true))
                    };
                }
            },
            _ => {
                return Ok(BaseNodeResponseType::Other);
            },
        }

        let completed_tx = match self.resources.db.get_completed_transaction(tx_id).await {
            Ok(tx) => tx,
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Cannot find Completed Transaction (TxId: {}) referred to by this Coinbase Monitoring Protocol: \
                     {:?}",
                    self.tx_id,
                    e
                );
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::TransactionDoesNotExistError,
                ));
            },
        };

        if completed_tx.status == TransactionStatus::Coinbase {
            let mut check = true;

            for output in returned_ouputs.iter() {
                let transaction_output = TransactionOutput::try_from(output.clone()).map_err(|_| {
                    TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::ConversionError("Could not convert Transaction Output".to_string()),
                    )
                })?;

                check = check &&
                    completed_tx
                        .transaction
                        .body
                        .outputs()
                        .iter()
                        .any(|item| item == &transaction_output);
            }
            // If all outputs are present then mark this transaction as mined.
            if check && !returned_ouputs.is_empty() {
                self.resources
                    .output_manager_service
                    .confirm_transaction(
                        completed_tx.tx_id,
                        completed_tx.transaction.body.inputs().clone(),
                        completed_tx.transaction.body.outputs().clone(),
                    )
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                self.resources
                    .db
                    .mine_completed_transaction(completed_tx.tx_id)
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionMined(completed_tx.tx_id)))
                    .map_err(|e| {
                        trace!(
                            target: LOG_TARGET,
                            "Error sending event, usually because there are no subscribers: {:?}",
                            e
                        );
                        e
                    });

                info!(
                    target: LOG_TARGET,
                    "Coinbase Transaction (TxId: {:?}) detected as mined on the Base Layer", completed_tx.tx_id
                );

                return Ok(BaseNodeResponseType::FetchUtxo(true));
            }
        }

        Ok(BaseNodeResponseType::FetchUtxo(false))
    }
}

enum BaseNodeResponseType {
    ChainMetadata(bool),
    FetchUtxo(bool),
    Other,
}

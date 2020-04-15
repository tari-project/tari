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
        storage::database::{TransactionBackend, TransactionStatus},
    },
};
use futures::{channel::mpsc::Receiver, FutureExt, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundEncryption};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    mempool::{
        proto::mempool as MempoolProto,
        service::{MempoolResponse, MempoolServiceResponse},
        TxStorageResponse,
    },
    transactions::transaction::TransactionOutput,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_p2p::tari_message::TariMessageType;
use tokio::time::delay_for;

const LOG_TARGET: &str = "wallet::transaction_service::protocols::chain_monitoring_protocol";

/// This protocol defines the process of monitoring a mempool and base node to detect when a Broadcast transaction is
/// Mined or leaves the mempool in which case it should be cancelled
pub struct TransactionChainMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    tx_id: TxId,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    mempool_response_receiver: Option<Receiver<MempoolServiceResponse>>,
    base_node_response_receiver: Option<Receiver<BaseNodeProto::BaseNodeServiceResponse>>,
}

impl<TBackend> TransactionChainMonitoringProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        tx_id: TxId,
        resources: TransactionServiceResources<TBackend>,
        timeout: Duration,
        base_node_public_key: CommsPublicKey,
        mempool_response_receiver: Receiver<MempoolServiceResponse>,
        base_node_response_receiver: Receiver<BaseNodeProto::BaseNodeServiceResponse>,
    ) -> Self
    {
        Self {
            id,
            tx_id,
            resources,
            timeout,
            base_node_public_key,
            mempool_response_receiver: Some(mempool_response_receiver),
            base_node_response_receiver: Some(base_node_response_receiver),
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut mempool_response_receiver =
            self.mempool_response_receiver
                .take()
                .ok_or(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::InvalidStateError,
                ))?;

        let mut base_node_response_receiver =
            self.base_node_response_receiver
                .take()
                .ok_or(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::InvalidStateError,
                ))?;

        trace!(
            target: LOG_TARGET,
            "Starting chain monitoring protocol for TxId: {} with Protocol ID: {}",
            self.tx_id,
            self.id
        );

        // This is the main loop of the protocol and following the following steps
        // 1) Check transaction being monitored is still in the Broadcast state and needs to be monitored
        // 2) Send a MempoolRequest::GetTxStateWithExcessSig to Mempool and a Mined? Request to base node
        // 3) Wait for both a Mempool response and Base Node response for the correct Id OR a Timeout
        //      a) If the Tx is not in the mempool AND is not mined the protocol ends and Tx should be cancelled
        //      b) If the Tx is in the mempool AND not mined > perform another iteration
        //      c) If the Tx is in the mempool AND mined then update the status of the Tx and end the protocol
        //      c) Timeout is reached > Start again
        loop {
            let completed_tx = match self.resources.db.get_completed_transaction(self.tx_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Cannot find Completed Transaction (TxId: {}) referred to by this Chain Monitoring Protocol: \
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

            if completed_tx.status != TransactionStatus::Broadcast {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) no longer in Broadcast state and will stop being monitored for being Mined",
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
                "Sending Transaction Mined? request for TxId: {} and Kernel Signature {} to Base Node (Contains {} \
                 outputs)",
                completed_tx.tx_id,
                completed_tx.transaction.body.kernels()[0]
                    .excess_sig
                    .get_signature()
                    .to_hex(),
                hashes.len(),
            );

            // Send Mempool query
            let tx_excess_sig = completed_tx.transaction.body.kernels()[0].excess_sig.clone();
            let mempool_request = MempoolProto::MempoolServiceRequest {
                request_key: self.id,
                request: Some(MempoolProto::mempool_service_request::Request::GetTxStateWithExcessSig(
                    tx_excess_sig.into(),
                )),
            };

            self.resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundEncryption::None,
                    OutboundDomainMessage::new(TariMessageType::MempoolRequest, mempool_request.clone()),
                )
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            // Send Base Node query
            let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs { outputs: hashes });
            let service_request = BaseNodeProto::BaseNodeServiceRequest {
                request_key: self.id,
                request: Some(request),
            };
            self.resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundEncryption::None,
                    OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
                )
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            let mut delay = delay_for(self.timeout).fuse();
            let mut received_mempool_response = false;
            let mut received_base_node_response = false;
            // Loop until both a Mempool response AND a Base node response is received OR the Timeout expires.
            loop {
                futures::select! {
                    mempool_response = mempool_response_receiver.select_next_some() => {
                        if !self
                        .handle_mempool_response(completed_tx.tx_id, mempool_response)
                        .await?
                        {
                            return Err(TransactionServiceProtocolError::new(
                                self.id,
                                TransactionServiceError::MempoolRejection,
                            ));
                        }
                        received_mempool_response = true;

                    },
                    base_node_response = base_node_response_receiver.select_next_some() => {
                        if self
                        .handle_base_node_response(completed_tx.tx_id, base_node_response)
                        .await?
                        {
                            // Tx is mined!
                            return Ok(self.id);
                        }
                        received_base_node_response = true;
                    },
                    () = delay => {
                        break;
                    },
                }

                // If we have received both responses from this round we can stop waiting for more responses
                if received_mempool_response && received_base_node_response {
                    break;
                }
            }

            if received_mempool_response && received_base_node_response {
                info!(
                    target: LOG_TARGET,
                    "Base node and Mempool response received. TxId: {:?} not mined yet.", completed_tx.tx_id,
                );
                // Finish out the rest of this period before moving onto next round
                delay.await;
            }

            info!(
                target: LOG_TARGET,
                "Chain monitoring process timed out for Transaction TX_ID: {}", completed_tx.tx_id
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

    async fn handle_mempool_response(
        &mut self,
        tx_id: TxId,
        response: MempoolServiceResponse,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        // Handle a receive Mempool Response
        match response.response {
            MempoolResponse::Stats(_) => {
                error!(target: LOG_TARGET, "Invalid Mempool response variant");
            },
            MempoolResponse::State(_) => {
                error!(target: LOG_TARGET, "Invalid Mempool response variant");
            },
            MempoolResponse::TxStorage(ts) => {
                let completed_tx = match self.resources.db.get_completed_transaction(tx_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "Cannot find Completed Transaction (TxId: {}) referred to by this Chain Monitoring \
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
                match completed_tx.status {
                    TransactionStatus::Broadcast => match ts {
                        // Getting this response means the Mempool Rejected this transaction so it will be
                        // cancelled.
                        TxStorageResponse::NotStored => {
                            error!(
                                target: LOG_TARGET,
                                "Mempool response received for TxId: {:?}. Transaction was REJECTED. Cancelling \
                                 transaction.",
                                tx_id
                            );
                            if let Err(e) = self
                                .resources
                                .output_manager_service
                                .cancel_transaction(completed_tx.tx_id)
                                .await
                            {
                                error!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel outputs for TX_ID: {} after failed sending attempt with error \
                                     {:?}",
                                    completed_tx.tx_id,
                                    e
                                );
                            }
                            if let Err(e) = self.resources.db.cancel_completed_transaction(completed_tx.tx_id).await {
                                error!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}",
                                    completed_tx.tx_id,
                                    e
                                );
                            }
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionSendDiscoveryComplete(
                                    completed_tx.tx_id,
                                    false,
                                )))
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
                                TransactionServiceError::MempoolRejection,
                            ));
                        },
                        // Any other variant of this enum means the transaction has been received by the
                        // base_node and is in one of the various mempools
                        _ => {
                            // If this transaction is still in the Completed State it should be upgraded to the
                            // Broadcast state
                            info!(
                                target: LOG_TARGET,
                                "Completed Transaction (TxId: {} and Kernel Excess Sig: {}) detected in Base Node \
                                 Mempool in {:?}",
                                completed_tx.tx_id,
                                completed_tx.transaction.body.kernels()[0]
                                    .excess_sig
                                    .get_signature()
                                    .to_hex(),
                                ts
                            );
                            return Ok(true);
                        },
                    },
                    _ => (),
                }
            },
        }

        Ok(true)
    }

    async fn handle_base_node_response(
        &mut self,
        tx_id: TxId,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        let response: Vec<tari_core::transactions::proto::types::TransactionOutput> = match response.response {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => outputs.outputs,
            _ => {
                return Ok(false);
            },
        };

        let completed_tx = match self.resources.db.get_completed_transaction(tx_id).await {
            Ok(tx) => tx,
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "Cannot find Completed Transaction (TxId: {}) referred to by this Chain Monitoring Protocol: {:?}",
                    self.tx_id,
                    e
                );
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::TransactionDoesNotExistError,
                ));
            },
        };

        if completed_tx.status == TransactionStatus::Broadcast {
            let mut check = true;

            for output in response.iter() {
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
            if check && !response.is_empty() {
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
                    "Transaction (TxId: {:?}) detected as mined on the Base Layer", completed_tx.tx_id
                );

                return Ok(true);
            }
        }

        Ok(false)
    }
}

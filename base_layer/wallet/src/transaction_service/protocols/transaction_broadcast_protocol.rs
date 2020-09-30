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

use crate::transaction_service::{
    error::{TransactionServiceError, TransactionServiceProtocolError},
    handle::TransactionEvent,
    service::TransactionServiceResources,
    storage::{database::TransactionBackend, models::TransactionStatus},
};
use futures::{channel::mpsc::Receiver, FutureExt, StreamExt};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
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
    mempool::{
        proto::mempool as MempoolProto,
        service::{MempoolResponse, MempoolServiceResponse},
        TxStorageResponse,
    },
    transactions::transaction::TransactionOutput,
};
use tari_crypto::tari_utilities::{hex::Hex, Hashable};
use tari_p2p::tari_message::TariMessageType;
use tokio::{sync::broadcast, time::delay_for};

const LOG_TARGET: &str = "wallet::transaction_service::protocols::broadcast_protocol";
const LOG_TARGET_STRESS: &str = "stress_test::broadcast_protocol";

/// This protocol defines the process of monitoring a mempool and base node to detect when a Completed transaction is
/// Broadcast to the mempool or potentially Mined
pub struct TransactionBroadcastProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    id: u64,
    resources: TransactionServiceResources<TBackend>,
    timeout: Duration,
    base_node_public_key: CommsPublicKey,
    mempool_response_receiver: Option<Receiver<MempoolServiceResponse>>,
    base_node_response_receiver: Option<Receiver<BaseNodeProto::BaseNodeServiceResponse>>,
    timeout_update_receiver: Option<broadcast::Receiver<Duration>>,
}

impl<TBackend> TransactionBroadcastProtocol<TBackend>
where TBackend: TransactionBackend + Clone + 'static
{
    pub fn new(
        id: u64,
        resources: TransactionServiceResources<TBackend>,
        timeout: Duration,
        base_node_public_key: CommsPublicKey,
        mempool_response_receiver: Receiver<MempoolServiceResponse>,
        base_node_response_receiver: Receiver<BaseNodeProto::BaseNodeServiceResponse>,
        timeout_update_receiver: broadcast::Receiver<Duration>,
    ) -> Self
    {
        Self {
            id,
            resources,
            timeout,
            base_node_public_key,
            mempool_response_receiver: Some(mempool_response_receiver),
            base_node_response_receiver: Some(base_node_response_receiver),
            timeout_update_receiver: Some(timeout_update_receiver),
        }
    }

    /// The task that defines the execution of the protocol.
    pub async fn execute(mut self) -> Result<u64, TransactionServiceProtocolError> {
        let mut mempool_response_receiver = self
            .mempool_response_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut base_node_response_receiver = self
            .base_node_response_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?;

        let mut timeout_update_receiver = self
            .timeout_update_receiver
            .take()
            .ok_or_else(|| TransactionServiceProtocolError::new(self.id, TransactionServiceError::InvalidStateError))?
            .fuse();

        // This is the main loop of the protocol and following the following steps
        // 1) Check transaction being monitored is still in the Completed state and needs to be monitored
        // 2) Send a MempoolRequest::SubmitTransaction to Mempool and a Mined? Request to base node
        // 3) Wait for a either a Mempool response, Base Node response for the correct Id OR a Timeout
        //      a) A Mempool response for this Id is received >  update the Tx status and end the protocol
        //      b) A Basenode response for this Id is received showing it is mined > Update Tx status and end protocol
        //      c) Timeout is reached > Start again
        loop {
            let completed_tx = match self.resources.db.get_completed_transaction(self.id).await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Cannot find Completed Transaction (TxId: {}) referred to by this Broadcast protocol: {:?}",
                        self.id,
                        e
                    );
                    return Err(TransactionServiceProtocolError::new(
                        self.id,
                        TransactionServiceError::TransactionDoesNotExistError,
                    ));
                },
            };

            if completed_tx.status != TransactionStatus::Completed {
                debug!(
                    target: LOG_TARGET,
                    "Transaction (TxId: {}) no longer in Completed state and will stop being broadcast", self.id
                );
                return Ok(self.id);
            }

            info!(
                target: LOG_TARGET,
                "Attempting to Broadcast Transaction (TxId: {} and Kernel Signature: {}) to Mempool",
                self.id,
                completed_tx.transaction.body.kernels()[0]
                    .excess_sig
                    .get_signature()
                    .to_hex()
            );
            trace!(target: LOG_TARGET, "{}", completed_tx.transaction);

            // Send Mempool Request
            let mempool_request = MempoolProto::MempoolServiceRequest {
                request_key: completed_tx.tx_id,
                request: Some(MempoolProto::mempool_service_request::Request::SubmitTransaction(
                    completed_tx.transaction.clone().into(),
                )),
            };

            self.resources
                .outbound_message_service
                .send_direct(
                    self.base_node_public_key.clone(),
                    OutboundDomainMessage::new(TariMessageType::MempoolRequest, mempool_request.clone()),
                )
                .await
                .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

            // Send Base Node query
            let mut hashes = Vec::new();
            for o in completed_tx.transaction.body.outputs() {
                hashes.push(o.hash());
            }

            let request = BaseNodeRequestProto::FetchUtxos(BaseNodeProto::HashOutputs { outputs: hashes });
            let service_request = BaseNodeProto::BaseNodeServiceRequest {
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
            futures::select! {
                mempool_response = mempool_response_receiver.select_next_some() => {
                    trace!(
                        target: LOG_TARGET,
                        "Transaction monitoring event 'mempool_response' triggered ({:?})",
                        mempool_response,
                    );
                    if self.handle_mempool_response(mempool_response).await? {
                        break;
                    }
                },
                base_node_response = base_node_response_receiver.select_next_some() => {
                    trace!(
                        target: LOG_TARGET,
                        "Transaction monitoring event 'base_node_response' triggered ({:?})",
                        base_node_response,
                    );
                    if self.handle_base_node_response(base_node_response).await? {
                        break;
                    }
                },
                updated_timeout = timeout_update_receiver.select_next_some() => {
                    if let Ok(to) = updated_timeout {
                        self.timeout = to;
                        info!(
                            target: LOG_TARGET,
                            "Transaction monitoring event 'updated_timeout' triggered (Id: {}), timeout updated to {:?}", self.id ,self.timeout
                        );
                    } else {
                        trace!(
                            target: LOG_TARGET,
                            "Transaction monitoring event 'updated_timeout' triggered (Id: {}) ({:?})",
                            self.id,
                            updated_timeout,
                        );
                    }
                },
                () = delay => {
                    trace!(
                        target: LOG_TARGET,
                        "Transaction monitoring event 'time_out' for Mempool broadcast (Id: {}) ", self.id
                    );
                    let _ = self
                        .resources
                        .event_publisher
                        .send(Arc::new(TransactionEvent::MempoolBroadcastTimedOut(self.id)))
                        .map_err(|e| {
                            trace!(
                                target: LOG_TARGET,
                                "Error sending event, usually because there are no subscribers: {:?}",
                                e
                            );
                            e
                        });
                },
            }
        }

        Ok(self.id)
    }

    async fn handle_mempool_response(
        &mut self,
        response: MempoolServiceResponse,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        if response.request_key != self.id {
            trace!(
                target: LOG_TARGET,
                "Mempool response key does not match this Broadcast Protocol Id"
            );
            return Ok(false);
        }

        // Handle a receive Mempool Response
        match response.response {
            MempoolResponse::Stats(_) => {
                warn!(target: LOG_TARGET, "Invalid Mempool response variant");
            },
            MempoolResponse::State(_) => {
                warn!(target: LOG_TARGET, "Invalid Mempool response variant");
            },
            MempoolResponse::TxStorage(ts) => {
                let completed_tx = match self.resources.db.get_completed_transaction(response.request_key).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "Cannot find Completed Transaction (TxId: {}) referred to by this Broadcast protocol: {:?}",
                            self.id,
                            e
                        );
                        return Err(TransactionServiceProtocolError::new(
                            self.id,
                            TransactionServiceError::TransactionDoesNotExistError,
                        ));
                    },
                };

                #[allow(clippy::single_match)]
                match completed_tx.status {
                    TransactionStatus::Completed => match ts {
                        // Getting this response means the Mempool Rejected this transaction so it will be
                        // cancelled.
                        TxStorageResponse::NotStored => {
                            error!(
                                target: LOG_TARGET,
                                "Mempool response received for TxId: {:?}. Transaction was Rejected. Cancelling \
                                 transaction.",
                                self.id
                            );
                            if let Err(e) = self
                                .resources
                                .output_manager_service
                                .cancel_transaction(completed_tx.tx_id)
                                .await
                            {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel outputs for TX_ID: {} after failed sending attempt with error \
                                     {:?}",
                                    completed_tx.tx_id,
                                    e
                                );
                            }
                            if let Err(e) = self.resources.db.cancel_completed_transaction(completed_tx.tx_id).await {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to Cancel TX_ID: {} after failed sending attempt with error {:?}",
                                    completed_tx.tx_id,
                                    e
                                );
                            }
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionCancelled(self.id)))
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
                                "Completed Transaction (TxId: {} and Kernel Excess Sig: {}) detected as Broadcast to \
                                 Base Node Mempool in {:?}",
                                self.id,
                                completed_tx.transaction.body.kernels()[0]
                                    .excess_sig
                                    .get_signature()
                                    .to_hex(),
                                ts
                            );
                            debug!(
                                target: LOG_TARGET_STRESS,
                                "Completed Transaction (TxId: {} and Kernel Excess Sig: {}) detected as Broadcast to \
                                 Base Node Mempool in {:?}",
                                self.id,
                                completed_tx.transaction.body.kernels()[0]
                                    .excess_sig
                                    .get_signature()
                                    .to_hex(),
                                ts
                            );

                            self.resources
                                .db
                                .broadcast_completed_transaction(self.id)
                                .await
                                .map_err(|e| {
                                    TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e))
                                })?;
                            let _ = self
                                .resources
                                .event_publisher
                                .send(Arc::new(TransactionEvent::TransactionBroadcast(self.id)))
                                .map_err(|e| {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error sending event, usually because there are no subscribers: {:?}",
                                        e
                                    );
                                    e
                                });
                            return Ok(true);
                        },
                    },
                    _ => (),
                }
            },
        }

        Ok(false)
    }

    async fn handle_base_node_response(
        &mut self,
        response: BaseNodeProto::BaseNodeServiceResponse,
    ) -> Result<bool, TransactionServiceProtocolError>
    {
        if response.request_key != self.id {
            trace!(
                target: LOG_TARGET,
                "Base Node response key does not match this Broadcast Protocol Id"
            );
            return Ok(false);
        }

        let response: Vec<tari_core::transactions::proto::types::TransactionOutput> = match response.response {
            Some(BaseNodeResponseProto::TransactionOutputs(outputs)) => outputs.outputs,
            _ => {
                return Ok(false);
            },
        };

        let completed_tx = match self.resources.db.get_completed_transaction(self.id).await {
            Ok(tx) => tx,
            Err(_) => {
                error!(
                    target: LOG_TARGET,
                    "Cannot find Completed Transaction (TxId: {}) referred to by this Broadcast protocol", self.id
                );
                return Err(TransactionServiceProtocolError::new(
                    self.id,
                    TransactionServiceError::TransactionDoesNotExistError,
                ));
            },
        };

        if !response.is_empty() &&
            (completed_tx.status == TransactionStatus::Broadcast ||
                completed_tx.status == TransactionStatus::Completed)
        {
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
                        self.id,
                        completed_tx.transaction.body.inputs().clone(),
                        completed_tx.transaction.body.outputs().clone(),
                    )
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                self.resources
                    .db
                    .mine_completed_transaction(self.id)
                    .await
                    .map_err(|e| TransactionServiceProtocolError::new(self.id, TransactionServiceError::from(e)))?;

                let _ = self
                    .resources
                    .event_publisher
                    .send(Arc::new(TransactionEvent::TransactionMined(self.id)))
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
                    "Transaction (TxId: {:?}) detected as mined on the Base Layer", self.id
                );

                return Ok(true);
            }
        }

        Ok(false)
    }
}

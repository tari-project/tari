// Copyright 2019 The Tari Project
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

use std::{convert::TryFrom, sync::Arc};

use futures::{pin_mut, stream::StreamExt, Stream};
use log::*;
use tari_comms::peer_manager::NodeId;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester},
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::{reply_channel, reply_channel::RequestContext};
use tokio::{sync::mpsc, task};

use crate::{
    base_node::{
        comms_interface::{BlockEvent, BlockEventReceiver},
        StateMachineHandle,
    },
    mempool::service::{
        error::MempoolServiceError,
        inbound_handlers::MempoolInboundHandlers,
        MempoolRequest,
        MempoolResponse,
    },
    proto,
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::mempool::service::service";

/// A convenience struct to hold all the Mempool service streams
pub struct MempoolStreams<STxIn, SLocalReq> {
    pub outbound_tx_stream: mpsc::UnboundedReceiver<(Arc<Transaction>, Vec<NodeId>)>,
    pub inbound_transaction_stream: STxIn,
    pub local_request_stream: SLocalReq,
    pub block_event_stream: BlockEventReceiver,
    pub request_receiver: reply_channel::TryReceiver<MempoolRequest, MempoolResponse, MempoolServiceError>,
}

/// The Mempool Service is responsible for handling inbound requests and responses and for sending new requests to the
/// Mempools of remote Base nodes.
pub struct MempoolService {
    outbound_message_service: OutboundMessageRequester,
    inbound_handlers: MempoolInboundHandlers,
    state_machine: StateMachineHandle,
}

impl MempoolService {
    pub fn new(
        outbound_message_service: OutboundMessageRequester,
        inbound_handlers: MempoolInboundHandlers,
        state_machine: StateMachineHandle,
    ) -> Self {
        Self {
            outbound_message_service,
            inbound_handlers,
            state_machine,
        }
    }

    pub async fn start<STxIn, SLocalReq>(
        mut self,
        streams: MempoolStreams<STxIn, SLocalReq>,
    ) -> Result<(), MempoolServiceError>
    where
        STxIn: Stream<Item = DomainMessage<Transaction>>,
        SLocalReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
    {
        let mut outbound_tx_stream = streams.outbound_tx_stream;
        let inbound_transaction_stream = streams.inbound_transaction_stream.fuse();
        pin_mut!(inbound_transaction_stream);
        let local_request_stream = streams.local_request_stream.fuse();
        pin_mut!(local_request_stream);
        let mut block_event_stream = streams.block_event_stream;
        let mut request_receiver = streams.request_receiver;

        loop {
            tokio::select! {
                // Requests sent from the handle
                Some(request) = request_receiver.next() => {
                    let (request, reply) = request.split();
                    let _ = reply.send(self.handle_request(request).await);
                },

                // Outbound tx messages from the OutboundMempoolServiceInterface
                Some((txn, excluded_peers)) = outbound_tx_stream.recv() => {
                    self.spawn_handle_outbound_tx(txn, excluded_peers);
                },

                // Incoming transaction messages from the Comms layer
                Some(transaction_msg) = inbound_transaction_stream.next() => {
                    self.spawn_handle_incoming_tx(transaction_msg);
                }

                // Incoming local request messages from the LocalMempoolServiceInterface and other local services
                Some(local_request_context) = local_request_stream.next() => {
                    self.spawn_handle_local_request(local_request_context);
                },

                // Block events from local Base Node.
                block_event = block_event_stream.recv() => {
                    if let Ok(block_event) = block_event {
                        self.spawn_handle_block_event(block_event);
                    }
                },


                else => {
                    info!(target: LOG_TARGET, "Mempool service shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(&mut self, request: MempoolRequest) -> Result<MempoolResponse, MempoolServiceError> {
        // TODO: Move db calls into MempoolService
        self.inbound_handlers.handle_request(request).await
    }

    fn spawn_handle_outbound_tx(&self, tx: Arc<Transaction>, excluded_peers: Vec<NodeId>) {
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let result = handle_outbound_tx(outbound_message_service, tx, excluded_peers).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound tx message {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_tx(&self, tx_msg: DomainMessage<Transaction>) {
        // Determine if we are bootstrapped
        let status_watch = self.state_machine.get_status_info_watch();

        if !(*status_watch.borrow()).bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Transaction with Message {} from peer `{}` not processed while busy with initial sync.",
                tx_msg.dht_header.message_tag,
                tx_msg.source_peer.node_id.short_str(),
            );
            return;
        }
        let inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let result = handle_incoming_tx(inbound_handlers, tx_msg).await;
            if let Err(e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming transaction message: {:?}", e
                );
            }
        });
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    ) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let result = reply_tx.send(inbound_handlers.handle_request(request).await);

            if let Err(e) = result {
                error!(
                    target: LOG_TARGET,
                    "MempoolService failed to send reply to local request {:?}", e
                );
            }
        });
    }

    fn spawn_handle_block_event(&self, block_event: Arc<BlockEvent>) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let result = inbound_handlers.handle_block_event(&block_event).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle base node block event: {:?}", e);
            }
        });
    }
}

async fn handle_incoming_tx(
    mut inbound_handlers: MempoolInboundHandlers,
    domain_transaction_msg: DomainMessage<Transaction>,
) -> Result<(), MempoolServiceError> {
    let DomainMessage::<_> { source_peer, inner, .. } = domain_transaction_msg;

    debug!(
        "New transaction received: {}, from: {}",
        inner
            .first_kernel_excess_sig()
            .map(|s| s.get_signature().to_hex())
            .unwrap_or_else(|| "No kernels!".to_string()),
        source_peer.public_key,
    );
    trace!(
        target: LOG_TARGET,
        "New transaction: {}, from: {}",
        inner,
        source_peer.public_key
    );
    inbound_handlers
        .handle_transaction(inner, Some(source_peer.node_id))
        .await?;

    Ok(())
}

async fn handle_outbound_tx(
    mut outbound_message_service: OutboundMessageRequester,
    tx: Arc<Transaction>,
    exclude_peers: Vec<NodeId>,
) -> Result<(), MempoolServiceError> {
    let result = outbound_message_service
        .flood(
            NodeDestination::Unknown,
            OutboundEncryption::ClearText,
            exclude_peers,
            OutboundDomainMessage::new(
                TariMessageType::NewTransaction,
                proto::types::Transaction::try_from(tx).map_err(MempoolServiceError::ConversionError)?,
            ),
        )
        .await;

    if let Err(e) = result {
        return match e {
            DhtOutboundError::NoMessagesQueued => Ok(()),
            _ => {
                error!(target: LOG_TARGET, "Handle outbound tx failure. {:?}", e);
                Err(MempoolServiceError::OutboundMessageService(e.to_string()))
            },
        };
    }

    Ok(())
}

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

use std::{convert::TryFrom, io, sync::Arc};

use futures::{pin_mut, stream::StreamExt, Stream};
use log::*;
use tari_network::{GossipMessage, GossipSubscription};
use tari_p2p::proto;
use tari_service_framework::{reply_channel, reply_channel::RequestContext};
use tari_utilities::hex::Hex;
use tokio::task;

use crate::{
    base_node::comms_interface::{BlockEvent, BlockEventReceiver},
    mempool::service::{
        error::MempoolServiceError,
        inbound_handlers::MempoolInboundHandlers,
        MempoolRequest,
        MempoolResponse,
    },
    transactions::transaction_components::Transaction,
};

const LOG_TARGET: &str = "c::mempool::service::service";

/// A convenience struct to hold all the Mempool service streams
pub struct MempoolStreams<SLocalReq> {
    pub transaction_subscription: GossipSubscription<proto::common::Transaction>,
    pub local_request_stream: SLocalReq,
    pub block_event_stream: BlockEventReceiver,
    pub request_receiver: reply_channel::TryReceiver<MempoolRequest, MempoolResponse, MempoolServiceError>,
}

/// The Mempool Service is responsible for handling inbound requests and responses and for sending new requests to the
/// Mempools of remote Base nodes.
pub struct MempoolService {
    inbound_handlers: MempoolInboundHandlers,
}

impl MempoolService {
    pub fn new(inbound_handlers: MempoolInboundHandlers) -> Self {
        Self { inbound_handlers }
    }

    pub async fn start<SLocalReq>(mut self, streams: MempoolStreams<SLocalReq>) -> Result<(), MempoolServiceError>
    where SLocalReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>> {
        let local_request_stream = streams.local_request_stream.fuse();
        pin_mut!(local_request_stream);
        let mut block_event_stream = streams.block_event_stream;
        let mut request_receiver = streams.request_receiver;
        let mut transaction_subscription = streams.transaction_subscription;

        loop {
            tokio::select! {
                // Requests sent from the handle
                Some(request) = request_receiver.next() => {
                    let (request, reply) = request.split();
                    let _result = reply.send(self.handle_request(request).await);
                },


                // Incoming transaction messages from the Comms layer
                Some(transaction_msg) = transaction_subscription.next_message() => self.handle_incoming_tx(transaction_msg),

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
        self.inbound_handlers.handle_request(request).await
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    ) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let result = reply_tx.send(inbound_handlers.handle_request(request).await);

            if let Err(res) = result {
                error!(
                    target: LOG_TARGET,
                    "MempoolService failed to send reply to local request {:?}",
                    res.map(|r| r.to_string()).map_err(|e| e.to_string())
                );
            }
        });
    }

    fn spawn_handle_block_event(&self, block_event: Arc<BlockEvent>) {
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let result = inbound_handlers.handle_block_event(&block_event).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle base node block event: {}", e);
            }
        });
    }

    fn handle_incoming_tx(&self, result: io::Result<GossipMessage<proto::common::Transaction>>) {
        let msg = match result {
            Ok(msg) => msg,
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to decode gossip message: {err}");
                return;
            },
        };

        let source_peer_id = msg.source;
        let transaction = match Transaction::try_from(msg.message) {
            Ok(tx) => tx,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Received transaction message from {} with invalid transaction: {:?}", source_peer_id, e
                );
                return;
            },
        };
        debug!(
            "New transaction received: {}, from: {}",
            transaction
                .first_kernel_excess_sig()
                .map(|s| s.get_signature().to_hex())
                .unwrap_or_else(|| "No kernels!".to_string()),
            source_peer_id,
        );
        trace!(
            target: LOG_TARGET,
            "New transaction: {}, from: {}",
            transaction,
            source_peer_id,
        );
        let mut inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            if let Err(e) = inbound_handlers.handle_transaction(transaction, source_peer_id).await {
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming transaction message: {:?}", e
                );
            }
        });
    }
}

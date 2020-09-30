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

use crate::{
    base_node::{
        comms_interface::{BlockEvent, BlockEventReceiver},
        generate_request_key,
        RequestKey,
        StateMachineHandle,
        WaitingRequests,
    },
    mempool::{
        proto,
        service::{
            error::MempoolServiceError,
            inbound_handlers::MempoolInboundHandlers,
            MempoolRequest,
            MempoolResponse,
        },
        MempoolServiceConfig,
    },
    transactions::{proto::types::Transaction as ProtoTransaction, transaction::Transaction},
};
use futures::{
    channel::{
        mpsc::{channel, Receiver, Sender, UnboundedReceiver},
        oneshot::Sender as OneshotSender,
    },
    pin_mut,
    stream::StreamExt,
    SinkExt,
    Stream,
};
use log::*;
use rand::rngs::OsRng;
use std::{convert::TryInto, sync::Arc, time::Duration};
use tari_comms::peer_manager::NodeId;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester},
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::RequestContext;
use tokio::task;

const LOG_TARGET: &str = "c::mempool::service::service";

/// A convenience struct to hold all the Mempool service streams
pub struct MempoolStreams<SOutReq, SInReq, SInRes, STxIn, SLocalReq> {
    outbound_request_stream: SOutReq,
    outbound_tx_stream: UnboundedReceiver<(Transaction, Vec<NodeId>)>,
    inbound_request_stream: SInReq,
    inbound_response_stream: SInRes,
    inbound_transaction_stream: STxIn,
    local_request_stream: SLocalReq,
    block_event_stream: BlockEventReceiver,
}

impl<SOutReq, SInReq, SInRes, STxIn, SLocalReq> MempoolStreams<SOutReq, SInReq, SInRes, STxIn, SLocalReq> {
    pub fn new(
        outbound_request_stream: SOutReq,
        outbound_tx_stream: UnboundedReceiver<(Transaction, Vec<NodeId>)>,
        inbound_request_stream: SInReq,
        inbound_response_stream: SInRes,
        inbound_transaction_stream: STxIn,
        local_request_stream: SLocalReq,
        block_event_stream: BlockEventReceiver,
    ) -> Self
    {
        Self {
            outbound_request_stream,
            outbound_tx_stream,
            inbound_request_stream,
            inbound_response_stream,
            inbound_transaction_stream,
            local_request_stream,
            block_event_stream,
        }
    }
}

/// The Mempool Service is responsible for handling inbound requests and responses and for sending new requests to the
/// Mempools of remote Base nodes.
pub struct MempoolService {
    outbound_message_service: OutboundMessageRequester,
    inbound_handlers: MempoolInboundHandlers,
    waiting_requests: WaitingRequests<Result<MempoolResponse, MempoolServiceError>>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: MempoolServiceConfig,
    state_machine: StateMachineHandle,
}

impl MempoolService {
    pub fn new(
        outbound_message_service: OutboundMessageRequester,
        inbound_handlers: MempoolInboundHandlers,
        config: MempoolServiceConfig,
        state_machine: StateMachineHandle,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            outbound_message_service,
            inbound_handlers,
            waiting_requests: WaitingRequests::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
            state_machine,
        }
    }

    pub async fn start<SOutReq, SInReq, SInRes, STxIn, SLocalReq>(
        mut self,
        streams: MempoolStreams<SOutReq, SInReq, SInRes, STxIn, SLocalReq>,
    ) -> Result<(), MempoolServiceError>
    where
        SOutReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
        SInReq: Stream<Item = DomainMessage<proto::MempoolServiceRequest>>,
        SInRes: Stream<Item = DomainMessage<proto::MempoolServiceResponse>>,
        STxIn: Stream<Item = DomainMessage<Transaction>>,
        SLocalReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
    {
        let outbound_request_stream = streams.outbound_request_stream.fuse();
        pin_mut!(outbound_request_stream);
        let outbound_tx_stream = streams.outbound_tx_stream.fuse();
        pin_mut!(outbound_tx_stream);
        let inbound_request_stream = streams.inbound_request_stream.fuse();
        pin_mut!(inbound_request_stream);
        let inbound_response_stream = streams.inbound_response_stream.fuse();
        pin_mut!(inbound_response_stream);
        let inbound_transaction_stream = streams.inbound_transaction_stream.fuse();
        pin_mut!(inbound_transaction_stream);
        let local_request_stream = streams.local_request_stream.fuse();
        pin_mut!(local_request_stream);
        let block_event_stream = streams.block_event_stream.fuse();
        pin_mut!(block_event_stream);
        let timeout_receiver_stream = self
            .timeout_receiver_stream
            .take()
            .expect("Mempool Service initialized without timeout_receiver_stream")
            .fuse();
        pin_mut!(timeout_receiver_stream);

        loop {
            futures::select! {
                // Outbound request messages from the OutboundMempoolServiceInterface
                outbound_request_context = outbound_request_stream.select_next_some() => {
                    self.spawn_handle_outbound_request(outbound_request_context);
                },

                // Outbound tx messages from the OutboundMempoolServiceInterface
                (txn, excluded_peers) = outbound_tx_stream.select_next_some() => {
                    self.spawn_handle_outbound_tx(txn, excluded_peers);
                },

                // Incoming request messages from the Comms layer
                domain_msg = inbound_request_stream.select_next_some() => {
                    self.spawn_handle_incoming_request(domain_msg);
                },

                // Incoming response messages from the Comms layer
                domain_msg = inbound_response_stream.select_next_some() => {
                    self.spawn_handle_incoming_response(domain_msg);
                },

                // Incoming transaction messages from the Comms layer
                transaction_msg = inbound_transaction_stream.select_next_some() => {
                    self.spawn_handle_incoming_tx(transaction_msg).await;
                }

                // Incoming local request messages from the LocalMempoolServiceInterface and other local services
                local_request_context = local_request_stream.select_next_some() => {
                    self.spawn_handle_local_request(local_request_context);
                },

                // Block events from local Base Node.
                block_event = block_event_stream.select_next_some() => {
                    if let Ok(block_event) = block_event {
                        self.spawn_handle_block_event(block_event);
                    }
                },

                // Timeout events for waiting requests
                timeout_request_key = timeout_receiver_stream.select_next_some() => {
                    self.spawn_handle_request_timeout(timeout_request_key);
                },

                complete => {
                    info!(target: LOG_TARGET, "Mempool service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    fn spawn_handle_outbound_request(
        &self,
        request_context: RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>,
    )
    {
        let outbound_message_service = self.outbound_message_service.clone();
        let waiting_requests = self.waiting_requests.clone();
        let timeout_sender = self.timeout_sender.clone();
        let config = self.config;
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let result = handle_outbound_request(
                outbound_message_service,
                waiting_requests,
                timeout_sender,
                reply_tx,
                request,
                config,
            )
            .await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_outbound_tx(&self, tx: Transaction, excluded_peers: Vec<NodeId>) {
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let result = handle_outbound_tx(outbound_message_service, tx, excluded_peers).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound tx message {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_request(&self, domain_msg: DomainMessage<proto::mempool::MempoolServiceRequest>) {
        let inbound_handlers = self.inbound_handlers.clone();
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let result = handle_incoming_request(inbound_handlers, outbound_message_service, domain_msg).await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle incoming request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_response(&self, domain_msg: DomainMessage<proto::mempool::MempoolServiceResponse>) {
        let waiting_requests = self.waiting_requests.clone();
        task::spawn(async move {
            let result = handle_incoming_response(waiting_requests, domain_msg.into_inner()).await;

            if let Err(e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming response message: {:?}", e
                );
            }
        });
    }

    async fn spawn_handle_incoming_tx(&self, tx_msg: DomainMessage<Transaction>) {
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
    )
    {
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
        let inbound_handlers = self.inbound_handlers.clone();
        task::spawn(async move {
            let result = handle_block_event(inbound_handlers, &block_event).await;
            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle base node block event: {:?}", e);
            }
        });
    }

    fn spawn_handle_request_timeout(&self, timeout_request_key: u64) {
        let waiting_requests = self.waiting_requests.clone();
        task::spawn(async move {
            let result = handle_request_timeout(waiting_requests, timeout_request_key).await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle request timeout event: {:?}", e);
            }
        });
    }
}

async fn handle_incoming_request(
    mut inbound_handlers: MempoolInboundHandlers,
    mut outbound_message_service: OutboundMessageRequester,
    domain_request_msg: DomainMessage<proto::MempoolServiceRequest>,
) -> Result<(), MempoolServiceError>
{
    let (origin_public_key, inner_msg) = domain_request_msg.into_origin_and_inner();

    // Convert proto::MempoolServiceRequest to a MempoolServiceRequest
    let request = inner_msg
        .request
        .ok_or_else(|| MempoolServiceError::InvalidRequest("Received invalid mempool service request".to_string()))?;

    let response = inbound_handlers
        .handle_request(request.try_into().map_err(MempoolServiceError::InvalidRequest)?)
        .await?;

    let message = proto::MempoolServiceResponse {
        request_key: inner_msg.request_key,
        response: Some(response.into()),
    };

    outbound_message_service
        .send_direct(
            origin_public_key,
            OutboundDomainMessage::new(TariMessageType::MempoolResponse, message),
        )
        .await?;

    Ok(())
}

async fn handle_incoming_response(
    waiting_requests: WaitingRequests<Result<MempoolResponse, MempoolServiceError>>,
    incoming_response: proto::MempoolServiceResponse,
) -> Result<(), MempoolServiceError>
{
    let proto::MempoolServiceResponse { request_key, response } = incoming_response;
    let response: MempoolResponse = response
        .and_then(|r| r.try_into().ok())
        .ok_or_else(|| MempoolServiceError::InvalidResponse("Received an invalid mempool response".to_string()))?;

    if let Some((reply_tx, started)) = waiting_requests.remove(request_key).await {
        trace!(
            target: LOG_TARGET,
            "Response for {} (request key: {}) received after {}ms",
            response,
            &request_key,
            started.elapsed().as_millis()
        );
        let _ = reply_tx.send(Ok(response).map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Failed to finalize request (request key:{}): {:?}", &request_key, e
            );
            e
        }));
    }

    Ok(())
}

async fn handle_outbound_request(
    mut outbound_message_service: OutboundMessageRequester,
    waiting_requests: WaitingRequests<Result<MempoolResponse, MempoolServiceError>>,
    timeout_sender: Sender<RequestKey>,
    reply_tx: OneshotSender<Result<MempoolResponse, MempoolServiceError>>,
    request: MempoolRequest,
    config: MempoolServiceConfig,
) -> Result<(), MempoolServiceError>
{
    let request_key = generate_request_key(&mut OsRng);
    let service_request = proto::MempoolServiceRequest {
        request_key,
        request: Some(request.into()),
    };

    let send_result = outbound_message_service
        .send_random(
            1,
            NodeDestination::Unknown,
            OutboundEncryption::None,
            OutboundDomainMessage::new(TariMessageType::MempoolRequest, service_request),
        )
        .await;

    match send_result {
        Ok(_) => {
            // Spawn timeout and wait for matching response to arrive
            waiting_requests.insert(request_key, reply_tx).await;
            // Spawn timeout for waiting_request
            spawn_request_timeout(timeout_sender, request_key, config.request_timeout);
            Ok(())
        },
        Err(DhtOutboundError::NoMessagesQueued) => {
            let _ = reply_tx.send(Err(MempoolServiceError::NoBootstrapNodesConfigured).map_err(|e| {
                error!(
                    target: LOG_TARGET,
                    "Failed to send outbound request as no bootstrap nodes were configured"
                );
                e
            }));

            Ok(())
        },
        Err(e) => {
            error!(target: LOG_TARGET, "mempool outbound request failure. {:?}", e);
            Err(MempoolServiceError::OutboundMessageService(e.to_string()))
        },
    }
}

async fn handle_incoming_tx(
    mut inbound_handlers: MempoolInboundHandlers,
    domain_transaction_msg: DomainMessage<Transaction>,
) -> Result<(), MempoolServiceError>
{
    let DomainMessage::<_> { source_peer, inner, .. } = domain_transaction_msg;

    debug!(
        "New transaction received: {}, from: {}",
        inner.body.kernels()[0].excess_sig.get_signature().to_hex(),
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

async fn handle_request_timeout(
    waiting_requests: WaitingRequests<Result<MempoolResponse, MempoolServiceError>>,
    request_key: RequestKey,
) -> Result<(), MempoolServiceError>
{
    if let Some((reply_tx, started)) = waiting_requests.remove(request_key).await {
        warn!(
            target: LOG_TARGET,
            "Request (request key {}) timed out after {}ms",
            &request_key,
            started.elapsed().as_millis()
        );
        let reply_msg = Err(MempoolServiceError::RequestTimedOut);

        let _ = reply_tx.send(reply_msg.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Failed to send outbound request (request key: {}): {:?}", &request_key, e
            );
            e
        }));
    }

    Ok(())
}

async fn handle_outbound_tx(
    mut outbound_message_service: OutboundMessageRequester,
    tx: Transaction,
    exclude_peers: Vec<NodeId>,
) -> Result<(), MempoolServiceError>
{
    let result = outbound_message_service
        .propagate(
            NodeDestination::Unknown,
            OutboundEncryption::None,
            exclude_peers,
            OutboundDomainMessage::new(TariMessageType::NewTransaction, ProtoTransaction::from(tx)),
        )
        .await;

    if let Err(e) = result {
        error!(target: LOG_TARGET, "Handle outbound tx failure. {:?}", e);
        return Err(MempoolServiceError::OutboundMessageService(e.to_string()));
    }

    Ok(())
}

async fn handle_block_event(
    mut inbound_handlers: MempoolInboundHandlers,
    block_event: &BlockEvent,
) -> Result<(), MempoolServiceError>
{
    inbound_handlers.handle_block_event(block_event).await?;
    Ok(())
}

fn spawn_request_timeout(mut timeout_sender: Sender<RequestKey>, request_key: RequestKey, timeout: Duration) {
    task::spawn(async move {
        tokio::time::delay_for(timeout).await;
        let _ = timeout_sender.send(request_key).await;
    });
}

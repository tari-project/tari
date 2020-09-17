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
        comms_interface::{
            Broadcast,
            CommsInterfaceError,
            InboundNodeCommsHandlers,
            NodeCommsRequest,
            NodeCommsResponse,
        },
        consts::{BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION, BASE_NODE_SERVICE_REQUEST_TIMEOUT},
        generate_request_key,
        proto,
        service::error::BaseNodeServiceError,
        state_machine_service::states::StateInfo,
        RequestKey,
        StateMachineHandle,
        WaitingRequests,
    },
    blocks::{Block, NewBlock},
    chain_storage::BlockchainBackend,
    proto as shared_protos,
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
use std::{convert::TryInto, time::Duration};
use tari_comms::peer_manager::NodeId;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageParams},
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::RequestContext;
use tokio::task;

const LOG_TARGET: &str = "c::bn::base_node_service::service";

/// Configuration for the BaseNodeService.
#[derive(Clone, Copy)]
pub struct BaseNodeServiceConfig {
    /// The allocated waiting time for a request waiting for service responses from remote base nodes.
    pub request_timeout: Duration,
    /// The fraction of responses that need to be received for a corresponding service request to be finalize.
    pub desired_response_fraction: f32,
}

impl Default for BaseNodeServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout: BASE_NODE_SERVICE_REQUEST_TIMEOUT,
            desired_response_fraction: BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION,
        }
    }
}

/// A convenience struct to hold all the BaseNode streams
pub struct BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock> {
    /// `NodeCommsRequest` messages to send to a remote peer. If a specific peer is not provided, a random peer is
    /// chosen.
    outbound_request_stream: SOutReq,
    /// Blocks to be propagated out to the network. The second element of the tuple is a list of peers to exclude from
    /// this round of propagation
    outbound_block_stream: UnboundedReceiver<(NewBlock, Vec<NodeId>)>,
    /// `BaseNodeRequest` messages received from external peers
    inbound_request_stream: SInReq,
    /// `BaseNodeResponse` messages received from external peers
    inbound_response_stream: SInRes,
    /// `NewBlock` messages received from external peers
    inbound_block_stream: SBlockIn,
    /// Incoming local request messages from the LocalNodeCommsInterface and other local services
    local_request_stream: SLocalReq,
    /// The stream of blocks sent from local services `LocalCommsNodeInterface::submit_block` e.g. block sync and
    /// miner
    local_block_stream: SLocalBlock,
}

impl<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>
    BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>
where
    SOutReq: Stream<
        Item = RequestContext<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
    >,
    SInReq: Stream<Item = DomainMessage<proto::BaseNodeServiceRequest>>,
    SInRes: Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>>,
    SBlockIn: Stream<Item = DomainMessage<NewBlock>>,
    SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
    SLocalBlock: Stream<Item = RequestContext<(Block, Broadcast), Result<(), CommsInterfaceError>>>,
{
    pub fn new(
        outbound_request_stream: SOutReq,
        outbound_block_stream: UnboundedReceiver<(NewBlock, Vec<NodeId>)>,
        inbound_request_stream: SInReq,
        inbound_response_stream: SInRes,
        inbound_block_stream: SBlockIn,
        local_request_stream: SLocalReq,
        local_block_stream: SLocalBlock,
    ) -> Self
    {
        Self {
            outbound_request_stream,
            outbound_block_stream,
            inbound_request_stream,
            inbound_response_stream,
            inbound_block_stream,
            local_request_stream,
            local_block_stream,
        }
    }
}

/// The Base Node Service is responsible for handling inbound requests and responses and for sending new requests to
/// remote Base Node Services.
pub struct BaseNodeService<B> {
    outbound_message_service: OutboundMessageRequester,
    inbound_nch: InboundNodeCommsHandlers<B>,
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: BaseNodeServiceConfig,
    state_machine_handle: StateMachineHandle,
}

impl<B> BaseNodeService<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        outbound_message_service: OutboundMessageRequester,
        inbound_nch: InboundNodeCommsHandlers<B>,
        config: BaseNodeServiceConfig,
        state_machine_handle: StateMachineHandle,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            outbound_message_service,
            inbound_nch,
            waiting_requests: WaitingRequests::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
            state_machine_handle,
        }
    }

    pub async fn start<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>(
        mut self,
        streams: BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>,
    ) -> Result<(), BaseNodeServiceError>
    where
        SOutReq: Stream<
            Item = RequestContext<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>,
        >,
        SInReq: Stream<Item = DomainMessage<proto::BaseNodeServiceRequest>>,
        SInRes: Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>>,
        SBlockIn: Stream<Item = DomainMessage<NewBlock>>,
        SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
        SLocalBlock: Stream<Item = RequestContext<(Block, Broadcast), Result<(), CommsInterfaceError>>>,
    {
        let outbound_request_stream = streams.outbound_request_stream.fuse();
        pin_mut!(outbound_request_stream);
        let outbound_block_stream = streams.outbound_block_stream.fuse();
        pin_mut!(outbound_block_stream);
        let inbound_request_stream = streams.inbound_request_stream.fuse();
        pin_mut!(inbound_request_stream);
        let inbound_response_stream = streams.inbound_response_stream.fuse();
        pin_mut!(inbound_response_stream);
        let inbound_block_stream = streams.inbound_block_stream.fuse();
        pin_mut!(inbound_block_stream);
        let local_request_stream = streams.local_request_stream.fuse();
        pin_mut!(local_request_stream);
        let local_block_stream = streams.local_block_stream.fuse();
        pin_mut!(local_block_stream);
        let timeout_receiver_stream = self
            .timeout_receiver_stream
            .take()
            .expect("Base Node Service initialized without timeout_receiver_stream")
            .fuse();
        pin_mut!(timeout_receiver_stream);
        loop {
            futures::select! {
                // Outbound request messages from the OutboundNodeCommsInterface
                outbound_request_context = outbound_request_stream.select_next_some() => {
                    self.spawn_handle_outbound_request(outbound_request_context);
                },

                // Outbound block messages from the OutboundNodeCommsInterface
                (block, excluded_peers) = outbound_block_stream.select_next_some() => {
                    self.spawn_handle_outbound_block(block, excluded_peers);
                },

                // Incoming request messages from the Comms layer
                domain_msg = inbound_request_stream.select_next_some() => {
                    self.spawn_handle_incoming_request(domain_msg);
                },

                // Incoming response messages from the Comms layer
                domain_msg = inbound_response_stream.select_next_some() => {
                    self.spawn_handle_incoming_response(domain_msg);
                },

                // Timeout events for waiting requests
                timeout_request_key = timeout_receiver_stream.select_next_some() => {
                    self.spawn_handle_request_timeout(timeout_request_key);
                },

                // Incoming block messages from the Comms layer
                block_msg = inbound_block_stream.select_next_some() => {
                    self.spawn_handle_incoming_block(block_msg).await;
                }

                // Incoming local request messages from the LocalNodeCommsInterface and other local services
                local_request_context = local_request_stream.select_next_some() => {
                    self.spawn_handle_local_request(local_request_context);
                },

                // Incoming local block messages from the LocalNodeCommsInterface e.g. miner and block sync
                local_block_context = local_block_stream.select_next_some() => {
                    self.spawn_handle_local_block(local_block_context);
                },

                complete => {
                    info!(target: LOG_TARGET, "Base Node service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    fn spawn_handle_outbound_request(
        &self,
        request_context: RequestContext<
            (NodeCommsRequest, Option<NodeId>),
            Result<NodeCommsResponse, CommsInterfaceError>,
        >,
    )
    {
        let outbound_message_service = self.outbound_message_service.clone();
        let waiting_requests = self.waiting_requests.clone();
        let timeout_sender = self.timeout_sender.clone();
        let config = self.config;
        task::spawn(async move {
            let ((request, node_id), reply_tx) = request_context.split();
            let _ = handle_outbound_request(
                outbound_message_service,
                waiting_requests,
                timeout_sender,
                reply_tx,
                request,
                node_id,
                config,
            )
            .await
            .or_else(|err| {
                error!(
                    target: LOG_TARGET,
                    "Failed to handle outbound request message: {:?}", err
                );
                Err(err)
            });
        });
    }

    fn spawn_handle_outbound_block(&self, new_block: NewBlock, excluded_peers: Vec<NodeId>) {
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let _ = handle_outbound_block(outbound_message_service, new_block, excluded_peers)
                .await
                .or_else(|err| {
                    error!(target: LOG_TARGET, "Failed to handle outbound block message {:?}", err);
                    Err(err)
                });
        });
    }

    fn spawn_handle_incoming_request(&self, domain_msg: DomainMessage<proto::base_node::BaseNodeServiceRequest>) {
        let inbound_nch = self.inbound_nch.clone();
        let outbound_message_service = self.outbound_message_service.clone();
        let state_machine_handle = self.state_machine_handle.clone();
        task::spawn(async move {
            let _ = handle_incoming_request(inbound_nch, outbound_message_service, state_machine_handle, domain_msg)
                .await
                .or_else(|err| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to handle incoming request message: {:?}", err
                    );
                    Err(err)
                });
        });
    }

    fn spawn_handle_incoming_response(&self, domain_msg: DomainMessage<proto::base_node::BaseNodeServiceResponse>) {
        let waiting_requests = self.waiting_requests.clone();
        task::spawn(async move {
            let _ = handle_incoming_response(waiting_requests, domain_msg.into_inner())
                .await
                .or_else(|err| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to handle incoming response message: {:?}", err
                    );
                    Err(err)
                });
        });
    }

    fn spawn_handle_request_timeout(&self, timeout_request_key: u64) {
        let waiting_requests = self.waiting_requests.clone();
        task::spawn(async move {
            let _ = handle_request_timeout(waiting_requests, timeout_request_key)
                .await
                .or_else(|err| {
                    error!(target: LOG_TARGET, "Failed to handle request timeout event: {:?}", err);
                    Err(err)
                });
        });
    }

    async fn spawn_handle_incoming_block(&self, new_block: DomainMessage<NewBlock>) {
        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();

        if !(*status_watch.borrow()).bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Propagated block `{}` from peer `{}` not processed while busy with initial sync.",
                new_block.inner.block_hash.to_hex(),
                new_block.source_peer.node_id.short_str(),
            );
            return;
        }
        let inbound_nch = self.inbound_nch.clone();
        task::spawn(async move {
            let _ = handle_incoming_block(inbound_nch, new_block).await.or_else(|err| {
                error!(target: LOG_TARGET, "Failed to handle incoming block message: {:?}", err);
                Err(err)
            });
        });
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
    )
    {
        let inbound_nch = self.inbound_nch.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let _ = reply_tx
                .send(inbound_nch.handle_request(&request).await)
                .or_else(|err| {
                    error!(
                        target: LOG_TARGET,
                        "BaseNodeService failed to send reply to local request {:?}", err
                    );
                    Err(err)
                });
        });
    }

    fn spawn_handle_local_block(
        &self,
        block_context: RequestContext<(Block, Broadcast), Result<(), CommsInterfaceError>>,
    )
    {
        let inbound_nch = self.inbound_nch.clone();
        task::spawn(async move {
            let ((block, broadcast), reply_tx) = block_context.split();
            let _ = reply_tx
                .send(inbound_nch.handle_block(block, broadcast, None).await)
                .or_else(|err| {
                    error!(
                        target: LOG_TARGET,
                        "BaseNodeService failed to send reply to local block submitter {:?}", err
                    );
                    Err(err)
                });
        });
    }
}

async fn handle_incoming_request<B: BlockchainBackend + 'static>(
    inbound_nch: InboundNodeCommsHandlers<B>,
    mut outbound_message_service: OutboundMessageRequester,
    state_machine_handle: StateMachineHandle,
    domain_request_msg: DomainMessage<proto::BaseNodeServiceRequest>,
) -> Result<(), BaseNodeServiceError>
{
    let (origin_public_key, inner_msg) = domain_request_msg.into_origin_and_inner();

    // Convert proto::BaseNodeServiceRequest to a BaseNodeServiceRequest
    let request = inner_msg
        .request
        .ok_or_else(|| BaseNodeServiceError::InvalidRequest("Received invalid base node request".to_string()))?;

    let response = inbound_nch
        .handle_request(&request.try_into().map_err(BaseNodeServiceError::InvalidRequest)?)
        .await?;

    // Determine if we are synced
    let status_watch = state_machine_handle.get_status_info_watch();
    let is_synced = match (*status_watch.borrow()).state_info {
        StateInfo::Listening(li) => li.is_synced(),
        _ => false,
    };

    let message = proto::BaseNodeServiceResponse {
        request_key: inner_msg.request_key,
        response: Some(response.into()),
        is_synced,
    };

    trace!(
        target: LOG_TARGET,
        "Attempting outbound message in response to inbound request ({})",
        inner_msg.request_key
    );

    let send_message_response = outbound_message_service
        .send_direct(
            origin_public_key,
            OutboundDomainMessage::new(TariMessageType::BaseNodeResponse, message),
        )
        .await?;

    // Wait for the response to be sent and log the result
    let request_key = inner_msg.request_key;
    match send_message_response.resolve().await {
        Err(err) => {
            error!(
                target: LOG_TARGET,
                "Incoming request ({}) response failed to send: {}", request_key, err
            );
        },
        Ok(send_states) => {
            let msg_tag = send_states[0].tag;
            trace!(
                target: LOG_TARGET,
                "Incoming request ({}) response queued with {}",
                request_key,
                &msg_tag,
            );
            if send_states.wait_single().await {
                trace!(
                    target: LOG_TARGET,
                    "Incoming request ({}) response Direct Send was successful {}",
                    request_key,
                    msg_tag
                );
            } else {
                error!(
                    target: LOG_TARGET,
                    "Incoming request ({}) response Direct Send was unsuccessful and no message was sent {}",
                    request_key,
                    msg_tag
                );
            }
        },
    };

    Ok(())
}

async fn handle_incoming_response(
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    incoming_response: proto::BaseNodeServiceResponse,
) -> Result<(), BaseNodeServiceError>
{
    let proto::BaseNodeServiceResponse {
        request_key,
        response,
        is_synced,
    } = incoming_response;
    let response: NodeCommsResponse = response
        .and_then(|r| r.try_into().ok())
        .ok_or_else(|| BaseNodeServiceError::InvalidResponse("Received an invalid base node response".to_string()))?;

    if let Some((reply_tx, started)) = waiting_requests.remove(request_key).await {
        trace!(
            target: LOG_TARGET,
            "Response for {} (request key: {}) received after {}ms and is_synced: {}",
            response,
            &request_key,
            started.elapsed().as_millis(),
            is_synced
        );
        let _ = reply_tx.send(Ok(response).or_else(|resp| {
            warn!(
                target: LOG_TARGET,
                "Failed to finalize request (request key:{}): {:?}", &request_key, resp
            );
            Err(resp)
        }));
    }

    Ok(())
}

async fn handle_outbound_request(
    mut outbound_message_service: OutboundMessageRequester,
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: Sender<RequestKey>,
    reply_tx: OneshotSender<Result<NodeCommsResponse, CommsInterfaceError>>,
    request: NodeCommsRequest,
    node_id: Option<NodeId>,
    config: BaseNodeServiceConfig,
) -> Result<(), CommsInterfaceError>
{
    let request_key = generate_request_key(&mut OsRng);
    let service_request = proto::BaseNodeServiceRequest {
        request_key,
        request: Some(request.into()),
    };

    let mut send_msg_params = SendMessageParams::new();
    match node_id {
        Some(node_id) => send_msg_params.direct_node_id(node_id),
        None => send_msg_params.random(1),
    };

    trace!(target: LOG_TARGET, "Attempting outbound request ({})", request_key);
    let send_result = outbound_message_service
        .send_message(
            send_msg_params.finish(),
            OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
        )
        .await?;

    match send_result.resolve().await {
        Ok(send_states) if send_states.is_empty() => {
            let _ = reply_tx
                .send(Err(CommsInterfaceError::NoBootstrapNodesConfigured))
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send outbound request as no bootstrap nodes were configured"
                    );
                    Err(resp)
                });
        },
        Ok(send_states) => {
            // Wait for matching responses to arrive
            waiting_requests.insert(request_key, reply_tx).await;
            // Spawn timeout for waiting_request
            spawn_request_timeout(timeout_sender, request_key, config.request_timeout);
            // Log messages
            let msg_tag = send_states[0].tag;
            debug!(
                target: LOG_TARGET,
                "Outbound request ({}) response queued with {}", request_key, &msg_tag,
            );

            if send_states.wait_single().await {
                debug!(
                    target: LOG_TARGET,
                    "Outbound request ({}) response Direct Send was successful {}", request_key, msg_tag
                );
            } else {
                error!(
                    target: LOG_TARGET,
                    "Outbound request ({}) response Direct Send was unsuccessful and no message was sent", request_key
                );
            };
        },
        Err(err) => {
            debug!(target: LOG_TARGET, "Failed to send outbound request: {}", err);
            let _ = reply_tx
                .send(Err(CommsInterfaceError::BroadcastFailed))
                .or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send outbound request ({}) because DHT outbound broadcast failed", request_key
                    );
                    Err(resp)
                });
        },
    }
    Ok(())
}

async fn handle_outbound_block(
    mut outbound_message_service: OutboundMessageRequester,
    new_block: NewBlock,
    exclude_peers: Vec<NodeId>,
) -> Result<(), CommsInterfaceError>
{
    outbound_message_service
        .propagate(
            NodeDestination::Unknown,
            OutboundEncryption::None,
            exclude_peers,
            OutboundDomainMessage::new(
                TariMessageType::NewBlock,
                shared_protos::core::NewBlock::from(new_block),
            ),
        )
        .await?;
    Ok(())
}

async fn handle_request_timeout(
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    request_key: RequestKey,
) -> Result<(), CommsInterfaceError>
{
    if let Some((reply_tx, started)) = waiting_requests.remove(request_key).await {
        warn!(
            target: LOG_TARGET,
            "Request (request key {}) timed out after {}ms",
            &request_key,
            started.elapsed().as_millis()
        );
        let reply_msg = Err(CommsInterfaceError::RequestTimedOut);
        let _ = reply_tx.send(reply_msg.or_else(|resp| {
            error!(
                target: LOG_TARGET,
                "Failed to send outbound request (request key: {}): {:?}", &request_key, resp
            );
            Err(resp)
        }));
    }
    Ok(())
}

fn spawn_request_timeout(mut timeout_sender: Sender<RequestKey>, request_key: RequestKey, timeout: Duration) {
    task::spawn(async move {
        tokio::time::delay_for(timeout).await;
        let _ = timeout_sender.send(request_key).await;
    });
}

async fn handle_incoming_block<B: BlockchainBackend + 'static>(
    mut inbound_nch: InboundNodeCommsHandlers<B>,
    domain_block_msg: DomainMessage<NewBlock>,
) -> Result<(), BaseNodeServiceError>
{
    let DomainMessage::<_> {
        source_peer,
        inner: new_block,
        ..
    } = domain_block_msg;

    debug!(
        target: LOG_TARGET,
        "New candidate block with hash `{}` received from `{}`.",
        new_block.block_hash.to_hex(),
        source_peer.node_id.short_str()
    );

    inbound_nch
        .handle_new_block_message(new_block, source_peer.node_id)
        .await?;

    // TODO - retain peer info for stats and potential banning for sending invalid blocks

    Ok(())
}

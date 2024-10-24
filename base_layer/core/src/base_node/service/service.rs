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

use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};

use futures::{pin_mut, stream::StreamExt, Stream};
use log::*;
use rand::rngs::OsRng;
use tari_common_types::types::BlockHash;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester, SendMessageParams},
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::reply_channel::RequestContext;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{
        mpsc,
        mpsc::{Receiver, Sender, UnboundedReceiver},
        oneshot::Sender as OneshotSender,
    },
    task,
};

use crate::{
    base_node::{
        comms_interface::{CommsInterfaceError, InboundNodeCommsHandlers, NodeCommsRequest, NodeCommsResponse},
        service::{error::BaseNodeServiceError, initializer::ExtractBlockError},
        state_machine_service::states::StateInfo,
        BaseNodeStateMachineConfig,
        StateMachineHandle,
    },
    blocks::{Block, NewBlock},
    chain_storage::{BlockchainBackend, ChainStorageError},
    common::{
        waiting_requests::{generate_request_key, RequestKey, WaitingRequests},
        BanPeriod,
    },
    proto as shared_protos,
    proto::base_node as proto,
};

const LOG_TARGET: &str = "c::bn::base_node_service::service";

/// A convenience struct to hold all the BaseNode streams
pub(super) struct BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock> {
    /// `NodeCommsRequest` messages to send to a remote peer. If a specific peer is not provided, a random peer is
    /// chosen.
    pub outbound_request_stream: SOutReq,
    /// Blocks to be propagated out to the network. The second element of the tuple is a list of peers to exclude from
    /// this round of propagation
    pub outbound_block_stream: UnboundedReceiver<(NewBlock, Vec<NodeId>)>,
    /// `BaseNodeRequest` messages received from external peers
    pub inbound_request_stream: SInReq,
    /// `BaseNodeResponse` messages received from external peers
    pub inbound_response_stream: SInRes,
    /// `NewBlock` messages received from external peers
    pub inbound_block_stream: SBlockIn,
    /// Incoming local request messages from the LocalNodeCommsInterface and other local services
    pub local_request_stream: SLocalReq,
    /// The stream of blocks sent from local services `LocalCommsNodeInterface::submit_block` e.g. block sync and
    /// miner
    pub local_block_stream: SLocalBlock,
}

/// The Base Node Service is responsible for handling inbound requests and responses and for sending new requests to
/// remote Base Node Services.
pub(super) struct BaseNodeService<B> {
    outbound_message_service: OutboundMessageRequester,
    inbound_nch: InboundNodeCommsHandlers<B>,
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    service_request_timeout: Duration,
    state_machine_handle: StateMachineHandle,
    connectivity: ConnectivityRequester,
    base_node_config: BaseNodeStateMachineConfig,
}

impl<B> BaseNodeService<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        outbound_message_service: OutboundMessageRequester,
        inbound_nch: InboundNodeCommsHandlers<B>,
        service_request_timeout: Duration,
        state_machine_handle: StateMachineHandle,
        connectivity: ConnectivityRequester,
        base_node_config: BaseNodeStateMachineConfig,
    ) -> Self {
        let (timeout_sender, timeout_receiver) = mpsc::channel(100);
        Self {
            outbound_message_service,
            inbound_nch,
            waiting_requests: WaitingRequests::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            service_request_timeout,
            state_machine_handle,
            connectivity,
            base_node_config,
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
        SInReq: Stream<Item = DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>>,
        SInRes: Stream<Item = DomainMessage<Result<proto::BaseNodeServiceResponse, prost::DecodeError>>>,
        SBlockIn: Stream<Item = DomainMessage<Result<NewBlock, ExtractBlockError>>>,
        SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
        SLocalBlock: Stream<Item = RequestContext<Block, Result<BlockHash, CommsInterfaceError>>>,
    {
        let outbound_request_stream = streams.outbound_request_stream.fuse();
        pin_mut!(outbound_request_stream);
        let outbound_block_stream = streams.outbound_block_stream;
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
            .expect("Base Node Service initialized without timeout_receiver_stream");
        pin_mut!(timeout_receiver_stream);
        loop {
            tokio::select! {
                // Outbound request messages from the OutboundNodeCommsInterface
                Some(outbound_request_context) = outbound_request_stream.next() => {
                    self.spawn_handle_outbound_request(outbound_request_context);
                },

                // Outbound block messages from the OutboundNodeCommsInterface
                Some((block, excluded_peers)) = outbound_block_stream.recv() => {
                    self.spawn_handle_outbound_block(block, excluded_peers);
                },

                // Incoming request messages from the Comms layer
                Some(domain_msg) = inbound_request_stream.next() => {
                    self.spawn_handle_incoming_request(domain_msg);
                },

                // Incoming response messages from the Comms layer
                Some(domain_msg) = inbound_response_stream.next() => {
                    self.spawn_handle_incoming_response(domain_msg);
                },

                // Timeout events for waiting requests
                Some(timeout_request_key) = timeout_receiver_stream.recv() => {
                    self.spawn_handle_request_timeout(timeout_request_key);
                },

                // Incoming block messages from the Comms layer
                Some(block_msg) = inbound_block_stream.next() => {
                    self.spawn_handle_incoming_block(block_msg);
                }

                // Incoming local request messages from the LocalNodeCommsInterface and other local services
                Some(local_request_context) = local_request_stream.next() => {
                    self.spawn_handle_local_request(local_request_context);
                },

                // Incoming local block messages from the LocalNodeCommsInterface e.g. miner and block sync
                Some(local_block_context) = local_block_stream.next() => {
                    self.spawn_handle_local_block(local_block_context);
                },

                else => {
                    info!(target: LOG_TARGET, "Base Node service shutting down because all streams ended");
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
    ) {
        let outbound_message_service = self.outbound_message_service.clone();
        let waiting_requests = self.waiting_requests.clone();
        let timeout_sender = self.timeout_sender.clone();
        let service_request_timeout = self.service_request_timeout;
        task::spawn(async move {
            let ((request, node_id), reply_tx) = request_context.split();

            let result = handle_outbound_request(
                outbound_message_service,
                waiting_requests,
                timeout_sender,
                reply_tx,
                request,
                node_id,
                service_request_timeout,
            )
            .await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_outbound_block(&self, new_block: NewBlock, excluded_peers: Vec<NodeId>) {
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let result = handle_outbound_block(outbound_message_service, new_block, excluded_peers).await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound block message {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_request(
        &self,
        domain_msg: DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>,
    ) {
        let inbound_nch = self.inbound_nch.clone();
        let outbound_message_service = self.outbound_message_service.clone();
        let state_machine_handle = self.state_machine_handle.clone();
        let mut connectivity = self.connectivity.clone();
        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            let result = handle_incoming_request(
                inbound_nch,
                outbound_message_service,
                state_machine_handle,
                domain_msg.clone(),
            )
            .await;
            if let Err(e) = result {
                if let Some(ban_reason) = e.get_ban_reason() {
                    let duration = match ban_reason.ban_duration {
                        BanPeriod::Short => short_ban,
                        BanPeriod::Long => long_ban,
                    };
                    let _drop = connectivity
                        .ban_peer_until(domain_msg.source_peer.node_id.clone(), duration, ban_reason.reason)
                        .await
                        .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {:?}", e));
                }
                error!(target: LOG_TARGET, "Failed to handle incoming request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_response(
        &self,
        domain_msg: DomainMessage<Result<proto::BaseNodeServiceResponse, prost::DecodeError>>,
    ) {
        let waiting_requests = self.waiting_requests.clone();
        let mut connectivity_requester = self.connectivity.clone();

        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            let source_peer = domain_msg.source_peer.clone();
            let result = handle_incoming_response(waiting_requests, domain_msg).await;

            if let Err(e) = result {
                if let Some(ban_reason) = e.get_ban_reason() {
                    let duration = match ban_reason.ban_duration {
                        BanPeriod::Short => short_ban,
                        BanPeriod::Long => long_ban,
                    };
                    let _drop = connectivity_requester
                        .ban_peer_until(source_peer.node_id, duration, ban_reason.reason)
                        .await
                        .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {:?}", e));
                }
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming response message: {:?}", e
                );
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

    fn spawn_handle_incoming_block(&self, new_block: DomainMessage<Result<NewBlock, ExtractBlockError>>) {
        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();

        if !(status_watch.borrow()).bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Propagated block from peer `{}` not processed while busy with initial sync.",
                new_block.source_peer.node_id.short_str(),
            );
            return;
        }
        let inbound_nch = self.inbound_nch.clone();
        let mut connectivity_requester = self.connectivity.clone();
        let source_peer = new_block.source_peer.clone();
        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            let result = handle_incoming_block(inbound_nch, new_block).await;

            match result {
                Ok(()) => {},
                Err(BaseNodeServiceError::CommsInterfaceError(CommsInterfaceError::ChainStorageError(
                    ChainStorageError::AddBlockOperationLocked,
                ))) => {
                    // Special case, dont log this again as an error
                },
                Err(e) => {
                    if let Some(ban_reason) = e.get_ban_reason() {
                        let duration = match ban_reason.ban_duration {
                            BanPeriod::Short => short_ban,
                            BanPeriod::Long => long_ban,
                        };
                        let _drop = connectivity_requester
                            .ban_peer_until(source_peer.node_id, duration, ban_reason.reason)
                            .await
                            .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {:?}", e));
                    }
                    error!(target: LOG_TARGET, "Failed to handle incoming block message: {}", e)
                },
            }
        });
    }

    fn spawn_handle_local_request(
        &self,
        request_context: RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>,
    ) {
        let inbound_nch = self.inbound_nch.clone();
        task::spawn(async move {
            let (request, reply_tx) = request_context.split();
            let res = inbound_nch.handle_request(request).await;
            if let Err(ref e) = res {
                error!(
                    target: LOG_TARGET,
                    "BaseNodeService failed to handle local request {:?}", e
                );
            }
            let result = reply_tx.send(res);
            if let Err(res) = result {
                error!(
                    target: LOG_TARGET,
                    "BaseNodeService failed to send reply to local request {:?}",
                    res.map(|r| r.to_string()).map_err(|e| e.to_string())
                );
            }
        });
    }

    fn spawn_handle_local_block(&self, block_context: RequestContext<Block, Result<BlockHash, CommsInterfaceError>>) {
        let mut inbound_nch = self.inbound_nch.clone();
        task::spawn(async move {
            let (block, reply_tx) = block_context.split();
            let result = reply_tx.send(inbound_nch.handle_block(block, None).await);

            if let Err(res) = result {
                error!(
                    target: LOG_TARGET,
                    "BaseNodeService Caller dropped the oneshot receiver before reply could be sent. Reply: {:?}",
                    res.map(|r| r.to_string()).map_err(|e| e.to_string())
                );
            }
        });
    }
}

async fn handle_incoming_request<B: BlockchainBackend + 'static>(
    inbound_nch: InboundNodeCommsHandlers<B>,
    mut outbound_message_service: OutboundMessageRequester,
    state_machine_handle: StateMachineHandle,
    domain_request_msg: DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>,
) -> Result<(), BaseNodeServiceError> {
    let (origin_public_key, inner_msg) = domain_request_msg.into_origin_and_inner();

    // Convert proto::BaseNodeServiceRequest to a BaseNodeServiceRequest
    let inner_msg = match inner_msg {
        Ok(i) => i,
        Err(e) => {
            return Err(BaseNodeServiceError::InvalidRequest(format!(
                "Received invalid base node request: {}",
                e
            )));
        },
    };

    let request = match inner_msg.request {
        Some(r) => r,
        None => {
            return Err(BaseNodeServiceError::InvalidRequest(
                "Received invalid base node request with no inner request".to_string(),
            ));
        },
    };

    let request = match request.try_into() {
        Ok(r) => r,
        Err(e) => {
            return Err(BaseNodeServiceError::InvalidRequest(format!(
                "Received invalid base node request. It could not be converted:  {}",
                e
            )));
        },
    };

    let response = inbound_nch.handle_request(request).await?;

    // Determine if we are synced
    let status_watch = state_machine_handle.get_status_info_watch();
    let is_synced = match (status_watch.borrow()).state_info {
        StateInfo::Listening(li) => li.is_synced(),
        _ => false,
    };

    let message = proto::BaseNodeServiceResponse {
        request_key: inner_msg.request_key,
        response: Some(response.try_into().map_err(BaseNodeServiceError::InvalidResponse)?),
        is_synced,
    };

    trace!(
        target: LOG_TARGET,
        "Attempting outbound message in response to inbound request ({})",
        inner_msg.request_key
    );

    let send_message_response = outbound_message_service
        .send_direct_unencrypted(
            origin_public_key,
            OutboundDomainMessage::new(&TariMessageType::BaseNodeResponse, message),
            "Outbound response message from base node".to_string(),
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
            if send_states.wait_single().await {
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
    domain_msg: DomainMessage<Result<proto::BaseNodeServiceResponse, prost::DecodeError>>,
) -> Result<(), BaseNodeServiceError> {
    let incoming_response = domain_msg
        .inner()
        .clone()
        .map_err(|e| BaseNodeServiceError::InvalidResponse(format!("Received invalid base node response: {}", e)))?;
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
        let _result = reply_tx.send(Ok(response).map_err(|e| {
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
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: Sender<RequestKey>,
    reply_tx: OneshotSender<Result<NodeCommsResponse, CommsInterfaceError>>,
    request: NodeCommsRequest,
    node_id: Option<NodeId>,
    service_request_timeout: Duration,
) -> Result<(), CommsInterfaceError> {
    let debug_info = format!(
        "Node request:{} to {}",
        &request,
        node_id
            .as_ref()
            .map(|n| n.short_str())
            .unwrap_or_else(|| "random".to_string())
    );
    let request_key = generate_request_key(&mut OsRng);
    let service_request = proto::BaseNodeServiceRequest {
        request_key,
        request: Some(request.try_into().map_err(CommsInterfaceError::InternalError)?),
    };

    let mut send_msg_params = SendMessageParams::new();
    send_msg_params.with_debug_info(debug_info);
    match node_id {
        Some(node_id) => send_msg_params.direct_node_id(node_id),
        None => send_msg_params.random(1),
    };

    trace!(target: LOG_TARGET, "Attempting outbound request ({})", request_key);
    let send_result = outbound_message_service
        .send_message(
            send_msg_params.finish(),
            OutboundDomainMessage::new(&TariMessageType::BaseNodeRequest, service_request.clone()),
        )
        .await?;

    match send_result.resolve().await {
        Ok(send_states) if send_states.is_empty() => {
            let result = reply_tx.send(Err(CommsInterfaceError::NoBootstrapNodesConfigured));

            if let Err(_e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to send outbound request as no bootstrap nodes were configured"
                );
            }
        },
        Ok(send_states) => {
            // Wait for matching responses to arrive
            waiting_requests.insert(request_key, reply_tx).await;
            // Spawn timeout for waiting_request
            if service_request.request.is_some() {
                trace!(
                    target: LOG_TARGET,
                    "Timeout for service request ... ({}) set at {:?}",
                    request_key,
                    service_request_timeout
                );
                spawn_request_timeout(timeout_sender, request_key, service_request_timeout)
            };
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
            let result = reply_tx.send(Err(CommsInterfaceError::BroadcastFailed));

            if let Err(_e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to send outbound request ({}) because DHT outbound broadcast failed", request_key
                );
            }
        },
    }
    Ok(())
}

async fn handle_outbound_block(
    mut outbound_message_service: OutboundMessageRequester,
    new_block: NewBlock,
    exclude_peers: Vec<NodeId>,
) -> Result<(), CommsInterfaceError> {
    let result = outbound_message_service
        .propagate(
            NodeDestination::Unknown,
            OutboundEncryption::ClearText,
            exclude_peers,
            OutboundDomainMessage::new(
                &TariMessageType::NewBlock,
                shared_protos::core::NewBlock::try_from(new_block).map_err(CommsInterfaceError::InternalError)?,
            ),
            "Outbound new block from base node".to_string(),
        )
        .await;
    if let Err(e) = result {
        return match e {
            DhtOutboundError::NoMessagesQueued => Ok(()),
            _ => Err(e.into()),
        };
    }
    Ok(())
}

async fn handle_request_timeout(
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    request_key: RequestKey,
) -> Result<(), CommsInterfaceError> {
    if let Some((reply_tx, started)) = waiting_requests.remove(request_key).await {
        warn!(
            target: LOG_TARGET,
            "Request (request key {}) timed out after {}ms",
            &request_key,
            started.elapsed().as_millis()
        );
        let reply_msg = Err(CommsInterfaceError::RequestTimedOut);
        let _result = reply_tx.send(reply_msg.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Failed to process outbound request (request key: {}): {:?}", &request_key, e
            );
            e
        }));
    }
    Ok(())
}

fn spawn_request_timeout(timeout_sender: Sender<RequestKey>, request_key: RequestKey, timeout: Duration) {
    task::spawn(async move {
        tokio::time::sleep(timeout).await;
        let _ = timeout_sender.send(request_key).await;
    });
}

async fn handle_incoming_block<B: BlockchainBackend + 'static>(
    mut inbound_nch: InboundNodeCommsHandlers<B>,
    domain_block_msg: DomainMessage<Result<NewBlock, ExtractBlockError>>,
) -> Result<(), BaseNodeServiceError> {
    let DomainMessage::<_> {
        source_peer,
        inner: new_block,
        ..
    } = domain_block_msg;

    let new_block = new_block.map_err(BaseNodeServiceError::InvalidBlockMessage)?;
    debug!(
        target: LOG_TARGET,
        "New candidate block with hash `{}` received from `{}`.",
        new_block.header.hash().to_hex(),
        source_peer.node_id.short_str()
    );

    inbound_nch
        .handle_new_block_message(new_block, source_peer.node_id)
        .await?;

    Ok(())
}

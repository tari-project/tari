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
    sync::Arc,
    time::Duration,
};

use futures::{Stream, pin_mut, stream::StreamExt};
use log::*;
use tari_common_types::types::BlockHash;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::NodeId};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{DhtOutboundError, OutboundEncryption, OutboundMessageRequester, SendMessageParams},
};
use tari_node_components::blocks::{Block, NewBlock};
use tari_p2p::{comms_connector::PeerMessage, domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::reply_channel::RequestContext;
use tari_transaction_components::BanPeriod;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{
        Semaphore,
        mpsc,
        mpsc::{Receiver, Sender, UnboundedReceiver},
        oneshot::Sender as OneshotSender,
    },
    task,
};

use crate::{
    base_node::{
        BaseNodeStateMachineConfig,
        StateMachineHandle,
        comms_interface::{CommsInterfaceError, InboundNodeCommsHandlers, NodeCommsRequest, NodeCommsResponse},
        service::{
            error::BaseNodeServiceError,
            initializer::{ExtractBlockError, extract_block},
        },
        state_machine_service::states::StateInfo,
    },
    chain_storage::{BlockchainBackend, ChainStorageError},
    common::{
        RequestKey,
        inbound_backpressure::PendingByPeer,
        waiting_requests::{WaitingRequests, generate_request_key},
    },
    proto as shared_protos,
    proto::base_node as proto,
};
const LOG_TARGET: &str = "c::bn::base_node_service::service";

/// The maximum number of inbound `NewBlock` messages decoded concurrently. Decoding is CPU-bound and done on a blocking
/// thread; this has its own bound so that it never competes with block reconstruction for mempool permits.
const MAX_CONCURRENT_BLOCK_DECODES: usize = 2;
/// The maximum number of inbound `NewBlock` messages from a single peer that are accepted but not yet handled. Further
/// messages from that peer are dropped until one finishes; other peers (e.g. a competing miner) are unaffected.
const MAX_PENDING_INBOUND_BLOCKS_PER_PEER: usize = 8;
/// A backstop on the number of inbound `NewBlock` messages pending from all peers together, only reached if many peers
/// flood at once.
const MAX_PENDING_INBOUND_BLOCKS_TOTAL: usize = 64;

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
    block_decode_permits: Arc<Semaphore>,
    pending_inbound_blocks: PendingByPeer,
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
            block_decode_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_BLOCK_DECODES)),
            pending_inbound_blocks: PendingByPeer::new(
                MAX_PENDING_INBOUND_BLOCKS_PER_PEER,
                MAX_PENDING_INBOUND_BLOCKS_TOTAL,
                "block",
                LOG_TARGET,
            ),
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
        SBlockIn: Stream<Item = Arc<PeerMessage>>,
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
                    let _spawned = self.spawn_handle_incoming_block(block_msg);
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
                error!(target: LOG_TARGET, "Failed to handle outbound request message: {e:?}");
            }
        });
    }

    fn spawn_handle_outbound_block(&self, new_block: NewBlock, excluded_peers: Vec<NodeId>) {
        let outbound_message_service = self.outbound_message_service.clone();
        task::spawn(async move {
            let result = handle_outbound_block(outbound_message_service, new_block, excluded_peers).await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound block message {e:?}");
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
                        .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {e:?}"));
                }
                error!(target: LOG_TARGET, "Failed to handle incoming request message: {e:?}");
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
                        .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {e:?}"));
                }
                error!(
                    target: LOG_TARGET,
                    "Failed to handle incoming response message: {e:?}"
                );
            }
        });
    }

    fn spawn_handle_request_timeout(&self, timeout_request_key: u64) {
        let waiting_requests = self.waiting_requests.clone();
        task::spawn(async move {
            let result = handle_request_timeout(waiting_requests, timeout_request_key).await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle request timeout event: {e:?}");
            }
        });
    }

    /// Handle a raw inbound `NewBlock` message. Decoding and reconciliation are both done in a spawned task, so that
    /// the service loop is never blocked decoding a block; decoding is bounded by its own semaphore
    /// ([MAX_CONCURRENT_BLOCK_DECODES]).
    ///
    /// At most [MAX_PENDING_INBOUND_BLOCKS_PER_PEER] messages per peer (and [MAX_PENDING_INBOUND_BLOCKS_TOTAL] overall)
    /// are handled at once; beyond that, the message is dropped. Returns `true` if a task was spawned to handle the
    /// message.
    fn spawn_handle_incoming_block(&mut self, new_block: Arc<PeerMessage>) -> bool {
        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();

        if !(status_watch.borrow()).bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Propagated block from peer `{}` not processed while busy with initial sync.",
                new_block.source_peer.node_id.short_str(),
            );
            return false;
        }
        let Some(pending_guard) = self.pending_inbound_blocks.try_accept(&new_block.source_peer.node_id) else {
            return false;
        };
        let decode_permits = self.block_decode_permits.clone();
        let inbound_nch = self.inbound_nch.clone();
        let mut connectivity_requester = self.connectivity.clone();
        let source_peer = new_block.source_peer.clone();
        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            // Released when the task finishes, however it finishes
            let _pending_guard = pending_guard;
            let result = handle_incoming_block(inbound_nch, decode_permits, new_block).await;

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
                            .ban_peer_until(source_peer.node_id.clone(), duration, ban_reason.reason)
                            .await
                            .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {e:?}"));
                    }
                    // A block on a chain we cannot check yet, one we already hold as bad, or a reorg that failed on an
                    // orphan someone else sent us, is dropped as expected
                    let expected = matches!(
                        &e,
                        BaseNodeServiceError::CommsInterfaceError(
                            CommsInterfaceError::UnknownSpentOutputs { .. } |
                                CommsInterfaceError::KnownBadBlock { .. } |
                                CommsInterfaceError::ChainStorageError(
                                    ChainStorageError::UnverifiedHeldBlockInvalid { .. }
                                )
                        )
                    );
                    if expected {
                        info!(
                            target: LOG_TARGET,
                            "Dropped incoming block message from peer {}: {e}",
                            source_peer.node_id
                        );
                    } else {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle incoming block message from peer {}: {e}",
                            source_peer.node_id
                        );
                    }
                },
            }
        });
        true
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
                warn!(
                    target: LOG_TARGET,
                    "BaseNodeService failed to handle local request {e:?}"
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
                "Received invalid base node request: {e}"
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
                "Received invalid base node request. It could not be converted:  {e}"
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
                "Incoming request ({request_key}) response failed to send: {err}"
            );
        },
        Ok(send_states) => {
            let msg_tag = send_states[0].tag;
            if send_states.wait_single().await {
            } else {
                error!(
                    target: LOG_TARGET,
                    "Incoming request ({request_key}) response Direct Send was unsuccessful and no message was sent {msg_tag}"
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
        .map_err(|e| BaseNodeServiceError::InvalidResponse(format!("Received invalid base node response: {e}")))?;
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
            request_key,
            started.elapsed().as_millis(),
            is_synced
        );
        let _result = reply_tx.send(Ok(response).map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "Failed to finalize request (request key:{}): {:?}", request_key, e
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
        request,
        node_id
            .as_ref()
            .map(|n| n.short_str())
            .unwrap_or_else(|| "random".to_string())
    );
    let request_key = generate_request_key(&mut rand::rng());
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

    trace!(target: LOG_TARGET, "Attempting outbound request ({request_key})");
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
                    "Timeout for service request ... ({request_key}) set at {service_request_timeout:?}"
                );
                spawn_request_timeout(timeout_sender, request_key, service_request_timeout)
            };
            // Log messages
            let msg_tag = send_states[0].tag;
            debug!(
                target: LOG_TARGET,
                "Outbound request ({request_key}) response queued with {msg_tag}"
            );

            if send_states.wait_single().await {
                debug!(
                    target: LOG_TARGET,
                    "Outbound request ({request_key}) response Direct Send was successful {msg_tag}"
                );
            } else {
                error!(
                    target: LOG_TARGET,
                    "Outbound request ({request_key}) response Direct Send was unsuccessful and no message was sent"
                );
            };
        },
        Err(err) => {
            debug!(target: LOG_TARGET, "Failed to send outbound request: {err}");
            let result = reply_tx.send(Err(CommsInterfaceError::BroadcastFailed));

            if let Err(_e) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to send outbound request ({request_key}) because DHT outbound broadcast failed"
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
            request_key,
            started.elapsed().as_millis()
        );
        let reply_msg = Err(CommsInterfaceError::RequestTimedOut);
        let _result = reply_tx.send(reply_msg.map_err(|e| {
            error!(
                target: LOG_TARGET,
                "Failed to process outbound request (request key: {request_key}): {e:?}"
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

/// Decode an inbound `NewBlock` message on a blocking thread, holding a block decode permit only for the duration of
/// the decode. The permit bounds this CPU-bound work; it is released before the block is reconciled, since
/// reconciliation may wait on the network (up to the request timeout) and on block processing.
async fn decode_block_message(
    decode_permits: Arc<Semaphore>,
    msg: Arc<PeerMessage>,
) -> Result<DomainMessage<Result<NewBlock, ExtractBlockError>>, BaseNodeServiceError> {
    let _permit = decode_permits
        .acquire_owned()
        .await
        .map_err(|e| CommsInterfaceError::InternalError(format!("Block decode semaphore closed: {e}")))?;
    let decoded = task::spawn_blocking(move || extract_block(&msg))
        .await
        .map_err(|e| CommsInterfaceError::InternalError(format!("Failed to decode inbound block message: {e}")))?;
    Ok(decoded)
}

async fn handle_incoming_block<B: BlockchainBackend + 'static>(
    mut inbound_nch: InboundNodeCommsHandlers<B>,
    decode_permits: Arc<Semaphore>,
    msg: Arc<PeerMessage>,
) -> Result<(), BaseNodeServiceError> {
    let domain_block_msg = decode_block_message(decode_permits, msg).await?;
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

#[cfg(test)]
mod test {
    use std::cmp::max;

    use tari_common_types::types::{FixedHash, PrivateKey};
    use tari_comms::test_utils::mocks::create_connectivity_mock;
    use tari_node_components::blocks::BlockHeader;
    use tari_p2p::tari_message::TariMessageType;
    use tari_service_framework::reply_channel;
    use tari_transaction_components::tari_proof_of_work::PowAlgorithm;
    use tokio::sync::broadcast;

    use super::*;
    use crate::{
        base_node::comms_interface::OutboundNodeCommsInterface,
        chain_storage::BlockchainDatabase,
        mempool::{Mempool, MempoolConfig},
        proof_of_work::{randomx_factory::RandomXFactory, sha3x_difficulty},
        test_helpers::{
            blockchain::{TempDatabase, create_new_blockchain},
            create_consensus_rules,
            create_peer_message,
        },
        validation::mocks::MockValidator,
    };

    type OutboundRequests =
        reply_channel::Receiver<(NodeCommsRequest, Option<NodeId>), Result<NodeCommsResponse, CommsInterfaceError>>;

    fn create_handlers() -> (
        InboundNodeCommsHandlers<TempDatabase>,
        BlockchainDatabase<TempDatabase>,
        Mempool,
        OutboundRequests,
    ) {
        let mempool = Mempool::new(
            MempoolConfig::default(),
            create_consensus_rules(),
            Box::new(MockValidator::new(true)),
        );
        let db = create_new_blockchain();
        let (block_event_sender, _) = broadcast::channel(50);
        let (request_sender, request_receiver) = reply_channel::unbounded();
        let (block_sender, _) = mpsc::unbounded_channel();
        let (connectivity, _) = create_connectivity_mock();
        let inbound_nch = InboundNodeCommsHandlers::new(
            block_event_sender,
            db.clone().into(),
            mempool.clone(),
            create_consensus_rules(),
            OutboundNodeCommsInterface::new(request_sender, block_sender),
            connectivity,
            RandomXFactory::new(1),
        );
        (inbound_nch, db, mempool, request_receiver)
    }

    fn decode_permits() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(MAX_CONCURRENT_BLOCK_DECODES))
    }

    #[tokio::test]
    async fn malformed_block_message_is_rejected_and_releases_the_decode_permit() {
        let (inbound_nch, _db, mempool, _requests) = create_handlers();
        let reconciliation_permits = mempool.available_reconciliation_permits();
        let decode_permits = decode_permits();

        let msg = create_peer_message(TariMessageType::NewBlock, vec![0xff; 64]);
        let err = handle_incoming_block(inbound_nch.clone(), decode_permits.clone(), msg)
            .await
            .unwrap_err();
        // Unchanged behaviour: an undecodable block message is an `InvalidBlockMessage`
        assert!(matches!(err, BaseNodeServiceError::InvalidBlockMessage(_)));
        assert_eq!(decode_permits.available_permits(), MAX_CONCURRENT_BLOCK_DECODES);

        let body = prost::Message::encode_to_vec(&shared_protos::core::NewBlock::default());
        let msg = create_peer_message(TariMessageType::NewBlock, body);
        let err = handle_incoming_block(inbound_nch, decode_permits.clone(), msg)
            .await
            .unwrap_err();
        assert!(matches!(err, BaseNodeServiceError::InvalidBlockMessage(_)));
        assert_eq!(decode_permits.available_permits(), MAX_CONCURRENT_BLOCK_DECODES);
        // Decoding never touches the mempool's reconciliation permits
        assert_eq!(mempool.available_reconciliation_permits(), reconciliation_permits);
    }

    /// The decode permit only covers decoding: it is back in the pool while reconciliation waits on the network, and
    /// decoding does not use the mempool's reconciliation permits at all.
    #[tokio::test]
    async fn decode_permit_is_released_before_reconciliation_waits_on_the_network() {
        let (inbound_nch, db, mempool, mut requests) = create_handlers();
        let reconciliation_permits = mempool.available_reconciliation_permits();
        let decode_permits = decode_permits();

        // An orphan block (its parent is unknown) with enough proof of work to pass the anti-spam gate, and a kernel
        // that is not in our mempool, so reconciliation must ask the announcing peer for the full block
        let tip = db.fetch_last_chain_header().unwrap();
        let constants = db.consensus_constants().unwrap().clone();
        let mut min_difficulty = constants.min_pow_difficulty(PowAlgorithm::Sha3x);
        if tip.header().pow_algo() == PowAlgorithm::Sha3x {
            min_difficulty = max(
                tip.accumulated_data()
                    .target_difficulty
                    .checked_div_u64(2)
                    .unwrap_or(min_difficulty),
                min_difficulty,
            );
        }
        let mut header = BlockHeader::new(tip.header().version);
        header.height = tip.height() + 5;
        header.prev_hash = FixedHash::from([7u8; 32]);
        while sha3x_difficulty(&header).unwrap() < min_difficulty {
            header.nonce += 1;
        }
        let new_block = NewBlock {
            header,
            coinbase_kernels: vec![],
            coinbase_outputs: vec![],
            kernel_excess_sigs: vec![PrivateKey::default()],
        };
        let body = prost::Message::encode_to_vec(&shared_protos::core::NewBlock::try_from(new_block).unwrap());
        let msg = create_peer_message(TariMessageType::NewBlock, body);

        let task = task::spawn(handle_incoming_block(inbound_nch, decode_permits.clone(), msg));
        // The full block request reaches the (never answering) outbound interface
        let request = tokio::time::timeout(Duration::from_secs(30), requests.next())
            .await
            .expect("reconciliation did not request the full block")
            .expect("request stream closed");
        assert!(matches!(
            request.request().0,
            NodeCommsRequest::GetBlockFromAllChains(_)
        ));
        // ... while the decode permit has already been returned, and no reconciliation permit is held
        assert_eq!(decode_permits.available_permits(), MAX_CONCURRENT_BLOCK_DECODES);
        assert_eq!(mempool.available_reconciliation_permits(), reconciliation_permits);
        assert!(!task.is_finished());
        task.abort();
    }
}

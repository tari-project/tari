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
use tari_comms::{
    RefKind,
    connectivity::ConnectivityRequester,
    peer_manager::NodeId,
    protocol::messaging::SendFailReason,
};
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
/// The maximum number of inbound `BaseNodeRequest`s from a single peer handled at once, counted until the reply has
/// left our outbound queue. Each request can produce a large reply (e.g. a full block), so a peer that keeps asking
/// without reading the replies is held to a few at a time. Further requests from that peer are dropped; other peers are
/// unaffected. It matches [MAX_PENDING_INBOUND_BLOCKS_PER_PEER]: an honest peer reconciles up to that many of our
/// blocks at once, each of which can ask us for the full block, and a request we drop makes that peer time out and ban
/// us.
///
/// There is deliberately no total across peers: slots are held while replies wait in the outbound queue, so peers that
/// never read could pin any total cap and make us drop honest peers' requests, which those peers then time out and
/// blame on us.
const MAX_INFLIGHT_REQUESTS_PER_PEER: usize = MAX_PENDING_INBOUND_BLOCKS_PER_PEER;

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
    inflight_inbound_requests: PendingByPeer,
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
            inflight_inbound_requests: PendingByPeer::new(
                MAX_INFLIGHT_REQUESTS_PER_PEER,
                // No total cap; see MAX_INFLIGHT_REQUESTS_PER_PEER
                usize::MAX,
                "base node request",
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
        &mut self,
        domain_msg: DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>,
    ) {
        // Our requests are never signed, so a request signed by some other identity than the peer sending it was
        // relayed on someone else's behalf. Answering it would reflect the reply through us; drop it instead, before it
        // takes one of the sender's slots (it is not necessarily the sending peer's fault, so no ban).
        if let Some(origin) = &domain_msg.authenticated_origin &&
            *origin != domain_msg.source_peer.public_key
        {
            debug!(
                target: LOG_TARGET,
                "Dropping base node request from peer '{}' signed by another identity ({})",
                domain_msg.source_peer.node_id.short_str(),
                origin
            );
            return;
        }
        // Dropped (and logged at a limited rate) if the peer already has too many requests in flight; never banned
        let Some(inflight_guard) = self
            .inflight_inbound_requests
            .try_accept(&domain_msg.source_peer.node_id)
        else {
            return;
        };
        let inbound_nch = self.inbound_nch.clone();
        let outbound_message_service = self.outbound_message_service.clone();
        let state_machine_handle = self.state_machine_handle.clone();
        let mut connectivity = self.connectivity.clone();
        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            // Held until the reply has been sent (or failed), so it covers the reply's time in the outbound queue
            let _inflight_guard = inflight_guard;
            let result = handle_incoming_request(
                inbound_nch,
                outbound_message_service,
                connectivity.clone(),
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
            let mut block_hash = None;
            let result = handle_incoming_block(inbound_nch, decode_permits, new_block, &mut block_hash).await;

            match result {
                Ok(()) => {},
                Err(BaseNodeServiceError::CommsInterfaceError(CommsInterfaceError::ChainStorageError(
                    ChainStorageError::AddBlockOperationLocked,
                ))) => {
                    // Special case, dont log this again as an error
                },
                Err(e) => {
                    // Names the block in the logs below, if it was decoded far enough to have one
                    let block = block_hash
                        .map(|h| h.to_hex())
                        .unwrap_or_else(|| "<undecoded>".to_string());
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
                    // A block on a chain we cannot check yet, one we already hold as bad, or one a reorg dropped
                    // without its sender being to blame (`HeldBlockInvalid`, which is not a ban), is dropped as
                    // expected.
                    let expected = matches!(
                        &e,
                        BaseNodeServiceError::CommsInterfaceError(
                            CommsInterfaceError::UnknownSpentOutputs { .. } |
                                CommsInterfaceError::KnownBadBlock { .. } |
                                CommsInterfaceError::ChainStorageError(ChainStorageError::HeldBlockInvalid { .. })
                        )
                    );
                    if expected {
                        debug!(
                            target: LOG_TARGET,
                            "Dropped incoming block message {block} from peer {}: {e}",
                            source_peer.node_id
                        );
                    } else {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle incoming block message {block} from peer {}: {e}",
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
    mut connectivity: ConnectivityRequester,
    state_machine_handle: StateMachineHandle,
    domain_request_msg: DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>>,
) -> Result<(), BaseNodeServiceError> {
    // Reply to the connected peer that sent the request, not to the request's authenticated origin. The origin is
    // whatever identity signed the message: anyone can sign a cheap request with a made-up identity, and replying to
    // it would make us dial that identity (and run DHT discovery for it) to deliver a large reply. Our own requests
    // are sent directly to the peer we ask (see `handle_outbound_request`) and are not signed, so a legitimate
    // request's origin is its source peer anyway.
    // Relayed requests (signed by another identity) were already dropped by `spawn_handle_incoming_request`.
    let reply_node_id = domain_request_msg.source_peer.node_id.clone();
    let reply_public_key = domain_request_msg.source_peer.public_key.clone();
    let inner_msg = domain_request_msg.into_inner();

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

    // Only answer a peer that is still connected. A direct send to a peer that has gone would dial it, holding the
    // reply in our outbound queues for the whole dial; a requester that disconnects (perhaps with an unreachable
    // address) does not get to make us do that. It is checked as late as possible, just before sending. If the
    // connectivity manager cannot answer (e.g. it is busy), the reply is sent anyway: not answering would make an
    // honest requester time out and ban us.
    match connectivity.get_connection(reply_node_id.clone(), RefKind::Weak).await {
        Ok(Some(conn)) if conn.is_connected() => {},
        Ok(_) => {
            debug!(
                target: LOG_TARGET,
                "Not answering request ({}) from peer '{}': it is no longer connected",
                inner_msg.request_key,
                reply_node_id.short_str()
            );
            return Ok(());
        },
        Err(err) => {
            warn!(
                target: LOG_TARGET,
                "Could not check whether peer '{}' is still connected ({err}); answering request ({}) anyway",
                reply_node_id.short_str(),
                inner_msg.request_key
            );
        },
    }

    let send_message_response = outbound_message_service
        .send_direct_unencrypted(
            reply_public_key,
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
            // Wait for matching responses to arrive. Inserted before the send completes, so a response that arrives
            // before we hear the send succeeded is still matched.
            waiting_requests.insert(request_key, reply_tx).await;
            // Log messages
            let msg_tag = send_states[0].tag;
            debug!(
                target: LOG_TARGET,
                "Outbound request ({request_key}) response queued with {msg_tag}"
            );

            match send_states.wait_single_result().await {
                Ok(()) => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound request ({request_key}) response Direct Send was successful {msg_tag}"
                    );
                    // The timeout only starts once the peer has the request: a timeout is blamed on the peer, so time
                    // spent dialing it or waiting in our own outbound queue must not count towards it
                    if service_request.request.is_some() {
                        trace!(
                            target: LOG_TARGET,
                            "Timeout for service request ... ({request_key}) set at {service_request_timeout:?}"
                        );
                        spawn_request_timeout(timeout_sender, request_key, service_request_timeout)
                    };
                },
                Err(reason) => {
                    error!(
                        target: LOG_TARGET,
                        "Outbound request ({request_key}) response Direct Send was unsuccessful and no message was \
                         sent: {reason}"
                    );
                    // Fail the request now rather than leave it to a timeout that never comes, unless a response
                    // already took it
                    if let Some((reply_tx, _)) = waiting_requests.remove(request_key).await {
                        let _result = reply_tx.send(Err(request_send_failure(reason)));
                    }
                },
            }
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

/// The error an outbound request fails with when it could not be sent, which decides whether the peer is blamed. Only
/// two outcomes are the peer's doing and fail as `RequestTimedOut`, which short-bans it as a request it never answered
/// would: our own write of the request timed out because the peer stopped reading (`SendTimeout`), or every retry was
/// used up by the peer resetting the substream while the connection stayed up (`MaxRetriesReached`). Everything else
/// is a `BroadcastFailed`, which bans nobody: our side failed (queue full, too large, expired, dropped, queued behind
/// another message's timeout), the connection went away (which we may have caused), or the dial failed - which covers
/// our own connectivity problems, our establishment deadline and peers that only accept inbound connections.
fn request_send_failure(reason: SendFailReason) -> CommsInterfaceError {
    match reason {
        SendFailReason::SendTimeout | SendFailReason::MaxRetriesReached(_) => CommsInterfaceError::RequestTimedOut,
        SendFailReason::QueueFull |
        SendFailReason::MessageTooLarge |
        SendFailReason::Expired |
        SendFailReason::Dropped |
        SendFailReason::QueuedBehindTimeout |
        SendFailReason::ConnectionLost(_) |
        SendFailReason::PeerDialFailed |
        // Not produced by the messaging protocol
        SendFailReason::SubstreamOpenFailed |
        SendFailReason::SubstreamSendFailed => CommsInterfaceError::BroadcastFailed,
    }
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

/// `block_hash` is set to the block's hash as soon as the message has been decoded, so that a caller handling an error
/// can name the block.
async fn handle_incoming_block<B: BlockchainBackend + 'static>(
    mut inbound_nch: InboundNodeCommsHandlers<B>,
    decode_permits: Arc<Semaphore>,
    msg: Arc<PeerMessage>,
    block_hash: &mut Option<BlockHash>,
) -> Result<(), BaseNodeServiceError> {
    let domain_block_msg = decode_block_message(decode_permits, msg).await?;
    let DomainMessage::<_> {
        source_peer,
        inner: new_block,
        ..
    } = domain_block_msg;

    let new_block = new_block.map_err(BaseNodeServiceError::InvalidBlockMessage)?;
    let hash = new_block.header.hash();
    *block_hash = Some(hash);
    debug!(
        target: LOG_TARGET,
        "New candidate block with hash `{}` received from `{}`.",
        hash.to_hex(),
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

    fn test_state_machine_handle() -> StateMachineHandle {
        use tokio::sync::watch;

        use crate::base_node::state_machine_service::states::StatusInfo;

        let (state_change_tx, _) = broadcast::channel(10);
        // Only the current status is read, which stays readable after the sender is dropped
        let (_status_tx, status_rx) = watch::channel(StatusInfo::new());
        StateMachineHandle::new(state_change_tx, status_rx, tari_shutdown::Shutdown::new().to_signal())
    }

    /// A connectivity manager mock with a live connection to `peer`
    async fn connected_to(peer: &tari_comms::peer_manager::Peer) -> ConnectivityRequester {
        use tari_comms::{
            peer_manager::PeerFeatures,
            test_utils::{mocks::create_peer_connection_mock_pair, node_identity::build_node_identity},
        };

        let (connectivity, connectivity_mock) = create_connectivity_mock();
        let connectivity_state = connectivity_mock.spawn();
        let ourselves = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        let (conn, _, _, _) = create_peer_connection_mock_pair(ourselves, peer.clone()).await;
        connectivity_state.add_active_connection(conn).await;
        connectivity
    }

    /// A `BaseNodeRequest` for the tip block from `source_peer`, signed by `origin` if given
    fn block_request(
        db: &BlockchainDatabase<TempDatabase>,
        source_peer: tari_comms::peer_manager::Peer,
        origin: Option<tari_comms::types::CommsPublicKey>,
    ) -> DomainMessage<Result<proto::BaseNodeServiceRequest, prost::DecodeError>> {
        let tip_hash = *db.fetch_last_chain_header().unwrap().hash();
        let request = proto::BaseNodeServiceRequest {
            request_key: 123,
            request: Some(NodeCommsRequest::GetBlockFromAllChains(tip_hash).try_into().unwrap()),
        };
        DomainMessage {
            source_peer,
            dht_header: create_peer_message(TariMessageType::BaseNodeRequest, vec![])
                .dht_header
                .clone(),
            authenticated_origin: origin,
            inner: Ok(request),
        }
    }

    /// Requests that are not signed, or signed by the peer that sent them, are answered to that peer
    #[tokio::test]
    async fn a_request_is_answered_to_its_source_peer() {
        use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
        use tari_comms_dht::{broadcast_strategy::BroadcastStrategy, outbound::mock::create_outbound_service_mock};

        let source = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        for origin in [None, Some(source.public_key().clone())] {
            let (inbound_nch, db, _mempool, _requests) = create_handlers();
            let (outbound_requester, outbound_mock) = create_outbound_service_mock();
            let outbound_state = outbound_mock.get_state();
            task::spawn(outbound_mock.run());
            let connectivity = connected_to(&source.to_peer()).await;

            let domain_msg = block_request(&db, source.to_peer(), origin);
            handle_incoming_request(
                inbound_nch,
                outbound_requester,
                connectivity,
                test_state_machine_handle(),
                domain_msg,
            )
            .await
            .unwrap();
            let (params, _body) = outbound_state.pop_call().await.unwrap();
            match params.broadcast_strategy {
                BroadcastStrategy::DirectPublicKey(public_key) => assert_eq!(*public_key, *source.public_key()),
                other => panic!("unexpected broadcast strategy {other}"),
            }
        }
    }

    /// A request signed by some other identity than the peer that sent it was relayed, and is dropped unanswered
    #[tokio::test]
    async fn a_request_signed_by_another_identity_is_dropped() {
        use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
        use tari_comms_dht::outbound::mock::create_outbound_service_mock;

        let (inbound_nch, db, _mempool, _requests) = create_handlers();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock();
        let outbound_state = outbound_mock.get_state();
        task::spawn(outbound_mock.run());
        let source_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        let connectivity = connected_to(&source_peer).await;
        let mut service = BaseNodeService::new(
            outbound_requester,
            inbound_nch,
            Duration::from_secs(60),
            test_state_machine_handle(),
            connectivity,
            BaseNodeStateMachineConfig::default(),
        );

        let origin = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let domain_msg = block_request(&db, source_peer.clone(), Some(origin.public_key().clone()));
        service.spawn_handle_incoming_request(domain_msg);
        // Dropped before it took one of the sender's slots
        assert_eq!(service.inflight_inbound_requests.pending_for(&source_peer.node_id), 0);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(outbound_state.call_count().await, 0);
    }

    /// If the connectivity manager cannot say whether the requester is connected, the reply is sent anyway
    #[tokio::test]
    async fn a_request_is_answered_when_the_connection_check_fails() {
        use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
        use tari_comms_dht::outbound::mock::create_outbound_service_mock;

        let (inbound_nch, db, _mempool, _requests) = create_handlers();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock();
        let outbound_state = outbound_mock.get_state();
        task::spawn(outbound_mock.run());
        // The connectivity manager is gone, so asking it for the connection fails
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        drop(connectivity_mock);

        let source_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        let domain_msg = block_request(&db, source_peer, None);
        handle_incoming_request(
            inbound_nch,
            outbound_requester,
            connectivity,
            test_state_machine_handle(),
            domain_msg,
        )
        .await
        .unwrap();
        outbound_state
            .wait_call_count(1, Duration::from_secs(5))
            .await
            .expect("the reply was not sent");
    }

    /// A reply is not sent to a requester that is no longer connected: sending it would dial the requester
    #[tokio::test]
    async fn a_request_from_a_peer_that_has_disconnected_is_not_answered() {
        use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
        use tari_comms_dht::outbound::mock::create_outbound_service_mock;

        let (inbound_nch, db, _mempool, _requests) = create_handlers();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock();
        let outbound_state = outbound_mock.get_state();
        task::spawn(outbound_mock.run());
        // The connectivity manager knows no connection to the requester
        let (connectivity, connectivity_mock) = create_connectivity_mock();
        connectivity_mock.spawn();

        let source_peer = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        let domain_msg = block_request(&db, source_peer, None);
        handle_incoming_request(
            inbound_nch,
            outbound_requester,
            connectivity,
            test_state_machine_handle(),
            domain_msg,
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(outbound_state.call_count().await, 0);
    }

    #[tokio::test]
    async fn requests_beyond_a_peers_in_flight_cap_are_dropped() {
        use tari_comms::{peer_manager::PeerFeatures, test_utils::node_identity::build_node_identity};
        use tari_comms_dht::outbound::mock::{MockBehaviour, ResponseType, create_outbound_service_mock};

        let (inbound_nch, db, _mempool, _requests) = create_handlers();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock();
        let outbound_state = outbound_mock.get_state();
        // Replies are slow to leave, so the handlers stay in flight
        outbound_state
            .set_behaviour(MockBehaviour {
                direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(60)),
                broadcast: ResponseType::Queued,
            })
            .await;
        task::spawn(outbound_mock.run());
        let peer_a = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        let peer_b = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
        // Peer A is connected, so its requests are answered (and held by the slow outbound mock)
        let connectivity = connected_to(&peer_a).await;
        let mut service = BaseNodeService::new(
            outbound_requester,
            inbound_nch,
            Duration::from_secs(60),
            test_state_machine_handle(),
            connectivity,
            BaseNodeStateMachineConfig::default(),
        );

        // An honest peer can be reconciling as many of our blocks at once as we accept from it
        const { assert!(MAX_INFLIGHT_REQUESTS_PER_PEER == MAX_PENDING_INBOUND_BLOCKS_PER_PEER) };
        // One more request than the cap: the 9th is dropped
        for _ in 0..=MAX_INFLIGHT_REQUESTS_PER_PEER {
            service.spawn_handle_incoming_request(block_request(&db, peer_a.clone(), None));
        }
        // The extra request was dropped, not queued
        assert_eq!(
            service.inflight_inbound_requests.pending_for(&peer_a.node_id),
            MAX_INFLIGHT_REQUESTS_PER_PEER
        );
        // Many other peers, each at their own cap, do not crowd out another peer: there is no total cap
        for _ in 0..64 {
            let other = build_node_identity(PeerFeatures::COMMUNICATION_NODE).to_peer();
            for _ in 0..MAX_INFLIGHT_REQUESTS_PER_PEER {
                service.spawn_handle_incoming_request(block_request(&db, other.clone(), None));
            }
            assert_eq!(
                service.inflight_inbound_requests.pending_for(&other.node_id),
                MAX_INFLIGHT_REQUESTS_PER_PEER
            );
        }
        service.spawn_handle_incoming_request(block_request(&db, peer_b.clone(), None));
        assert_eq!(service.inflight_inbound_requests.pending_for(&peer_b.node_id), 1);

        // The first reply is held in the outbound mock; the rest wait behind it, all still in flight
        outbound_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            service.inflight_inbound_requests.pending_for(&peer_a.node_id),
            MAX_INFLIGHT_REQUESTS_PER_PEER
        );
    }

    /// Runs `handle_outbound_request` with an outbound mock whose send of the request resolves to whatever is sent on
    /// the returned sender. Returns that sender, the request's reply, the timeout channel, and the handler task.
    async fn outbound_request_with_controlled_send() -> (
        tokio::sync::oneshot::Sender<Result<(), SendFailReason>>,
        tokio::sync::oneshot::Receiver<Result<NodeCommsResponse, CommsInterfaceError>>,
        mpsc::Receiver<RequestKey>,
        task::JoinHandle<()>,
    ) {
        use tari_comms::message::MessageTag;
        use tari_comms_dht::outbound::{MessageSendState, SendMessageResponse, mock::create_outbound_service_mock};
        use tokio::sync::oneshot;

        let (outbound_requester, outbound_mock) = create_outbound_service_mock();
        let outbound_state = outbound_mock.get_state();
        let (send_reply_tx, send_reply_rx) = oneshot::channel();
        outbound_state
            .set_next_response(SendMessageResponse::Queued(
                vec![MessageSendState::new(MessageTag::new(), send_reply_rx)].into(),
            ))
            .await;
        task::spawn(outbound_mock.run());

        let (timeout_sender, timeout_receiver) = mpsc::channel(10);
        let (reply_tx, reply_rx) = oneshot::channel();
        let handler = task::spawn(async move {
            handle_outbound_request(
                outbound_requester,
                WaitingRequests::new(),
                timeout_sender,
                reply_tx,
                NodeCommsRequest::GetBlockFromAllChains(FixedHash::zero()),
                Some(NodeId::default()),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        });
        (send_reply_tx, reply_rx, timeout_receiver, handler)
    }

    /// A request that never leaves this node because of us fails as a broadcast failure (no ban), not later as a
    /// timeout (a ban)
    #[tokio::test]
    async fn an_outbound_request_that_is_not_sent_fails_without_timing_out() {
        let (send_reply_tx, reply_rx, _timeout_receiver, handler) = outbound_request_with_controlled_send().await;
        send_reply_tx.send(Err(SendFailReason::QueueFull)).unwrap();
        handler.await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(5), reply_rx)
            .await
            .expect("an unsent request must fail straight away")
            .unwrap();
        assert!(matches!(reply, Err(CommsInterfaceError::BroadcastFailed)), "{reply:?}");
    }

    /// A request the peer did not take (e.g. it stopped reading) fails straight away with the error that blames it
    #[tokio::test]
    async fn an_outbound_request_the_peer_did_not_take_fails_with_a_ban() {
        let (send_reply_tx, reply_rx, _timeout_receiver, handler) = outbound_request_with_controlled_send().await;
        send_reply_tx.send(Err(SendFailReason::SendTimeout)).unwrap();
        handler.await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(5), reply_rx)
            .await
            .expect("a request the peer did not take must fail straight away")
            .unwrap();
        let err = reply.unwrap_err();
        assert!(matches!(err, CommsInterfaceError::RequestTimedOut), "{err:?}");
        assert!(err.get_ban_reason().is_some());
    }

    /// Every send failure is attributed one way or the other; this lists which way, so a change is deliberate
    #[test]
    fn send_failures_are_attributed() {
        let not_the_peers_fault = [
            SendFailReason::QueueFull,
            SendFailReason::MessageTooLarge,
            SendFailReason::Expired,
            SendFailReason::Dropped,
            SendFailReason::QueuedBehindTimeout,
            SendFailReason::ConnectionLost(2),
            SendFailReason::PeerDialFailed,
            SendFailReason::SubstreamOpenFailed,
            SendFailReason::SubstreamSendFailed,
        ];
        for reason in not_the_peers_fault {
            let err = request_send_failure(reason);
            assert!(matches!(err, CommsInterfaceError::BroadcastFailed), "{reason}");
            assert!(err.get_ban_reason().is_none(), "{reason}");
        }
        let the_peers_fault = [SendFailReason::SendTimeout, SendFailReason::MaxRetriesReached(2)];
        for reason in the_peers_fault {
            assert!(request_send_failure(reason).get_ban_reason().is_some(), "{reason}");
        }
    }

    /// A request whose dial failed is not the peer's fault (it may be our connectivity, or a peer that only accepts
    /// inbound connections), so it fails without a ban
    #[tokio::test]
    async fn an_outbound_request_whose_dial_failed_fails_without_a_ban() {
        let (send_reply_tx, reply_rx, _timeout_receiver, handler) = outbound_request_with_controlled_send().await;
        send_reply_tx.send(Err(SendFailReason::PeerDialFailed)).unwrap();
        handler.await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(5), reply_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(reply, Err(CommsInterfaceError::BroadcastFailed)), "{reply:?}");
    }

    /// The request timeout starts when the request has been sent, not when it is queued: a slow dial or a long wait in
    /// our own outbound queue must not make the peer time out
    #[tokio::test(start_paused = true)]
    async fn the_request_timeout_starts_once_the_request_is_sent() {
        let (send_reply_tx, mut reply_rx, mut timeout_receiver, handler) =
            outbound_request_with_controlled_send().await;
        // The send takes longer than the request timeout
        tokio::time::sleep(Duration::from_secs(90)).await;
        assert!(
            timeout_receiver.try_recv().is_err(),
            "the timeout started before the send completed"
        );
        assert!(reply_rx.try_recv().is_err());

        send_reply_tx.send(Ok(())).unwrap();
        handler.await.unwrap();
        // Now the timeout runs
        let timed_out = tokio::time::timeout(Duration::from_secs(61), timeout_receiver.recv())
            .await
            .expect("the request timeout did not start once the request was sent");
        assert!(timed_out.is_some());
    }

    #[tokio::test]
    async fn malformed_block_message_is_rejected_and_releases_the_decode_permit() {
        let (inbound_nch, _db, mempool, _requests) = create_handlers();
        let reconciliation_permits = mempool.available_reconciliation_permits();
        let decode_permits = decode_permits();

        let msg = create_peer_message(TariMessageType::NewBlock, vec![0xff; 64]);
        let mut block_hash = None;
        let err = handle_incoming_block(inbound_nch.clone(), decode_permits.clone(), msg, &mut block_hash)
            .await
            .unwrap_err();
        // Unchanged behaviour: an undecodable block message is an `InvalidBlockMessage`
        assert!(matches!(err, BaseNodeServiceError::InvalidBlockMessage(_)));
        // ... and there is no block to name
        assert!(block_hash.is_none());
        assert_eq!(decode_permits.available_permits(), MAX_CONCURRENT_BLOCK_DECODES);

        let body = prost::Message::encode_to_vec(&shared_protos::core::NewBlock::default());
        let msg = create_peer_message(TariMessageType::NewBlock, body);
        let err = handle_incoming_block(inbound_nch, decode_permits.clone(), msg, &mut None)
            .await
            .unwrap_err();
        assert!(matches!(err, BaseNodeServiceError::InvalidBlockMessage(_)));
        assert_eq!(decode_permits.available_permits(), MAX_CONCURRENT_BLOCK_DECODES);
        // Decoding never touches the mempool's reconciliation permits
        assert_eq!(mempool.available_reconciliation_permits(), reconciliation_permits);
    }

    /// A block that decodes but is then rejected is named by its hash, so the failure can be attributed to it
    #[tokio::test]
    async fn a_decoded_block_that_is_rejected_reports_its_hash() {
        let (inbound_nch, db, _mempool, _requests) = create_handlers();
        let tip = db.fetch_last_chain_header().unwrap();
        // Merge mining data that cannot be parsed, so the anti-spam gate rejects it before any network request
        let mut header = BlockHeader::new(tip.header().version);
        header.height = tip.height() + 1;
        header.prev_hash = *tip.hash();
        header.pow.pow_algo = PowAlgorithm::RandomXM;
        header.pow.pow_data = vec![0u8; 64].try_into().unwrap();
        let expected_hash = header.hash();
        let new_block = NewBlock {
            header,
            coinbase_kernels: vec![],
            coinbase_outputs: vec![],
            kernel_excess_sigs: vec![],
        };
        let body = prost::Message::encode_to_vec(&shared_protos::core::NewBlock::try_from(new_block).unwrap());
        let msg = create_peer_message(TariMessageType::NewBlock, body);

        let mut block_hash = None;
        let err = tokio::time::timeout(
            Duration::from_secs(30),
            handle_incoming_block(inbound_nch, decode_permits(), msg, &mut block_hash),
        )
        .await
        .expect("the block should be rejected without waiting on the network")
        .unwrap_err();
        assert!(err.get_ban_reason().is_some(), "{err:?}");
        assert_eq!(block_hash, Some(expected_hash));
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

        let task_permits = decode_permits.clone();
        let task = task::spawn(async move { handle_incoming_block(inbound_nch, task_permits, msg, &mut None).await });
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

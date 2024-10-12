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
use tari_network::{
    identity::PeerId,
    GossipMessage,
    GossipPublisher,
    GossipSubscription,
    NetworkHandle,
    NetworkingService,
    OutboundMessager,
    OutboundMessaging,
};
use tari_p2p::{
    message::{tari_message, DomainMessage, TariNodeMessageSpec},
    proto,
    proto::message::TariMessage,
};
use tari_service_framework::reply_channel::RequestContext;
use tari_utilities::hex::Hex;
use tokio::{
    sync::{mpsc, oneshot},
    task,
};

use crate::{
    base_node::{
        comms_interface::{CommsInterfaceError, InboundNodeCommsHandlers, NodeCommsRequest, NodeCommsResponse},
        service::error::BaseNodeServiceError,
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
};

const LOG_TARGET: &str = "c::bn::base_node_service::service";

/// A convenience struct to hold all the BaseNode streams
pub(super) struct BaseNodeStreams<SOutReq, SLocalReq, SLocalBlock> {
    /// `NodeCommsRequest` messages to send to a remote peer. If a specific peer is not provided, a random peer is
    /// chosen.
    pub outbound_request_stream: SOutReq,
    /// `BaseNodeRequest` and `BaseNodeResponse` messages received from external peers
    pub inbound_messages: mpsc::UnboundedReceiver<DomainMessage<TariMessage>>,
    /// `NewBlock` messages received from external peers
    pub block_subscription: GossipSubscription<proto::common::NewBlock>,
    /// Incoming local request messages from the LocalNodeCommsInterface and other local services
    pub local_request_stream: SLocalReq,
    /// The stream of blocks sent from local services `LocalCommsNodeInterface::submit_block` e.g. block sync and
    /// miner
    pub local_block_stream: SLocalBlock,
}

/// The Base Node Service is responsible for handling inbound requests and responses and for sending new requests to
/// remote Base Node Services.
pub(super) struct BaseNodeService<B> {
    outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
    inbound_nch: InboundNodeCommsHandlers<B>,
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: mpsc::Sender<RequestKey>,
    timeout_receiver_stream: Option<mpsc::Receiver<RequestKey>>,
    service_request_timeout: Duration,
    state_machine_handle: StateMachineHandle,
    network: NetworkHandle,
    base_node_config: BaseNodeStateMachineConfig,
}

impl<B> BaseNodeService<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
        inbound_nch: InboundNodeCommsHandlers<B>,
        service_request_timeout: Duration,
        state_machine_handle: StateMachineHandle,
        network: NetworkHandle,
        base_node_config: BaseNodeStateMachineConfig,
    ) -> Self {
        let (timeout_sender, timeout_receiver) = mpsc::channel(100);
        Self {
            outbound_messaging,
            inbound_nch,
            waiting_requests: WaitingRequests::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            service_request_timeout,
            state_machine_handle,
            network,
            base_node_config,
        }
    }

    pub async fn start<SOutReq, SLocalReq, SLocalBlock>(
        mut self,
        streams: BaseNodeStreams<SOutReq, SLocalReq, SLocalBlock>,
    ) -> Result<(), BaseNodeServiceError>
    where
        SOutReq:
            Stream<Item = RequestContext<(NodeCommsRequest, PeerId), Result<NodeCommsResponse, CommsInterfaceError>>>,
        SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
        SLocalBlock: Stream<Item = RequestContext<Block, Result<BlockHash, CommsInterfaceError>>>,
    {
        let outbound_request_stream = streams.outbound_request_stream.fuse();
        pin_mut!(outbound_request_stream);
        let mut inbound_messages = streams.inbound_messages;
        let mut block_subscription = streams.block_subscription;
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

                // Incoming request messages from the Comms layer
                Some(domain_msg) = inbound_messages.recv() => {
                    self.spawn_handle_incoming_request_or_response_message(domain_msg);
                },

                // Timeout events for waiting requests
                Some(timeout_request_key) = timeout_receiver_stream.recv() => {
                    self.spawn_handle_request_timeout(timeout_request_key);
                },

                // Incoming block messages from the network
                Some(Ok(msg)) = block_subscription.next_message() => {
                    self.spawn_handle_incoming_block(msg);
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
        request_context: RequestContext<(NodeCommsRequest, PeerId), Result<NodeCommsResponse, CommsInterfaceError>>,
    ) {
        let outbound_messaging = self.outbound_messaging.clone();
        let waiting_requests = self.waiting_requests.clone();
        let timeout_sender = self.timeout_sender.clone();
        let service_request_timeout = self.service_request_timeout;
        task::spawn(async move {
            let ((request, peer_id), reply_tx) = request_context.split();

            let result = handle_outbound_request(
                outbound_messaging,
                waiting_requests,
                timeout_sender,
                reply_tx,
                request,
                peer_id,
                service_request_timeout,
            )
            .await;

            if let Err(e) = result {
                error!(target: LOG_TARGET, "Failed to handle outbound request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_request_or_response_message(&self, domain_msg: DomainMessage<TariMessage>) {
        match domain_msg.inner().message.as_ref() {
            Some(tari_message::Message::BaseNodeRequest(_)) => {
                self.spawn_handle_incoming_request(domain_msg.map(|msg| msg.into_base_node_request().expect("checked")))
            },
            Some(tari_message::Message::BaseNodeResponse(_)) => self
                .spawn_handle_incoming_response(domain_msg.map(|msg| msg.into_base_node_response().expect("checked"))),
            Some(msg) => {
                // Not possible: Dispatcher would not have sent this service this message
                error!(target: LOG_TARGET, "Base Node Service received unexpected message type {}", msg.as_type().as_str_name())
            },
            None => {
                // Not possible: Dispatcher would not have sent this service this message
                error!(target: LOG_TARGET, "Base Node Service received empty")
            },
        }
    }

    fn spawn_handle_incoming_request(&self, domain_msg: DomainMessage<proto::base_node::BaseNodeServiceRequest>) {
        let inbound_nch = self.inbound_nch.clone();
        let outbound_messaging = self.outbound_messaging.clone();
        let state_machine_handle = self.state_machine_handle.clone();
        let mut network = self.network.clone();
        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            let result = handle_incoming_request(
                inbound_nch,
                outbound_messaging,
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
                    let _drop = network
                        .ban_peer(domain_msg.source_peer_id, ban_reason.reason, Some(duration))
                        .await
                        .map_err(|e| error!(target: LOG_TARGET, "Failed to ban peer: {:?}", e));
                }
                error!(target: LOG_TARGET, "Failed to handle incoming request message: {:?}", e);
            }
        });
    }

    fn spawn_handle_incoming_response(&self, domain_msg: DomainMessage<proto::base_node::BaseNodeServiceResponse>) {
        let waiting_requests = self.waiting_requests.clone();
        let mut network = self.network.clone();

        let short_ban = self.base_node_config.blockchain_sync_config.short_ban_period;
        let long_ban = self.base_node_config.blockchain_sync_config.ban_period;
        task::spawn(async move {
            let source_peer = domain_msg.source_peer_id;
            let result = handle_incoming_response(waiting_requests, domain_msg).await;

            if let Err(e) = result {
                if let Some(ban_reason) = e.get_ban_reason() {
                    let duration = match ban_reason.ban_duration {
                        BanPeriod::Short => short_ban,
                        BanPeriod::Long => long_ban,
                    };
                    let _drop = network
                        .ban_peer(source_peer, ban_reason.reason, Some(duration))
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

    fn spawn_handle_incoming_block(&self, new_block: GossipMessage<proto::common::NewBlock>) {
        // Determine if we are bootstrapped
        let status_watch = self.state_machine_handle.get_status_info_watch();

        if !(status_watch.borrow()).bootstrapped {
            debug!(
                target: LOG_TARGET,
                "Propagated block from peer `{}` not processed while busy with initial sync.",
                new_block.origin_or_source()
            );
            return;
        }
        let inbound_nch = self.inbound_nch.clone();
        let mut network = self.network.clone();
        let source_peer = new_block.origin_or_source();
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
                        let _drop = network
                            .ban_peer(source_peer, ban_reason.reason, Some(duration))
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
    mut outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
    state_machine_handle: StateMachineHandle,
    domain_request_msg: DomainMessage<proto::base_node::BaseNodeServiceRequest>,
) -> Result<(), BaseNodeServiceError> {
    let peer_id = domain_request_msg.source_peer_id;
    let inner_msg = domain_request_msg.into_payload();

    let request = inner_msg.request.ok_or_else(|| {
        BaseNodeServiceError::InvalidRequest("Received invalid base node request with no inner request".to_string())
    })?;

    let request = request.try_into().map_err(|e| {
        BaseNodeServiceError::InvalidRequest(format!(
            "Received invalid base node request. It could not be converted:  {}",
            e
        ))
    })?;

    let response = inbound_nch.handle_request(request).await?;

    // Determine if we are synced
    let status_watch = state_machine_handle.get_status_info_watch();
    let is_synced = match status_watch.borrow().state_info {
        StateInfo::Listening(li) => li.is_synced(),
        _ => false,
    };

    let message = proto::base_node::BaseNodeServiceResponse {
        request_key: inner_msg.request_key,
        response: Some(response.try_into().map_err(BaseNodeServiceError::InvalidResponse)?),
        is_synced,
    };

    trace!(
        target: LOG_TARGET,
        "Attempting outbound message in response to inbound request ({})",
        inner_msg.request_key
    );

    outbound_messaging.send_message(peer_id, message).await?;

    Ok(())
}

async fn handle_incoming_response(
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    domain_msg: DomainMessage<proto::base_node::BaseNodeServiceResponse>,
) -> Result<(), BaseNodeServiceError> {
    let proto::base_node::BaseNodeServiceResponse {
        request_key,
        response,
        is_synced,
    } = domain_msg.into_payload();
    let response = response
        .ok_or_else(|| BaseNodeServiceError::InvalidResponse("Received an empty base node response".to_string()))?;

    let response = NodeCommsResponse::try_from(response)
        .map_err(|e| BaseNodeServiceError::InvalidResponse(format!("Received an invalid base node response: {e}")))?;

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
    mut outbound_messaging: OutboundMessaging<TariNodeMessageSpec>,
    waiting_requests: WaitingRequests<Result<NodeCommsResponse, CommsInterfaceError>>,
    timeout_sender: mpsc::Sender<RequestKey>,
    reply_tx: oneshot::Sender<Result<NodeCommsResponse, CommsInterfaceError>>,
    request: NodeCommsRequest,
    peer_id: PeerId,
    service_request_timeout: Duration,
) -> Result<(), CommsInterfaceError> {
    debug!("Node request:{} to {}", request, peer_id);
    let request_key = generate_request_key(&mut OsRng);
    let service_request = proto::base_node::BaseNodeServiceRequest {
        request_key,
        request: Some(request.try_into().map_err(CommsInterfaceError::InternalError)?),
    };

    trace!(target: LOG_TARGET, "Attempting outbound request ({})", request_key);
    let result = outbound_messaging.send_message(peer_id, service_request).await;

    match result {
        Ok(()) => {
            // Wait for matching responses to arrive
            waiting_requests.insert(request_key, reply_tx).await;
            // Spawn timeout for waiting_request
            trace!(
                target: LOG_TARGET,
                "Timeout for service request ... ({}) set at {:?}",
                request_key,
                service_request_timeout
            );
            spawn_request_timeout(timeout_sender, request_key, service_request_timeout)
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

fn spawn_request_timeout(timeout_sender: mpsc::Sender<RequestKey>, request_key: RequestKey, timeout: Duration) {
    task::spawn(async move {
        tokio::time::sleep(timeout).await;
        let _ = timeout_sender.send(request_key).await;
    });
}

async fn handle_incoming_block<B: BlockchainBackend + 'static>(
    mut inbound_nch: InboundNodeCommsHandlers<B>,
    domain_block_msg: GossipMessage<proto::common::NewBlock>,
) -> Result<(), BaseNodeServiceError> {
    let GossipMessage::<_> {
        source,
        origin,
        message,
        ..
    } = domain_block_msg;

    let from = origin.unwrap_or(source);

    let new_block = NewBlock::try_from(message).map_err(BaseNodeServiceError::InvalidBlockMessage)?;
    debug!(
        target: LOG_TARGET,
        "New candidate block with hash `{}` received from `{}`.",
        new_block.header.hash().to_hex(),
        from
    );

    inbound_nch.handle_new_block_message(new_block, from).await?;

    Ok(())
}

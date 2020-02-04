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
            CommsInterfaceError,
            InboundNodeCommsHandlers,
            NodeCommsRequest,
            NodeCommsRequestType,
            NodeCommsResponse,
        },
        consts::{BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION, BASE_NODE_SERVICE_REQUEST_TIMEOUT},
        proto,
        service::{
            error::BaseNodeServiceError,
            service_request::{generate_request_key, RequestKey, WaitingRequest},
        },
    },
    blocks::Block,
    chain_storage::BlockchainBackend,
    proto::core::Block as ProtoBlock,
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
use std::{collections::HashMap, convert::TryInto, time::Duration};
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageParams},
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::RequestContext;
use tokio::runtime;

const LOG_TARGET: &'static str = "tari_core::base_node::base_node_service::service";

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
    outbound_request_stream: SOutReq,
    outbound_block_stream: UnboundedReceiver<(Block, Vec<CommsPublicKey>)>,
    inbound_request_stream: SInReq,
    inbound_response_stream: SInRes,
    inbound_block_stream: SBlockIn,
    local_request_stream: SLocalReq,
    local_block_stream: SLocalBlock,
}

impl<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>
    BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>
where
    SOutReq: Stream<
        Item = RequestContext<
            (NodeCommsRequest, NodeCommsRequestType),
            Result<Vec<NodeCommsResponse>, CommsInterfaceError>,
        >,
    >,
    SInReq: Stream<Item = DomainMessage<proto::BaseNodeServiceRequest>>,
    SInRes: Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>>,
    SBlockIn: Stream<Item = DomainMessage<Block>>,
    SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
    SLocalBlock: Stream<Item = RequestContext<Block, Result<(), CommsInterfaceError>>>,
{
    pub fn new(
        outbound_request_stream: SOutReq,
        outbound_block_stream: UnboundedReceiver<(Block, Vec<CommsPublicKey>)>,
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
pub struct BaseNodeService<B: BlockchainBackend> {
    executor: runtime::Handle,
    outbound_message_service: OutboundMessageRequester,
    inbound_nch: InboundNodeCommsHandlers<B>,
    waiting_requests: HashMap<RequestKey, WaitingRequest>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: BaseNodeServiceConfig,
}

impl<B> BaseNodeService<B>
where B: BlockchainBackend + 'static
{
    pub fn new(
        executor: runtime::Handle,
        outbound_message_service: OutboundMessageRequester,
        inbound_nch: InboundNodeCommsHandlers<B>,
        config: BaseNodeServiceConfig,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            executor,
            outbound_message_service,
            inbound_nch,
            waiting_requests: HashMap::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
        }
    }

    pub async fn start<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>(
        mut self,
        streams: BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn, SLocalReq, SLocalBlock>,
    ) -> Result<(), BaseNodeServiceError>
    where
        SOutReq: Stream<
            Item = RequestContext<
                (NodeCommsRequest, NodeCommsRequestType),
                Result<Vec<NodeCommsResponse>, CommsInterfaceError>,
            >,
        >,
        SInReq: Stream<Item = DomainMessage<proto::BaseNodeServiceRequest>>,
        SInRes: Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>>,
        SBlockIn: Stream<Item = DomainMessage<Block>>,
        SLocalReq: Stream<Item = RequestContext<NodeCommsRequest, Result<NodeCommsResponse, CommsInterfaceError>>>,
        SLocalBlock: Stream<Item = RequestContext<Block, Result<(), CommsInterfaceError>>>,
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
                    let ((request,request_type), reply_tx) = outbound_request_context.split();
                    let _ = self.handle_outbound_request(reply_tx,request,request_type).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle outbound request message: {:?}", err);
                        Err(err)
                    });
                },

                // Outbound block messages from the OutboundNodeCommsInterface
                outbound_block_context = outbound_block_stream.select_next_some() => {
                    let (block, excluded_peers) = outbound_block_context;
                    let _ = self.handle_outbound_block(block,excluded_peers).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle outbound block message {:?}",err);
                        Err(err)
                    });
                },

                // Incoming request messages from the Comms layer
                domain_msg = inbound_request_stream.select_next_some() => {
                    let _ = self.handle_incoming_request(domain_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming request message: {:?}", err);
                        Err(err)
                    });
                },

                // Incoming response messages from the Comms layer
                domain_msg = inbound_response_stream.select_next_some() => {
                    let _ = self.handle_incoming_response(domain_msg.into_inner()).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming response message: {:?}", err);
                        Err(err)
                    });
                },

                // Timeout events for waiting requests
                timeout_request_key = timeout_receiver_stream.select_next_some() => {
                    let _ =self.handle_request_timeout(timeout_request_key).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle request timeout event: {:?}", err);
                        Err(err)
                    });
                },

                // Incoming block messages from the Comms layer
                block_msg = inbound_block_stream.select_next_some() => {
                    let _ = self.handle_incoming_block(block_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming block message: {:?}", err);
                        Err(err)
                    });
                }

                // Incoming local request messages from the LocalNodeCommsInterface and other local services
                local_request_context = local_request_stream.select_next_some() => {
                    let (request, reply_tx) = local_request_context.split();
                    let _ = reply_tx.send(self.inbound_nch.handle_request(&request.into()).await).or_else(|err| {
                        error!(target: LOG_TARGET, "BaseNodeService failed to send reply to local request {:?}",err);
                        Err(err)
                    });
                },

                 // Incoming local block messages from the LocalNodeCommsInterface and other local services
                local_block_context = local_block_stream.select_next_some() => {
                    let (block, reply_tx) = local_block_context.split();
                    let _ = reply_tx.send(self.inbound_nch.handle_block(&block.into(),None).await).or_else(|err| {
                        error!(target: LOG_TARGET, "BaseNodeService failed to send reply to local block submitter {:?}",err);
                        Err(err)
                    });
                },

                complete => {
                    info!(target: LOG_TARGET, "Base Node service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming_request(
        &mut self,
        domain_request_msg: DomainMessage<proto::BaseNodeServiceRequest>,
    ) -> Result<(), BaseNodeServiceError>
    {
        let (origin_public_key, inner_msg) = domain_request_msg.into_origin_and_inner();

        // Convert proto::BaseNodeServiceRequest to a BaseNodeServiceRequest
        let request = inner_msg.request.ok_or(BaseNodeServiceError::InvalidRequest(
            "Received invalid base node request".to_string(),
        ))?;

        let response = self
            .inbound_nch
            .handle_request(
                &request
                    .try_into()
                    .map_err(|e| BaseNodeServiceError::InvalidRequest(e))?,
            )
            .await?;

        let message = proto::BaseNodeServiceResponse {
            request_key: inner_msg.request_key,
            response: Some(response.into()),
        };

        self.outbound_message_service
            .send_direct(
                origin_public_key,
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::BaseNodeResponse, message),
            )
            .await?;

        Ok(())
    }

    async fn handle_incoming_response(
        &mut self,
        incoming_response: proto::BaseNodeServiceResponse,
    ) -> Result<(), BaseNodeServiceError>
    {
        let proto::BaseNodeServiceResponse { request_key, response } = incoming_response;

        let mut finalize_request = false;
        match self.waiting_requests.get_mut(&request_key) {
            Some(waiting_request) => {
                let response =
                    response
                        .and_then(|r| r.try_into().ok())
                        .ok_or(BaseNodeServiceError::InvalidResponse(
                            "Received an invalid base node response".to_string(),
                        ))?;
                waiting_request.received_responses.push(response);
                finalize_request = waiting_request.received_responses.len() >= waiting_request.desired_resp_count;
            },
            None => {
                info!(target: LOG_TARGET, "Discard incoming unmatched response");
            },
        }

        if finalize_request {
            if let Some(waiting_request) = self.waiting_requests.remove(&request_key) {
                let WaitingRequest {
                    mut reply_tx,
                    received_responses,
                    ..
                } = waiting_request;
                if let Some(reply_tx) = reply_tx.take() {
                    let _ = reply_tx.send(Ok(received_responses).or_else(|resp| {
                        error!(target: LOG_TARGET, "Failed to send outbound request from Base Node");
                        Err(resp)
                    }));
                }
            }
        }

        Ok(())
    }

    async fn handle_outbound_request(
        &mut self,
        reply_tx: OneshotSender<Result<Vec<NodeCommsResponse>, CommsInterfaceError>>,
        request: NodeCommsRequest,
        request_type: NodeCommsRequestType,
    ) -> Result<(), CommsInterfaceError>
    {
        let request_key = generate_request_key(&mut OsRng);
        let service_request = proto::BaseNodeServiceRequest {
            request_key,
            request: Some(request.into()),
        };

        let mut send_msg_params = SendMessageParams::new();

        match request_type {
            NodeCommsRequestType::Single => send_msg_params.random(1),
            NodeCommsRequestType::Many => send_msg_params.neighbours(Vec::new()),
        };

        let send_result = self
            .outbound_message_service
            .send_message(
                send_msg_params.finish(),
                OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
            )
            .await
            .map_err(|e| CommsInterfaceError::OutboundMessageService(e.to_string()))?;

        match send_result.resolve_ok().await {
            Some(tags) if tags.len() == 0 => {
                let _ = reply_tx
                    .send(Err(CommsInterfaceError::NoBootstrapNodesConfigured))
                    .or_else(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send outbound request from Base Node as no bootstrap nodes were configured"
                        );
                        Err(resp)
                    });
            },
            Some(tags) => {
                let dest_count = tags.len();
                // Wait for matching responses to arrive
                self.waiting_requests.insert(request_key, WaitingRequest {
                    reply_tx: Some(reply_tx),
                    received_responses: Vec::new(),
                    desired_resp_count: (BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION * dest_count as f32).ceil()
                        as usize,
                });
                // Spawn timeout for waiting_request
                self.spawn_request_timeout(request_key, self.config.request_timeout)
                    .await;
            },
            None => {
                let _ = reply_tx
                    .send(Err(CommsInterfaceError::BroadcastFailed))
                    .or_else(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send outbound request from Base Node because DHT outbound broadcast failed"
                        );
                        Err(resp)
                    });
            },
        }
        Ok(())
    }

    async fn handle_outbound_block(
        &mut self,
        block: Block,
        exclude_peers: Vec<CommsPublicKey>,
    ) -> Result<(), CommsInterfaceError>
    {
        self.outbound_message_service
            .propagate(
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForPeer,
                exclude_peers,
                OutboundDomainMessage::new(TariMessageType::NewBlock, ProtoBlock::from(block)),
            )
            .await
            .map_err(|e| CommsInterfaceError::OutboundMessageService(e.to_string()))
            .map(|_| ())
    }

    async fn handle_request_timeout(&mut self, request_key: RequestKey) -> Result<(), CommsInterfaceError> {
        if let Some(mut waiting_request) = self.waiting_requests.remove(&request_key) {
            if let Some(reply_tx) = waiting_request.reply_tx.take() {
                let reply_msg = if waiting_request.received_responses.len() >= 1 {
                    Ok(waiting_request.received_responses.clone())
                } else {
                    Err(CommsInterfaceError::RequestTimedOut)
                };
                let _ = reply_tx.send(reply_msg.or_else(|resp| {
                    error!(target: LOG_TARGET, "Failed to send outbound request from Base Node");
                    Err(resp)
                }));
            }
        }
        Ok(())
    }

    async fn spawn_request_timeout(&self, request_key: RequestKey, timeout: Duration) {
        let mut timeout_sender = self.timeout_sender.clone();
        self.executor.spawn(async move {
            tokio::time::delay_for(timeout).await;
            let _ = timeout_sender.send(request_key).await;
        });
    }

    async fn handle_incoming_block(
        &mut self,
        domain_block_msg: DomainMessage<Block>,
    ) -> Result<(), BaseNodeServiceError>
    {
        let DomainMessage::<_> { source_peer, inner, .. } = domain_block_msg;

        info!("New candidate block received for height {}", inner.header.height);

        self.inbound_nch
            .handle_block(&inner.clone().into(), Some(source_peer.public_key))
            .await?;

        // TODO - retain peer info for stats and potential banning for sending invalid blocks

        Ok(())
    }
}

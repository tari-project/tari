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
            InboundNodeCommsInterface,
            NodeCommsRequest,
            NodeCommsRequestType,
            NodeCommsResponse,
        },
        proto,
        service::{
            error::BaseNodeServiceError,
            service_request::{generate_request_key, RequestKey, WaitingRequest},
        },
    },
    blocks::Block,
    chain_storage::BlockchainBackend,
    consts::{BASE_NODE_RNG, BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION, BASE_NODE_SERVICE_REQUEST_TIMEOUT},
};
use futures::{
    channel::{
        mpsc::{channel, Receiver, Sender},
        oneshot::Sender as OneshotSender,
    },
    future::Fuse,
    pin_mut,
    stream::StreamExt,
    SinkExt,
    Stream,
};
use log::*;
use std::{
    collections::HashMap,
    convert::TryInto,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{BroadcastStrategy, OutboundEncryption, OutboundMessageRequester},
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::RequestContext;
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "tari_core::base_node::base_node_service::service";

// TODO: Add streams for BlockchainMessage::NewBlock and BlockchainMessage::Transaction

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
pub struct BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn> {
    outbound_request_stream: SOutReq,
    inbound_request_stream: SInReq,
    inbound_response_stream: SInRes,
    inbound_block_stream: SBlockIn,
}

impl<SOutReq, SInReq, SInRes, SBlockIn> BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn>
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
{
    pub fn new(
        outbound_request_stream: SOutReq,
        inbound_request_stream: SInReq,
        inbound_response_stream: SInRes,
        inbound_block_stream: SBlockIn,
    ) -> Self
    {
        BaseNodeStreams {
            outbound_request_stream,
            inbound_request_stream,
            inbound_response_stream,
            inbound_block_stream,
        }
    }
}

/// The Base Node Service is responsible for handling inbound requests and responses and for sending new requests to
/// remote Base Node Services.
pub struct BaseNodeService<B: BlockchainBackend> {
    executor: TaskExecutor,
    outbound_message_service: OutboundMessageRequester,
    inbound_nci: Arc<InboundNodeCommsInterface<B>>,
    waiting_requests: HashMap<RequestKey, WaitingRequest>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: BaseNodeServiceConfig,
}

impl<B> BaseNodeService<B>
where B: BlockchainBackend
{
    pub fn new(
        executor: TaskExecutor,
        outbound_message_service: OutboundMessageRequester,
        inbound_nci: Arc<InboundNodeCommsInterface<B>>,
        config: BaseNodeServiceConfig,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            executor,
            outbound_message_service,
            inbound_nci,
            waiting_requests: HashMap::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
        }
    }

    pub async fn start<SOutReq, SInReq, SInRes, SBlockIn>(
        mut self,
        streams: BaseNodeStreams<SOutReq, SInReq, SInRes, SBlockIn>,
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
    {
        let outbound_request_stream = streams.outbound_request_stream.fuse();
        pin_mut!(outbound_request_stream);
        let inbound_request_stream = streams.inbound_request_stream.fuse();
        pin_mut!(inbound_request_stream);
        let inbound_response_stream = streams.inbound_response_stream.fuse();
        pin_mut!(inbound_response_stream);
        let inbound_block_stream = streams.inbound_block_stream.fuse();
        pin_mut!(inbound_block_stream);
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

                block_msg = inbound_block_stream.select_next_some() => {
                    // TODO - retain peer info for stats and potential banning for sending invalid blocks
                    let block = block_msg.into_inner();
                    info!("New candidate block received for height {}", block.header.height)
                    // TODO - process the block
                }

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
        let DomainMessage::<_> {
            origin_pubkey, inner, ..
        } = domain_request_msg;

        // Convert proto::BaseNodeServiceRequest to a BaseNodeServiceRequest
        let request = inner.request.ok_or(BaseNodeServiceError::InvalidRequest(
            "Received invalid base node request".to_string(),
        ))?;

        let response = self.inbound_nci.handle_request(&request.into()).await?;

        let message = proto::BaseNodeServiceResponse {
            request_key: inner.request_key,
            response: Some(response.into()),
        };

        self.outbound_message_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(origin_pubkey.clone()),
                NodeDestination::PublicKey(origin_pubkey),
                OutboundEncryption::EncryptForDestination,
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
        let request_key = BASE_NODE_RNG.with(|rng| generate_request_key(&mut *rng.borrow_mut()));
        let service_request = proto::BaseNodeServiceRequest {
            request_key,
            request: Some(request.into()),
        };

        let broadcast_strategy = match request_type {
            NodeCommsRequestType::Single => BroadcastStrategy::Random(1),
            NodeCommsRequestType::Many => BroadcastStrategy::Neighbours(Box::new(Vec::new())),
        };

        let dest_count = self
            .outbound_message_service
            .send_message(
                broadcast_strategy,
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request),
            )
            .await
            .map_err(|e| CommsInterfaceError::OutboundMessageService(e.to_string()))?;

        if dest_count > 0 {
            // Wait for matching responses to arrive
            self.waiting_requests.insert(request_key, WaitingRequest {
                reply_tx: Some(reply_tx),
                received_responses: Vec::new(),
                desired_resp_count: (BASE_NODE_SERVICE_DESIRED_RESPONSE_FRACTION * dest_count as f32).ceil() as usize,
            });
            // Spawn timeout for waiting_request
            self.spawn_request_timeout(request_key, self.config.request_timeout)
                .await;
        } else {
            let _ = reply_tx.send(Err(CommsInterfaceError::NoBootstrapNodesConfigured).or_else(|resp| {
                error!(
                    target: LOG_TARGET,
                    "Failed to send outbound request from Base Node as no bootstrap nodes were configured"
                );
                Err(resp)
            }));
        }
        Ok(())
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
            tokio::timer::delay(Instant::now() + timeout).await;
            let _ = timeout_sender.send(request_key).await;
        });
    }
}

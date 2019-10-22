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
        comms_interface::{CommsInterfaceError, InboundNodeCommsInterface, NodeCommsRequest, NodeCommsResponse},
        service::{
            error::BaseNodeServiceError,
            service_request::{generate_request_key, BaseNodeServiceRequest, RequestKey, WaitingRequest},
            service_response::BaseNodeServiceResponse,
        },
    },
    chain_storage::BlockchainBackend,
    consts::{
        BASE_NODE_RNG,
        BASE_NODE_SERVICE_BROADCAST_PEER_COUNT,
        BASE_NODE_SERVICE_DESIRED_RESPONSE_COUNT,
        BASE_NODE_SERVICE_REQUEST_TIMEOUT,
    },
};
use futures::{
    channel::{
        mpsc::{channel, Receiver, Sender},
        oneshot::Sender as OneshotSender,
    },
    pin_mut,
    stream::StreamExt,
    SinkExt,
    Stream,
};
use log::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::peer_manager::NodeIdentity;
use tari_comms_dht::{
    envelope::NodeDestination,
    outbound::{BroadcastClosestRequest, BroadcastStrategy, OutboundEncryption, OutboundMessageRequester},
};
use tari_p2p::{
    domain_message::DomainMessage,
    tari_message::{BlockchainMessage, TariMessageType},
};
use tari_service_framework::RequestContext;
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "tari_core::base_node::base_node_service::service";

// TODO: Add streams for BlockchainMessage::NewBlock and BlockchainMessage::Transaction

/// Configuration for the BaseNodeService.
#[derive(Clone, Copy)]
pub struct BaseNodeServiceConfig {
    /// The allocated waiting time for a request waiting for service responses from remote base nodes.
    pub request_timeout: Duration,
    /// The number of remote peers that Base Node Service requests are sent to.
    pub broadcast_peer_count: usize,
    /// The number of responses that need to be received for a corresponding service request to be finalize.
    pub desired_response_count: usize,
}

impl Default for BaseNodeServiceConfig {
    fn default() -> Self {
        Self {
            request_timeout: BASE_NODE_SERVICE_REQUEST_TIMEOUT,
            broadcast_peer_count: BASE_NODE_SERVICE_BROADCAST_PEER_COUNT,
            desired_response_count: BASE_NODE_SERVICE_DESIRED_RESPONSE_COUNT,
        }
    }
}

/// The Base Node Service is responsible for handling inbound requests and responses and for sending new requests to
/// remote Base Node Services.
pub struct BaseNodeService<TOutbReqStream, TInbReqStream, TInbRespStream, TChainBackend>
where TChainBackend: BlockchainBackend
{
    executor: TaskExecutor,
    outbound_request_stream: Option<TOutbReqStream>,
    inbound_request_stream: Option<TInbReqStream>,
    inbound_response_stream: Option<TInbRespStream>,
    outbound_message_service: OutboundMessageRequester,
    node_identity: Arc<NodeIdentity>,
    inbound_nci: Arc<InboundNodeCommsInterface<TChainBackend>>,
    waiting_requests: HashMap<RequestKey, WaitingRequest>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: BaseNodeServiceConfig,
}

impl<TOutbReqStream, TInbReqStream, TInbRespStream, TChainBackend>
    BaseNodeService<TOutbReqStream, TInbReqStream, TInbRespStream, TChainBackend>
where
    TOutbReqStream:
        Stream<Item = RequestContext<NodeCommsRequest, Result<Vec<NodeCommsResponse>, CommsInterfaceError>>>,
    TInbReqStream: Stream<Item = DomainMessage<BaseNodeServiceRequest>>,
    TInbRespStream: Stream<Item = DomainMessage<BaseNodeServiceResponse>>,
    TChainBackend: BlockchainBackend,
{
    pub fn new(
        executor: TaskExecutor,
        outbound_request_stream: TOutbReqStream,
        inbound_request_stream: TInbReqStream,
        inbound_response_stream: TInbRespStream,
        outbound_message_service: OutboundMessageRequester,
        node_identity: Arc<NodeIdentity>,
        inbound_nci: Arc<InboundNodeCommsInterface<TChainBackend>>,
        config: BaseNodeServiceConfig,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            executor,
            outbound_request_stream: Some(outbound_request_stream),
            inbound_request_stream: Some(inbound_request_stream),
            inbound_response_stream: Some(inbound_response_stream),
            outbound_message_service,
            node_identity,
            inbound_nci,
            waiting_requests: HashMap::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
        }
    }

    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        let outbound_request_stream = self
            .outbound_request_stream
            .take()
            .expect("Base Node Service initialized without outbound_request_stream")
            .fuse();
        pin_mut!(outbound_request_stream);
        let inbound_request_stream = self
            .inbound_request_stream
            .take()
            .expect("Base Node Service initialized without inbound_request_stream")
            .fuse();
        pin_mut!(inbound_request_stream);
        let inbound_response_stream = self
            .inbound_response_stream
            .take()
            .expect("Base Node Service initialized without inbound_response_stream")
            .fuse();
        pin_mut!(inbound_response_stream);
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
                    let (request, reply_tx) = outbound_request_context.split();
                    let _ = self.handle_outbound_request(reply_tx,request).await.or_else(|err| {
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
                    let _ = self.handle_incoming_response(&domain_msg.inner()).await.or_else(|err| {
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
        domain_request_msg: DomainMessage<BaseNodeServiceRequest>,
    ) -> Result<(), BaseNodeServiceError>
    {
        self.outbound_message_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(domain_request_msg.origin_pubkey.clone()),
                NodeDestination::PublicKey(domain_request_msg.origin_pubkey.clone()),
                OutboundEncryption::EncryptForDestination,
                TariMessageType::new(BlockchainMessage::BaseNodeResponse),
                BaseNodeServiceResponse {
                    request_key: domain_request_msg.inner().request_key,
                    response: self
                        .inbound_nci
                        .handle_request(&domain_request_msg.inner().request)
                        .await?,
                },
            )
            .await?;
        Ok(())
    }

    async fn handle_incoming_response(
        &mut self,
        incoming_response: &BaseNodeServiceResponse,
    ) -> Result<(), BaseNodeServiceError>
    {
        let mut finalize_request = false;
        match self.waiting_requests.get_mut(&incoming_response.request_key) {
            Some(waiting_request) => {
                waiting_request
                    .received_responses
                    .push(incoming_response.response.clone());
                finalize_request = waiting_request.received_responses.len() >= waiting_request.desired_resp_count;
            },
            None => {
                info!(target: LOG_TARGET, "Discard incoming unmatched response");
            },
        }

        if finalize_request {
            if let Some(waiting_request) = self.waiting_requests.remove(&incoming_response.request_key) {
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
    ) -> Result<(), CommsInterfaceError>
    {
        let request_key = BASE_NODE_RNG.with(|rng| generate_request_key(&mut *rng.borrow_mut()));
        let service_request = BaseNodeServiceRequest {
            request_key: request_key.clone(),
            request,
        };
        self.outbound_message_service
            .send_message(
                BroadcastStrategy::Closest(BroadcastClosestRequest {
                    n: self.config.broadcast_peer_count,
                    node_id: self.node_identity.identity.node_id.clone(),
                    excluded_peers: Vec::new(),
                }),
                NodeDestination::NodeId(self.node_identity.identity.node_id.clone()),
                OutboundEncryption::None,
                TariMessageType::new(BlockchainMessage::BaseNodeRequest),
                service_request,
            )
            .await
            .map_err(|_| CommsInterfaceError::UnexpectedApiResponse)?;

        // Wait for matching responses to arrive
        self.waiting_requests.insert(request_key, WaitingRequest {
            reply_tx: Some(reply_tx),
            received_responses: Vec::new(),
            desired_resp_count: self.config.desired_response_count.clone(),
        });
        // Spawn timeout for waiting_request
        self.spawn_request_timeout(request_key, self.config.request_timeout)
            .await;
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

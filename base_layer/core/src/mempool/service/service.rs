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
    base_node::comms_interface::BlockEvent,
    chain_storage::BlockchainBackend,
    mempool::{
        proto,
        service::{
            error::MempoolServiceError,
            inbound_handlers::MempoolInboundHandlers,
            request::{generate_request_key, RequestKey},
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
use std::{collections::HashMap, convert::TryInto, time::Duration};
use tari_broadcast_channel::Subscriber;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester},
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::RequestContext;
use tokio::runtime;

const LOG_TARGET: &str = "c::mempool::service::service";

/// A convenience struct to hold all the Mempool service streams
pub struct MempoolStreams<SOutReq, SInReq, SInRes, STxIn> {
    outbound_request_stream: SOutReq,
    outbound_tx_stream: UnboundedReceiver<(Transaction, Vec<CommsPublicKey>)>,
    inbound_request_stream: SInReq,
    inbound_response_stream: SInRes,
    inbound_transaction_stream: STxIn,
    block_event_stream: Subscriber<BlockEvent>,
}

impl<SOutReq, SInReq, SInRes, STxIn> MempoolStreams<SOutReq, SInReq, SInRes, STxIn>
where
    SOutReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
    SInReq: Stream<Item = DomainMessage<proto::MempoolServiceRequest>>,
    SInRes: Stream<Item = DomainMessage<proto::MempoolServiceResponse>>,
    STxIn: Stream<Item = DomainMessage<Transaction>>,
{
    pub fn new(
        outbound_request_stream: SOutReq,
        outbound_tx_stream: UnboundedReceiver<(Transaction, Vec<CommsPublicKey>)>,
        inbound_request_stream: SInReq,
        inbound_response_stream: SInRes,
        inbound_transaction_stream: STxIn,
        block_event_stream: Subscriber<BlockEvent>,
    ) -> Self
    {
        Self {
            outbound_request_stream,
            outbound_tx_stream,
            inbound_request_stream,
            inbound_response_stream,
            inbound_transaction_stream,
            block_event_stream,
        }
    }
}

/// The Mempool Service is responsible for handling inbound requests and responses and for sending new requests to the
/// Mempools of remote Base nodes.
pub struct MempoolService<B: BlockchainBackend> {
    executor: runtime::Handle,
    outbound_message_service: OutboundMessageRequester,
    inbound_handlers: MempoolInboundHandlers<B>,
    waiting_requests: HashMap<RequestKey, Option<OneshotSender<Result<MempoolResponse, MempoolServiceError>>>>,
    timeout_sender: Sender<RequestKey>,
    timeout_receiver_stream: Option<Receiver<RequestKey>>,
    config: MempoolServiceConfig,
}

impl<B> MempoolService<B>
where B: BlockchainBackend
{
    pub fn new(
        executor: runtime::Handle,
        outbound_message_service: OutboundMessageRequester,
        inbound_handlers: MempoolInboundHandlers<B>,
        config: MempoolServiceConfig,
    ) -> Self
    {
        let (timeout_sender, timeout_receiver) = channel(100);
        Self {
            executor,
            outbound_message_service,
            inbound_handlers,
            waiting_requests: HashMap::new(),
            timeout_sender,
            timeout_receiver_stream: Some(timeout_receiver),
            config,
        }
    }

    pub async fn start<SOutReq, SInReq, SInRes, STxIn>(
        mut self,
        streams: MempoolStreams<SOutReq, SInReq, SInRes, STxIn>,
    ) -> Result<(), MempoolServiceError>
    where
        SOutReq: Stream<Item = RequestContext<MempoolRequest, Result<MempoolResponse, MempoolServiceError>>>,
        SInReq: Stream<Item = DomainMessage<proto::MempoolServiceRequest>>,
        SInRes: Stream<Item = DomainMessage<proto::MempoolServiceResponse>>,
        STxIn: Stream<Item = DomainMessage<Transaction>>,
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
                    let (request, reply_tx) = outbound_request_context.split();
                    let _ = self.handle_outbound_request(reply_tx,request).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle outbound request message: {:?}", err);
                        Err(err)
                    });
                },

                // Outbound tx messages from the OutboundMempoolServiceInterface
                outbound_tx_context = outbound_tx_stream.select_next_some() => {
                    let (tx, excluded_peers) = outbound_tx_context;
                    let _ = self.handle_outbound_tx(tx,excluded_peers).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle outbound tx message {:?}",err);
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

                // Incoming transaction messages from the Comms layer
                transaction_msg = inbound_transaction_stream.select_next_some() => {
                    let _ = self.handle_incoming_transaction(transaction_msg).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle incoming transaction message: {:?}", err);
                        Err(err)
                    });
                }

                // Block events from local Base Node.
                block_event = block_event_stream.select_next_some() => {
                    let _ = self.handle_block_event(&block_event).await.or_else(|err| {
                        error!(target: LOG_TARGET, "Failed to handle base node block event: {:?}", err);
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
                    info!(target: LOG_TARGET, "Mempool service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_incoming_request(
        &mut self,
        domain_request_msg: DomainMessage<proto::MempoolServiceRequest>,
    ) -> Result<(), MempoolServiceError>
    {
        let (origin_public_key, inner_msg) = domain_request_msg.into_origin_and_inner();

        // Convert proto::MempoolServiceRequest to a MempoolServiceRequest
        let request = inner_msg.request.ok_or_else(|| {
            MempoolServiceError::InvalidRequest("Received invalid mempool service request".to_string())
        })?;

        let response = self
            .inbound_handlers
            .handle_request(&request.try_into().map_err(MempoolServiceError::InvalidRequest)?)
            .await?;

        let message = proto::MempoolServiceResponse {
            request_key: inner_msg.request_key,
            response: Some(response.into()),
        };

        self.outbound_message_service
            .send_direct(
                origin_public_key,
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::MempoolResponse, message),
            )
            .await?;

        Ok(())
    }

    async fn handle_incoming_response(
        &mut self,
        incoming_response: proto::MempoolServiceResponse,
    ) -> Result<(), MempoolServiceError>
    {
        let proto::MempoolServiceResponse { request_key, response } = incoming_response;

        match self.waiting_requests.remove(&request_key) {
            Some(mut reply_tx) => {
                if let Some(reply_tx) = reply_tx.take() {
                    let response = response.and_then(|r| r.try_into().ok()).ok_or_else(|| {
                        MempoolServiceError::InvalidResponse("Received an invalid Mempool response".to_string())
                    })?;
                    let _ = reply_tx.send(Ok(response).or_else(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send outbound request from Mempool service"
                        );
                        Err(resp)
                    }));
                }
            },
            None => {
                info!(target: LOG_TARGET, "Discard incoming unmatched response");
            },
        }

        Ok(())
    }

    async fn handle_outbound_request(
        &mut self,
        reply_tx: OneshotSender<Result<MempoolResponse, MempoolServiceError>>,
        request: MempoolRequest,
    ) -> Result<(), MempoolServiceError>
    {
        let request_key = generate_request_key(&mut OsRng);
        let service_request = proto::MempoolServiceRequest {
            request_key,
            request: Some(request.into()),
        };

        let send_result = self
            .outbound_message_service
            .send_random(
                1,
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForPeer,
                OutboundDomainMessage::new(TariMessageType::MempoolRequest, service_request),
            )
            .await
            .or_else(|e| {
                error!(target: LOG_TARGET, "mempool outbound request failure. {:?}", e);
                Err(e)
            })
            .map_err(|e| MempoolServiceError::OutboundMessageService(e.to_string()))?;

        match send_result.resolve_ok().await {
            Some(tags) if !tags.is_empty() => {
                // Spawn timeout and wait for matching response to arrive
                self.waiting_requests.insert(request_key, Some(reply_tx));
                self.spawn_request_timeout(request_key, self.config.request_timeout)
                    .await;
            },
            Some(_) => {
                let _ = reply_tx.send(Err(MempoolServiceError::NoBootstrapNodesConfigured).or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send outbound request from Mempool service as no bootstrap nodes were configured"
                    );
                    Err(resp)
                }));
            },
            None => {
                let _ = reply_tx
                    .send(Err(MempoolServiceError::BroadcastFailed))
                    .or_else(|resp| {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send outbound request from Mempool service because of a failure in DHT \
                             broadcast"
                        );
                        Err(resp)
                    });
            },
        }

        Ok(())
    }

    async fn handle_incoming_transaction(
        &mut self,
        domain_transaction_msg: DomainMessage<Transaction>,
    ) -> Result<(), MempoolServiceError>
    {
        let DomainMessage::<_> { source_peer, inner, .. } = domain_transaction_msg;

        self.inbound_handlers
            .handle_transaction(&inner, Some(source_peer.public_key))
            .await?;

        Ok(())
    }

    async fn handle_request_timeout(&mut self, request_key: RequestKey) -> Result<(), MempoolServiceError> {
        if let Some(mut waiting_request) = self.waiting_requests.remove(&request_key) {
            if let Some(reply_tx) = waiting_request.take() {
                let reply_msg = Err(MempoolServiceError::RequestTimedOut);
                let _ = reply_tx.send(reply_msg.or_else(|resp| {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send outbound request from Mempool service"
                    );
                    Err(resp)
                }));
            }
        }
        Ok(())
    }

    async fn handle_outbound_tx(
        &mut self,
        tx: Transaction,
        exclude_peers: Vec<CommsPublicKey>,
    ) -> Result<(), MempoolServiceError>
    {
        self.outbound_message_service
            .propagate(
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForPeer,
                exclude_peers,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, ProtoTransaction::from(tx)),
            )
            .await
            .or_else(|e| {
                error!(target: LOG_TARGET, "Handle outbound tx failure. {:?}", e);
                Err(e)
            })
            .map_err(|e| MempoolServiceError::OutboundMessageService(e.to_string()))
            .map(|_| ())
    }

    /// Handle block events from local base node service.
    async fn handle_block_event(&mut self, block_event: &BlockEvent) -> Result<(), MempoolServiceError> {
        self.inbound_handlers.handle_block_event(block_event).await?;

        Ok(())
    }

    async fn spawn_request_timeout(&self, request_key: RequestKey, timeout: Duration) {
        let mut timeout_sender = self.timeout_sender.clone();
        self.executor.spawn(async move {
            tokio::time::delay_for(timeout).await;
            let _ = timeout_sender.send(request_key).await;
        });
    }
}

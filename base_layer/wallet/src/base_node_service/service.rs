// Copyright 2020. The Tari Project
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
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    config::BaseNodeServiceConfig,
    error::BaseNodeServiceError,
    handle::{BaseNodeEvent, BaseNodeEventSender, BaseNodeServiceRequest, BaseNodeServiceResponse},
};

use chrono::{NaiveDateTime, Utc};
use futures::{pin_mut, Stream, StreamExt};
use log::*;
use rand::rngs::OsRng;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundMessageRequester};
use tari_core::{
    base_node::{
        generate_request_key,
        proto::{
            base_node as BaseNodeProto,
            base_node::{
                base_node_service_request::Request as BaseNodeRequestProto,
                base_node_service_response::Response as BaseNodeResponseProto,
            },
        },
        RequestKey,
    },
    chain_storage::ChainMetadata,
};
use tari_p2p::{domain_message::DomainMessage, tari_message::TariMessageType};
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;
use tokio::time;

const LOG_TARGET: &str = "wallet::base_node_service::service";

/// State determined from Base Node Service Requests
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BaseNodeState {
    pub chain_metadata: Option<ChainMetadata>,
    pub is_synced: Option<bool>,
    pub updated: Option<NaiveDateTime>,
}

impl Default for BaseNodeState {
    fn default() -> Self {
        Self {
            chain_metadata: None,
            is_synced: None,
            updated: None,
        }
    }
}

/// To keep track of when a request was sent
#[derive(Debug, Clone, Copy)]
struct RequestMetadata {
    request_key: RequestKey,
    sent: NaiveDateTime,
}

/// The wallet base node service is responsible for handling requests to be sent to the connected base node.
pub struct BaseNodeService<BNResponseStream> {
    config: BaseNodeServiceConfig,
    base_node_response_stream: Option<BNResponseStream>,
    request_stream: Option<Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>>,
    outbound_messaging: OutboundMessageRequester,
    event_publisher: BaseNodeEventSender,
    base_node_public_key: Option<CommsPublicKey>,
    shutdown_signal: Option<ShutdownSignal>,
    state: BaseNodeState,
    requests: Vec<RequestMetadata>,
}

impl<BNResponseStream> BaseNodeService<BNResponseStream>
where BNResponseStream: Stream<Item = DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>
{
    pub fn new(
        config: BaseNodeServiceConfig,
        base_node_response_stream: BNResponseStream,
        request_stream: Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>,
        outbound_messaging: OutboundMessageRequester,
        event_publisher: BaseNodeEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            config,
            base_node_response_stream: Some(base_node_response_stream),
            request_stream: Some(request_stream),
            outbound_messaging,
            event_publisher,
            base_node_public_key: None,
            shutdown_signal: Some(shutdown_signal),
            state: BaseNodeState::default(),
            requests: Vec::new(),
        }
    }

    fn set_state(&mut self, state: BaseNodeState) {
        self.state = state;
    }

    /// Returns the last known state of the connected base node.
    pub fn get_state(&self) -> &BaseNodeState {
        &self.state
    }

    /// Starts the service.
    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let base_node_response_stream = self
            .base_node_response_stream
            .take()
            .expect("Wallet Base Node Service initialized without base_node_response_stream")
            .fuse();
        pin_mut!(base_node_response_stream);

        let interval = self.config.refresh_interval;
        let mut refresh_tick = time::interval_at((Instant::now() + interval).into(), interval).fuse();

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Wallet Base Node Service initialized without shutdown signal");

        info!(target: LOG_TARGET, "Wallet Base Node Service started");
        loop {
            futures::select! {
                // Incoming requests
                request_context = request_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Handling Base Node Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let _ = reply_tx.send(self.handle_request(request).await.or_else(|resp| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", resp);
                        Err(resp)
                    })).or_else(|resp| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        Err(resp)
                    });
                },

                // Base Node Responses
                response = base_node_response_stream.select_next_some() => {
                    if let Err(e) = self.handle_base_node_response(response).await {
                        warn!(target: LOG_TARGET, "Failed to handle base node response: {}", e);
                    }
                },

                // Refresh Interval Tick
                _ = refresh_tick.select_next_some() => {
                    if let Err(e) = self.refresh_chain_metadata().await {
                        warn!(target: LOG_TARGET, "Error when sending refresh chain metadata request: {}", e);
                    }
                },

                // Shutdown
                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Wallet Base Node Service shutting down because the shutdown signal was received");
                    break Ok(());
                }
            }
        }
    }

    /// Sends a request to the connected base node to retrieve chain metadata.
    async fn refresh_chain_metadata(&mut self) -> Result<(), BaseNodeServiceError> {
        debug!(target: LOG_TARGET, "Refresh chain metadata");
        let dest_public_key = self
            .base_node_public_key
            .clone()
            .ok_or_else(|| BaseNodeServiceError::NoBaseNodePublicKey)?;

        let request_key = generate_request_key(&mut OsRng);
        let now = Utc::now().naive_utc();

        self.requests.push(RequestMetadata { request_key, sent: now });

        // remove old request keys
        let (current_requests, old): (Vec<RequestMetadata>, Vec<RequestMetadata>) =
            self.requests.iter().partition(|r| {
                let age = Utc::now().naive_utc() - r.sent;
                // convert to std Duration
                let age = Duration::from_millis(age.num_milliseconds() as u64);

                age <= self.config.request_keys_max_age
            });

        trace!(target: LOG_TARGET, "current requests: {:?}", current_requests);
        trace!(target: LOG_TARGET, "discarded requests : {:?}", old);

        self.requests = current_requests;

        let request = BaseNodeRequestProto::GetChainMetadata(true);
        let service_request = BaseNodeProto::BaseNodeServiceRequest {
            request_key,
            request: Some(request),
        };

        let message = OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request);

        self.outbound_messaging
            .send_direct(dest_public_key, message)
            .await
            .map_err(BaseNodeServiceError::OutboundError)?;

        debug!(target: LOG_TARGET, "Refresh chain metadata sent");

        Ok(())
    }

    /// Handles the response from the connected base node.
    async fn handle_base_node_response(
        &mut self,
        response: DomainMessage<BaseNodeProto::BaseNodeServiceResponse>,
    ) -> Result<(), BaseNodeServiceError>
    {
        let DomainMessage::<_> { inner: message, .. } = response;

        let (found, remaining): (Vec<RequestMetadata>, Vec<RequestMetadata>) = self
            .requests
            .iter()
            .partition(|&&r| r.request_key == message.request_key);

        if !found.is_empty() {
            debug!(target: LOG_TARGET, "Handle base node response message: {:?}", message);
            match message.response {
                Some(BaseNodeResponseProto::ChainMetadata(chain_metadata)) => {
                    trace!(target: LOG_TARGET, "Chain Metadata response {:?}", chain_metadata);
                    let now = Utc::now().naive_utc();
                    let state = BaseNodeState {
                        is_synced: Some(message.is_synced),
                        chain_metadata: Some(chain_metadata.into()),
                        updated: Some(now),
                    };
                    self.publish_event(BaseNodeEvent::BaseNodeState(state.clone()));
                    self.set_state(state);
                    self.requests = remaining;
                },
                _ => {
                    trace!(
                        target: LOG_TARGET,
                        "Received a base node response currently unaccounted for: {:?}",
                        message
                    );
                },
            }
        }

        Ok(())
    }

    fn set_base_node_public_key(&mut self, base_node_public_key: CommsPublicKey) {
        self.base_node_public_key = Some(base_node_public_key);
    }

    fn publish_event(&mut self, event: BaseNodeEvent) {
        debug!(target: LOG_TARGET, "Publishing event: {:?}", event);
        let _ = self.event_publisher.send(Arc::new(event)).map_err(|_| {
            trace!(
                target: LOG_TARGET,
                "Could not publish BaseNodeEvent as there are no subscribers"
            )
        });
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: BaseNodeServiceRequest,
    ) -> Result<BaseNodeServiceResponse, BaseNodeServiceError>
    {
        debug!(
            target: LOG_TARGET,
            "Handling Wallet Base Node Service Request: {:?}", request
        );
        match request {
            BaseNodeServiceRequest::SetBaseNodePublicKey(public_key) => {
                self.set_base_node_public_key(public_key);
                Ok(BaseNodeServiceResponse::BaseNodePublicKeySet)
            },
            BaseNodeServiceRequest::GetChainMetadata => Ok(BaseNodeServiceResponse::ChainMetadata(
                self.state.chain_metadata.clone(),
            )),
        }
    }
}

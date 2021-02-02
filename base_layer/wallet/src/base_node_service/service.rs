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

use super::{
    config::BaseNodeServiceConfig,
    error::BaseNodeServiceError,
    handle::{BaseNodeEvent, BaseNodeEventSender, BaseNodeServiceRequest, BaseNodeServiceResponse},
};
use crate::storage::database::WalletDatabase;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, Utc};
use futures::{pin_mut, Stream, StreamExt};
use log::*;
use rand::rngs::OsRng;
use std::convert::TryInto;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    waiting_requests::{generate_request_key, RequestKey},
};
use tari_comms::peer_manager::Peer;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundMessageRequester};
use tari_core::proto::{base_node as proto, base_node::base_node_service_request::Request as BaseNodeRequestProto};

use crate::storage::database::WalletBackend;
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
    pub latency: Option<Duration>,
    pub online: OnlineState,
}

impl BaseNodeState {
    fn set_online(&mut self, online: OnlineState) -> Self {
        self.online = online;

        self.clone()
    }
}

/// Connection state of the Base Node
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OnlineState {
    Connecting,
    Online,
    Offline,
}

impl Default for BaseNodeState {
    fn default() -> Self {
        Self {
            chain_metadata: None,
            is_synced: None,
            updated: None,
            latency: None,
            online: OnlineState::Connecting,
        }
    }
}

/// Keep track of the identity and the time the request was sent
#[derive(Debug, Clone, Copy)]
struct RequestMetadata {
    request_key: RequestKey,
    sent: NaiveDateTime,
}

/// The wallet base node service is responsible for handling requests to be sent to the connected base node.
pub struct BaseNodeService<BNResponseStream, T>
where T: WalletBackend + 'static
{
    config: BaseNodeServiceConfig,
    base_node_response_stream: Option<BNResponseStream>,
    request_stream: Option<Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>>,
    outbound_messaging: OutboundMessageRequester,
    event_publisher: BaseNodeEventSender,
    base_node_peer: Option<Peer>,
    shutdown_signal: Option<ShutdownSignal>,
    state: BaseNodeState,
    requests: Vec<RequestMetadata>,
    db: WalletDatabase<T>,
}

impl<BNResponseStream, T> BaseNodeService<BNResponseStream, T>
where
    T: WalletBackend + 'static,
    BNResponseStream: Stream<Item = DomainMessage<proto::BaseNodeServiceResponse>>,
{
    pub fn new(
        config: BaseNodeServiceConfig,
        base_node_response_stream: BNResponseStream,
        request_stream: Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>,
        outbound_messaging: OutboundMessageRequester,
        event_publisher: BaseNodeEventSender,
        shutdown_signal: ShutdownSignal,
        db: WalletDatabase<T>,
    ) -> Self
    {
        Self {
            config,
            base_node_response_stream: Some(base_node_response_stream),
            request_stream: Some(request_stream),
            outbound_messaging,
            event_publisher,
            base_node_peer: None,
            shutdown_signal: Some(shutdown_signal),
            state: Default::default(),
            requests: Default::default(),
            db,
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
                    let response = self.handle_request(request).await.map_err(|e| {
                        error!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _ = reply_tx.send(response).map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
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
        trace!(target: LOG_TARGET, "Refresh chain metadata");
        let base_node_peer = self
            .base_node_peer
            .clone()
            .ok_or_else(|| BaseNodeServiceError::NoBaseNodePeer)?;

        let request_key = generate_request_key(&mut OsRng);
        let now = Utc::now().naive_utc();

        self.requests.push(RequestMetadata { request_key, sent: now });

        // remove old requests
        let (current_requests, old_requests): (Vec<RequestMetadata>, Vec<RequestMetadata>) =
            self.requests.iter().partition(|r| {
                let age = Utc::now().naive_utc() - r.sent;
                // convert to std Duration
                let age = Duration::from_millis(age.num_milliseconds() as u64);

                age <= self.config.request_max_age
            });

        self.requests = current_requests;

        // check if base node is offline
        self.check_online_status(old_requests.len());

        // send the new request
        let request = BaseNodeRequestProto::GetChainMetadata(true);
        let service_request = proto::BaseNodeServiceRequest {
            request_key,
            request: Some(request),
        };

        let message = OutboundDomainMessage::new(TariMessageType::BaseNodeRequest, service_request);

        let dest_public_key = base_node_peer.public_key;

        self.outbound_messaging
            .send_direct(dest_public_key, message)
            .await
            .map_err(BaseNodeServiceError::OutboundError)?;

        Ok(())
    }

    fn check_online_status(&mut self, discarded: usize) {
        // if we are discarding old requests and have never received a response
        let never_connected = discarded > 0 && self.state.updated.is_none();

        // or if the last time we received a response is greater than the max request age config
        let timing_out = if let Some(updated) = self.state.updated {
            let now = Utc::now().naive_utc();
            let millis = (now - updated).num_milliseconds() as u64;
            let last_updated = Duration::from_millis(millis);

            matches!(self.state.online, OnlineState::Online) && last_updated > self.config.request_max_age
        } else {
            false
        };

        // then the base node is currently not responding
        if never_connected || timing_out {
            info!(
                target: LOG_TARGET,
                "Base node is offline. Either we never connected ({}), or haven't received a response newer than the \
                 max request age ({}).",
                never_connected,
                timing_out
            );
            let state = self.state.set_online(OnlineState::Offline);
            let event = BaseNodeEvent::BaseNodeState(state);
            self.publish_event(event);
        }
    }

    /// Handles the response from the connected base node.
    async fn handle_base_node_response(
        &mut self,
        response: DomainMessage<proto::BaseNodeServiceResponse>,
    ) -> Result<(), BaseNodeServiceError>
    {
        let DomainMessage::<_> { inner: message, .. } = response;

        let (found, remaining): (Vec<RequestMetadata>, Vec<RequestMetadata>) = self
            .requests
            .iter()
            .partition(|&&r| r.request_key == message.request_key);

        if !found.is_empty() {
            debug!(target: LOG_TARGET, "Handle base node response message: {:?}", message);

            let now = Utc::now().naive_utc();
            let time_sent = found.first().unwrap().sent;
            let millis = (now - time_sent).num_milliseconds() as u64;
            let latency = Duration::from_millis(millis);

            match message.response {
                Some(proto::base_node_service_response::Response::ChainMetadata(chain_metadata)) => {
                    trace!(target: LOG_TARGET, "Chain Metadata response {:?}", chain_metadata);
                    info!(target: LOG_TARGET, "Base node latency: {}ms", millis);
                    let metadata: ChainMetadata = chain_metadata
                        .try_into()
                        .map_err(BaseNodeServiceError::InvalidBaseNodeResponse)?;
                    self.db.set_chain_meta(metadata.clone()).await?;
                    let state = BaseNodeState {
                        is_synced: Some(message.is_synced),
                        chain_metadata: Some(metadata),
                        updated: Some(now),
                        latency: Some(latency),
                        online: OnlineState::Online,
                    };
                    self.publish_event(BaseNodeEvent::BaseNodeState(state.clone()));
                    self.set_state(state);
                },
                _ => {
                    trace!(
                        target: LOG_TARGET,
                        "Received a base node response currently unaccounted for: {:?}",
                        message
                    );
                },
            }

            self.requests = remaining;
        }

        Ok(())
    }

    fn reset_state(&mut self) {
        // drop outstanding reqs
        self.requests = Vec::new();

        // reset base node state
        let state = BaseNodeState::default();
        self.publish_event(BaseNodeEvent::BaseNodeState(state.clone()));
        self.set_state(state);
    }

    fn set_base_node_peer(&mut self, peer: Peer) {
        self.reset_state();

        self.base_node_peer = Some(peer.clone());
        self.publish_event(BaseNodeEvent::BaseNodePeerSet(Box::new(peer)));
    }

    fn publish_event(&mut self, event: BaseNodeEvent) {
        trace!(target: LOG_TARGET, "Publishing event: {:?}", event);
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
            BaseNodeServiceRequest::SetBaseNodePeer(peer) => {
                self.set_base_node_peer(*peer);
                Ok(BaseNodeServiceResponse::BaseNodePeerSet)
            },
            BaseNodeServiceRequest::GetChainMetadata => match self.state.chain_metadata.clone() {
                Some(v) => Ok(BaseNodeServiceResponse::ChainMetadata(Some(v))),
                None => Ok(BaseNodeServiceResponse::ChainMetadata(self.db.get_chain_meta().await?)),
            },
        }
    }
}

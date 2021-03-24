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
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, Utc};
use futures::{pin_mut, StreamExt};
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::Peer};

use crate::storage::database::WalletBackend;
use tari_core::base_node::rpc::BaseNodeWalletRpcClient;
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

/// The wallet base node service is responsible for handling requests to be sent to the connected base node.
pub struct BaseNodeService<T>
where T: WalletBackend + 'static
{
    config: BaseNodeServiceConfig,
    request_stream: Option<Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>>,
    connectivity_manager: ConnectivityRequester,
    event_publisher: BaseNodeEventSender,
    base_node_peer: Option<Peer>,
    shutdown_signal: Option<ShutdownSignal>,
    state: BaseNodeState,
    db: WalletDatabase<T>,
}

impl<T> BaseNodeService<T>
where T: WalletBackend + 'static
{
    pub fn new(
        config: BaseNodeServiceConfig,
        request_stream: Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>,
        connectivity_manager: ConnectivityRequester,
        event_publisher: BaseNodeEventSender,
        shutdown_signal: ShutdownSignal,
        db: WalletDatabase<T>,
    ) -> Self
    {
        Self {
            config,
            request_stream: Some(request_stream),
            connectivity_manager,
            event_publisher,
            base_node_peer: None,
            shutdown_signal: Some(shutdown_signal),
            state: Default::default(),
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

    /// Utility function to set and publish offline state
    fn set_offline(&mut self) {
        let now = Utc::now().naive_utc();
        let state = BaseNodeState {
            chain_metadata: None,
            is_synced: None,
            updated: Some(now),
            latency: None,
            online: OnlineState::Offline,
        };

        let event = BaseNodeEvent::BaseNodeState(state.clone());
        self.publish_event(event);
        self.set_state(state);
    }

    /// Starts the service.
    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

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

        let peer = base_node_peer.node_id;
        let now = Utc::now().naive_utc();

        let mut connection = self.connectivity_manager.dial_peer(peer).await.map_err(|e| {
            self.set_offline();
            error!(target: LOG_TARGET, "Error dialing base node peer: {}", e);
            e
        })?;

        let mut client = connection.connect_rpc::<BaseNodeWalletRpcClient>().await.map_err(|e| {
            self.set_offline();
            e
        })?;

        let latency = client.get_last_request_latency().await?;

        trace!(
            target: LOG_TARGET,
            "Base node latency: {} ms",
            latency.unwrap_or_default().as_millis()
        );

        let tip_info = client.get_tip_info().await?;

        // Note: Dropping the client here reduces the number of concurrent RPC connections
        drop(client);

        let metadata = tip_info
            .metadata
            .ok_or_else(|| BaseNodeServiceError::InvalidBaseNodeResponse("Tip info no metadata".to_string()))?;

        let chain_metadata = ChainMetadata::try_from(metadata).map_err(|details| {
            BaseNodeServiceError::InvalidBaseNodeResponse(format!("Base node sent invalid chain metadata: {}", details))
        })?;

        // store chain metadata in the wallet db
        self.db.set_chain_metadata(chain_metadata.clone()).await?;

        let state = BaseNodeState {
            chain_metadata: Some(chain_metadata),
            is_synced: Some(tip_info.is_synced),
            updated: Some(now),
            latency,
            online: OnlineState::Online,
        };

        let event = BaseNodeEvent::BaseNodeState(state.clone());
        self.publish_event(event);
        self.set_state(state);

        Ok(())
    }

    // reset base node state
    fn reset_state(&mut self) {
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
                Some(metadata) => Ok(BaseNodeServiceResponse::ChainMetadata(Some(metadata))),
                None => {
                    // if we don't have live state, check if we've previously stored state in the wallet db
                    let metadata = self.db.get_chain_metadata().await?;
                    Ok(BaseNodeServiceResponse::ChainMetadata(metadata))
                },
            },
        }
    }
}

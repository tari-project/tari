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
use crate::{
    base_node_service::monitor::BaseNodeMonitor,
    storage::database::{WalletBackend, WalletDatabase},
};
use chrono::NaiveDateTime;
use futures::StreamExt;
use log::*;
use std::{sync::Arc, time::Duration};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::Peer};
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;
use tokio::sync::RwLock;

const LOG_TARGET: &str = "wallet::base_node_service::service";

/// State determined from Base Node Service Requests
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BaseNodeState {
    pub chain_metadata: Option<ChainMetadata>,
    pub is_synced: Option<bool>,
    pub updated: Option<NaiveDateTime>,
    pub latency: Option<Duration>,
    pub online: OnlineState,
    pub base_node_peer: Option<Peer>,
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
            base_node_peer: None,
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
    shutdown_signal: Option<ShutdownSignal>,
    state: Arc<RwLock<BaseNodeState>>,
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
            shutdown_signal: Some(shutdown_signal),
            state: Default::default(),
            db,
        }
    }

    /// Returns the last known state of the connected base node.
    pub async fn get_state(&self) -> BaseNodeState {
        self.state.read().await.clone()
    }

    /// Starts the service.
    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        let shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Wallet Base Node Service initialized without shutdown signal");

        let monitor = BaseNodeMonitor::new(
            self.config.base_node_monitor_refresh_interval,
            self.state.clone(),
            self.db.clone(),
            self.connectivity_manager.clone(),
            self.event_publisher.clone(),
            shutdown_signal.clone(),
        );

        tokio::spawn(monitor.run());

        let mut request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .take_until(shutdown_signal);

        info!(target: LOG_TARGET, "Wallet Base Node Service started");
        while let Some(request_context) = request_stream.next().await {
            // Incoming requests
            let (request, reply_tx) = request_context.split();
            let response = self.handle_request(request).await.map_err(|e| {
                error!(target: LOG_TARGET, "Error handling request: {:?}", e);
                e
            });
            let _ = reply_tx.send(response).map_err(|e| {
                warn!(target: LOG_TARGET, "Failed to send reply");
                e
            });
        }

        info!(
            target: LOG_TARGET,
            "Wallet Base Node Service shutting down because the shutdown signal was received"
        );
        Ok(())
    }

    async fn set_base_node_peer(&self, peer: Peer) {
        let mut new_state = BaseNodeState::default();
        new_state.base_node_peer = Some(peer.clone());

        {
            let mut lock = self.state.write().await;
            *lock = new_state.clone();
        };

        self.publish_event(BaseNodeEvent::BaseNodeStateChanged(new_state));
        self.publish_event(BaseNodeEvent::BaseNodePeerSet(Box::new(peer)));
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
                self.set_base_node_peer(*peer).await;
                Ok(BaseNodeServiceResponse::BaseNodePeerSet)
            },
            BaseNodeServiceRequest::GetChainMetadata => match self.get_state().await.chain_metadata.clone() {
                Some(metadata) => Ok(BaseNodeServiceResponse::ChainMetadata(Some(metadata))),
                None => {
                    // if we don't have live state, check if we've previously stored state in the wallet db
                    let metadata = self.db.get_chain_metadata().await?;
                    Ok(BaseNodeServiceResponse::ChainMetadata(metadata))
                },
            },
        }
    }

    fn publish_event(&self, event: BaseNodeEvent) {
        trace!(target: LOG_TARGET, "Publishing event: {:?}", event);
        let _ = self.event_publisher.send(Arc::new(event)).map_err(|_| {
            trace!(
                target: LOG_TARGET,
                "Could not publish BaseNodeEvent as there are no subscribers"
            )
        });
    }
}

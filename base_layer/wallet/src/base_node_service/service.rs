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

use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;
use tokio::sync::RwLock;

use super::{
    error::BaseNodeServiceError,
    handle::{BaseNodeEventSender, BaseNodeServiceRequest, BaseNodeServiceResponse},
};
use crate::{
    base_node_service::monitor::BaseNodeMonitor,
    client::http_client_factory::HttpClientFactory,
    connectivity_service::WalletConnectivityHandle,
};

const LOG_TARGET: &str = "wallet::base_node_service::service";

/// State determined from Base Node Service Requests
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BaseNodeState {
    pub chain_metadata: Option<ChainMetadata>,
    pub is_synced: Option<bool>,
    pub updated: Option<DateTime<Utc>>,
    pub latency: Option<Duration>,
}

/// The base node service is responsible for handling requests to be sent to the connected base node.
pub struct BaseNodeService<TClientFactory>
where TClientFactory: HttpClientFactory
{
    request_stream: Option<Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>>,
    wallet_connectivity: WalletConnectivityHandle<TClientFactory>,
    event_publisher: BaseNodeEventSender,
    shutdown_signal: ShutdownSignal,
    state: Arc<RwLock<BaseNodeState>>,
}

impl<TClientFactory> BaseNodeService<TClientFactory>
where TClientFactory: HttpClientFactory
{
    pub fn new(
        request_stream: Receiver<BaseNodeServiceRequest, Result<BaseNodeServiceResponse, BaseNodeServiceError>>,
        wallet_connectivity: WalletConnectivityHandle<TClientFactory>,
        event_publisher: BaseNodeEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            request_stream: Some(request_stream),
            wallet_connectivity,
            event_publisher,
            shutdown_signal,
            state: Default::default(),
        }
    }

    /// Returns the last known state of the connected base node.
    pub async fn get_state(&self) -> BaseNodeState {
        self.state.read().await.clone()
    }

    /// Starts the service.
    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        self.spawn_monitor();

        let mut request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .take_until(self.shutdown_signal.clone());

        debug!(target: LOG_TARGET, "Wallet Base Node Service started");
        while let Some(request_context) = request_stream.next().await {
            // Incoming requests
            let (request, reply_tx) = request_context.split();
            let response = self.handle_request(request).await.map_err(|e| {
                error!(target: LOG_TARGET, "Error handling request: {:?}", e);
                e
            });
            let _result = reply_tx.send(response).inspect_err(|_| {
                warn!(target: LOG_TARGET, "Failed to send reply");
            });
        }

        info!(
            target: LOG_TARGET,
            "Wallet Base Node Service shutting down because the shutdown signal was received"
        );
        Ok(())
    }

    fn spawn_monitor(&self) {
        let monitor = BaseNodeMonitor::new(
            self.state.clone(),
            self.wallet_connectivity.clone(),
            self.event_publisher.clone(),
        );

        let shutdown_signal = self.shutdown_signal.clone();
        tokio::spawn(async move {
            monitor.run(shutdown_signal.clone()).await;
        });
    }

    /// This handler is called when requests arrive from the various streams
    async fn handle_request(
        &mut self,
        request: BaseNodeServiceRequest,
    ) -> Result<BaseNodeServiceResponse, BaseNodeServiceError> {
        trace!(
            target: LOG_TARGET,
            "Handling Wallet Base Node Service Request: {:?}", request
        );
        match request {
            BaseNodeServiceRequest::GetBaseNodeLatency => {
                Ok(BaseNodeServiceResponse::Latency(self.state.read().await.latency))
            },
        }
    }
}

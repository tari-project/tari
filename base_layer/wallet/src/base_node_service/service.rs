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
use crate::storage::database::{WalletBackend, WalletDatabase};
use chrono::{NaiveDateTime, Utc};
use futures::{future::Fuse, pin_mut, select, FutureExt, StreamExt};
use log::*;
use std::{
    convert::TryFrom,
    sync::{Arc, RwLock},
    time::Duration,
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{connectivity::ConnectivityRequester, peer_manager::Peer};
use tari_core::base_node::rpc::BaseNodeWalletRpcClient;
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;
use tokio::time::{delay_for, Delay};

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
            state: Arc::new(RwLock::new(BaseNodeState::default())),
            db,
        }
    }

    /// Returns the last known state of the connected base node.
    pub fn get_state(&self) -> BaseNodeState {
        let lock = acquire_read_lock!(self.state);
        (*lock).clone()
    }

    /// Starts the service.
    pub async fn start(mut self) -> Result<(), BaseNodeServiceError> {
        let request_stream = self
            .request_stream
            .take()
            .expect("Wallet Base Node Service initialized without request_stream")
            .fuse();
        pin_mut!(request_stream);

        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Wallet Base Node Service initialized without shutdown signal");

        let join_handle = tokio::spawn(monitor_chain_metadata_task(
            self.config.refresh_interval,
            self.state.clone(),
            self.db.clone(),
            self.connectivity_manager.clone(),
            self.event_publisher.clone(),
            shutdown_signal.clone(),
        ));

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
                // Shutdown
                _ = shutdown_signal => {
                    let _ = join_handle.await;
                    info!(target: LOG_TARGET, "Wallet Base Node Service shutting down because the shutdown signal was received");
                    break Ok(());
                }
            }
        }
    }

    fn set_base_node_peer(&mut self, peer: Peer) {
        reset_state(self.state.clone(), &mut self.event_publisher);

        {
            let mut lock = acquire_write_lock!(self.state);
            (*lock).base_node_peer = Some(peer.clone());
        }

        publish_event(
            &mut self.event_publisher,
            BaseNodeEvent::BaseNodePeerSet(Box::new(peer)),
        );
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
            BaseNodeServiceRequest::GetChainMetadata => match self.get_state().chain_metadata.clone() {
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

async fn monitor_chain_metadata_task<T: WalletBackend + 'static>(
    interval: Duration,
    state: Arc<RwLock<BaseNodeState>>,
    db: WalletDatabase<T>,
    mut connectivity_manager: ConnectivityRequester,
    mut event_publisher: BaseNodeEventSender,
    mut shutdown_signal: ShutdownSignal,
)
{
    // RPC connectivity loop
    loop {
        let base_node_peer = {
            let lock = acquire_read_lock!(state);
            (*lock).base_node_peer.clone()
        };

        let delay = delay_for(interval).fuse();

        let base_node_peer = match base_node_peer {
            None => {
                if wait_or_shutdown(delay, &mut shutdown_signal).await {
                    continue;
                } else {
                    return;
                }
            },
            Some(p) => p,
        };

        let mut connection = match connectivity_manager.dial_peer(base_node_peer.node_id.clone()).await {
            Ok(c) => c,
            Err(e) => {
                set_offline(state.clone(), &mut event_publisher);
                error!(target: LOG_TARGET, "Error dialing base node peer: {}", e);
                if wait_or_shutdown(delay, &mut shutdown_signal).await {
                    continue;
                } else {
                    return;
                }
            },
        };

        let mut client = match connection.connect_rpc::<BaseNodeWalletRpcClient>().await {
            Ok(c) => c,
            Err(e) => {
                set_offline(state.clone(), &mut event_publisher);
                error!(
                    target: LOG_TARGET,
                    "Error establishing RPC connection to base node peer: {}", e
                );
                if wait_or_shutdown(delay, &mut shutdown_signal).await {
                    continue;
                } else {
                    return;
                }
            },
        };

        'inner: loop {
            trace!(target: LOG_TARGET, "Refresh chain metadata");

            let now = Utc::now().naive_utc();
            let delay = delay_for(interval).fuse();

            let latency = match client.get_last_request_latency().await {
                Ok(l) => l,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error making `get_last_request_latency` RPC call to base node peer: {}", e
                    );
                    if wait_or_shutdown(delay, &mut shutdown_signal).await {
                        break 'inner;
                    } else {
                        return;
                    }
                },
            };

            trace!(
                target: LOG_TARGET,
                "Base node latency: {} ms",
                latency.unwrap_or_default().as_millis()
            );

            let tip_info = match client.get_tip_info().await {
                Ok(t) => t,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Error making `get_tip_info` RPC call to base node peer: {}", e
                    );
                    if wait_or_shutdown(delay, &mut shutdown_signal).await {
                        break 'inner;
                    } else {
                        return;
                    }
                },
            };

            let metadata = match tip_info
                .metadata
                .ok_or_else(|| BaseNodeServiceError::InvalidBaseNodeResponse("Tip info no metadata".to_string()))
            {
                Ok(m) => m,
                Err(e) => {
                    warn!(target: LOG_TARGET, "RPC return type error: {}", e);
                    if wait_or_shutdown(delay, &mut shutdown_signal).await {
                        continue;
                    } else {
                        return;
                    }
                },
            };

            let chain_metadata = match ChainMetadata::try_from(metadata) {
                Ok(m) => m,
                Err(e) => {
                    warn!(target: LOG_TARGET, "RPC return type conversion error: {}", e);
                    if wait_or_shutdown(delay, &mut shutdown_signal).await {
                        continue;
                    } else {
                        return;
                    }
                },
            };

            // store chain metadata in the wallet db
            if let Err(e) = db.set_chain_metadata(chain_metadata.clone()).await {
                warn!(target: LOG_TARGET, "Error storing chain metadata: {:?}", e);
                if wait_or_shutdown(delay, &mut shutdown_signal).await {
                    continue;
                } else {
                    return;
                }
            };

            let new_state = {
                let mut lock = acquire_write_lock!(state);

                let new_state = BaseNodeState {
                    chain_metadata: Some(chain_metadata),
                    is_synced: Some(tip_info.is_synced),
                    updated: Some(now),
                    latency,
                    online: OnlineState::Online,
                    base_node_peer: (*lock).base_node_peer.clone(),
                };
                (*lock) = new_state.clone();
                new_state
            };

            let event = BaseNodeEvent::BaseNodeState(new_state);

            publish_event(&mut event_publisher, event);
        }
    }
}

/// Utility function to set and publish offline state
fn set_offline(state: Arc<RwLock<BaseNodeState>>, event_publisher: &mut BaseNodeEventSender) {
    let mut lock = acquire_write_lock!(*state);

    let now = Utc::now().naive_utc();
    let new_state = BaseNodeState {
        chain_metadata: None,
        is_synced: None,
        updated: Some(now),
        latency: None,
        online: OnlineState::Offline,
        base_node_peer: (*lock).base_node_peer.clone(),
    };

    let event = BaseNodeEvent::BaseNodeState(new_state.clone());
    publish_event(event_publisher, event);

    (*lock) = new_state;
}

fn publish_event(event_publisher: &mut BaseNodeEventSender, event: BaseNodeEvent) {
    trace!(target: LOG_TARGET, "Publishing event: {:?}", event);
    let _ = event_publisher.send(Arc::new(event)).map_err(|_| {
        trace!(
            target: LOG_TARGET,
            "Could not publish BaseNodeEvent as there are no subscribers"
        )
    });
}

fn reset_state(state: Arc<RwLock<BaseNodeState>>, event_publisher: &mut BaseNodeEventSender) {
    let new_state = BaseNodeState::default();
    let mut lock = acquire_write_lock!(state);
    publish_event(event_publisher, BaseNodeEvent::BaseNodeState((*lock).clone()));
    (*lock) = new_state;
}

// Utility function to wait for the delay to complete and return true, or return false if the shutdown signal fired.
async fn wait_or_shutdown(mut delay: Fuse<Delay>, mut shutdown_signal: &mut ShutdownSignal) -> bool {
    select! {
        _ = delay => {
            return true;
        },
         _ = shutdown_signal => {
            debug!(target: LOG_TARGET, "Wallet Base Node Service chain metadata task shutting down because the shutdown signal was received");
            return false;
        }
    }
}

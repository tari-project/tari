//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    base_node_service::{
        handle::{BaseNodeEvent, BaseNodeEventSender},
        service::{BaseNodeState, OnlineState},
    },
    error::WalletStorageError,
    storage::database::{WalletBackend, WalletDatabase},
};
use chrono::Utc;
use futures::{future, future::Either};
use log::*;
use std::{convert::TryFrom, sync::Arc, time::Duration};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityRequester},
    peer_manager::NodeId,
    protocol::rpc::RpcError,
    PeerConnection,
};
use tari_core::base_node::rpc::BaseNodeWalletRpcClient;
use tari_shutdown::ShutdownSignal;
use tokio::{
    stream::StreamExt,
    sync::{broadcast, RwLock},
    time,
};

const LOG_TARGET: &str = "wallet::base_node_service::chain_metadata_monitor";

pub struct BaseNodeMonitor<T> {
    interval: Duration,
    state: Arc<RwLock<BaseNodeState>>,
    db: WalletDatabase<T>,
    connectivity_manager: ConnectivityRequester,
    event_publisher: BaseNodeEventSender,
    shutdown_signal: ShutdownSignal,
}

impl<T: WalletBackend + 'static> BaseNodeMonitor<T> {
    pub fn new(
        interval: Duration,
        state: Arc<RwLock<BaseNodeState>>,
        db: WalletDatabase<T>,
        connectivity_manager: ConnectivityRequester,
        event_publisher: BaseNodeEventSender,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            interval,
            state,
            db,
            connectivity_manager,
            event_publisher,
            shutdown_signal,
        }
    }

    pub async fn run(mut self) {
        loop {
            trace!(target: LOG_TARGET, "Beginning new base node monitoring round");
            match self.process().await {
                Ok(_) => continue,
                Err(BaseNodeMonitorError::NodeShuttingDown) => {
                    debug!(
                        target: LOG_TARGET,
                        "Wallet Base Node Service chain metadata task shutting down because the shutdown signal was \
                         received"
                    );
                    break;
                },
                Err(e @ BaseNodeMonitorError::RpcFailed(_)) | Err(e @ BaseNodeMonitorError::DialFailed(_)) => {
                    debug!(target: LOG_TARGET, "Connectivity failure to base node: {}", e,);
                    debug!(
                        target: LOG_TARGET,
                        "Setting as OFFLINE and retrying after {:.2?}", self.interval
                    );

                    self.set_offline().await;
                    if self.sleep_or_shutdown().await.is_err() {
                        break;
                    }
                    continue;
                },
                Err(BaseNodeMonitorError::BaseNodeChanged) => {
                    debug!(
                        target: LOG_TARGET,
                        "Base node has changed. Connecting to new base node...",
                    );

                    self.set_connecting().await;
                    continue;
                },
                Err(e @ BaseNodeMonitorError::InvalidBaseNodeResponse(_)) |
                Err(e @ BaseNodeMonitorError::WalletStorageError(_)) => {
                    error!(target: LOG_TARGET, "{}", e);
                    if self.sleep_or_shutdown().await.is_err() {
                        break;
                    }
                    continue;
                },
            }
        }
        debug!(
            target: LOG_TARGET,
            "Base Node Service Monitor shutting down because it received the shutdown signal"
        );
    }

    async fn process(&mut self) -> Result<(), BaseNodeMonitorError> {
        let peer = self.wait_for_peer_to_be_set().await?;
        let connection = self.attempt_dial(peer.clone()).await?;
        debug!(
            target: LOG_TARGET,
            "Base node connected. Establishing RPC connection...",
        );
        let client = self.connect_client(connection).await?;
        debug!(target: LOG_TARGET, "RPC established",);
        self.monitor_node(peer, client).await?;
        Ok(())
    }

    async fn wait_for_peer_to_be_set(&mut self) -> Result<NodeId, BaseNodeMonitorError> {
        // We aren't worried about late subscription here because we also check the state for a set base node peer, as
        // long as we subscribe before checking state.
        let mut event_subscription = self.event_publisher.subscribe();
        loop {
            let peer = self
                .state
                .read()
                .await
                .base_node_peer
                .as_ref()
                .map(|p| p.node_id.clone());

            match peer {
                Some(peer) => return Ok(peer),
                None => {
                    trace!(target: LOG_TARGET, "Base node peer not set yet. Waiting for event");
                    let either = future::select(event_subscription.next(), &mut self.shutdown_signal).await;
                    match either {
                        Either::Left((Some(Ok(_)), _)) |
                        Either::Left((Some(Err(broadcast::RecvError::Lagged(_))), _)) => {
                            trace!(target: LOG_TARGET, "Base node monitor got event");
                            // If we get any event (or some were missed), let's check base node peer has been set
                            continue;
                        },
                        // All of these indicate that the node has been shut down
                        Either::Left((Some(Err(broadcast::RecvError::Closed)), _)) |
                        Either::Left((None, _)) |
                        Either::Right((_, _)) => return Err(BaseNodeMonitorError::NodeShuttingDown),
                    }
                },
            }
        }
    }

    async fn attempt_dial(&mut self, peer: NodeId) -> Result<PeerConnection, BaseNodeMonitorError> {
        let conn = self.connectivity_manager.dial_peer(peer).await?;
        Ok(conn)
    }

    async fn connect_client(&self, mut conn: PeerConnection) -> Result<BaseNodeWalletRpcClient, BaseNodeMonitorError> {
        let client = conn.connect_rpc().await?;
        Ok(client)
    }

    async fn monitor_node(
        &self,
        peer_node_id: NodeId,
        mut client: BaseNodeWalletRpcClient,
    ) -> Result<(), BaseNodeMonitorError>
    {
        loop {
            let latency = client.get_last_request_latency().await?;
            trace!(
                target: LOG_TARGET,
                "Base node latency: {} ms",
                latency.unwrap_or_default().as_millis()
            );

            let tip_info = client.get_tip_info().await?;
            let is_synced = tip_info.is_synced;

            let chain_metadata = tip_info
                .metadata
                .ok_or_else(|| BaseNodeMonitorError::InvalidBaseNodeResponse("Tip info no metadata".to_string()))
                .and_then(|metadata| {
                    ChainMetadata::try_from(metadata).map_err(BaseNodeMonitorError::InvalidBaseNodeResponse)
                })?;

            self.db.set_chain_metadata(chain_metadata.clone()).await?;

            self.map_state(move |state| BaseNodeState {
                chain_metadata: Some(chain_metadata),
                is_synced: Some(is_synced),
                updated: Some(Utc::now().naive_utc()),
                latency,
                online: OnlineState::Online,
                base_node_peer: state.base_node_peer.clone(),
            })
            .await;

            self.sleep_or_shutdown().await?;
            self.check_if_base_node_changed(&peer_node_id).await?;
        }

        // loop only exits on shutdown/error
        #[allow(unreachable_code)]
        Ok(())
    }

    async fn check_if_base_node_changed(&self, peer_node_id: &NodeId) -> Result<(), BaseNodeMonitorError> {
        // Check if the base node peer is no longer set or has changed
        if self
            .state
            .read()
            .await
            .base_node_peer
            .as_ref()
            .filter(|p| &p.node_id == peer_node_id)
            .is_some()
        {
            Ok(())
        } else {
            Err(BaseNodeMonitorError::BaseNodeChanged)
        }
    }

    async fn set_connecting(&self) {
        self.map_state(|state| BaseNodeState {
            chain_metadata: None,
            is_synced: None,
            updated: Some(Utc::now().naive_utc()),
            latency: None,
            online: OnlineState::Connecting,
            base_node_peer: state.base_node_peer.clone(),
        })
        .await;
    }

    async fn set_offline(&self) {
        self.map_state(|state| BaseNodeState {
            chain_metadata: None,
            is_synced: None,
            updated: Some(Utc::now().naive_utc()),
            latency: None,
            online: OnlineState::Offline,
            base_node_peer: state.base_node_peer.clone(),
        })
        .await;
    }

    async fn map_state<F>(&self, transform: F)
    where F: FnOnce(&BaseNodeState) -> BaseNodeState {
        let new_state = {
            let mut lock = self.state.write().await;
            let new_state = transform(&*lock);
            *lock = new_state.clone();
            new_state
        };
        self.publish_event(BaseNodeEvent::BaseNodeStateChanged(new_state));
    }

    async fn sleep_or_shutdown(&self) -> Result<(), BaseNodeMonitorError> {
        let delay = time::delay_for(self.interval);
        let mut shutdown_signal = self.shutdown_signal.clone();
        if let Either::Right(_) = future::select(delay, &mut shutdown_signal).await {
            return Err(BaseNodeMonitorError::NodeShuttingDown);
        }
        Ok(())
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

#[derive(thiserror::Error, Debug)]
enum BaseNodeMonitorError {
    #[error("Node is shutting down")]
    NodeShuttingDown,
    #[error("Failed to dial base node: {0}")]
    DialFailed(#[from] ConnectivityError),
    #[error("Rpc error: {0}")]
    RpcFailed(#[from] RpcError),
    #[error("Invalid base node response: {0}")]
    InvalidBaseNodeResponse(String),
    #[error("Wallet storage error: {0}")]
    WalletStorageError(#[from] WalletStorageError),
    #[error("Base node changed")]
    BaseNodeChanged,
}

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

use std::{
    cmp,
    convert::TryFrom,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use futures::{future, future::Either};
use log::*;
use tari_common_types::{chain_metadata::ChainMetadata, types::BlockHash as BlockHashType};
use tari_rpc_framework::RpcError;
use tokio::{sync::RwLock, time};

use crate::{
    base_node_service::{
        backoff::{Backoff, ExponentialBackoff},
        handle::{BaseNodeEvent, BaseNodeEventSender},
        service::BaseNodeState,
    },
    connectivity_service::WalletConnectivityInterface,
    error::WalletStorageError,
    storage::database::{WalletBackend, WalletDatabase},
};

const LOG_TARGET: &str = "wallet::base_node_service::chain_metadata_monitor";

pub struct BaseNodeMonitor<TBackend, TWalletConnectivity> {
    max_interval: Duration,
    backoff: ExponentialBackoff,
    backoff_attempts: usize,
    state: Arc<RwLock<BaseNodeState>>,
    db: WalletDatabase<TBackend>,
    wallet_connectivity: TWalletConnectivity,
    event_publisher: BaseNodeEventSender,
}

impl<TBackend, TWalletConnectivity> BaseNodeMonitor<TBackend, TWalletConnectivity>
where
    TBackend: WalletBackend + 'static,
    TWalletConnectivity: WalletConnectivityInterface,
{
    pub fn new(
        max_interval: Duration,
        state: Arc<RwLock<BaseNodeState>>,
        db: WalletDatabase<TBackend>,
        wallet_connectivity: TWalletConnectivity,
        event_publisher: BaseNodeEventSender,
    ) -> Self {
        Self {
            max_interval,
            backoff: ExponentialBackoff::default(),
            backoff_attempts: 0,
            state,
            db,
            wallet_connectivity,
            event_publisher,
        }
    }

    pub async fn run(mut self) {
        loop {
            trace!(target: LOG_TARGET, "Beginning new base node monitoring round");
            match self.monitor_node().await {
                Ok(_) => continue,
                Err(BaseNodeMonitorError::NodeShuttingDown) => {
                    debug!(
                        target: LOG_TARGET,
                        "Wallet Base Node Service chain metadata task shutting down because the shutdown signal was \
                         received"
                    );
                    break;
                },
                Err(e @ BaseNodeMonitorError::RpcFailed(_)) => {
                    warn!(target: LOG_TARGET, "Connectivity failure to base node: {}", e);
                    self.update_state(BaseNodeState {
                        node_id: None,
                        chain_metadata: None,
                        is_synced: None,
                        updated: None,
                        latency: None,
                    })
                    .await;
                    continue;
                },
                Err(e @ BaseNodeMonitorError::InvalidBaseNodeResponse(_)) |
                Err(e @ BaseNodeMonitorError::WalletStorageError(_)) => {
                    error!(target: LOG_TARGET, "{}", e);
                    continue;
                },
            }
        }
        debug!(
            target: LOG_TARGET,
            "Base Node Service Monitor shutting down because it received the shutdown signal"
        );
    }

    async fn monitor_node(&mut self) -> Result<(), BaseNodeMonitorError> {
        let mut base_node_watch = self.wallet_connectivity.get_current_base_node_watcher();
        loop {
            let timer = Instant::now();
            let mut client = self
                .wallet_connectivity
                .obtain_base_node_wallet_rpc_client()
                .await
                .ok_or(BaseNodeMonitorError::NodeShuttingDown)?;
            trace!(target: LOG_TARGET, "Obtain RPC client {} ms", timer.elapsed().as_millis());

            let base_node_id = match self.wallet_connectivity.get_current_base_node_peer_node_id() {
                Some(n) => n,
                None => continue,
            };

            let timer = Instant::now();
            let tip_info = match interrupt(base_node_watch.changed(), client.get_tip_info()).await {
                Some(tip_info) => tip_info?,
                None => {
                    self.update_state(Default::default()).await;
                    continue;
                },
            };
            let chain_metadata = tip_info
                .metadata
                .ok_or_else(|| BaseNodeMonitorError::InvalidBaseNodeResponse("Tip info no metadata".to_string()))
                .and_then(|metadata| {
                    ChainMetadata::try_from(metadata).map_err(BaseNodeMonitorError::InvalidBaseNodeResponse)
                })?;
            trace!(target: LOG_TARGET, "Obtain tip info in {} ms", timer.elapsed().as_millis());

            let timer = Instant::now();
            let latency = match client.get_last_request_latency() {
                Some(latency) => latency,
                None => continue,
            };
            trace!(target: LOG_TARGET, "Obtain latency info in {} ms", timer.elapsed().as_millis());

            self.db.set_chain_metadata(chain_metadata.clone())?;

            let is_synced = tip_info.is_synced;
            let best_block_height = chain_metadata.best_block_height();

            let new_block = self
                .update_state(BaseNodeState {
                    node_id: Some(base_node_id.clone()),
                    chain_metadata: Some(chain_metadata),
                    is_synced: Some(is_synced),
                    updated: Some(Utc::now().naive_utc()),
                    latency: Some(latency),
                })
                .await;

            trace!(
                target: LOG_TARGET,
                "Base node {} Tip: {} ({}) Latency: {} ms",
                base_node_id,
                best_block_height,
                if is_synced { "Synced" } else { "Syncing..." },
                latency.as_millis()
            );

            // If there's a new block, try again immediately,
            if new_block {
                self.backoff_attempts = 0;
            } else {
                self.backoff_attempts += 1;
                let delay = time::sleep(
                    cmp::min(self.max_interval, self.backoff.calculate_backoff(self.backoff_attempts))
                        .saturating_sub(latency),
                );
                if interrupt(base_node_watch.changed(), delay).await.is_none() {
                    self.update_state(Default::default()).await;
                }
            }
        }

        // loop only exits on shutdown/error
        #[allow(unreachable_code)]
        Ok(())
    }

    // returns true if a new block, otherwise false
    async fn update_state(&self, new_state: BaseNodeState) -> bool {
        let mut lock = self.state.write().await;
        let (new_block_detected, height, hash) = match (new_state.chain_metadata.clone(), lock.chain_metadata.clone()) {
            (Some(new_metadata), Some(old_metadata)) => (
                new_metadata.best_block_hash() != old_metadata.best_block_hash(),
                new_metadata.best_block_height(),
                *new_metadata.best_block_hash(),
            ),
            (Some(new_metadata), _) => (true, new_metadata.best_block_height(), *new_metadata.best_block_hash()),
            (None, _) => (false, 0, BlockHashType::default()),
        };

        if new_block_detected {
            self.publish_event(BaseNodeEvent::NewBlockDetected(hash, height));
        }

        *lock = new_state.clone();

        self.publish_event(BaseNodeEvent::BaseNodeStateChanged(new_state));
        new_block_detected
    }

    fn publish_event(&self, event: BaseNodeEvent) {
        let _size = self.event_publisher.send(Arc::new(event));
    }
}

#[derive(thiserror::Error, Debug)]
enum BaseNodeMonitorError {
    #[error("Node is shutting down")]
    NodeShuttingDown,
    #[error("Rpc error: {0}")]
    RpcFailed(#[from] RpcError),
    #[error("Invalid base node response: {0}")]
    InvalidBaseNodeResponse(String),
    #[error("Wallet storage error: {0}")]
    WalletStorageError(#[from] WalletStorageError),
}

async fn interrupt<F1, F2>(interrupt: F1, fut: F2) -> Option<F2::Output>
where
    F1: Future,
    F2: Future,
{
    tokio::pin!(interrupt);
    tokio::pin!(fut);
    match future::select(interrupt, fut).await {
        Either::Left(_) => None,
        Either::Right((v, _)) => Some(v),
    }
}

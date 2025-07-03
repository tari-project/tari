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

use std::{sync::Arc, time::Duration};

use chrono::Utc;
use log::*;
use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_comms::protocol::rpc::RpcError;
use tari_shutdown::ShutdownSignal;
use tokio::{select, sync::RwLock, time::interval};

use crate::{
    base_node_service::{
        handle::{BaseNodeEvent, BaseNodeEventSender},
        service::BaseNodeState,
    },
    connectivity_service::WalletConnectivityInterface,
    error::WalletStorageError,
};

const LOG_TARGET: &str = "wallet::base_node_service::chain_metadata_monitor";

pub struct BaseNodeMonitor<TWalletConnectivity> {
    state: Arc<RwLock<BaseNodeState>>,
    wallet_connectivity: TWalletConnectivity,
    event_publisher: BaseNodeEventSender,
}

impl<TWalletConnectivity> BaseNodeMonitor<TWalletConnectivity>
where TWalletConnectivity: WalletConnectivityInterface
{
    pub fn new(
        state: Arc<RwLock<BaseNodeState>>,
        wallet_connectivity: TWalletConnectivity,
        event_publisher: BaseNodeEventSender,
    ) -> Self {
        Self {
            state,
            wallet_connectivity,
            event_publisher,
        }
    }

    pub async fn run(mut self, shutdown_signal: ShutdownSignal) {
        match self.monitor_node(shutdown_signal).await {
            Ok(_) => {
                debug!(
                    target: LOG_TARGET,
                    "Wallet Base Node Service chain metadata task completed successfully"
                );
            },

            Err(e @ BaseNodeMonitorError::RpcFailed(_)) => {
                warn!(target: LOG_TARGET, "Connectivity failure to base node: {}", e);
                self.update_state(BaseNodeState {
                    chain_metadata: None,
                    is_synced: None,
                    updated: None,
                    latency: None,
                })
                .await;
            },
            Err(e @ BaseNodeMonitorError::InvalidBaseNodeResponse(_)) |
            Err(e @ BaseNodeMonitorError::WalletStorageError(_)) => {
                error!(target: LOG_TARGET, "{}", e);
            },
        }
    }

    async fn monitor_node(&mut self, mut shutdown_signal: ShutdownSignal) -> Result<(), BaseNodeMonitorError> {
        let mut interval = interval(Duration::from_secs(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            select! {

                        _ = shutdown_signal.wait() => {
                                return Ok(());
                        },
                        _ = interval.tick() => {
                            // continue to the next iteration
                    let  client = self.wallet_connectivity.obtain_base_node_wallet_rpc_client().await;



                    let tip_info = client
                        .get_tip_info()
                        .await
                        .map_err(|e| BaseNodeMonitorError::InvalidBaseNodeResponse(e.to_string()))?;
                    let chain_metadata = tip_info
                        .metadata
                        .ok_or_else(|| BaseNodeMonitorError::InvalidBaseNodeResponse("Tip info no metadata".to_string()))?;

                    let latency = match client.get_last_request_latency() {
                        Some(latency) => latency,
                        None => {
                            continue;
                        },
                    };
                    debug!(
                        target: LOG_TARGET,
                        "Base node height:{} latency: {} ms",
                        chain_metadata.best_block_height(),
                        latency.as_millis()
                    );

                    let is_synced = tip_info.is_synced;
                    let best_block_height = chain_metadata.best_block_height();

                    self
                        .update_state(BaseNodeState {
                            chain_metadata: Some(chain_metadata),
                            is_synced: Some(is_synced),
                            updated: Some(Utc::now()),
                            latency: Some(latency),
                        })
                        .await;

                    trace!(
                        target: LOG_TARGET,
                        "Base node Tip: {} ({}) Latency: {} ms",
                        best_block_height,
                        if is_synced { "Synced" } else { "Syncing..." },
                        latency.as_millis()
                    );

               }
            }
        }

        // loop only exits on shutdown/error
        #[allow(unreachable_code)]
        Ok(())
    }

    // returns true if a new block, otherwise false
    async fn update_state(&self, new_state: BaseNodeState) {
        let mut lock = self.state.write().await;

        *lock = new_state.clone();

        self.publish_event(BaseNodeEvent::BaseNodeStateChanged(new_state));
    }

    fn publish_event(&self, event: BaseNodeEvent) {
        let _size = self.event_publisher.send(Arc::new(event));
    }
}

#[derive(thiserror::Error, Debug)]
enum BaseNodeMonitorError {
    #[error("Rpc error: {0}")]
    RpcFailed(#[from] RpcError),
    #[error("Invalid base node response: {0}")]
    InvalidBaseNodeResponse(String),
    #[error("Wallet storage error: {0}")]
    WalletStorageError(#[from] WalletStorageError),
}

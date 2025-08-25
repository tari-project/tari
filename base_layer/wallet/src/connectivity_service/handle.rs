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

use std::time::Duration;

use minotari_node_wallet_client::BaseNodeWalletClient;
use tokio::sync::watch;

use crate::{client::http_client_factory::HttpClientFactory, connectivity_service::WalletConnectivityInterface};

/// Sentinel used when the latency is unknown/unavailable.
pub const UNKNOWN_LATENCY_MS: u64 = 0;
/// Requests slower than this are considered degraded.
pub const DEGRADED_LATENCY_THRESHOLD: Duration = Duration::from_secs(10);

/// Connection status of the Base Node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnlineStatus {
    Connecting = 0,
    Online = 1,
    Offline = 2,
}

/// Extended connection status of the Base Node
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExtendedOnlineStatus {
    Connecting,
    Online { latency_ms: u64, url: String },
    Offline,
    Degraded { latency_ms: u64, url: String },
}

impl ExtendedOnlineStatus {
    pub fn as_u8(&self) -> u8 {
        match self {
            ExtendedOnlineStatus::Connecting => 0,
            ExtendedOnlineStatus::Online { .. } => 1,
            ExtendedOnlineStatus::Offline => 2,
            ExtendedOnlineStatus::Degraded { .. } => 3,
        }
    }
}

#[derive(Clone)]
pub struct WalletConnectivityHandle<TWalletClientFactory: HttpClientFactory> {
    client_factory: TWalletClientFactory,
    online_status_watch: watch::Sender<OnlineStatus>,
    extended_online_status_watch: watch::Sender<ExtendedOnlineStatus>,
}

impl<TWalletClientFactory: HttpClientFactory> WalletConnectivityHandle<TWalletClientFactory> {
    pub fn new(client_factory: TWalletClientFactory) -> Self {
        let (online_status_watch, _) = watch::channel(OnlineStatus::Connecting);
        let (extended_online_status_watch, _) = watch::channel(ExtendedOnlineStatus::Connecting);
        Self {
            client_factory,
            online_status_watch,
            extended_online_status_watch,
        }
    }
}

#[async_trait::async_trait]
impl<TWalletClientFactory: HttpClientFactory> WalletConnectivityInterface
    for WalletConnectivityHandle<TWalletClientFactory>
{
    type BaseNodeClient = TWalletClientFactory::Client;

    /// This can be relied on to obtain a pooled BaseNodeWalletRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeWalletRpcClient RPC session.
    async fn obtain_base_node_wallet_rpc_client(&self) -> Self::BaseNodeClient {
        self.client_factory.create_http_client()
    }

    async fn get_connectivity_status(&self) -> OnlineStatus {
        if self.client_factory.create_http_client().is_online().await {
            OnlineStatus::Online
        } else {
            OnlineStatus::Offline
        }
    }

    async fn get_extended_connectivity_status(&self) -> ExtendedOnlineStatus {
        let client = self.obtain_base_node_wallet_rpc_client().await;
        let status = if client.is_online().await {
            let url = client.get_address().await;
            if let Some(latency) = client.get_last_request_latency().await {
                let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
                if latency >= DEGRADED_LATENCY_THRESHOLD {
                    ExtendedOnlineStatus::Degraded { latency_ms, url }
                } else {
                    ExtendedOnlineStatus::Online { latency_ms, url }
                }
            } else {
                // Latency unavailable; report degraded with unknown latency sentinel.
                ExtendedOnlineStatus::Degraded {
                    latency_ms: UNKNOWN_LATENCY_MS,
                    url,
                }
            }
        } else {
            ExtendedOnlineStatus::Offline
        };
        let _unused = self.extended_online_status_watch.send(status.clone());

        status
    }

    fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
        self.online_status_watch.subscribe()
    }

    fn get_extended_connectivity_status_watch(&self) -> watch::Receiver<ExtendedOnlineStatus> {
        self.extended_online_status_watch.subscribe()
    }

    async fn get_last_request_latency(&self) -> Option<Duration> {
        let client = self.obtain_base_node_wallet_rpc_client().await;
        client.get_last_request_latency().await
    }

    async fn get_address(&self) -> String {
        let client = self.obtain_base_node_wallet_rpc_client().await;
        client.get_address().await
    }
}

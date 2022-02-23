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

use std::{mem, time::Duration};

use log::*;
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityRequester},
    peer_manager::{NodeId, Peer},
    protocol::rpc::{RpcClientLease, RpcClientPool},
    PeerConnection,
};
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time,
    time::MissedTickBehavior,
};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    connectivity_service::{error::WalletConnectivityError, handle::WalletConnectivityRequest},
    util::watch::Watch,
};

const LOG_TARGET: &str = "wallet::connectivity";

/// Connection status of the Base Node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnlineStatus {
    Connecting,
    Online,
    Offline,
}

pub struct WalletConnectivityService {
    config: BaseNodeServiceConfig,
    request_receiver: mpsc::Receiver<WalletConnectivityRequest>,
    connectivity: ConnectivityRequester,
    base_node_watch: watch::Receiver<Option<Peer>>,
    pools: Option<ClientPoolContainer>,
    online_status_watch: Watch<OnlineStatus>,
    pending_requests: Vec<ReplyOneshot>,
}

struct ClientPoolContainer {
    pub base_node_wallet_rpc_client: RpcClientPool<BaseNodeWalletRpcClient>,
    pub base_node_sync_rpc_client: RpcClientPool<BaseNodeSyncRpcClient>,
}

impl WalletConnectivityService {
    pub(super) fn new(
        config: BaseNodeServiceConfig,
        request_receiver: mpsc::Receiver<WalletConnectivityRequest>,
        base_node_watch: watch::Receiver<Option<Peer>>,
        online_status_watch: Watch<OnlineStatus>,
        connectivity: ConnectivityRequester,
    ) -> Self {
        Self {
            config,
            request_receiver,
            connectivity,
            base_node_watch,
            pools: None,
            pending_requests: Vec::new(),
            online_status_watch,
        }
    }

    pub async fn start(mut self) {
        debug!(target: LOG_TARGET, "Wallet connectivity service has started.");
        let mut check_connection =
            time::interval_at(time::Instant::now() + Duration::from_secs(5), Duration::from_secs(5));
        self.set_online_status(OnlineStatus::Offline);
        check_connection.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                // BIASED: select branches are in order of priority
                biased;

                Ok(_) = self.base_node_watch.changed() => {
                    if self.base_node_watch.borrow().is_some() {
                        // This will block the rest until the connection is established. This is what we want.
                        self.setup_base_node_connection().await;
                    }
                },

                Some(req) = self.request_receiver.recv() => {
                    self.handle_request(req).await;
                },

                _ = check_connection.tick() => {
                    self.check_connection().await;
                }
            }
        }
    }

    async fn check_connection(&mut self) {
        match self.pools.as_ref() {
            Some(pool) => {
                if !pool.base_node_wallet_rpc_client.is_connected().await {
                    debug!(target: LOG_TARGET, "Peer connection lost. Attempting to reconnect...");
                    self.set_online_status(OnlineStatus::Offline);
                    self.setup_base_node_connection().await;
                }
            },
            None => {
                debug!(target: LOG_TARGET, "No connection. Attempting to connect...");
                self.set_online_status(OnlineStatus::Offline);
                self.setup_base_node_connection().await;
            },
        }
    }

    async fn handle_request(&mut self, request: WalletConnectivityRequest) {
        use WalletConnectivityRequest::*;
        match request {
            ObtainBaseNodeWalletRpcClient(reply) => {
                self.handle_pool_request(reply.into()).await;
            },
            ObtainBaseNodeSyncRpcClient(reply) => {
                self.handle_pool_request(reply.into()).await;
            },
        }
    }

    async fn handle_pool_request(&mut self, reply: ReplyOneshot) {
        use ReplyOneshot::*;
        match reply {
            WalletRpc(tx) => self.handle_get_base_node_wallet_rpc_client(tx).await,
            SyncRpc(tx) => self.handle_get_base_node_sync_rpc_client(tx).await,
        }
    }

    async fn handle_get_base_node_wallet_rpc_client(
        &mut self,
        reply: oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>,
    ) {
        match self.pools {
            Some(ref pools) => match pools.base_node_wallet_rpc_client.get().await {
                Ok(client) => {
                    let _ = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node connection failed: {}. Reconnecting...", e
                    );
                    if let Some(node_id) = self.current_base_node() {
                        self.disconnect_base_node(node_id).await;
                    };
                    self.pending_requests.push(reply.into());
                },
            },
            None => {
                self.pending_requests.push(reply.into());
                if self.base_node_watch.borrow().is_none() {
                    warn!(
                        target: LOG_TARGET,
                        "{} requests are waiting for base node to be set",
                        self.pending_requests.len()
                    );
                }
            },
        }
    }

    async fn handle_get_base_node_sync_rpc_client(
        &mut self,
        reply: oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>,
    ) {
        match self.pools {
            Some(ref pools) => match pools.base_node_sync_rpc_client.get().await {
                Ok(client) => {
                    let _ = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node connection failed: {}. Reconnecting...", e
                    );
                    if let Some(node_id) = self.current_base_node() {
                        self.disconnect_base_node(node_id).await;
                    };
                    self.pending_requests.push(reply.into());
                },
            },
            None => {
                self.pending_requests.push(reply.into());
                if self.base_node_watch.borrow().is_none() {
                    warn!(
                        target: LOG_TARGET,
                        "{} requests are waiting for base node to be set",
                        self.pending_requests.len()
                    );
                }
            },
        }
    }

    fn current_base_node(&self) -> Option<NodeId> {
        self.base_node_watch.borrow().as_ref().map(|p| p.node_id.clone())
    }

    async fn disconnect_base_node(&mut self, node_id: NodeId) {
        if let Ok(Some(mut connection)) = self.connectivity.get_connection(node_id.clone()).await {
            match connection.disconnect().await {
                Ok(_) => debug!(target: LOG_TARGET, "Disconnected base node peer {}", node_id),
                Err(e) => error!(target: LOG_TARGET, "Failed to disconnect base node: {}", e),
            }
            self.pools = None;
        };
    }

    async fn setup_base_node_connection(&mut self) {
        self.pools = None;
        loop {
            let node_id = match self.current_base_node() {
                Some(n) => n,
                None => {
                    self.set_online_status(OnlineStatus::Offline);
                    return;
                },
            };
            debug!(
                target: LOG_TARGET,
                "Attempting to connect to base node peer {}...", node_id
            );
            self.set_online_status(OnlineStatus::Connecting);
            match self.try_setup_rpc_pool(node_id.clone()).await {
                Ok(true) => {
                    self.set_online_status(OnlineStatus::Online);
                    debug!(
                        target: LOG_TARGET,
                        "Wallet is ONLINE and connected to base node {}", node_id
                    );
                    break;
                },
                Ok(false) => {
                    debug!(
                        target: LOG_TARGET,
                        "The peer has changed while connecting. Attempting to connect to new base node."
                    );
                    continue;
                },
                Err(WalletConnectivityError::ConnectivityError(ConnectivityError::DialCancelled)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial was cancelled. Retrying after {}s ...",
                        self.config.base_node_monitor_refresh_interval.as_secs()
                    );
                    self.set_online_status(OnlineStatus::Offline);
                    time::sleep(self.config.base_node_monitor_refresh_interval).await;
                    continue;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "{}", e);
                    if self.current_base_node().as_ref() == Some(&node_id) {
                        self.disconnect_base_node(node_id).await;
                        self.set_online_status(OnlineStatus::Offline);
                        time::sleep(self.config.base_node_monitor_refresh_interval).await;
                    }
                    continue;
                },
            }
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        let _ = self.online_status_watch.send(status);
    }

    async fn try_setup_rpc_pool(&mut self, peer: NodeId) -> Result<bool, WalletConnectivityError> {
        let conn = match self.try_dial_peer(peer.clone()).await? {
            Some(c) => c,
            None => {
                warn!(target: LOG_TARGET, "Could not dial base node peer {}", peer);
                return Ok(false);
            },
        };
        debug!(
            target: LOG_TARGET,
            "Successfully established peer connection to base node {}",
            conn.peer_node_id()
        );
        self.pools = Some(ClientPoolContainer {
            base_node_sync_rpc_client: conn
                .create_rpc_client_pool(self.config.base_node_rpc_pool_size, Default::default()),
            base_node_wallet_rpc_client: conn
                .create_rpc_client_pool(self.config.base_node_rpc_pool_size, Default::default()),
        });
        self.notify_pending_requests().await?;
        debug!(target: LOG_TARGET, "Successfully established RPC connection {}", peer);
        Ok(true)
    }

    async fn try_dial_peer(&mut self, peer: NodeId) -> Result<Option<PeerConnection>, WalletConnectivityError> {
        tokio::select! {
            biased;

            _ = self.base_node_watch.changed() => {
                Ok(None)
            }
            result = self.connectivity.dial_peer(peer) => {
                Ok(Some(result?))
            }
        }
    }

    async fn notify_pending_requests(&mut self) -> Result<(), WalletConnectivityError> {
        let current_pending = mem::take(&mut self.pending_requests);
        for reply in current_pending {
            if reply.is_canceled() {
                continue;
            }

            self.handle_pool_request(reply).await;
        }
        Ok(())
    }
}

enum ReplyOneshot {
    WalletRpc(oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>),
    SyncRpc(oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>),
}

impl ReplyOneshot {
    pub fn is_canceled(&self) -> bool {
        use ReplyOneshot::*;
        match self {
            WalletRpc(tx) => tx.is_closed(),
            SyncRpc(tx) => tx.is_closed(),
        }
    }
}

impl From<oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>> for ReplyOneshot {
    fn from(tx: oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>) -> Self {
        ReplyOneshot::WalletRpc(tx)
    }
}
impl From<oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>> for ReplyOneshot {
    fn from(tx: oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>) -> Self {
        ReplyOneshot::SyncRpc(tx)
    }
}

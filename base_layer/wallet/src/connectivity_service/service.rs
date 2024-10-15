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
    collections::{HashMap, HashSet},
    mem,
    time::Duration,
};

use log::*;
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tari_network::{identity::PeerId, DialError, NetworkHandle, NetworkingService, ToPeerId};
use tari_rpc_framework::{
    pool::{RpcClientLease, RpcClientPool},
    RpcClient,
    RpcConnector,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time,
    time::MissedTickBehavior,
};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    connectivity_service::{error::WalletConnectivityError, handle::WalletConnectivityRequest, BaseNodePeerManager},
    util::watch::Watch,
};

const LOG_TARGET: &str = "wallet::connectivity";
pub(crate) const CONNECTIVITY_WAIT: u64 = 5;
pub(crate) const COOL_OFF_PERIOD: u64 = 60;

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
    network_handle: NetworkHandle,
    base_node_watch_receiver: watch::Receiver<Option<BaseNodePeerManager>>,
    base_node_watch: Watch<Option<BaseNodePeerManager>>,
    pools: HashMap<PeerId, ClientPoolContainer>,
    online_status_watch: Watch<OnlineStatus>,
    pending_requests: Vec<ReplyOneshot>,
}

struct ClientPoolContainer {
    pub base_node_wallet_rpc_client: RpcClientPool<NetworkHandle, BaseNodeWalletRpcClient>,
    pub base_node_sync_rpc_client: RpcClientPool<NetworkHandle, BaseNodeSyncRpcClient>,
}

impl WalletConnectivityService {
    pub(super) fn new(
        config: BaseNodeServiceConfig,
        request_receiver: mpsc::Receiver<WalletConnectivityRequest>,
        base_node_watch: Watch<Option<BaseNodePeerManager>>,
        online_status_watch: Watch<OnlineStatus>,
        network_handle: NetworkHandle,
    ) -> Self {
        Self {
            config,
            request_receiver,
            network_handle,
            base_node_watch_receiver: base_node_watch.get_receiver(),
            base_node_watch,
            pools: HashMap::new(),
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

                Ok(_) = self.base_node_watch_receiver.changed() => {
                    if self.base_node_watch_receiver.borrow().is_some() {
                        // This will block the rest until the connection is established. This is what we want.
                        trace!(target: LOG_TARGET, "start: base_node_watch_receiver.changed");
                        self.check_connection().await;
                    }
                },

                Some(req) = self.request_receiver.recv() => {
                    self.handle_request(req).await;
                },

                _ = check_connection.tick() => {
                    trace!(target: LOG_TARGET, "start: check_connection.tick");
                    self.check_connection().await;
                }
            }
        }
    }

    async fn check_connection(&mut self) {
        if let Some(peer_manager) = self.get_base_node_peer_manager() {
            let current_base_node = peer_manager.get_current_peer_id();
            trace!(target: LOG_TARGET, "check_connection: has current_base_node");
            if let Ok(Some(_)) = self.network_handle.get_connection(current_base_node).await {
                trace!(target: LOG_TARGET, "check_connection: has connection");
                trace!(target: LOG_TARGET, "check_connection: is connected");
                if self.pools.contains_key(&current_base_node) {
                    trace!(target: LOG_TARGET, "check_connection: has rpc pool");
                    trace!(target: LOG_TARGET, "check_connection: rpc pool is already connected");
                    self.set_online_status(OnlineStatus::Online);
                    return;
                    // debug!(
                    //     target: LOG_TARGET,
                    //     "Peer RPC connection '{:?}' lost. Attempting to reconnect...",
                    //     self.current_base_node()
                    // );
                }
                trace!(target: LOG_TARGET, "check_connection: no rpc pool for connection");
                trace!(target: LOG_TARGET, "check_connection: current base node has connection but not connected");
            }
            trace!(
                target: LOG_TARGET,
                "check_connection: current base node has no connection, setup connection to: '{}'",
                peer_manager
            );
            self.set_online_status(OnlineStatus::Connecting);
            self.setup_base_node_connection().await;
        } else {
            self.set_online_status(OnlineStatus::Offline);
            debug!(target: LOG_TARGET, "Base node peer manager has not been set, cannot connect");
        }
    }

    async fn handle_request(&mut self, request: WalletConnectivityRequest) {
        use WalletConnectivityRequest::{ObtainBaseNodeSyncRpcClient, ObtainBaseNodeWalletRpcClient};
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
        use ReplyOneshot::{SyncRpc, WalletRpc};
        match reply {
            WalletRpc(tx) => self.handle_get_base_node_wallet_rpc_client(tx).await,
            SyncRpc(tx) => self.handle_get_base_node_sync_rpc_client(tx).await,
        }
    }

    async fn handle_get_base_node_wallet_rpc_client(
        &mut self,
        reply: oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>,
    ) {
        let node_id = if let Some(val) = self.current_base_node() {
            val
        } else {
            self.pending_requests.push(reply.into());
            warn!(target: LOG_TARGET, "{} wallet requests waiting for connection", self.pending_requests.len());
            return;
        };

        match self.pools.get(&node_id) {
            Some(pools) => match pools.base_node_wallet_rpc_client.get().await {
                Ok(client) => {
                    let _result = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node '{}' wallet RPC pool connection failed ({}). Reconnecting...",
                        node_id,
                        e
                    );
                    if let Some(node_id) = self.current_base_node() {
                        self.disconnect_base_node(node_id).await;
                    };
                    self.pending_requests.push(reply.into());
                },
            },
            None => {
                self.pending_requests.push(reply.into());
                warn!(
                    target: LOG_TARGET,
                    "Wallet RPC pool for base node `{}` not found, {} requests waiting",
                    node_id,
                    self.pending_requests.len()
                );
            },
        }
    }

    async fn handle_get_base_node_sync_rpc_client(
        &mut self,
        reply: oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>,
    ) {
        let node_id = if let Some(val) = self.current_base_node() {
            val
        } else {
            self.pending_requests.push(reply.into());
            warn!(target: LOG_TARGET, "{} sync requests waiting for connection", self.pending_requests.len());
            return;
        };

        match self.pools.get(&node_id) {
            Some(pools) => match pools.base_node_sync_rpc_client.get().await {
                Ok(client) => {
                    let _result = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node '{}' sync RPC pool connection failed ({}). Reconnecting...",
                        node_id,
                        e
                    );
                    if let Some(node_id) = self.current_base_node() {
                        self.disconnect_base_node(node_id).await;
                    };
                    self.pending_requests.push(reply.into());
                },
            },
            None => {
                self.pending_requests.push(reply.into());
                warn!(
                    target: LOG_TARGET,
                    "Sync RPC pool for base node `{}` not found, {} requests waiting",
                    node_id,
                    self.pending_requests.len()
                );
            },
        }
    }

    fn current_base_node(&self) -> Option<PeerId> {
        self.base_node_watch_receiver
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer_id())
    }

    fn get_base_node_peer_manager(&self) -> Option<BaseNodePeerManager> {
        self.base_node_watch_receiver.borrow().as_ref().map(|p| p.clone())
    }

    async fn disconnect_base_node(&mut self, peer_id: PeerId) {
        if let Err(e) = self.network_handle.disconnect_peer(peer_id).await {
            error!(target: LOG_TARGET, "Failed to disconnect base node: {}", e);
        }
        self.pools.remove(&peer_id);
    }

    async fn setup_base_node_connection(&mut self) {
        let Some(mut peer_manager) = self.get_base_node_peer_manager() else {
            debug!(target: LOG_TARGET, "No base node peer manager set");
            return;
        };
        loop {
            let peer_id = if let Some(time) = peer_manager.time_since_last_connection_attempt() {
                if time < Duration::from_secs(COOL_OFF_PERIOD) &&
                    peer_manager.get_current_peer_id().to_peer_id() == peer_manager.get_next_peer_id().to_peer_id()
                {
                    // If we only have one peer in the list, wait a bit before retrying
                    time::sleep(Duration::from_secs(CONNECTIVITY_WAIT)).await;
                }

                peer_manager.get_current_peer_id().to_peer_id()
            } else {
                peer_manager.get_current_peer_id().to_peer_id()
            };
            peer_manager.set_last_connection_attempt();

            debug!(
                target: LOG_TARGET,
                "Attempting to connect to base node peer '{}'... (last attempt {:?})",
                peer_id,
                peer_manager.time_since_last_connection_attempt()
            );
            self.pools.remove(&peer_id);
            match self.try_setup_rpc_pool(peer_id).await {
                Ok(true) => {
                    // if self.peer_list_change_detected(&peer_manager) {
                    //     debug!(
                    //         target: LOG_TARGET,
                    //         "The peer list has changed while connecting, aborting connection attempt."
                    //     );
                    //     self.set_online_status(OnlineStatus::Offline);
                    //     break;
                    // }
                    self.base_node_watch.send(Some(peer_manager.clone()));
                    if let Err(e) = self.notify_pending_requests().await {
                        warn!(target: LOG_TARGET, "Error notifying pending RPC requests: {}", e);
                    }
                    self.set_online_status(OnlineStatus::Online);
                    debug!(
                        target: LOG_TARGET,
                        "Wallet is ONLINE and connected to base node '{}'", peer_id
                    );
                    break;
                },
                Ok(false) => {
                    debug!(
                        target: LOG_TARGET,
                        "The peer has changed while connecting. Attempting to connect to new base node."
                    );
                },
                Err(WalletConnectivityError::DialError(DialError::Aborted)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial was cancelled. Retrying after {}s ...",
                        Duration::from_secs(CONNECTIVITY_WAIT).as_secs()
                    );
                    time::sleep(Duration::from_secs(CONNECTIVITY_WAIT)).await;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "{}", e);
                    if self.current_base_node().as_ref() == Some(&peer_id) {
                        self.disconnect_base_node(peer_id).await;
                        time::sleep(Duration::from_secs(CONNECTIVITY_WAIT)).await;
                    }
                },
            }
            if self.peer_list_change_detected(&peer_manager) {
                debug!(
                    target: LOG_TARGET,
                    "The peer list has changed while connecting, aborting connection attempt."
                );
                self.set_online_status(OnlineStatus::Offline);
                break;
            }
        }
    }

    fn peer_list_change_detected(&self, peer_manager: &BaseNodePeerManager) -> bool {
        if let Some(current) = self.get_base_node_peer_manager() {
            let (_, current_list) = current.get_state();
            let (_, list) = peer_manager.get_state();

            if current_list.len() != list.len() {
                return true;
            }
            // Check the lists are the same, disregarding ordering
            let mut c = current_list.iter().map(|p| p.peer_id()).collect::<HashSet<_>>();
            for p in list {
                if !c.remove(&p.peer_id()) {
                    return true;
                }
            }
            !c.is_empty()
        } else {
            true
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        self.online_status_watch.send(status);
    }

    async fn try_setup_rpc_pool(&mut self, peer_id: PeerId) -> Result<bool, WalletConnectivityError> {
        let container = ClientPoolContainer {
            base_node_sync_rpc_client: self
                .network_handle
                .create_rpc_client_pool(1, RpcClient::builder(peer_id)),
            base_node_wallet_rpc_client: self
                .network_handle
                .create_rpc_client_pool(self.config.base_node_rpc_pool_size, RpcClient::builder(peer_id)),
        };
        match container.base_node_wallet_rpc_client.get().await {
            Ok(a) => a,
            Err(err) => {
                error!(target: LOG_TARGET, "{err}");
                return Ok(false);
            },
        };

        self.pools.insert(peer_id, container);

        debug!(target: LOG_TARGET, "Successfully established RPC connection to base node '{}'", peer_id);
        Ok(true)
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
        use ReplyOneshot::{SyncRpc, WalletRpc};
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

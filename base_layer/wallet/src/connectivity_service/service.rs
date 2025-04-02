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
    cmp::{max, min},
    collections::HashMap,
    mem,
    time::Duration,
};

use log::*;
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityRequester},
    peer_manager::NodeId,
    protocol::rpc::{RpcClientLease, RpcClientPool},
    Minimized,
    PeerConnection,
};
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time,
    time::{timeout, Duration as TokioDuration, MissedTickBehavior},
};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    connectivity_service::{error::WalletConnectivityError, handle::WalletConnectivityRequest, BaseNodePeerManager},
    util::watch::Watch,
};

const LOG_TARGET: &str = "wallet::connectivity";
pub(crate) const CONNECTIVITY_WAIT: u64 = 5;

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
    base_node_watch_receiver: watch::Receiver<Option<BaseNodePeerManager>>,
    base_node_watch: Watch<Option<BaseNodePeerManager>>,
    pools: HashMap<NodeId, ClientPoolContainer>,
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
        base_node_watch: Watch<Option<BaseNodePeerManager>>,
        online_status_watch: Watch<OnlineStatus>,
        connectivity: ConnectivityRequester,
    ) -> Self {
        Self {
            config,
            request_receiver,
            connectivity,
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
            let current_base_node = peer_manager.get_current_peer().node_id.clone();
            trace!(target: LOG_TARGET, "check_connection: has current_base_node");
            if let Ok(Some(connection)) = self.connectivity.get_connection(current_base_node.clone()).await {
                trace!(target: LOG_TARGET, "check_connection: has connection");
                if connection.is_connected() {
                    trace!(target: LOG_TARGET, "check_connection: is connected");
                    if let Some(pool) = self.pools.get(&current_base_node) {
                        trace!(target: LOG_TARGET, "check_connection: has rpc pool");
                        if pool.base_node_wallet_rpc_client.is_connected().await {
                            trace!(target: LOG_TARGET, "check_connection: rpc pool is already connected");
                            self.set_online_status(OnlineStatus::Online);
                            return;
                        }
                        debug!(
                            target: LOG_TARGET,
                            "Peer RPC connection '{:?}' lost. Attempting to reconnect...",
                            self.current_base_node()
                        );
                    }
                    trace!(target: LOG_TARGET, "check_connection: no rpc pool for connection");
                }
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
            debug!(target: LOG_TARGET, "Base node peer manger has not been set, cannot connect");
        }
    }

    async fn handle_request(&mut self, request: WalletConnectivityRequest) {
        use WalletConnectivityRequest::{
            DisconnectBaseNode,
            ObtainBaseNodeSyncRpcClient,
            ObtainBaseNodeWalletRpcClient,
        };
        match request {
            ObtainBaseNodeWalletRpcClient(reply) => {
                self.handle_pool_request(reply.into()).await;
            },
            ObtainBaseNodeSyncRpcClient(reply) => {
                self.handle_pool_request(reply.into()).await;
            },
            DisconnectBaseNode(node_id) => {
                self.disconnect_base_node(node_id).await;
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
                    debug!(target: LOG_TARGET, "Obtained pool RPC 'wallet' connection to base node '{}'", node_id);
                    let _result = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node '{}' pool RPC 'wallet' connection failed ({}). Reconnecting...",
                        node_id,
                        e
                    );
                    self.disconnect_base_node(node_id).await;
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
                    debug!(target: LOG_TARGET, "Obtained pool RPC 'sync' connection to base node '{}'", node_id);
                    let _result = reply.send(client);
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Base node '{}' pool RPC 'sync' connection failed ({}). Reconnecting...",
                        node_id,
                        e
                    );
                    self.disconnect_base_node(node_id).await;
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

    fn current_base_node(&self) -> Option<NodeId> {
        self.base_node_watch_receiver
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer().node_id.clone())
    }

    fn get_base_node_peer_manager(&self) -> Option<BaseNodePeerManager> {
        self.base_node_watch_receiver.borrow().as_ref().map(|p| p.clone())
    }

    async fn disconnect_base_node(&mut self, node_id: NodeId) {
        if let Ok(Some(mut connection)) = self.connectivity.get_connection(node_id.clone()).await {
            match connection.disconnect(Minimized::No).await {
                Ok(_) => debug!(target: LOG_TARGET, "Disconnected base node peer {}", node_id),
                Err(e) => error!(target: LOG_TARGET, "Failed to disconnect base node: {}", e),
            }
            self.pools.remove(&node_id);
            // We want to ensure any active RPC clients are dropped when this connection (a clone) is dropped
            connection.set_force_disconnect_rpc_clients_when_clone_drops();
        };
    }

    async fn setup_base_node_connection(&mut self) {
        let mut peer_manager = if let Some(val) = self.get_base_node_peer_manager() {
            val
        } else {
            return;
        };
        let mut loop_count = 0;
        let number_of_seeds = peer_manager.get_state().1.len();
        loop {
            loop_count += 1;
            let node_id = if let Some(_time) = peer_manager.time_since_last_connection_attempt() {
                if peer_manager.get_current_peer().node_id == peer_manager.get_next_peer().node_id {
                    // If we only have one peer in the list, wait a bit before retrying
                    debug!(target: LOG_TARGET,
                        "Retrying after {}s ...",
                        Duration::from_secs(CONNECTIVITY_WAIT).as_secs()
                    );
                    time::sleep(Duration::from_secs(CONNECTIVITY_WAIT)).await;
                }
                // If 'peer_manager.get_next_peer()' is called, 'current_peer' is advanced to the next peer
                peer_manager.get_current_peer().node_id
            } else {
                peer_manager.get_current_peer().node_id
            };
            peer_manager.set_last_connection_attempt();

            debug!(
                target: LOG_TARGET,
                "Attempting base node peer '{}'... (last attempt {:?})",
                node_id,
                peer_manager.time_since_last_connection_attempt()
            );
            self.pools.remove(&node_id);
            match self
                .try_setup_rpc_pool(node_id.clone(), loop_count / number_of_seeds + 1)
                .await
            {
                Ok(true) => {
                    if self.peer_list_change_detected(&peer_manager) {
                        debug!(
                            target: LOG_TARGET,
                            "The peer list has changed while connecting, aborting connection attempt."
                        );
                        self.set_online_status(OnlineStatus::Offline);
                        break;
                    }
                    self.base_node_watch.send(Some(peer_manager.clone()));
                    if let Ok(true) = self.notify_pending_requests().await {
                        self.set_online_status(OnlineStatus::Online);
                        debug!(
                            target: LOG_TARGET,
                            "Wallet is ONLINE and connected to base node '{}'", node_id
                        );
                    }
                    break;
                },
                Ok(false) => {
                    debug!(
                        target: LOG_TARGET,
                        "The peer has changed while connecting. Attempting to connect to new base node."
                    );
                    self.disconnect_base_node(node_id).await;
                },
                Err(WalletConnectivityError::ConnectivityError(ConnectivityError::DialCancelled)) => {
                    debug!(target: LOG_TARGET, "Dial was cancelled.");
                    self.disconnect_base_node(node_id).await;
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "{}", e);
                    self.disconnect_base_node(node_id).await;
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
            current
                .get_state()
                .1
                .iter()
                .map(|p| p.node_id.clone())
                .collect::<Vec<_>>() !=
                peer_manager
                    .get_state()
                    .1
                    .iter()
                    .map(|p| p.node_id.clone())
                    .collect::<Vec<_>>()
        } else {
            true
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        self.online_status_watch.send(status);
    }

    async fn try_setup_rpc_pool(
        &mut self,
        peer_node_id: NodeId,
        dial_cycle: usize,
    ) -> Result<bool, WalletConnectivityError> {
        // dial_timeout: 1 = 1s, 2 = 10s, 3 = 20s, 4 = 30s, 5 = 40s, 6 = 50s, 7 = 60s, 8 = 70s, 9 = 80s, 10 = 90s
        let dial_timeout = TokioDuration::from_secs(min((max(1, 10 * (dial_cycle.saturating_sub(1)))) as u64, 90));
        trace!(target: LOG_TARGET, "Attempt dial with client timeout {:?}", dial_timeout);
        let conn = match timeout(dial_timeout, self.try_dial_peer(peer_node_id.clone())).await {
            Ok(Ok(Some(c))) => c,
            Ok(Ok(None)) => {
                warn!(target: LOG_TARGET, "Could not dial base node peer '{}'", peer_node_id);
                return Ok(false);
            },
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(WalletConnectivityError::ConnectivityError(
                    ConnectivityError::ClientCancelled(format!(
                        "Could not connect to '{}' in {:?}",
                        peer_node_id, dial_timeout
                    )),
                ));
            },
        };
        debug!(
            target: LOG_TARGET,
            "Established peer connection to base node '{}'",
            conn.peer_node_id()
        );
        self.pools.insert(peer_node_id.clone(), ClientPoolContainer {
            base_node_sync_rpc_client: conn.create_rpc_client_pool(1, Default::default()),
            base_node_wallet_rpc_client: conn
                .create_rpc_client_pool(self.config.base_node_rpc_pool_size, Default::default()),
        });
        trace!(target: LOG_TARGET, "Created RPC pools for '{}'", peer_node_id);
        Ok(true)
    }

    async fn try_dial_peer(&mut self, peer: NodeId) -> Result<Option<PeerConnection>, WalletConnectivityError> {
        tokio::select! {
            biased;

            _ = self.base_node_watch_receiver.changed() => {
                Ok(None)
            }
            result = self.connectivity.dial_peer(peer) => {
                Ok(Some(result?))
            }
        }
    }

    async fn notify_pending_requests(&mut self) -> Result<bool, WalletConnectivityError> {
        let current_pending = mem::take(&mut self.pending_requests);
        let mut count = 0;
        let current_pending_len = current_pending.len();
        for reply in current_pending {
            if reply.is_canceled() {
                continue;
            }
            count += 1;
            trace!(target: LOG_TARGET, "Handle {} of {} pending RPC pool requests", count, current_pending_len);
            self.handle_pool_request(reply).await;
        }
        if self.pending_requests.is_empty() {
            Ok(true)
        } else {
            warn!(target: LOG_TARGET, "{} of {} pending RPC pool requests not handled", count, current_pending_len);
            Ok(false)
        }
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

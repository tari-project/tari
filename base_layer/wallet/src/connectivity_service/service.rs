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

use futures::{future, future::Either};
use log::*;
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tari_network::{identity::PeerId, DialError, NetworkHandle, NetworkingService};
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
pub(crate) const CONNECTIVITY_WAIT: Duration = Duration::from_secs(5);

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
    current_pool: Option<ClientPoolContainer>,
    online_status_watch: Watch<OnlineStatus>,
    pending_requests: Vec<ReplyOneshot>,
}

struct ClientPoolContainer {
    pub peer_id: PeerId,
    pub base_node_wallet_rpc_client: RpcClientPool<NetworkHandle, BaseNodeWalletRpcClient>,
    pub base_node_sync_rpc_client: RpcClientPool<NetworkHandle, BaseNodeSyncRpcClient>,
}

impl ClientPoolContainer {
    pub async fn close(self) {
        self.base_node_wallet_rpc_client.close().await;
        self.base_node_sync_rpc_client.close().await;
    }
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
            // base_node_watch,
            current_pool: None,
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
                        self.check_connection_and_connect_if_required().await;
                    }
                },

                Some(req) = self.request_receiver.recv() => {
                    self.handle_request(req).await;
                },

                _ = check_connection.tick() => {
                    trace!(target: LOG_TARGET, "start: check_connection.tick");
                    self.check_connection_and_connect_if_required().await;
                }
            }
        }
    }

    async fn check_connection_and_connect_if_required(&mut self) {
        if let Some(peer_manager) = self.get_base_node_peer_manager() {
            let current_base_node = peer_manager.get_current_peer_id();
            trace!(target: LOG_TARGET, "check_connection: has current_base_node");
            if let Ok(Some(conn)) = self.network_handle.get_connection(current_base_node).await {
                trace!(target: LOG_TARGET, "check_connection: has connection with ID {}", conn.connection_id);
                trace!(target: LOG_TARGET, "check_connection: is connected");
                match self.current_pool.as_ref() {
                    Some(pool) if pool.peer_id == current_base_node => {
                        trace!(target: LOG_TARGET, "check_connection: has rpc pool");
                        trace!(target: LOG_TARGET, "check_connection: rpc pool is already connected");
                        pool.base_node_sync_rpc_client.clear_unused_leases().await;
                        pool.base_node_wallet_rpc_client.clear_unused_leases().await;
                        self.set_online_status(OnlineStatus::Online);
                        return;
                    },
                    Some(pool) => {
                        warn!(target: LOG_TARGET, "check_connection: current pool connected to peer {} but the base node peer is {}", pool.peer_id, current_base_node);
                    },
                    None => {
                        info!(target: LOG_TARGET, "check_connection: current base node has connection but no rpc pool for connection");
                    },
                }
            }
            trace!(
                target: LOG_TARGET,
                "check_connection: current base node has no connection, setup connection to: '{}'",
                peer_manager
            );
            self.setup_base_node_connection(peer_manager).await;
        } else {
            self.set_online_status(OnlineStatus::Offline);
            debug!(target: LOG_TARGET, "Base node peer manager has not been set, cannot connect");
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

        match self.current_pool {
            Some(ref pools) => match pools.base_node_wallet_rpc_client.get().await {
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

        match self.current_pool {
            Some(ref pools) => match pools.base_node_sync_rpc_client.get().await {
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

    fn current_base_node(&self) -> Option<PeerId> {
        self.base_node_watch_receiver
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer_id())
    }

    fn get_base_node_peer_manager(&self) -> Option<BaseNodePeerManager> {
        self.base_node_watch_receiver.borrow().as_ref().cloned()
    }

    async fn disconnect_base_node(&mut self, peer_id: PeerId) {
        if let Some(pool) = self.current_pool.take() {
            pool.close().await;
        }
        if let Err(e) = self.network_handle.disconnect_peer(peer_id).await {
            error!(target: LOG_TARGET, "Failed to disconnect base node: {}", e);
        }
    }

    async fn setup_base_node_connection(&mut self, mut peer_manager: BaseNodePeerManager) {
        let mut peer_id = peer_manager.get_current_peer_id();
        loop {
            self.set_online_status(OnlineStatus::Connecting);
            let maybe_last_attempt = peer_manager.time_since_last_connection_attempt();

            debug!(
                target: LOG_TARGET,
                "Attempting to connect to base node peer '{}'... (last attempt {:?})",
                peer_id,
                maybe_last_attempt
            );

            peer_manager.set_last_connection_attempt();

            match self.try_setup_rpc_pool(peer_id).await {
                Ok(true) => {
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

                    // NOTE: we do not strictly need to update our local copy of BaseNodePeerManager since state is
                    // atomically shared. However, since None is a possibility (although in practice
                    // it should never be) we handle that here.
                    peer_manager = match self.get_base_node_peer_manager() {
                        Some(pm) => pm,
                        None => {
                            warn!(target: LOG_TARGET, "⚠️ NEVER HAPPEN: Base node peer manager set to None while connecting");
                            return;
                        },
                    };
                    self.disconnect_base_node(peer_id).await;
                    self.set_online_status(OnlineStatus::Offline);
                },
                Err(WalletConnectivityError::DialError(DialError::Aborted)) => {
                    debug!(target: LOG_TARGET, "Dial was cancelled.");
                    self.disconnect_base_node(peer_id).await;
                    self.set_online_status(OnlineStatus::Offline);
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "{}", e);
                    self.disconnect_base_node(peer_id).await;
                    self.set_online_status(OnlineStatus::Offline);
                },
            }

            // Select the next peer (if available)
            let next_peer_id = peer_manager.select_next_peer().peer_id();
            // If we only have one peer in the list, wait a bit before retrying
            if peer_id == next_peer_id {
                debug!(target: LOG_TARGET,
                    "Only single peer in base node peer list. Waiting {}s before retrying again ...",
                    CONNECTIVITY_WAIT.as_secs()
                );
                time::sleep(CONNECTIVITY_WAIT).await;
            }
            peer_id = next_peer_id;
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        if *self.online_status_watch.borrow() == status {
            return;
        }
        self.online_status_watch.send(status);
    }

    async fn try_setup_rpc_pool(&mut self, peer_id: PeerId) -> Result<bool, WalletConnectivityError> {
        let container = ClientPoolContainer {
            peer_id,
            base_node_sync_rpc_client: self
                .network_handle
                .create_rpc_client_pool(1, RpcClient::builder(peer_id)),
            base_node_wallet_rpc_client: self
                .network_handle
                .create_rpc_client_pool(self.config.base_node_rpc_pool_size, RpcClient::builder(peer_id)),
        };

        // Create the first RPC session to ensure that we can connect.
        {
            let connect_fut = container.base_node_wallet_rpc_client.get();
            futures::pin_mut!(connect_fut);
            let bn_changed_fut = self.base_node_watch_receiver.changed();
            futures::pin_mut!(bn_changed_fut);
            let client = match future::select(connect_fut, bn_changed_fut).await {
                Either::Left((result, _)) => result?,
                Either::Right(_) => return Ok(false),
            };
            if client.is_connected() {
                debug!(
                    target: LOG_TARGET,
                    "Established peer connection to base node '{}'",
                    peer_id
                );
            } else {
                return Err(WalletConnectivityError::ClientConnectionLost);
            }
        }

        if let Some(container) = self.current_pool.replace(container) {
            container.close().await;
        }

        debug!(target: LOG_TARGET, "Created RPC pools for '{}'", peer_id);
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

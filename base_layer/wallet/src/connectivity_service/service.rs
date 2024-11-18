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

use std::{mem, pin::pin, time::Duration};

use futures::{future, future::Either};
use log::*;
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tari_network::{
    identity::PeerId,
    swarm::dial_opts::{DialOpts, PeerCondition},
    DialError,
    NetworkHandle,
    NetworkingService,
    Peer,
};
use tari_rpc_framework::{
    pool::{RpcClientLease, RpcClientPool},
    RpcClient,
    RpcConnector,
};
use tokio::{
    sync::{mpsc, oneshot},
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
    Connecting = 0,
    Online,
    Offline,
}

pub struct WalletConnectivityService {
    config: BaseNodeServiceConfig,
    request_receiver: mpsc::Receiver<WalletConnectivityRequest>,
    network_handle: NetworkHandle,
    base_node_watch: Watch<Option<BaseNodePeerManager>>,
    current_pool: Option<ClientPoolContainer>,
    online_status_watch: Watch<OnlineStatus>,
    pending_requests: Vec<ReplyOneshot>,
    last_attempted_peer: Option<PeerId>,
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
            base_node_watch,
            current_pool: None,
            pending_requests: Vec::new(),
            online_status_watch,
            last_attempted_peer: None,
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

                _ = self.base_node_watch.changed() => {
                    if self.base_node_watch.borrow().is_some() {
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
                match self.current_pool.as_ref() {
                    Some(pool) if pool.peer_id == current_base_node => {
                        trace!(target: LOG_TARGET, "check_connection: has rpc pool, already connected");
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
            debug!(target: LOG_TARGET, "{} wallet requests waiting for connection", self.pending_requests.len());
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
        self.base_node_watch.borrow().as_ref().map(|p| p.get_current_peer_id())
    }

    fn get_base_node_peer_manager(&self) -> Option<BaseNodePeerManager> {
        self.base_node_watch.borrow().as_ref().cloned()
    }

    async fn disconnect_base_node(&mut self, peer_id: PeerId) {
        trace!(target: LOG_TARGET, "Disconnecting base node '{}'...", peer_id);
        if let Some(pool) = self.current_pool.take() {
            pool.close().await;
        }
        if let Err(e) = self.network_handle.disconnect_peer(peer_id).await {
            error!(target: LOG_TARGET, "Failed to disconnect base node: {}", e);
        }
    }

    async fn setup_base_node_connection(&mut self, mut peer_manager: BaseNodePeerManager) {
        let mut peer = if self.last_attempted_peer.is_some() {
            peer_manager.select_next_peer().clone()
        } else {
            peer_manager.get_current_peer().clone()
        };

        loop {
            self.set_online_status(OnlineStatus::Connecting);
            match self.try_setup_rpc_pool(&peer).await {
                Ok(true) => {
                    self.base_node_watch.send(Some(peer_manager.clone()));
                    self.notify_pending_requests().await;
                    self.set_online_status(OnlineStatus::Online);
                    debug!(
                        target: LOG_TARGET,
                        "Wallet is ONLINE and connected to base node '{}'", peer
                    );
                    break;
                },
                Ok(false) => {
                    debug!(
                        target: LOG_TARGET,
                        "The peer has changed while connecting. Attempting to connect to new base node."
                    );
                    self.disconnect_base_node(peer.peer_id()).await;
                    self.set_online_status(OnlineStatus::Offline);
                    return;
                },
                Err(WalletConnectivityError::DialError(DialError::Aborted)) => {
                    debug!(target: LOG_TARGET, "Dial was cancelled.");
                    self.disconnect_base_node(peer.peer_id()).await;
                    self.set_online_status(OnlineStatus::Offline);
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "{}", e);
                    self.disconnect_base_node(peer.peer_id()).await;
                    self.set_online_status(OnlineStatus::Offline);
                },
            }

            // Select the next peer (if available)
            let next_peer = peer_manager.select_next_peer().clone();
            // If we only have one peer in the list, wait a bit before retrying
            if peer.peer_id() == next_peer.peer_id() {
                debug!(target: LOG_TARGET,
                    "Only single peer in base node peer list. Waiting {}s before retrying again ...",
                    CONNECTIVITY_WAIT.as_secs()
                );
                time::sleep(CONNECTIVITY_WAIT).await;
            }
            peer = next_peer;
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        if *self.online_status_watch.borrow() == status {
            return;
        }
        self.online_status_watch.send(status);
    }

    async fn try_setup_rpc_pool(&mut self, peer: &Peer) -> Result<bool, WalletConnectivityError> {
        self.last_attempted_peer = Some(peer.peer_id());
        let peer_id = peer.peer_id();
        let dial_wait = self
            .network_handle
            .dial_peer(
                DialOpts::peer_id(peer.peer_id())
                    .condition(PeerCondition::DisconnectedAndNotDialing)
                    .addresses(peer.addresses().to_vec())
                    .build(),
            )
            .await?;

        let container = ClientPoolContainer {
            peer_id,
            base_node_sync_rpc_client: self
                .network_handle
                .create_rpc_client_pool(1, RpcClient::builder(peer_id)),
            base_node_wallet_rpc_client: self
                .network_handle
                .create_rpc_client_pool(self.config.max_base_node_rpc_pool_size, RpcClient::builder(peer_id)),
        };

        // Create the first RPC session to ensure that we can connect.
        {
            let mut bn_changed_fut = pin!(self.base_node_watch.changed());
            match future::select(dial_wait, &mut bn_changed_fut).await {
                Either::Left((result, _)) => result?,
                Either::Right(_) => return Ok(false),
            };
            debug!(target: LOG_TARGET, "Dial succeeded for {peer_id}");
            let connect_fut = pin!(container.base_node_wallet_rpc_client.get());
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

    async fn notify_pending_requests(&mut self) {
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
        if !self.pending_requests.is_empty() {
            warn!(target: LOG_TARGET, "{} of {} pending RPC pool requests not handled", count, current_pending_len);
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

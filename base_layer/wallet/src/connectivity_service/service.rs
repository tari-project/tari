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
    base_node_service::config::BaseNodeServiceConfig,
    connectivity_service::{error::WalletConnectivityError, handle::WalletConnectivityRequest, watch::Watch},
};
use core::mem;
use futures::{
    channel::{mpsc, oneshot},
    future,
    future::Either,
    stream::Fuse,
    StreamExt,
};
use log::*;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer},
    protocol::rpc::{RpcClientLease, RpcClientPool},
    PeerConnection,
};
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::time;

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
    request_stream: Fuse<mpsc::Receiver<WalletConnectivityRequest>>,
    connectivity: ConnectivityRequester,
    base_node_watch: Watch<Option<Peer>>,
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
        request_stream: mpsc::Receiver<WalletConnectivityRequest>,
        base_node_watch: Watch<Option<Peer>>,
        online_status_watch: Watch<OnlineStatus>,
        connectivity: ConnectivityRequester,
    ) -> Self {
        Self {
            config,
            request_stream: request_stream.fuse(),
            connectivity,
            base_node_watch,
            pools: None,
            pending_requests: Vec::new(),
            online_status_watch,
        }
    }

    pub async fn start(mut self) {
        debug!(target: LOG_TARGET, "Wallet connectivity service has started.");
        let mut base_node_watch_rx = self.base_node_watch.get_receiver().fuse();
        loop {
            futures::select! {
                req = self.request_stream.select_next_some() => {
                    self.handle_request(req).await;
                },
                maybe_peer = base_node_watch_rx.select_next_some() => {
                    if maybe_peer.is_some() {
                        // This will block the rest until the connection is established. This is what we want.
                        self.setup_base_node_connection().await;
                    }
                }
            }
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
                    self.trigger_reconnect();
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
                    self.trigger_reconnect();
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

    fn trigger_reconnect(&mut self) {
        let peer = self
            .base_node_watch
            .borrow()
            .clone()
            .expect("trigger_reconnect called before base node is set");
        // Trigger the watch so that a peer connection is reinitiated
        self.set_base_node_peer(peer);
    }

    fn set_base_node_peer(&mut self, peer: Peer) {
        self.pools = None;
        self.base_node_watch.broadcast(Some(peer));
    }

    fn current_base_node(&self) -> Option<NodeId> {
        self.base_node_watch.borrow().as_ref().map(|p| p.node_id.clone())
    }

    async fn setup_base_node_connection(&mut self) {
        self.pools = None;
        loop {
            let node_id = match self.current_base_node() {
                Some(n) => n,
                None => return,
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
                    // Retry with updated peer
                    continue;
                },
                Err(e) => {
                    if self.current_base_node() != Some(node_id) {
                        self.set_online_status(OnlineStatus::Connecting);
                    } else {
                        self.set_online_status(OnlineStatus::Offline);
                    }
                    error!(target: LOG_TARGET, "{}", e);
                    time::delay_for(self.config.base_node_monitor_refresh_interval).await;
                    continue;
                },
            }
        }
    }

    fn set_online_status(&self, status: OnlineStatus) {
        let _ = self.online_status_watch.broadcast(status);
    }

    async fn try_setup_rpc_pool(&mut self, peer: NodeId) -> Result<bool, WalletConnectivityError> {
        self.connectivity.add_managed_peers(vec![peer.clone()]).await?;
        let conn = match self.try_dial_peer(peer).await? {
            Some(peer) => peer,
            None => return Ok(false),
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
        debug!(
            target: LOG_TARGET,
            "Successfully established RPC connection {}",
            conn.peer_node_id()
        );
        Ok(true)
    }

    async fn try_dial_peer(&mut self, peer: NodeId) -> Result<Option<PeerConnection>, WalletConnectivityError> {
        let recv_fut = self.base_node_watch.recv();
        futures::pin_mut!(recv_fut);
        let dial_fut = self.connectivity.dial_peer(peer);
        futures::pin_mut!(dial_fut);
        match future::select(recv_fut, dial_fut).await {
            Either::Left(_) => Ok(None),
            Either::Right((conn, _)) => Ok(Some(conn?)),
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
            WalletRpc(tx) => tx.is_canceled(),
            SyncRpc(tx) => tx.is_canceled(),
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

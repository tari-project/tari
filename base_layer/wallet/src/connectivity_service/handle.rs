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

use tari_comms::{
    peer_manager::{NodeId, Peer},
    protocol::rpc::RpcClientLease,
    types::CommsPublicKey,
};
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::sync::{mpsc, oneshot, watch};

use super::service::OnlineStatus;
use crate::{connectivity_service::WalletConnectivityInterface, util::watch::Watch};

pub enum WalletConnectivityRequest {
    ObtainBaseNodeWalletRpcClient(oneshot::Sender<RpcClientLease<BaseNodeWalletRpcClient>>),
    ObtainBaseNodeSyncRpcClient(oneshot::Sender<RpcClientLease<BaseNodeSyncRpcClient>>),
}

#[derive(Clone)]
pub struct WalletConnectivityHandle {
    sender: mpsc::Sender<WalletConnectivityRequest>,
    base_node_watch: Watch<Option<Peer>>,
    online_status_rx: watch::Receiver<OnlineStatus>,
}

impl WalletConnectivityHandle {
    pub(super) fn new(
        sender: mpsc::Sender<WalletConnectivityRequest>,
        base_node_watch: Watch<Option<Peer>>,
        online_status_rx: watch::Receiver<OnlineStatus>,
    ) -> Self {
        Self {
            sender,
            base_node_watch,
            online_status_rx,
        }
    }
}

#[async_trait::async_trait]
impl WalletConnectivityInterface for WalletConnectivityHandle {
    fn set_base_node(&mut self, base_node_peer: Peer) {
        {
            // Ensure the watch borrow is dropped immediately after use
            if let Some(peer) = self.base_node_watch.borrow().as_ref() {
                if peer.public_key == base_node_peer.public_key {
                    return;
                }
            }
        }
        self.base_node_watch.send(Some(base_node_peer));
    }

    fn get_current_base_node_watcher(&self) -> watch::Receiver<Option<Peer>> {
        self.base_node_watch.get_receiver()
    }

    /// Obtain a BaseNodeWalletRpcClient.
    ///
    /// This can be relied on to obtain a pooled BaseNodeWalletRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeWalletRpcClient RPC session.
    async fn obtain_base_node_wallet_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeWalletRpcClient>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        // Under what conditions do the (1) mpsc channel and (2) oneshot channel error?
        // (1) when the receiver has been dropped
        // (2) when the sender has been dropped
        // When can this happen?
        // Only when the service is shutdown (or there is a bug in the service that should be fixed)
        // None is returned in these cases, which we say means that you will never ever get a client connection
        // because the node is shutting down.
        self.sender
            .send(WalletConnectivityRequest::ObtainBaseNodeWalletRpcClient(reply_tx))
            .await
            .ok()?;

        reply_rx.await.ok()
    }

    /// Obtain a BaseNodeSyncRpcClient.
    ///
    /// This can be relied on to obtain a pooled BaseNodeSyncRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeSyncRpcClient RPC session.
    async fn obtain_base_node_sync_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeSyncRpcClient>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(WalletConnectivityRequest::ObtainBaseNodeSyncRpcClient(reply_tx))
            .await
            .ok()?;

        reply_rx.await.ok()
    }

    fn get_connectivity_status(&mut self) -> OnlineStatus {
        *self.online_status_rx.borrow()
    }

    fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
        self.online_status_rx.clone()
    }

    fn get_current_base_node_peer(&self) -> Option<Peer> {
        self.base_node_watch.borrow().clone()
    }

    fn get_current_base_node_peer_public_key(&self) -> Option<CommsPublicKey> {
        self.base_node_watch.borrow().as_ref().map(|p| p.public_key.clone())
    }

    fn get_current_base_node_id(&self) -> Option<NodeId> {
        self.base_node_watch.borrow().as_ref().map(|p| p.node_id.clone())
    }

    fn is_base_node_set(&self) -> bool {
        self.base_node_watch.borrow().is_some()
    }
}

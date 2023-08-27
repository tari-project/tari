//  Copyright 2021, The Taiji Project
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

use taiji_comms::{
    peer_manager::{NodeId, Peer},
    protocol::rpc::RpcClientLease,
    types::CommsPublicKey,
};
use taiji_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::sync::watch::Receiver;

use crate::{
    connectivity_service::{OnlineStatus, WalletConnectivityInterface},
    util::watch::Watch,
};

pub fn create() -> WalletConnectivityMock {
    WalletConnectivityMock::new()
}

#[derive(Clone)]
pub struct WalletConnectivityMock {
    online_status_watch: Watch<OnlineStatus>,
    base_node_watch: Watch<Option<Peer>>,
    base_node_wallet_rpc_client: Watch<Option<RpcClientLease<BaseNodeWalletRpcClient>>>,
    base_node_sync_rpc_client: Watch<Option<RpcClientLease<BaseNodeSyncRpcClient>>>,
}

impl WalletConnectivityMock {
    pub(self) fn new() -> Self {
        Self {
            online_status_watch: Watch::new(OnlineStatus::Offline),
            base_node_watch: Watch::new(None),
            base_node_wallet_rpc_client: Watch::new(None),
            base_node_sync_rpc_client: Watch::new(None),
        }
    }
}

impl WalletConnectivityMock {
    pub fn set_base_node_wallet_rpc_client(&self, client: BaseNodeWalletRpcClient) {
        self.base_node_wallet_rpc_client.send(Some(RpcClientLease::new(client)));
    }

    pub fn set_base_node_sync_rpc_client(&self, client: BaseNodeSyncRpcClient) {
        self.base_node_sync_rpc_client.send(Some(RpcClientLease::new(client)));
    }

    pub fn notify_base_node_set(&self, base_node_peer: Peer) {
        self.base_node_watch.send(Some(base_node_peer));
    }

    pub async fn base_node_changed(&mut self) -> Option<Peer> {
        self.base_node_watch.changed().await;
        self.base_node_watch.borrow().as_ref().cloned()
    }

    pub fn send_shutdown(&self) {
        self.base_node_wallet_rpc_client.send(None);
        self.base_node_sync_rpc_client.send(None);
    }
}

#[async_trait::async_trait]
impl WalletConnectivityInterface for WalletConnectivityMock {
    fn set_base_node(&mut self, base_node_peer: Peer) {
        self.notify_base_node_set(base_node_peer);
    }

    fn get_current_base_node_watcher(&self) -> Receiver<Option<Peer>> {
        self.base_node_watch.get_receiver()
    }

    async fn obtain_base_node_wallet_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeWalletRpcClient>> {
        let mut receiver = self.base_node_wallet_rpc_client.get_receiver();
        if let Some(client) = receiver.borrow().as_ref() {
            return Some(client.clone());
        }

        receiver.changed().await.unwrap();
        let borrow = receiver.borrow();
        borrow.as_ref().cloned()
    }

    async fn obtain_base_node_sync_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeSyncRpcClient>> {
        let mut receiver = self.base_node_sync_rpc_client.get_receiver();
        if let Some(client) = receiver.borrow().as_ref() {
            return Some(client.clone());
        }

        receiver.changed().await.unwrap();
        let borrow = receiver.borrow();
        borrow.as_ref().cloned()
    }

    fn get_connectivity_status(&mut self) -> OnlineStatus {
        *self.online_status_watch.borrow()
    }

    fn get_connectivity_status_watch(&self) -> Receiver<OnlineStatus> {
        self.online_status_watch.get_receiver()
    }

    fn get_current_base_node_peer(&self) -> Option<Peer> {
        self.base_node_watch.borrow().as_ref().cloned()
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

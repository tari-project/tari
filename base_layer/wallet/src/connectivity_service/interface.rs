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

use tari_comms::{
    peer_manager::{NodeId, Peer},
    protocol::rpc::RpcClientLease,
    types::CommsPublicKey,
};
use tari_core::base_node::{rpc::BaseNodeWalletRpcClient, sync::rpc::BaseNodeSyncRpcClient};
use tokio::sync::watch;

use crate::connectivity_service::{BaseNodePeerManager, OnlineStatus};

#[async_trait::async_trait]
pub trait WalletConnectivityInterface: Clone + Send + Sync + 'static {
    fn set_base_node(&mut self, base_node_peer: BaseNodePeerManager);

    fn get_current_base_node_watcher(&self) -> watch::Receiver<Option<BaseNodePeerManager>>;

    /// Obtain a BaseNodeWalletRpcClient.
    ///
    /// This can be relied on to obtain a pooled BaseNodeWalletRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeWalletRpcClient RPC session.
    async fn obtain_base_node_wallet_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeWalletRpcClient>>;

    async fn obtain_base_node_wallet_rpc_client_timeout(
        &mut self,
        timeout: Duration,
    ) -> Option<RpcClientLease<BaseNodeWalletRpcClient>> {
        tokio::time::timeout(timeout, self.obtain_base_node_wallet_rpc_client())
            .await
            .ok()
            .flatten()
    }

    /// Obtain a BaseNodeSyncRpcClient.
    ///
    /// This can be relied on to obtain a pooled BaseNodeSyncRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeSyncRpcClient RPC session.
    async fn obtain_base_node_sync_rpc_client(&mut self) -> Option<RpcClientLease<BaseNodeSyncRpcClient>>;

    fn get_connectivity_status(&mut self) -> OnlineStatus;

    fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus>;

    fn get_current_base_node_peer(&self) -> Option<Peer>;

    fn get_current_base_node_peer_public_key(&self) -> Option<CommsPublicKey>;

    fn get_current_base_node_peer_node_id(&self) -> Option<NodeId>;

    fn is_base_node_set(&self) -> bool;

    fn get_base_node_peer_manager_state(&self) -> Option<(usize, Vec<Peer>)>;
}

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

use minotari_node_wallet_client::BaseNodeWalletClient;
use tari_comms::{
    peer_manager::{NodeId, Peer},
    types::CommsPublicKey,
};
use tokio::sync::{mpsc, watch};

use crate::{
    client::http_client_factory::HttpClientFactory,
    connectivity_service::{BaseNodePeerManager, WalletConnectivityInterface},
    util::watch::Watch,
};
/// Connection status of the Base Node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnlineStatus {
    Connecting = 0,
    Online = 1,
    Offline = 2,
}

#[derive(Clone)]
pub struct WalletConnectivityHandle<TWalletClientFactory: HttpClientFactory> {
    base_node_watch: Watch<Option<BaseNodePeerManager>>,
    client_factory: TWalletClientFactory,
    online_status_watch: watch::Sender<OnlineStatus>,
}

impl<TWalletClientFactory: HttpClientFactory> WalletConnectivityHandle<TWalletClientFactory> {
    pub(super) fn new(
        base_node_watch: Watch<Option<BaseNodePeerManager>>,
        client_factory: TWalletClientFactory,
    ) -> Self {
        let (online_status_watch, _) = watch::channel(OnlineStatus::Connecting);
        Self {
            base_node_watch,
            client_factory,
            online_status_watch,
        }
    }
}

#[async_trait::async_trait]
impl<TWalletClientFactory: HttpClientFactory> WalletConnectivityInterface
    for WalletConnectivityHandle<TWalletClientFactory>
{
    type BaseNodeClient = TWalletClientFactory::Client;

    fn set_base_node(&mut self, base_node_peer_manager: BaseNodePeerManager) {
        if let Some(selected_peer) = self.base_node_watch.borrow().as_ref() {
            if selected_peer.get_current_peer().public_key == base_node_peer_manager.get_current_peer().public_key {
                return;
            }
        }
        self.base_node_watch.send(Some(base_node_peer_manager));
    }

    fn get_current_base_node_watcher(&self) -> watch::Receiver<Option<BaseNodePeerManager>> {
        self.base_node_watch.get_receiver()
    }

    fn get_base_node_peer_manager_state(&self) -> Option<(usize, Vec<Peer>)> {
        self.base_node_watch.borrow().as_ref().map(|p| p.get_state().clone())
    }

    /// Obtain a BaseNodeWalletRpcClient.
    ///
    /// This can be relied on to obtain a pooled BaseNodeWalletRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeWalletRpcClient RPC session.
    async fn obtain_base_node_wallet_rpc_client(&mut self) -> Self::BaseNodeClient {
        self.client_factory.create_http_client()
    }

    fn get_connectivity_status(&self) -> OnlineStatus {
        if self.client_factory.create_http_client().is_online() {
            OnlineStatus::Online
        } else {
            OnlineStatus::Offline
        }
    }

    fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
        self.online_status_watch.subscribe()
    }

    fn get_current_base_node_peer(&self) -> Option<Peer> {
        self.base_node_watch
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer().clone())
    }

    fn get_current_base_node_peer_public_key(&self) -> Option<CommsPublicKey> {
        self.base_node_watch
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer().public_key.clone())
    }

    fn get_current_base_node_peer_node_id(&self) -> Option<NodeId> {
        self.base_node_watch
            .borrow()
            .as_ref()
            .map(|p| p.get_current_peer().node_id.clone())
    }

    fn is_base_node_set(&self) -> bool {
        self.base_node_watch.borrow().is_some()
    }
}

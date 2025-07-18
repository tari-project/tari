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
use tokio::sync::watch;

use crate::{client::http_client_factory::HttpClientFactory, connectivity_service::WalletConnectivityInterface};
/// Connection status of the Base Node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnlineStatus {
    Connecting = 0,
    Online = 1,
    Offline = 2,
}

#[derive(Clone)]
pub struct WalletConnectivityHandle<TWalletClientFactory: HttpClientFactory> {
    client_factory: TWalletClientFactory,
    online_status_watch: watch::Sender<OnlineStatus>,
}

impl<TWalletClientFactory: HttpClientFactory> WalletConnectivityHandle<TWalletClientFactory> {
    pub fn new(client_factory: TWalletClientFactory) -> Self {
        let (online_status_watch, _) = watch::channel(OnlineStatus::Connecting);
        Self {
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

    /// This can be relied on to obtain a pooled BaseNodeWalletRpcClient rpc session from a currently selected base
    /// node/nodes. It will block until this happens. The ONLY other time it will return is if the node is
    /// shutting down, where it will return None. Use this function whenever no work can be done without a
    /// BaseNodeWalletRpcClient RPC session.
    async fn obtain_base_node_wallet_rpc_client(&mut self) -> Self::BaseNodeClient {
        self.client_factory.create_http_client()
    }

    async fn get_connectivity_status(&self) -> OnlineStatus {
        if self.client_factory.create_http_client().is_online().await {
            OnlineStatus::Online
        } else {
            OnlineStatus::Offline
        }
    }

    fn get_connectivity_status_watch(&self) -> watch::Receiver<OnlineStatus> {
        self.online_status_watch.subscribe()
    }
}

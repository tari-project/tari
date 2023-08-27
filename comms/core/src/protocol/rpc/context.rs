//  Copyright 2020, The Taiji Project
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

use std::{fmt, sync::Arc};

use async_trait::async_trait;

use super::RpcError;
use crate::{
    connectivity::{ConnectivityRequester, ConnectivitySelection},
    peer_manager::{NodeId, OrNotFound, Peer},
    PeerConnection,
    PeerManager,
};

/// Abstraction of the comms backend calls provided to RPC services.
#[async_trait]
pub trait RpcCommsProvider: Send + Sync {
    async fn fetch_peer(&self, node_id: &NodeId) -> Result<Peer, RpcError>;
    async fn dial_peer(&mut self, node_id: &NodeId) -> Result<PeerConnection, RpcError>;
    async fn select_connections(&mut self, selection: ConnectivitySelection) -> Result<Vec<PeerConnection>, RpcError>;
}

/// Provides access to the `PeerManager` and connectivity manager.
#[derive(Clone, Debug)]
pub(crate) struct RpcCommsBackend {
    connectivity: ConnectivityRequester,
    peer_manager: Arc<PeerManager>,
}

impl RpcCommsBackend {
    pub(super) fn new(peer_manager: Arc<PeerManager>, connectivity: ConnectivityRequester) -> Self {
        Self {
            connectivity,
            peer_manager,
        }
    }

    pub fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }
}

#[async_trait]
impl RpcCommsProvider for RpcCommsBackend {
    async fn fetch_peer(&self, node_id: &NodeId) -> Result<Peer, RpcError> {
        self.peer_manager
            .find_by_node_id(node_id)
            .await
            .or_not_found()
            .map_err(Into::into)
    }

    async fn dial_peer(&mut self, node_id: &NodeId) -> Result<PeerConnection, RpcError> {
        self.connectivity.dial_peer(node_id.clone()).await.map_err(Into::into)
    }

    async fn select_connections(&mut self, selection: ConnectivitySelection) -> Result<Vec<PeerConnection>, RpcError> {
        self.connectivity
            .select_connections(selection)
            .await
            .map_err(Into::into)
    }
}

pub struct RequestContext {
    request_id: u32,
    #[allow(dead_code)]
    backend: Box<dyn RpcCommsProvider>,
    node_id: NodeId,
}

impl RequestContext {
    pub(super) fn new(request_id: u32, node_id: NodeId, backend: Box<dyn RpcCommsProvider>) -> Self {
        Self {
            request_id,
            backend,
            node_id,
        }
    }

    pub fn peer_node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn request_id(&self) -> u32 {
        self.request_id
    }

    #[allow(dead_code)]
    pub async fn fetch_peer(&self) -> Result<Peer, RpcError> {
        self.backend.fetch_peer(&self.node_id).await
    }

    #[allow(dead_code)]
    async fn dial_peer(&mut self, node_id: &NodeId) -> Result<PeerConnection, RpcError> {
        self.backend.dial_peer(node_id).await
    }

    #[allow(dead_code)]
    async fn select_connections(&mut self, selection: ConnectivitySelection) -> Result<Vec<PeerConnection>, RpcError> {
        self.backend.select_connections(selection).await
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestContext")
            .field("node_id", &self.node_id)
            .field("backend", &"dyn RpcCommsProvider")
            .finish()
    }
}

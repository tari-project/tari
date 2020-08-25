//  Copyright 2020, The Tari Project
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

use super::RpcError;
use crate::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer},
    PeerManager,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct RpcCommsContext {
    connectivity: ConnectivityRequester,
    peer_manager: Arc<PeerManager>,
}

impl RpcCommsContext {
    pub(super) fn new(peer_manager: Arc<PeerManager>, connectivity: ConnectivityRequester) -> Self {
        Self {
            peer_manager,
            connectivity,
        }
    }

    pub fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }
}

#[derive(Debug, Clone)]
pub struct RequestContext {
    context: RpcCommsContext,
    node_id: NodeId,
}

impl RequestContext {
    pub(super) fn new(node_id: NodeId, context: RpcCommsContext) -> Self {
        Self { node_id, context }
    }

    pub async fn load_peer(&self) -> Result<Peer, RpcError> {
        let peer = self.context.peer_manager.find_by_node_id(&self.node_id).await?;
        Ok(peer)
    }

    pub fn connectivity(&self) -> ConnectivityRequester {
        self.context.connectivity.clone()
    }
}

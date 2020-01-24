// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    multiaddr::Multiaddr,
    peer_manager::{
        connection_stats::PeerConnectionStats,
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerId,
        PeerManager,
        PeerManagerError,
    },
    types::CommsPublicKey,
};
use std::sync::Arc;
use tokio::task;

#[derive(Clone)]
pub struct AsyncPeerManager {
    peer_manager: Arc<PeerManager>,
}

macro_rules! make_async {
    ($fn:ident()) => {
        make_async!($fn() -> ());
    };
    ($fn:ident() -> $rtype:ty) => {
        pub async fn $fn(&self) -> Result<$rtype, PeerManagerError> {
            let peer_manager = Arc::clone(&self.peer_manager);
            tokio::task::spawn_blocking(move || peer_manager.$fn())
                .await
                .map_err(|_| PeerManagerError::BlockingTaskSpawnError)?
        }
    };
     ($fn:ident($($param:ident:$ptype:ty),+)) => {
        make_async!($fn($($param),+) -> ());
    };

    ($fn:ident($($param:ident:$ptype:ty),+) -> $rtype:ty) => {
        pub async fn $fn(&self, $($param: $ptype),+) -> Result<$rtype, PeerManagerError> {
            let peer_manager = Arc::clone(&self.peer_manager);
            tokio::task::spawn_blocking(move || peer_manager.$fn($($param),+))
                .await
                .map_err(|_| PeerManagerError::BlockingTaskSpawnError)?
        }
    };
}

impl AsyncPeerManager {
    make_async!(add_peer(peer: Peer) -> PeerId);

    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }

    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        // TODO: When tokio block_in_place is more stable, this clone may not be necessary
        let node_id = node_id.clone();
        let peer_manager = Arc::clone(&self.peer_manager);
        task::spawn_blocking(move || peer_manager.find_by_node_id(&node_id))
            .await
            .map_err(|_| PeerManagerError::BlockingTaskSpawnError)?
    }

    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        // TODO: When tokio block_in_place is more stable, this clone may not be necessary
        let public_key = public_key.clone();
        let peer_manager = Arc::clone(&self.peer_manager);
        task::spawn_blocking(move || peer_manager.find_by_public_key(&public_key))
            .await
            .map_err(|_| PeerManagerError::BlockingTaskSpawnError)?
    }

    /// Updates fields for a peer. Any fields set to Some(xx) will be updated. All None
    /// fields will remain the same.
    pub async fn update_peer(
        &self,
        public_key: &CommsPublicKey,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<Multiaddr>>,
        flags: Option<PeerFlags>,
        peer_features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
    ) -> Result<(), PeerManagerError>
    {
        // TODO: When tokio block_in_place is more stable, this clone may not be necessary
        let public_key = public_key.clone();
        let peer_manager = Arc::clone(&self.peer_manager);
        task::spawn_blocking(move || {
            peer_manager.update_peer(
                &public_key,
                node_id,
                net_addresses,
                flags,
                peer_features,
                connection_stats,
            )
        })
        .await
        .map_err(|_| PeerManagerError::BlockingTaskSpawnError)?
    }
}

impl From<Arc<PeerManager>> for AsyncPeerManager {
    fn from(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }
}

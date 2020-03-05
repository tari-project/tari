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
    protocol::ProtocolId,
    types::CommsPublicKey,
};
use std::sync::Arc;
use tokio::task;

#[derive(Clone)]
pub struct AsyncPeerManager {
    peer_manager: Arc<PeerManager>,
}

impl AsyncPeerManager {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }

    pub async fn add_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        self.blocking_call(move |pm| pm.add_peer(peer)).await?
    }

    /// Get a peer matching the given node ID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        // TODO: When tokio block_in_place is more stable, this clone may not be necessary
        let node_id = node_id.clone();
        self.blocking_call(move |pm| pm.find_by_node_id(&node_id)).await?
    }

    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        let node_id = node_id.clone();
        self.blocking_call(move |pm| pm.direct_identity_node_id(&node_id))
            .await?
    }

    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let public_key = public_key.clone();
        self.blocking_call(move |pm| pm.find_by_public_key(&public_key)).await?
    }

    pub async fn exists(&self, public_key: &CommsPublicKey) -> Result<bool, PeerManagerError> {
        let public_key = public_key.clone();
        self.blocking_call(move |pm| Ok(pm.exists(&public_key))).await?
    }

    /// Set the last connection to this peer as a success
    pub async fn set_last_connect_success(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let node_id = node_id.clone();
        self.blocking_call(move |pm| pm.set_last_connect_success(&node_id))
            .await?
    }

    /// Set the last connection to this peer as a failure
    pub async fn set_last_connect_failed(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let node_id = node_id.clone();
        self.blocking_call(move |pm| pm.set_last_connect_failed(&node_id))
            .await?
    }

    pub fn inner(&self) -> Arc<PeerManager> {
        self.peer_manager.clone()
    }

    /// Updates fields for a peer. Any fields set to Some(xx) will be updated. All None
    /// fields will remain the same.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_peer(
        &self,
        public_key: &CommsPublicKey,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<Multiaddr>>,
        flags: Option<PeerFlags>,
        peer_features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
        supported_protocols: Option<Vec<ProtocolId>>,
    ) -> Result<(), PeerManagerError>
    {
        // TODO: When tokio block_in_place is more stable, this clone may not be necessary
        let public_key = public_key.clone();
        self.blocking_call(move |pm| {
            pm.update_peer(
                &public_key,
                node_id,
                net_addresses,
                flags,
                peer_features,
                connection_stats,
                supported_protocols,
            )
        })
        .await?
    }

    async fn blocking_call<F, R>(&self, f: F) -> Result<R, PeerManagerError>
    where
        F: FnOnce(Arc<PeerManager>) -> R + Send + 'static,
        R: Send + 'static,
    {
        let peer_manager = self.peer_manager.clone();
        task::spawn_blocking(move || f(peer_manager))
            .await
            .map_err(|_| PeerManagerError::BlockingTaskSpawnError)
    }
}

impl From<Arc<PeerManager>> for AsyncPeerManager {
    fn from(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::peer_manager::build_peer_manager;
    use rand::rngs::OsRng;
    use tari_crypto::keys::PublicKey;

    #[tokio_macros::test_basic]
    async fn update_peer() {
        let pm = AsyncPeerManager::from(build_peer_manager());
        let pk = CommsPublicKey::default();
        let node_id = NodeId::default();

        pm.add_peer(Peer::new(
            pk.clone(),
            node_id.clone(),
            Default::default(),
            Default::default(),
            Default::default(),
            &[],
        ))
        .await
        .unwrap();

        pm.find_by_node_id(&node_id).await.unwrap();

        let (_, pk2) = CommsPublicKey::random_keypair(&mut OsRng);
        let node_id2 = NodeId::from_key(&pk2).unwrap();

        pm.update_peer(&pk, Some(node_id2.clone()), None, None, None, None, None)
            .await
            .unwrap();

        pm.find_by_node_id(&node_id2).await.unwrap();

        let err = pm.find_by_node_id(&node_id).await.unwrap_err();
        assert!(err.is_peer_not_found());
    }
}

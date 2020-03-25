//  Copyright 2019 The Tari Project
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
    peer_manager::{
        connection_stats::PeerConnectionStats,
        node_id::NodeId,
        peer::{Peer, PeerFlags},
        peer_id::PeerId,
        peer_storage::PeerStorage,
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
    },
    protocol::ProtocolId,
    types::{CommsDatabase, CommsPublicKey},
};
use multiaddr::Multiaddr;
use tokio::sync::RwLock;

/// The PeerManager consist of a routing table of previously discovered peers.
/// It also provides functionality to add, find and delete peers. A subset of peers can also be requested from the
/// routing table based on the selected Broadcast strategy.
pub struct PeerManager {
    peer_storage: RwLock<PeerStorage<CommsDatabase>>,
}

impl PeerManager {
    /// Constructs a new empty PeerManager
    pub fn new(database: CommsDatabase) -> Result<PeerManager, PeerManagerError> {
        Ok(Self {
            peer_storage: RwLock::new(PeerStorage::new_indexed(database)?),
        })
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub async fn add_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        self.peer_storage.write().await.add_peer(peer)
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
        self.peer_storage.write().await.update_peer(
            public_key,
            node_id,
            net_addresses,
            flags,
            peer_features,
            connection_stats,
            supported_protocols,
        )
    }

    /// Set the last connection to this peer as a success
    pub async fn set_last_connect_success(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let mut storage = self.peer_storage.write().await;
        let mut peer = storage.find_by_node_id(node_id)?;
        peer.connection_stats.set_connection_success();
        peer.flags.remove(PeerFlags::OFFLINE);
        storage.update_peer(
            &peer.public_key,
            None,
            None,
            None,
            None,
            Some(peer.connection_stats),
            None,
        )
    }

    /// Set the last connection to this peer as a failure
    pub async fn set_last_connect_failed(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let mut storage = self.peer_storage.write().await;
        let mut peer = storage.find_by_node_id(node_id)?;
        peer.connection_stats.set_connection_failed();
        storage.update_peer(
            &peer.public_key,
            None,
            None,
            None,
            None,
            Some(peer.connection_stats),
            None,
        )
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub async fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage.write().await.delete_peer(node_id)
    }

    /// Performs the given [PeerQuery].
    ///
    /// [PeerQuery]: crate::peer_manager::peer_query::PeerQuery
    pub async fn perform_query(&self, peer_query: PeerQuery<'_>) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.read().await.perform_query(peer_query)
    }

    /// Find the peer with the provided NodeID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        self.peer_storage.read().await.find_by_node_id(node_id)
    }

    /// Find the peer with the provided PublicKey
    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        self.peer_storage.read().await.find_by_public_key(public_key)
    }

    /// Check if a peer exist using the specified public_key
    pub async fn exists(&self, public_key: &CommsPublicKey) -> bool {
        self.peer_storage.read().await.exists(public_key)
    }

    /// Check if a peer exist using the specified node_id
    pub async fn exists_node_id(&self, node_id: &NodeId) -> bool {
        self.peer_storage.read().await.exists_node_id(node_id)
    }

    /// Returns all peers
    pub async fn all(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.read().await.all()
    }

    /// Get a peer matching the given node ID
    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        match self.peer_storage.read().await.direct_identity_node_id(&node_id) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Get a peer matching the given public key
    pub async fn direct_identity_public_key(
        &self,
        public_key: &CommsPublicKey,
    ) -> Result<Option<Peer>, PeerManagerError>
    {
        match self.peer_storage.read().await.direct_identity_public_key(&public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Fetch all peers (except banned ones)
    pub async fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.read().await.flood_peers()
    }

    /// Fetch n nearest neighbour Communication Nodes
    pub async fn closest_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[CommsPublicKey],
    ) -> Result<Vec<Peer>, PeerManagerError>
    {
        self.peer_storage.read().await.closest_peers(node_id, n, excluded_peers)
    }

    /// Fetch n random peers
    pub async fn random_peers(&self, n: usize) -> Result<Vec<Peer>, PeerManagerError> {
        // Send to a random set of peers of size n that are Communication Nodes
        self.peer_storage.read().await.random_peers(n)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id
    pub async fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError>
    {
        self.peer_storage
            .read()
            .await
            .in_network_region(node_id, region_node_id, n)
    }

    /// Changes the ban flag bit of the peer
    pub async fn set_banned(&self, public_key: &CommsPublicKey, ban_flag: bool) -> Result<NodeId, PeerManagerError> {
        self.peer_storage.write().await.set_banned(public_key, ban_flag)
    }

    /// Changes the offline flag bit of the peer
    pub async fn set_offline(&self, public_key: &CommsPublicKey, is_offline: bool) -> Result<NodeId, PeerManagerError> {
        self.peer_storage.write().await.set_offline(public_key, is_offline)
    }

    /// Adds a new net address to the peer if it doesn't yet exist
    pub async fn add_net_address(&self, node_id: &NodeId, net_address: &Multiaddr) -> Result<(), PeerManagerError> {
        self.peer_storage.write().await.add_net_address(node_id, net_address)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        net_address::MultiaddressesWithStats,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
            PeerFeatures,
        },
    };
    use rand::rngs::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    fn create_test_peer(ban_flag: bool) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = MultiaddressesWithStats::from("/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap());
        let mut peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::MESSAGE_PROPAGATION,
            &[],
        );
        peer.set_banned(ban_flag);
        peer
    }

    #[tokio_macros::test_basic]
    async fn get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HashmapDatabase::new()).unwrap();
        let mut test_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        test_peers.push(create_test_peer(true));
        assert!(peer_manager
            .add_peer(test_peers[test_peers.len() - 1].clone())
            .await
            .is_ok());
        for _i in 0..18 {
            test_peers.push(create_test_peer(false));
            assert!(peer_manager
                .add_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok());
        }
        test_peers.push(create_test_peer(true));
        assert!(peer_manager
            .add_peer(test_peers[test_peers.len() - 1].clone())
            .await
            .is_ok());

        // Test Valid Direct
        let selected_peers = peer_manager
            .direct_identity_node_id(&test_peers[2].node_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected_peers.node_id, test_peers[2].node_id);
        assert_eq!(selected_peers.public_key, test_peers[2].public_key);
        // Test Invalid Direct
        let unmanaged_peer = create_test_peer(false);
        assert!(peer_manager
            .direct_identity_node_id(&unmanaged_peer.node_id)
            .await
            .unwrap()
            .is_none());

        // Test Flood
        let selected_peers = peer_manager.flood_peers().await.unwrap();
        assert_eq!(selected_peers.len(), 18);
        for peer_identity in &selected_peers {
            assert_eq!(
                peer_manager
                    .find_by_node_id(&peer_identity.node_id)
                    .await
                    .unwrap()
                    .is_banned(),
                false
            );
        }

        // Test Closest - No exclusions
        let selected_peers = peer_manager
            .closest_peers(&unmanaged_peer.node_id, 3, &Vec::new())
            .await
            .unwrap();
        assert_eq!(selected_peers.len(), 3);
        // Remove current identity nodes from test peers
        let mut unused_peers: Vec<Peer> = Vec::new();
        for peer in &test_peers {
            if !selected_peers
                .iter()
                .any(|peer_identity| peer.node_id == peer_identity.node_id || peer.is_banned())
            {
                unused_peers.push(peer.clone());
            }
        }
        // Check that none of the remaining unused peers have smaller distances compared to the selected peers
        for peer_identity in &selected_peers {
            let selected_dist = unmanaged_peer.node_id.distance(&peer_identity.node_id);
            for unused_peer in &unused_peers {
                let unused_dist = unmanaged_peer.node_id.distance(&unused_peer.node_id);
                assert!(unused_dist > selected_dist);
            }
        }

        // Test Closest - With an exclusion
        let excluded_peers = vec![
            selected_peers[0].public_key.clone(), // ,selected_peers[1].public_key.clone()
        ];
        let selected_peers = peer_manager
            .closest_peers(&unmanaged_peer.node_id, 3, &excluded_peers)
            .await
            .unwrap();
        assert_eq!(selected_peers.len(), 3);
        // Remove current identity nodes from test peers
        let mut unused_peers: Vec<Peer> = Vec::new();
        for peer in &test_peers {
            if !selected_peers.iter().any(|peer_identity| {
                peer.node_id == peer_identity.node_id || peer.is_banned() || excluded_peers.contains(&peer.public_key)
            }) {
                unused_peers.push(peer.clone());
            }
        }
        // Check that none of the remaining unused peers have smaller distances compared to the selected peers
        for peer_identity in &selected_peers {
            let selected_dist = unmanaged_peer.node_id.distance(&peer_identity.node_id);
            for unused_peer in &unused_peers {
                let unused_dist = unmanaged_peer.node_id.distance(&unused_peer.node_id);
                assert!(unused_dist > selected_dist);
            }
            assert!(!excluded_peers.contains(&peer_identity.public_key));
        }

        // Test Random
        let identities1 = peer_manager.random_peers(10).await.unwrap();
        let identities2 = peer_manager.random_peers(10).await.unwrap();
        assert_ne!(identities1, identities2);
    }

    #[tokio_macros::test_basic]
    async fn test_in_network_region() {
        let _rng = rand::rngs::OsRng;
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HashmapDatabase::new()).unwrap();
        let network_region_node_id = create_test_peer(false).node_id;
        // Create peers
        let mut test_peers: Vec<Peer> = Vec::new();
        for _ in 0..10 {
            test_peers.push(create_test_peer(false));
            assert!(peer_manager
                .add_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok());
        }
        test_peers[0].set_banned(true);
        test_peers[1].set_banned(true);

        // Get nearest neighbours
        let n = 5;
        let nearest_identities = peer_manager
            .closest_peers(&network_region_node_id, n, &Vec::new())
            .await
            .unwrap();

        for peer in &test_peers {
            if nearest_identities
                .iter()
                .any(|peer_identity| peer.node_id == peer_identity.node_id)
            {
                assert!(peer_manager
                    .in_network_region(&peer.node_id, &network_region_node_id, n)
                    .await
                    .unwrap());
            } else {
                assert!(!peer_manager
                    .in_network_region(&peer.node_id, &network_region_node_id, n)
                    .await
                    .unwrap());
            }
        }
    }
}

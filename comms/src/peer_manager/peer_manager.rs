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
    connection::net_address::NetAddress,
    peer_manager::{
        connection_stats::PeerConnectionStats,
        node_id::NodeId,
        peer::{Peer, PeerFlags},
        peer_storage::PeerStorage,
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
    },
    types::{CommsDatabase, CommsPublicKey},
};
use std::{sync::RwLock, time::Duration};

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
            peer_storage: RwLock::new(PeerStorage::new(database)?),
        })
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub fn add_peer(&self, peer: Peer) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).add_peer(peer)
    }

    pub fn update_peer(
        &self,
        public_key: &CommsPublicKey,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<NetAddress>>,
        flags: Option<PeerFlags>,
        peer_features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
    ) -> Result<(), PeerManagerError>
    {
        acquire_write_lock!(self.peer_storage).update_peer(
            public_key,
            node_id,
            net_addresses,
            flags,
            peer_features,
            connection_stats,
        )
    }

    pub fn set_success_connection_state(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let mut storage = acquire_write_lock!(self.peer_storage);
        let mut peer = storage.find_by_node_id(node_id)?;
        peer.connection_stats.set_connection_success();
        storage.add_peer(peer)
    }

    pub fn set_failed_connection_state(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let mut storage = acquire_write_lock!(self.peer_storage);
        let mut peer = storage.find_by_node_id(node_id)?;
        peer.connection_stats.set_connection_failed();
        storage.add_peer(peer)
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).delete_peer(node_id)
    }

    /// Performs the given [PeerQuery].
    ///
    /// [PeerQuery]: crate::peer_manager::peer_query::PeerQuery
    pub fn perform_query(&self, peer_query: PeerQuery) -> Result<Vec<Peer>, PeerManagerError> {
        acquire_read_lock!(self.peer_storage).perform_query(peer_query)
    }

    /// Find the peer with the provided NodeID
    pub fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        acquire_read_lock!(self.peer_storage).find_by_node_id(node_id)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        acquire_read_lock!(self.peer_storage).find_by_public_key(public_key)
    }

    /// Check if a peer exist using the specified public_key
    pub fn exists(&self, public_key: &CommsPublicKey) -> bool {
        acquire_read_lock!(self.peer_storage).exists(public_key)
    }

    /// Check if a peer exist using the specified node_id
    pub fn exists_node_id(&self, node_id: &NodeId) -> bool {
        acquire_read_lock!(self.peer_storage).exists_node_id(node_id)
    }

    /// Get a peer matching the given node ID
    pub fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        match acquire_read_lock!(self.peer_storage).direct_identity_node_id(&node_id) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Get a peer matching the given public key
    pub fn direct_identity_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        match acquire_read_lock!(self.peer_storage).direct_identity_public_key(&public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Fetch all peers (except banned ones)
    pub fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        acquire_read_lock!(self.peer_storage).flood_peers()
    }

    /// Fetch n nearest neighbour Communication Nodes
    pub fn closest_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &Vec<CommsPublicKey>,
    ) -> Result<Vec<Peer>, PeerManagerError>
    {
        acquire_read_lock!(self.peer_storage).closest_peers(node_id, n, excluded_peers)
    }

    /// Fetch n random peers
    pub fn random_peers(&self, n: usize) -> Result<Vec<Peer>, PeerManagerError> {
        // Send to a random set of peers of size n that are Communication Nodes
        acquire_read_lock!(self.peer_storage).random_peers(n)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id
    pub fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError>
    {
        acquire_read_lock!(self.peer_storage).in_network_region(node_id, region_node_id, n)
    }

    /// Thread safe access to peer - Changes the ban flag bit of the peer
    pub fn set_banned(&self, node_id: &NodeId, ban_flag: bool) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).set_banned(node_id, ban_flag)
    }

    /// Thread safe access to peer - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&self, node_id: &NodeId, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).add_net_address(node_id, net_address)
    }

    /// Thread safe access to peer - Finds and returns the highest priority net address until all connection attempts
    /// for each net address have been reached
    pub fn get_best_net_address(&self, node_id: &NodeId) -> Result<NetAddress, PeerManagerError> {
        acquire_write_lock!(self.peer_storage).get_best_net_address(node_id)
    }

    /// Thread safe access to peer - The average connection latency of the provided net address will be updated to
    /// include the current measured latency sample
    pub fn update_latency(
        &self,
        net_address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), PeerManagerError>
    {
        acquire_write_lock!(self.peer_storage).update_latency(net_address, latency_measurement)
    }

    /// Thread safe access to peer - Mark that a message was received from the specified net address
    pub fn mark_message_received(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).mark_message_received(net_address)
    }

    /// Thread safe access to peer - Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).mark_message_rejected(net_address)
    }

    /// Thread safe access to peer - Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).mark_successful_connection_attempt(net_address)
    }

    /// Thread safe access to peer - Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).mark_failed_connection_attempt(net_address)
    }

    /// Thread safe access to peer - Reset all connection attempts on all net addresses for peer
    pub fn reset_connection_attempts(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        acquire_write_lock!(self.peer_storage).reset_connection_attempts(node_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::net_address::{net_addresses::NetAddressesWithStats, NetAddress},
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
            PeerFeatures,
        },
    };
    use rand::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HMapDatabase;

    fn create_test_peer(rng: &mut OsRng, ban_flag: bool) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = NetAddressesWithStats::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let mut peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::MESSAGE_PROPAGATION,
        );
        peer.set_banned(ban_flag);
        peer
    }

    #[test]
    fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HMapDatabase::new()).unwrap();
        let mut test_peers: Vec<Peer> = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        let mut rng = rand::OsRng::new().unwrap();
        test_peers.push(create_test_peer(&mut rng, true));
        assert!(peer_manager.add_peer(test_peers[test_peers.len() - 1].clone()).is_ok());
        for _i in 0..18 {
            test_peers.push(create_test_peer(&mut rng, false));
            assert!(peer_manager.add_peer(test_peers[test_peers.len() - 1].clone()).is_ok());
        }
        test_peers.push(create_test_peer(&mut rng, true));
        assert!(peer_manager.add_peer(test_peers[test_peers.len() - 1].clone()).is_ok());

        // Test Valid Direct
        let selected_peers = peer_manager
            .direct_identity_node_id(&test_peers[2].node_id)
            .unwrap()
            .unwrap();
        assert_eq!(selected_peers.node_id, test_peers[2].node_id);
        assert_eq!(selected_peers.public_key, test_peers[2].public_key);
        // Test Invalid Direct
        let unmanaged_peer = create_test_peer(&mut rng, false);
        assert!(peer_manager
            .direct_identity_node_id(&unmanaged_peer.node_id)
            .unwrap()
            .is_none());

        // Test Flood
        let selected_peers = peer_manager.flood_peers().unwrap();
        assert_eq!(selected_peers.len(), 18);
        for peer_identity in &selected_peers {
            assert_eq!(
                peer_manager
                    .find_by_node_id(&peer_identity.node_id)
                    .unwrap()
                    .is_banned(),
                false
            );
        }

        // Test Closest - No exclusions
        let selected_peers = peer_manager
            .closest_peers(&unmanaged_peer.node_id, 3, &Vec::new())
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
        let identities1 = peer_manager.random_peers(10).unwrap();
        let identities2 = peer_manager.random_peers(10).unwrap();
        assert_ne!(identities1, identities2);
    }

    #[test]
    fn test_in_network_region() {
        let mut rng = rand::OsRng::new().unwrap();
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HMapDatabase::new()).unwrap();
        let network_region_node_id = create_test_peer(&mut rng, false).node_id;
        // Create peers
        let mut test_peers: Vec<Peer> = Vec::new();
        for _ in 0..10 {
            test_peers.push(create_test_peer(&mut rng, false));
            assert!(peer_manager.add_peer(test_peers[test_peers.len() - 1].clone()).is_ok());
        }
        test_peers[0].set_banned(true);
        test_peers[1].set_banned(true);

        // Get nearest neighbours
        let n = 5;
        let nearest_identities = peer_manager
            .closest_peers(&network_region_node_id, n, &Vec::new())
            .unwrap();

        for peer in &test_peers {
            if nearest_identities
                .iter()
                .any(|peer_identity| peer.node_id == peer_identity.node_id)
            {
                assert!(peer_manager
                    .in_network_region(&peer.node_id, &network_region_node_id, n)
                    .unwrap());
            } else {
                assert!(!peer_manager
                    .in_network_region(&peer.node_id, &network_region_node_id, n)
                    .unwrap());
            }
        }
    }

    #[test]
    fn test_peer_reset_connection_attempts() {
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HMapDatabase::new()).unwrap();
        let mut rng = rand::OsRng::new().unwrap();
        let peer = create_test_peer(&mut rng, false);
        peer_manager.add_peer(peer.clone()).unwrap();

        peer_manager
            .mark_failed_connection_attempt(&peer.addresses.addresses[0].clone().as_net_address())
            .unwrap();
        peer_manager
            .mark_failed_connection_attempt(&peer.addresses.addresses[0].clone().as_net_address())
            .unwrap();
        assert_eq!(
            peer_manager
                .find_by_node_id(&peer.node_id.clone())
                .unwrap()
                .addresses
                .addresses[0]
                .connection_attempts,
            2
        );
        peer_manager.reset_connection_attempts(&peer.node_id.clone()).unwrap();
        assert_eq!(
            peer_manager
                .find_by_node_id(&peer.node_id.clone())
                .unwrap()
                .addresses
                .addresses[0]
                .connection_attempts,
            0
        );
    }
}

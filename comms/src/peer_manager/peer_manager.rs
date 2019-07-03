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
    outbound_message_service::broadcast_strategy::BroadcastStrategy,
    peer_manager::{node_id::NodeId, node_identity::PeerNodeIdentity, peer::Peer, peer_storage::PeerStorage},
    types::{CommsDatabase, CommsPublicKey},
};

use crate::peer_manager::PeerManagerError;
use std::{sync::RwLock, time::Duration};
use tari_storage::keyvalue_store::{DataStore, DatastoreError};

/// The PeerManager consist of a routing table of previously discovered peers.
/// It also provides functionality to add, find and delete peers. A subset of peers can also be requested from the
/// routing table based on the selected Broadcast strategy.
pub struct PeerManager {
    peer_storage: RwLock<PeerStorage>,
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
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .add_peer(peer)
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .delete_peer(node_id)
    }

    /// Find the peer with the provided NodeID
    pub fn find_with_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_node_id(node_id)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_with_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_public_key(public_key)
    }

    /// Find the peer with the provided NetAddress
    pub fn find_with_net_address(&self, net_address: &NetAddress) -> Result<Peer, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_net_address(net_address)
    }

    /// Request a sub-set of peers based on the provided BroadcastStrategy
    pub fn get_broadcast_identities(
        &self,
        broadcast_strategy: BroadcastStrategy,
    ) -> Result<Vec<PeerNodeIdentity>, PeerManagerError>
    {
        match broadcast_strategy {
            BroadcastStrategy::DirectNodeId(node_id) => {
                // Send to a particular peer matching the given node ID
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .direct_identity_node_id(&node_id)
            },
            BroadcastStrategy::DirectPublicKey(public_key) => {
                // Send to a particular peer matching the given node ID
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .direct_identity_public_key(&public_key)
            },
            BroadcastStrategy::Flood => {
                // Send to all known Communication Node peers
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .flood_identities()
            },
            BroadcastStrategy::Closest(closest_request) => {
                // Send to all n nearest neighbour Communication Nodes
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .closest_identities(closest_request.node_id, closest_request.n)
            },
            BroadcastStrategy::Random(n) => {
                // Send to a random set of peers of size n that are Communication Nodes
                self.peer_storage
                    .write()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .random_identities(n)
            },
        }
    }

    /// Thread safe access to peer - Changes the ban flag bit of the peer
    pub fn set_banned(&self, node_id: &NodeId, ban_flag: bool) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .set_banned(node_id, ban_flag)
    }

    /// Thread safe access to peer - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&self, node_id: &NodeId, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .add_net_address(node_id, net_address)
    }

    /// Thread safe access to peer - Finds and returns the highest priority net address until all connection attempts
    /// for each net address have been reached
    pub fn get_best_net_address(&self, node_id: &NodeId) -> Result<NetAddress, PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .get_best_net_address(node_id)
    }

    /// Thread safe access to peer - The average connection latency of the provided net address will be updated to
    /// include the current measured latency sample
    pub fn update_latency(
        &self,
        net_address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), PeerManagerError>
    {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .update_latency(net_address, latency_measurement)
    }

    /// Thread safe access to peer - Mark that a message was received from the specified net address
    pub fn mark_message_received(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_message_received(net_address)
    }

    /// Thread safe access to peer - Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_message_rejected(net_address)
    }

    /// Thread safe access to peer - Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_successful_connection_attempt(net_address)
    }

    /// Thread safe access to peer - Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_failed_connection_attempt(net_address)
    }

    /// Thread safe access to peer - Reset all connection attempts on all net addresses for peer
    pub fn reset_connection_attempts(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .reset_connection_attempts(node_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::net_address::{net_addresses::NetAddressesWithStats, NetAddress},
        outbound_message_service::broadcast_strategy::ClosestRequest,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
        },
    };
    use rand::OsRng;
    use std::path::PathBuf;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

    fn create_test_peer(rng: &mut OsRng, ban_flag: bool) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = NetAddressesWithStats::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let mut peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default());
        peer.set_banned(ban_flag);
        peer
    }

    fn get_path(name: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/data");
        path.push(name);
        path.to_str().unwrap().to_string()
    }

    fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
        let path = get_path(name);
        let _ = std::fs::create_dir(&path).unwrap_or_default();
        LMDBBuilder::new()
            .set_path(&path)
            .set_environment_size(10)
            .set_max_number_of_databases(1)
            .add_database(name, lmdb_zero::db::CREATE)
            .build()
    }

    fn clean_up_datastore(name: &str) {
        std::fs::remove_dir_all(get_path(name)).unwrap();
    }

    #[test]
    fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let database_name = "pm_test_get_broadcast_identities"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();
        let peer_manager = PeerManager::new(peer_database).unwrap();
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
        let identities = peer_manager
            .get_broadcast_identities(BroadcastStrategy::DirectNodeId(test_peers[2].node_id.clone()))
            .unwrap();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].node_id, test_peers[2].node_id);
        assert_eq!(identities[0].public_key, test_peers[2].public_key);
        // Test Invalid Direct
        let unmanaged_peer = create_test_peer(&mut rng, false);
        assert!(peer_manager
            .get_broadcast_identities(BroadcastStrategy::DirectNodeId(unmanaged_peer.node_id.clone()))
            .is_err());

        // Test Flood
        let identities = peer_manager.get_broadcast_identities(BroadcastStrategy::Flood).unwrap();
        assert_eq!(identities.len(), 18);
        for peer_identity in &identities {
            assert_eq!(
                peer_manager
                    .find_with_node_id(&peer_identity.node_id)
                    .unwrap()
                    .is_banned(),
                false
            );
        }

        // Test Closest
        let identities = peer_manager
            .get_broadcast_identities(BroadcastStrategy::Closest(ClosestRequest {
                n: 3,
                node_id: unmanaged_peer.node_id.clone(),
            }))
            .unwrap();
        assert_eq!(identities.len(), 3);
        // Remove current identity nodes from test peers
        let mut unused_peers: Vec<Peer> = Vec::new();
        for peer in &test_peers {
            let mut found_flag = false;
            for peer_identity in &identities {
                if (peer.node_id == peer_identity.node_id) || (peer.is_banned()) {
                    found_flag = true;
                    break;
                }
            }
            if !found_flag {
                unused_peers.push(peer.clone());
            }
        }

        // Check that none of the remaining unused peers have smaller distances compared to the selected peers
        for peer_identity in &identities {
            let selected_dist = unmanaged_peer.node_id.distance(&peer_identity.node_id);
            for unused_peer in &unused_peers {
                let unused_dist = unmanaged_peer.node_id.distance(&unused_peer.node_id);
                assert!(unused_dist > selected_dist);
            }
        }

        // Test Closest
        let identities1 = peer_manager
            .get_broadcast_identities(BroadcastStrategy::Random(10))
            .unwrap();
        let identities2 = peer_manager
            .get_broadcast_identities(BroadcastStrategy::Random(10))
            .unwrap();
        assert_ne!(identities1, identities2);

        clean_up_datastore(database_name);
    }
    #[test]
    fn test_peer_reset_connection_attempts() {
        // Create peer manager with random peers
        let database_name = "pm_test_peer_reset_connection_attempts"; // Note: every test should have unique database
        let datastore = init_datastore(database_name).unwrap();
        let peer_database = datastore.get_handle(database_name).unwrap();
        let peer_manager = PeerManager::new(peer_database).unwrap();
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
                .find_with_node_id(&peer.node_id.clone())
                .unwrap()
                .addresses
                .addresses[0]
                .connection_attempts,
            2
        );
        peer_manager.reset_connection_attempts(&peer.node_id.clone()).unwrap();
        assert_eq!(
            peer_manager
                .find_with_node_id(&peer.node_id.clone())
                .unwrap()
                .addresses
                .addresses[0]
                .connection_attempts,
            0
        );

        clean_up_datastore(database_name);
    }
}

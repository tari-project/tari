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
    consts::{COMMS_RNG, PEER_MANAGER_MAX_FLOOD_PEERS},
    peer_manager::{
        connection_stats::PeerConnectionStats,
        node_id::{NodeDistance, NodeId},
        peer::{Peer, PeerFlags},
        peer_key::{generate_peer_key, PeerKey},
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
    },
    types::{CommsDatabase, CommsPublicKey},
};
use log::*;
use rand::Rng;
use std::{cmp::min, collections::HashMap, time::Duration};
use tari_storage::{IterationResult, KeyValueStore};

const LOG_TARGET: &str = "comms::peer_manager::peer_storage";

/// PeerStorage provides a mechanism to keep a datastore and a local copy of all peers in sync and allow fast searches
/// using the node_id, public key or net_address of a peer.
pub struct PeerStorage<DS> {
    pub(crate) peer_db: DS,
    node_id_hm: HashMap<NodeId, PeerKey>,
    public_key_hm: HashMap<CommsPublicKey, PeerKey>,
    net_address_hm: HashMap<NetAddress, PeerKey>,
}

impl<DS> PeerStorage<DS>
where DS: KeyValueStore<PeerKey, Peer>
{
    /// Constructs a new empty PeerStorage system
    pub fn new(database: DS) -> Result<PeerStorage<DS>, PeerManagerError> {
        // Restore peers and hashmap links from database
        let mut node_id_hm: HashMap<NodeId, PeerKey> = HashMap::new();
        let mut public_key_hm: HashMap<CommsPublicKey, PeerKey> = HashMap::new();
        let mut net_address_hm: HashMap<NetAddress, PeerKey> = HashMap::new();
        let mut total_entries = 0;
        database
            .for_each_ok(|(peer_key, peer)| {
                total_entries += 1;
                node_id_hm.insert(peer.node_id, peer_key);
                public_key_hm.insert(peer.public_key, peer_key);
                for net_address_with_stats in &peer.addresses.addresses {
                    net_address_hm.insert(net_address_with_stats.net_address.clone(), peer_key);
                }
                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        trace!(
            target: LOG_TARGET,
            "Peer storage is initializing. {} total entries. {} in node_hashmap. {} in public_key_hashmap. {} in \
             net_address_hashmap.",
            total_entries,
            node_id_hm.len(),
            public_key_hm.len(),
            net_address_hm.len()
        );

        Ok(PeerStorage {
            peer_db: database,
            node_id_hm,
            public_key_hm,
            net_address_hm,
        })
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exists, the stored version will be replaced with the newly provided peer.
    pub fn add_peer(&mut self, peer: Peer) -> Result<(), PeerManagerError> {
        match self.public_key_hm.get(&peer.public_key) {
            Some(&peer_key) => {
                trace!(target: LOG_TARGET, "Replacing peer that has NodeId '{}'", peer.node_id,);
                // Replace existing entry
                self.remove_hashmap_links(peer_key)?;
                self.add_hashmap_links(peer_key, &peer);
                self.peer_db
                    .insert(peer_key, peer)
                    .map_err(PeerManagerError::DatabaseError)?;
                Ok(())
            },
            None => {
                trace!(target: LOG_TARGET, "Adding peer with node id '{}'", peer.node_id,);
                // Add new entry
                // Generate new random peer key
                let peer_key = COMMS_RNG.with(|rng| generate_peer_key(&mut *rng.borrow_mut()));
                self.add_hashmap_links(peer_key, &peer);
                self.peer_db
                    .insert(peer_key, peer)
                    .map_err(PeerManagerError::DatabaseError)?;
                Ok(())
            },
        }
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub fn update_peer(
        &mut self,
        public_key: &CommsPublicKey,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<NetAddress>>,
        flags: Option<PeerFlags>,
        peer_features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
    ) -> Result<(), PeerManagerError>
    {
        match self.public_key_hm.get(public_key) {
            Some(peer_key) => {
                let peer_key = *peer_key;
                self.remove_hashmap_links(peer_key)?;

                let mut stored_peer: Peer = self
                    .peer_db
                    .get(&peer_key)
                    .map_err(PeerManagerError::DatabaseError)?
                    .ok_or(PeerManagerError::PeerNotFoundError)?;
                stored_peer.update(node_id, net_addresses, flags, peer_features, connection_stats);

                self.add_hashmap_links(peer_key, &stored_peer);
                self.peer_db
                    .insert(peer_key, stored_peer)
                    .map_err(PeerManagerError::DatabaseError)?;
                Ok(())
            },
            None => Err(PeerManagerError::PeerNotFoundError),
        }
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.remove_hashmap_links(peer_key)?;
        self.peer_db
            .delete(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?;
        Ok(())
    }

    /// Add key pairs to the search hashmaps for a newly added or moved peer
    fn add_hashmap_links(&mut self, peer_key: PeerKey, peer: &Peer) {
        self.node_id_hm.insert(peer.node_id.clone(), peer_key);
        self.public_key_hm.insert(peer.public_key.clone(), peer_key);
        for net_address_with_stats in &peer.addresses.addresses {
            self.net_address_hm
                .insert(net_address_with_stats.net_address.clone(), peer_key);
        }
    }

    /// Remove the peer specified by a given index from the database and remove hashmap keys
    fn remove_hashmap_links(&mut self, peer_key: PeerKey) -> Result<(), PeerManagerError> {
        let peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.node_id_hm.remove(&peer.node_id);
        self.public_key_hm.remove(&peer.public_key);
        for net_address_with_stats in &peer.addresses.addresses {
            self.net_address_hm.remove(&net_address_with_stats.net_address);
        }
        Ok(())
    }

    /// Find the peer with the provided NodeID
    pub fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer_key = *self
            .public_key_hm
            .get(&public_key)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)
    }

    /// Find the peer with the provided NetAddress
    pub fn find_by_net_address(&self, net_address: &NetAddress) -> Result<Peer, PeerManagerError> {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)
    }

    /// Check if a peer exist using the specified public_key
    pub fn exists(&self, public_key: &CommsPublicKey) -> bool {
        self.public_key_hm.get(&public_key).is_some()
    }

    /// Check if a peer exist using the specified node_id
    pub fn exists_node_id(&self, node_id: &NodeId) -> bool {
        self.node_id_hm.get(&node_id).is_some()
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        let peer_key = self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;

        let peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;

        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer.into())
        }
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer_key = *self
            .public_key_hm
            .get(&public_key)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer)
        }
    }

    /// Perform an ad-hoc query on the peer database.
    pub fn perform_query(&self, query: PeerQuery) -> Result<Vec<Peer>, PeerManagerError> {
        query.executor(&self.peer_db).get_results()
    }

    /// Compile a list of all known peers
    pub fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_db
            .filter_take(PEER_MANAGER_MAX_FLOOD_PEERS, |(_, peer)| {
                !peer.is_banned() && peer.has_features(PeerFeatures::MESSAGE_PROPAGATION)
            })
            .map(|pairs| pairs.into_iter().map(|(_, peer)| peer.into()).collect())
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Compile a list of peers
    pub fn closest_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &Vec<CommsPublicKey>,
    ) -> Result<Vec<Peer>, PeerManagerError>
    {
        let mut peer_keys: Vec<PeerKey> = Vec::new();
        let mut dists: Vec<NodeDistance> = Vec::new();
        self.peer_db
            .for_each_ok(|(peer_key, peer)| {
                if !peer.is_banned() && !excluded_peers.contains(&peer.public_key) {
                    peer_keys.push(peer_key);
                    dists.push(node_id.distance(&peer.node_id));
                }
                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;
        // Use all available peers up to a maximum of N
        let max_available = min(peer_keys.len(), n);
        if max_available == 0 {
            return Ok(Vec::new());
        }

        // Perform partial sort of elements only up to N elements
        let mut nearest_identities = Vec::with_capacity(max_available);
        for i in 0..max_available {
            for j in (i + 1)..peer_keys.len() {
                if dists[i] > dists[j] {
                    dists.swap(i, j);
                    peer_keys.swap(i, j);
                }
            }
            let peer = self
                .peer_db
                .get(&peer_keys[i])
                .map_err(PeerManagerError::DatabaseError)?
                .ok_or(PeerManagerError::PeerNotFoundError)?;
            nearest_identities.push(peer.into());
        }

        Ok(nearest_identities)
    }

    /// Compile a random list of peers of size _n_
    pub fn random_peers(&self, n: usize) -> Result<Vec<Peer>, PeerManagerError> {
        // TODO: Send to a random set of Communication Nodes
        let mut peer_keys = self
            .peer_db
            .filter(|(_, peer)| !peer.is_banned())
            .map(|pairs| pairs.into_iter().map(|(k, _)| k).collect::<Vec<_>>())
            .map_err(PeerManagerError::DatabaseError)?;

        // Use all available peers up to a maximum of N
        let max_available = min(peer_keys.len(), n);
        if max_available == 0 {
            return Ok(Vec::new());
        }

        // Shuffle first n elements
        COMMS_RNG.with(|rng| {
            for i in 0..max_available {
                let j = rng.borrow_mut().gen_range(0, peer_keys.len());
                peer_keys.swap(i, j);
            }
        });
        // Compile list of first n shuffled elements
        let mut random_identities = Vec::with_capacity(max_available);
        for i in 0..max_available {
            let peer = self
                .peer_db
                .get(&peer_keys[i])
                .map_err(PeerManagerError::DatabaseError)?
                .ok_or(PeerManagerError::PeerNotFoundError)?;
            random_identities.push(peer.into());
        }
        Ok(random_identities)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id. If there are less than N known peers, this will _always_ return true
    pub fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError>
    {
        let region2node_dist = region_node_id.distance(node_id);
        let mut dists = vec![NodeDistance::max_distance(); n];
        let last_index = dists.len() - 1;
        self.peer_db
            .for_each_ok(|(_, peer)| {
                if !peer.is_banned() {
                    let curr_dist = region_node_id.distance(&peer.node_id);
                    for i in 0..dists.len() {
                        if dists[i] > curr_dist {
                            dists.insert(i, curr_dist);
                            dists.pop();
                            break;
                        }
                    }

                    if region2node_dist > dists[last_index] {
                        return IterationResult::Break;
                    }
                }

                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        Ok(region2node_dist <= dists[last_index])
    }

    /// Enables Thread safe access - Changes the ban flag bit of the peer
    pub fn set_banned(&mut self, node_id: &NodeId, ban_flag: bool) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.set_banned(ban_flag);
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Enables Thread safe access - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&mut self, node_id: &NodeId, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.addresses.add_net_address(net_address);
        self.net_address_hm.insert(net_address.clone(), peer_key);
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Enables Thread safe access - Finds and returns the highest priority net address until all connection attempts
    /// for each net address have been reached
    pub fn get_best_net_address(&mut self, node_id: &NodeId) -> Result<NetAddress, PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let best_net_address = peer
            .addresses
            .get_best_net_address()
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)?;
        Ok(best_net_address)
    }

    /// Enables Thread safe access - The average connection latency of the provided net address will be updated to
    /// include the current measured latency sample
    pub fn update_latency(
        &mut self,
        net_address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), PeerManagerError>
    {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.addresses
            .update_latency(net_address, latency_measurement)
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Enables Thread safe access - Mark that a message was received from the specified net address
    pub fn mark_message_received(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.addresses
            .mark_message_received(net_address)
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Enables Thread safe access - Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.addresses
            .mark_message_rejected(net_address)
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;

        peer.addresses
            .mark_successful_connection_attempt(net_address)
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;

        peer.addresses
            .mark_failed_connection_attempt(net_address)
            .map_err(PeerManagerError::NetAddressError)?;
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Enables Thread safe access - Finds a peer and if it exists resets all connection attempts on all net address
    /// belonging to that peer
    pub fn reset_connection_attempts(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.addresses.reset_connection_attempts();

        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }
}

impl Into<CommsDatabase> for PeerStorage<CommsDatabase> {
    fn into(self) -> CommsDatabase {
        self.peer_db
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::net_address::{net_addresses::NetAddressesWithStats, NetAddress},
        peer_manager::{peer::PeerFlags, PeerFeatures},
    };
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HMapDatabase;

    #[test]
    fn test_restore() {
        // Create Peers
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = NetAddress::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let net_address2 = NetAddress::from("5.6.7.8:8000".parse::<NetAddress>().unwrap());
        let net_address3 = NetAddress::from("5.6.7.8:7000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddressesWithStats::from(net_address1.clone());
        net_addresses.add_net_address(&net_address2);
        net_addresses.add_net_address(&net_address3);
        let peer1 = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address4 = NetAddress::from("9.10.11.12:7000".parse::<NetAddress>().unwrap());
        let net_addresses = NetAddressesWithStats::from(net_address4.clone());
        let peer2: Peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address5 = NetAddress::from("13.14.15.16:6000".parse::<NetAddress>().unwrap());
        let net_address6 = NetAddress::from("17.18.19.20:8000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddressesWithStats::from(net_address5.clone());
        net_addresses.add_net_address(&net_address6);
        let peer3 = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());

        // Create new datastore with a peer database
        let mut db = Some(HMapDatabase::new());
        {
            let mut peer_storage = PeerStorage::new(db.take().unwrap()).unwrap();

            // Test adding and searching for peers
            assert!(peer_storage.add_peer(peer1.clone()).is_ok());
            assert!(peer_storage.add_peer(peer2.clone()).is_ok());
            assert!(peer_storage.add_peer(peer3.clone()).is_ok());

            assert_eq!(peer_storage.peer_db.size().unwrap(), 3);
            assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
            assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
            assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
            db = Some(peer_storage.peer_db);
        }
        // Restore from existing database
        let peer_storage = PeerStorage::new(db.take().unwrap()).unwrap();

        assert_eq!(peer_storage.peer_db.size().unwrap(), 3);
        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
    }

    #[test]
    fn test_add_delete_find_peer() {
        let mut peer_storage = PeerStorage::new(HMapDatabase::new()).unwrap();

        // Create Peers
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = NetAddress::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let net_address2 = NetAddress::from("5.6.7.8:8000".parse::<NetAddress>().unwrap());
        let net_address3 = NetAddress::from("5.6.7.8:7000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddressesWithStats::from(net_address1.clone());
        net_addresses.add_net_address(&net_address2);
        net_addresses.add_net_address(&net_address3);
        let peer1 = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address4 = NetAddress::from("9.10.11.12:7000".parse::<NetAddress>().unwrap());
        let net_addresses = NetAddressesWithStats::from(net_address4.clone());
        let peer2: Peer = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address5 = NetAddress::from("13.14.15.16:6000".parse::<NetAddress>().unwrap());
        let net_address6 = NetAddress::from("17.18.19.20:8000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddressesWithStats::from(net_address5.clone());
        net_addresses.add_net_address(&net_address6);
        let peer3 = Peer::new(pk, node_id, net_addresses, PeerFlags::default(), PeerFeatures::empty());
        // Test adding and searching for peers
        assert!(peer_storage.add_peer(peer1.clone()).is_ok());
        assert!(peer_storage.add_peer(peer2.clone()).is_ok());
        assert!(peer_storage.add_peer(peer3.clone()).is_ok());

        assert_eq!(peer_storage.peer_db.len().unwrap(), 3);

        assert_eq!(
            peer_storage.find_by_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_public_key(&peer2.public_key).unwrap().public_key,
            peer2.public_key
        );
        assert_eq!(
            peer_storage.find_by_public_key(&peer3.public_key).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(
            peer_storage.find_by_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer2.node_id).unwrap().node_id,
            peer2.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer3.node_id).unwrap().node_id,
            peer3.node_id
        );

        assert_eq!(
            peer_storage.find_by_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address4).unwrap().public_key,
            peer2.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address5).unwrap().public_key,
            peer3.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address6).unwrap().public_key,
            peer3.public_key
        );

        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());

        // Test delete of border case peer
        assert!(peer_storage.delete_peer(&peer3.node_id).is_ok());

        assert_eq!(peer_storage.peer_db.len().unwrap(), 2);

        assert_eq!(
            peer_storage.find_by_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_public_key(&peer2.public_key).unwrap().public_key,
            peer2.public_key
        );
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_err());

        assert_eq!(
            peer_storage.find_by_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer2.node_id).unwrap().node_id,
            peer2.node_id
        );
        assert!(peer_storage.find_by_node_id(&peer3.node_id).is_err());

        assert_eq!(
            peer_storage.find_by_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address4).unwrap().public_key,
            peer2.public_key
        );
        assert!(peer_storage.find_by_net_address(&net_address5).is_err());
        assert!(peer_storage.find_by_net_address(&net_address6).is_err());

        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_err());

        // Test of delete with moving behaviour
        assert!(peer_storage.add_peer(peer3.clone()).is_ok());
        assert!(peer_storage.delete_peer(&peer2.node_id).is_ok());

        assert_eq!(peer_storage.peer_db.len().unwrap(), 2);

        assert_eq!(
            peer_storage.find_by_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_err());
        assert_eq!(
            peer_storage.find_by_public_key(&peer3.public_key).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(
            peer_storage.find_by_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert!(peer_storage.find_by_node_id(&peer2.node_id).is_err());
        assert_eq!(
            peer_storage.find_by_node_id(&peer3.node_id).unwrap().node_id,
            peer3.node_id
        );

        assert_eq!(
            peer_storage.find_by_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert!(peer_storage.find_by_net_address(&net_address4).is_err());
        assert_eq!(
            peer_storage.find_by_net_address(&net_address5).unwrap().public_key,
            peer3.public_key
        );
        assert_eq!(
            peer_storage.find_by_net_address(&net_address6).unwrap().public_key,
            peer3.public_key
        );

        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_err());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
    }
}

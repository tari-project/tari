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
    consts::PEER_MANAGER_MAX_FLOOD_PEERS,
    peer_manager::{
        connection_stats::PeerConnectionStats,
        node_id::{NodeDistance, NodeId},
        peer::{Peer, PeerFlags},
        peer_id::{generate_peer_key, PeerId},
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
    },
    protocol::ProtocolId,
    types::{CommsDatabase, CommsPublicKey},
};
use log::*;
use multiaddr::Multiaddr;
use rand::{rngs::OsRng, Rng};
use std::{cmp, collections::HashMap, fmt, time::Duration};
use tari_storage::{IterationResult, KeyValueStore};

const LOG_TARGET: &str = "comms::peer_manager::peer_storage";

/// PeerStorage provides a mechanism to keep a datastore and a local copy of all peers in sync and allow fast searches
/// using the node_id, public key or net_address of a peer.
pub struct PeerStorage<DS> {
    pub(crate) peer_db: DS,
    public_key_index: HashMap<CommsPublicKey, PeerId>,
    node_id_index: HashMap<NodeId, PeerId>,
}

impl<DS> PeerStorage<DS>
where DS: KeyValueStore<PeerId, Peer>
{
    /// Constructs a new PeerStorage, with indexes populated from the given datastore
    pub fn new_indexed(database: DS) -> Result<PeerStorage<DS>, PeerManagerError> {
        // Restore peers and hashmap links from database
        let mut public_key_index = HashMap::new();
        let mut node_id_index = HashMap::new();
        let mut total_entries = 0;
        database
            .for_each_ok(|(peer_key, peer)| {
                total_entries += 1;
                public_key_index.insert(peer.public_key, peer_key);
                node_id_index.insert(peer.node_id, peer_key);
                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        trace!(
            target: LOG_TARGET,
            "Peer storage is initialized. {} total entries.",
            total_entries,
        );

        Ok(PeerStorage {
            peer_db: database,
            public_key_index,
            node_id_index,
        })
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exists, the stored version will be replaced with the newly provided peer.
    pub fn add_peer(&mut self, mut peer: Peer) -> Result<PeerId, PeerManagerError> {
        let (public_key, node_id) = (peer.public_key.clone(), peer.node_id.clone());
        match self.public_key_index.get(&peer.public_key).copied() {
            Some(peer_key) => {
                trace!(target: LOG_TARGET, "Replacing peer that has NodeId '{}'", peer.node_id);
                // Replace existing entry
                peer.set_id(peer_key);
                self.peer_db
                    .insert(peer_key, peer)
                    .map_err(PeerManagerError::DatabaseError)?;
                self.remove_index_links(peer_key);
                self.add_index_links(peer_key, public_key, node_id);
                Ok(peer_key)
            },
            None => {
                // Add new entry
                trace!(target: LOG_TARGET, "Adding peer with node id '{}'", peer.node_id);
                // Generate new random peer key
                let peer_key = generate_peer_key();
                peer.set_id(peer_key);
                self.peer_db
                    .insert(peer_key, peer)
                    .map_err(PeerManagerError::DatabaseError)?;
                self.add_index_links(peer_key, public_key, node_id);
                Ok(peer_key)
            },
        }
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    #[allow(clippy::too_many_arguments)]
    pub fn update_peer(
        &mut self,
        public_key: &CommsPublicKey,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<Multiaddr>>,
        flags: Option<PeerFlags>,
        #[allow(clippy::option_option)] banned_until: Option<Option<Duration>>,
        #[allow(clippy::option_option)] is_offline: Option<bool>,
        peer_features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
        supported_protocols: Option<Vec<ProtocolId>>,
    ) -> Result<(), PeerManagerError>
    {
        match self.public_key_index.get(public_key).copied() {
            Some(peer_key) => {
                let mut stored_peer = self
                    .peer_db
                    .get(&peer_key)
                    .map_err(PeerManagerError::DatabaseError)?
                    .expect("Public key index and peer database are out of sync!");

                trace!(target: LOG_TARGET, "Updating peer '{}'", stored_peer.node_id);

                let must_update_node_id = node_id.as_ref().filter(|n| *n != &stored_peer.node_id).is_some();
                if must_update_node_id {
                    trace!(
                        target: LOG_TARGET,
                        "Node id update from '{}' to '{}'",
                        stored_peer.node_id.short_str(),
                        node_id.as_ref().expect("already checked").short_str()
                    );
                }
                stored_peer.update(
                    node_id,
                    net_addresses,
                    flags,
                    banned_until,
                    is_offline,
                    peer_features,
                    connection_stats,
                    supported_protocols,
                );

                let public_key = stored_peer.public_key.clone();
                let node_id = stored_peer.node_id.clone();

                self.peer_db
                    .insert(peer_key, stored_peer)
                    .map_err(PeerManagerError::DatabaseError)?;

                if must_update_node_id {
                    trace!(target: LOG_TARGET, "Must update node id for peer '{}'", node_id);
                    self.remove_index_links(peer_key);
                    self.add_index_links(peer_key, public_key, node_id);
                }

                Ok(())
            },
            None => {
                trace!(
                    target: LOG_TARGET,
                    "Peer not found because the public key '{}' could not be found in the index",
                    public_key
                );
                Err(PeerManagerError::PeerNotFoundError)
            },
        }
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_index
            .get(&node_id)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        self.peer_db
            .delete(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?;

        self.remove_index_links(peer_key);
        Ok(())
    }

    /// Add key pairs to the search hashmaps for a newly added or moved peer
    fn add_index_links(&mut self, peer_key: PeerId, public_key: CommsPublicKey, node_id: NodeId) {
        self.node_id_index.insert(node_id, peer_key);
        self.public_key_index.insert(public_key, peer_key);
    }

    /// Remove the peer specified by a given index from the database and remove hashmap keys
    fn remove_index_links(&mut self, peer_key: PeerId) {
        let initial_size_pk = self.public_key_index.len();
        let initial_size_node_id = self.node_id_index.len();
        self.public_key_index = self.public_key_index.drain().filter(|(_, k)| k != &peer_key).collect();
        self.node_id_index = self.node_id_index.drain().filter(|(_, k)| k != &peer_key).collect();
        debug_assert_eq!(initial_size_pk - 1, self.public_key_index.len());
        debug_assert_eq!(initial_size_node_id - 1, self.node_id_index.len());
    }

    /// Find the peer with the provided NodeID
    pub fn find_by_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        let peer_key = self
            .node_id_index
            .get(node_id)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        Ok(self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .expect("public_key index and peer database are out of sync"))
    }

    /// Find the peer with the provided PublicKey
    pub fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer_key = self
            .public_key_index
            .get(public_key)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        Ok(self
            .peer_db
            .get(peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .expect("public_key index and peer database are out of sync"))
    }

    /// Check if a peer exist using the specified public_key
    pub fn exists(&self, public_key: &CommsPublicKey) -> bool {
        self.public_key_index.contains_key(public_key)
    }

    /// Check if a peer exist using the specified node_id
    pub fn exists_node_id(&self, node_id: &NodeId) -> bool {
        self.node_id_index.contains_key(node_id)
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        let peer = self.find_by_node_id(node_id)?;

        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer)
        }
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer = self.find_by_public_key(public_key)?;
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

    /// Return all peers
    pub fn all(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let mut peers = Vec::with_capacity(self.peer_db.size()?);
        self.peer_db.for_each_ok(|(_, peer)| {
            peers.push(peer);
            IterationResult::Continue
        })?;
        Ok(peers)
    }

    /// Compile a list of all known peers
    pub fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_db
            .filter_take(PEER_MANAGER_MAX_FLOOD_PEERS, |(_, peer)| !peer.is_banned())
            .map(|pairs| pairs.into_iter().map(|(_, peer)| peer).collect())
            .map_err(PeerManagerError::DatabaseError)
    }

    pub fn for_each<F>(&self, mut f: F) -> Result<(), PeerManagerError>
    where F: FnMut(Peer) -> IterationResult {
        self.peer_db.for_each_ok(|(_, peer)| f(peer)).map_err(Into::into)
    }

    /// Compile a list of peers
    pub fn closest_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[CommsPublicKey],
        features: Option<PeerFeatures>,
    ) -> Result<Vec<Peer>, PeerManagerError>
    {
        let mut peer_keys = Vec::new();
        let mut dists = Vec::new();
        self.peer_db
            .for_each_ok(|(peer_key, peer)| {
                if features.map(|f| peer.features == f).unwrap_or(true) &&
                    !peer.is_banned() &&
                    !peer.is_offline() &&
                    !excluded_peers.contains(&peer.public_key)
                {
                    peer_keys.push(peer_key);
                    dists.push(node_id.distance(&peer.node_id));
                }
                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;
        // Use all available peers up to a maximum of N
        let max_available = cmp::min(peer_keys.len(), n);
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
                .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
            nearest_identities.push(peer);
        }

        Ok(nearest_identities)
    }

    /// Compile a random list of peers of size _n_
    pub fn random_peers(&self, n: usize) -> Result<Vec<Peer>, PeerManagerError> {
        // TODO: Send to a random set of Communication Nodes
        let mut peer_keys = self
            .peer_db
            .filter(|(_, peer)| !peer.is_banned() && peer.features == PeerFeatures::COMMUNICATION_NODE)
            .map(|pairs| pairs.into_iter().map(|(k, _)| k).collect::<Vec<_>>())
            .map_err(PeerManagerError::DatabaseError)?;

        // Use all available peers up to a maximum of N
        let max_available = cmp::min(peer_keys.len(), n);
        if max_available == 0 {
            return Ok(Vec::new());
        }

        // Shuffle first n elements
        for i in 0..max_available {
            let j = OsRng.gen_range(0, peer_keys.len());
            peer_keys.swap(i, j);
        }
        // Compile list of first n shuffled elements
        let mut random_identities = Vec::with_capacity(max_available);
        // for i in 0..max_available {
        for peer_key in peer_keys.iter().take(max_available) {
            let peer = self
                .peer_db
                .get(peer_key)
                .map_err(PeerManagerError::DatabaseError)?
                .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
            random_identities.push(peer);
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
        let region_node_distance = region_node_id.distance(node_id);
        let node_threshold = self.calc_region_threshold(region_node_id, n, PeerFeatures::COMMUNICATION_NODE)?;
        // Is node ID in the base node threshold?
        if region_node_distance <= node_threshold {
            return Ok(true);
        }
        let client_threshold = self.calc_region_threshold(region_node_id, n, PeerFeatures::COMMUNICATION_CLIENT)?;
        // Is node ID in the base client threshold?
        Ok(region_node_distance <= client_threshold)
    }

    pub fn calc_region_threshold(
        &self,
        region_node_id: &NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<NodeDistance, PeerManagerError>
    {
        self.get_region_stats(region_node_id, n, features)
            .map(|stats| stats.distance)
    }

    /// Unban the peer
    pub fn unban(&mut self, public_key: &CommsPublicKey) -> Result<NodeId, PeerManagerError> {
        let peer_key = *self
            .public_key_index
            .get(&public_key)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        let mut peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        let node_id = peer.node_id.clone();

        if peer.banned_until.is_some() {
            peer.unban();
            self.peer_db
                .insert(peer_key, peer)
                .map_err(PeerManagerError::DatabaseError)?;
        }
        Ok(node_id)
    }

    /// Ban the peer for the given duration
    pub fn ban_for(&mut self, public_key: &CommsPublicKey, duration: Duration) -> Result<NodeId, PeerManagerError> {
        let peer_key = *self
            .public_key_index
            .get(&public_key)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        peer.ban_for(duration);
        let node_id = peer.node_id.clone();
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)?;
        Ok(node_id)
    }

    /// Changes the OFFLINE flag bit of the peer
    pub fn set_offline(&mut self, public_key: &CommsPublicKey, ban_flag: bool) -> Result<NodeId, PeerManagerError> {
        let peer_key = *self
            .public_key_index
            .get(&public_key)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        peer.set_offline(ban_flag);
        let node_id = peer.node_id.clone();
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)?;
        Ok(node_id)
    }

    /// Enables Thread safe access - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&mut self, node_id: &NodeId, net_address: &Multiaddr) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_index
            .get(&node_id)
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or_else(|| PeerManagerError::PeerNotFoundError)?;
        peer.addresses.add_net_address(net_address);
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)
    }

    /// Return some basic stats for the region surrounding the region_node_id
    pub fn get_region_stats<'a>(
        &self,
        region_node_id: &'a NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<RegionStats<'a>, PeerManagerError>
    {
        let mut dists = vec![NodeDistance::max_distance(); n];
        let last_index = n - 1;

        let mut neighbours = vec![None; n];
        self.peer_db
            .for_each_ok(|(_, peer)| {
                if peer.features != features {
                    return IterationResult::Continue;
                }

                if peer.is_banned() {
                    return IterationResult::Continue;
                }
                if peer.is_offline() {
                    return IterationResult::Continue;
                }

                let curr_dist = region_node_id.distance(&peer.node_id);
                for i in 0..dists.len() {
                    if dists[i] > curr_dist {
                        dists.insert(i, curr_dist);
                        dists.pop();
                        neighbours.insert(i, Some(peer));
                        neighbours.pop();
                        break;
                    }
                }

                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        let distance = dists.remove(last_index);
        let total = neighbours.iter().filter(|p| p.is_some()).count();
        let num_offline = neighbours
            .iter()
            .filter(|p| p.as_ref().map(|p| p.is_offline()).unwrap_or(false))
            .count();
        let num_banned = neighbours
            .iter()
            .filter(|p| p.as_ref().map(|p| p.is_banned()).unwrap_or(false))
            .count();

        Ok(RegionStats {
            distance,
            ref_node_id: region_node_id,
            total,
            num_offline,
            num_banned,
        })
    }
}

impl Into<CommsDatabase> for PeerStorage<CommsDatabase> {
    fn into(self) -> CommsDatabase {
        self.peer_db
    }
}

pub struct RegionStats<'a> {
    distance: NodeDistance,
    ref_node_id: &'a NodeId,
    total: usize,
    num_offline: usize,
    num_banned: usize,
}

impl RegionStats<'_> {
    pub fn in_region(&self, node_id: &NodeId) -> bool {
        node_id.distance(self.ref_node_id) <= self.distance
    }

    pub fn offline_ratio(&self) -> f32 {
        self.num_offline as f32 / self.total as f32
    }

    pub fn banned_ratio(&self) -> f32 {
        self.num_banned as f32 / self.total as f32
    }
}

impl fmt::Display for RegionStats<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RegionStats(distance = {}, total = {}, num offline = {}, num banned = {})",
            self.distance, self.total, self.num_offline, self.num_banned
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        net_address::MultiaddressesWithStats,
        peer_manager::{peer::PeerFlags, PeerFeatures},
    };
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    #[test]
    fn test_restore() {
        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = "/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/5.6.7.8/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/5.6.7.8/tcp/7000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address1.clone());
        net_addresses.add_net_address(&net_address2);
        net_addresses.add_net_address(&net_address3);
        let peer1 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address4 = "/ip4/9.10.11.12/tcp/7000".parse::<Multiaddr>().unwrap();
        let net_addresses = MultiaddressesWithStats::from(net_address4.clone());
        let peer2: Peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address5 = "/ip4/13.14.15.16/tcp/6000".parse::<Multiaddr>().unwrap();
        let net_address6 = "/ip4/17.18.19.20/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address5.clone());
        net_addresses.add_net_address(&net_address6);
        let peer3 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );

        // Create new datastore with a peer database
        let mut db = Some(HashmapDatabase::new());
        {
            let mut peer_storage = PeerStorage::new_indexed(db.take().unwrap()).unwrap();

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
        let peer_storage = PeerStorage::new_indexed(db.take().unwrap()).unwrap();

        assert_eq!(peer_storage.peer_db.size().unwrap(), 3);
        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
    }

    #[test]
    fn test_add_delete_find_peer() {
        let mut peer_storage = PeerStorage::new_indexed(HashmapDatabase::new()).unwrap();

        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = "/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/5.6.7.8/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/5.6.7.8/tcp/7000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address1.clone());
        net_addresses.add_net_address(&net_address2);
        net_addresses.add_net_address(&net_address3);
        let peer1 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address4 = "/ip4/9.10.11.12/tcp/7000".parse::<Multiaddr>().unwrap();
        let net_addresses = MultiaddressesWithStats::from(net_address4.clone());
        let peer2: Peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address5 = "/ip4/13.14.15.16/tcp/6000".parse::<Multiaddr>().unwrap();
        let net_address6 = "/ip4/17.18.19.20/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address5.clone());
        net_addresses.add_net_address(&net_address6);
        let peer3 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
        );
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

        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_err());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
    }
}

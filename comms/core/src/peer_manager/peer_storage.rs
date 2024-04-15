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

use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use log::*;
use rand::{rngs::OsRng, seq::SliceRandom};
use tari_storage::{IterationResult, KeyValueStore};
use tari_utilities::ByteArray;

use crate::{
    peer_manager::{
        peer::Peer,
        peer_id::{generate_peer_key, PeerId},
        NodeDistance,
        NodeId,
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
        PeerQuerySortBy,
    },
    types::{CommsDatabase, CommsPublicKey},
};

const LOG_TARGET: &str = "comms::peer_manager::peer_storage";
/// The maximum number of peers to return in peer manager
const PEER_MANAGER_SYNC_PEERS: usize = 100;
const PEER_ACTIVE_WITHIN_DURATION: u64 = 7 * 24 * 60 * 60; // 7 days, 24h, 60m, 60s = 1 week

/// PeerStorage provides a mechanism to keep a datastore and a local copy of all peers in sync and allow fast searches
/// using the node_id, public key or net_address of a peer.
pub struct PeerStorage<DS> {
    peer_db: DS,
    public_key_index: HashMap<CommsPublicKey, PeerId>,
    node_id_index: HashMap<NodeId, PeerId>,
}

impl<DS> PeerStorage<DS>
where DS: KeyValueStore<PeerId, Peer>
{
    /// Constructs a new PeerStorage, with indexes populated from the given datastore
    pub fn new_indexed(database: DS) -> Result<PeerStorage<DS>, PeerManagerError> {
        // mutable_key_type: CommsPublicKey uses interior mutability to lazily compress the key, but is otherwise
        // immutable so the Hashmap order can never change.
        #[allow(clippy::mutable_key_type)]
        let mut public_key_index = HashMap::new();
        let mut node_id_index = HashMap::new();
        let mut total_entries = 0;
        // Restore peers and hashmap links from database
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

    pub fn count(&self) -> usize {
        self.node_id_index.len()
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
                let mut existing_peer = self
                    .peer_db
                    .get(&peer_key)
                    .map_err(PeerManagerError::DatabaseError)?
                    .ok_or(PeerManagerError::PeerNotFoundError)?;
                existing_peer.merge(&peer);
                self.peer_db
                    .insert(peer_key, existing_peer)
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

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_index
            .get(node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        peer.deleted_at = Some(Utc::now().naive_utc());
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)?;
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
    pub fn find_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        match self.node_id_index.get(node_id) {
            Some(peer_key) => {
                let peer = self.peer_db.get(peer_key)?.ok_or_else(|| {
                    warn!(
                        target: LOG_TARGET,
                        "node_id_index and peer database are out of sync! (key={}, node_id={})", peer_key, node_id
                    );
                    PeerManagerError::DataInconsistency(format!(
                        "node_id_index and peer database are out of sync! (key={}, node_id={})",
                        peer_key, node_id
                    ))
                })?;
                Ok(Some(peer))
            },
            None => Ok(None),
        }
    }

    pub fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        if partial.is_empty() || partial.len() > NodeId::byte_size() {
            return Ok(Vec::new());
        }

        let keys = self
            .node_id_index
            .iter()
            .filter(|(k, _)| {
                let l = partial.len();
                &k.as_bytes()[..l] == partial
            })
            .map(|(_, id)| *id)
            .collect::<Vec<_>>();
        self.peer_db.get_many(&keys).map_err(PeerManagerError::DatabaseError)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        match self.public_key_index.get(public_key) {
            Some(peer_key) => {
                let peer = self
                    .peer_db
                    .get(peer_key)
                    .map_err(PeerManagerError::DatabaseError)?
                    .ok_or_else(|| {
                        warn!(
                            target: LOG_TARGET,
                            "public_key_index and peer database are out of sync! (key={}, public_key ={})",
                            peer_key,
                            public_key
                        );
                        PeerManagerError::DataInconsistency(format!(
                            "public_key_index and peer database are out of sync! (key={}, public_key ={})",
                            peer_key, public_key
                        ))
                    })?;

                Ok(Some(peer))
            },
            None => Ok(None),
        }
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
        let peer = self
            .find_by_node_id(node_id)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;

        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer)
        }
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer = self
            .find_by_public_key(public_key)?
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

    /// Return all peers
    pub fn all(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let mut peers = Vec::with_capacity(self.peer_db.size()?);
        self.peer_db.for_each_ok(|(_, peer)| {
            peers.push(peer);
            IterationResult::Continue
        })?;
        Ok(peers)
    }

    /// Return "good" peers for syncing
    /// Criteria:
    ///  - Peer is not banned
    ///  - Peer has been seen within a defined time span (1 week)
    ///  - Only returns a maximum number of syncable peers (corresponds with the max possible number of requestable
    ///    peers to sync)
    ///  - Uses 0 as max PEER_MANAGER_SYNC_PEERS
    pub fn discovery_syncing(
        &self,
        mut n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        if n == 0 {
            n = PEER_MANAGER_SYNC_PEERS
        };

        let query = PeerQuery::new()
            .select_where(|peer| is_active_peer(peer, features, excluded_peers))
            .limit(n);

        self.perform_query(query)
    }

    /// Compile a list of all known peers
    pub fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_db
            .filter_take(PEER_MANAGER_SYNC_PEERS, |(_, peer)| !peer.is_banned())
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
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let query = PeerQuery::new()
            .select_where(|peer| is_active_peer(peer, features, excluded_peers))
            .sort_by(PeerQuerySortBy::DistanceFrom(node_id))
            .limit(n);

        self.perform_query(query)
    }

    /// Compile a random list of communication node peers of size _n_ that are not banned or offline
    pub fn random_peers(&self, n: usize, exclude_peers: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut peers = self
            .peer_db
            .filter(|(_, peer)| {
                !peer.is_offline() &&
                    !peer.is_banned() &&
                    peer.features == PeerFeatures::COMMUNICATION_NODE &&
                    !exclude_peers.contains(&peer.node_id)
            })
            .map(|pairs| pairs.into_iter().map(|(_, p)| p).collect::<Vec<_>>())
            .map_err(PeerManagerError::DatabaseError)?;

        if peers.is_empty() {
            return Ok(Vec::new());
        }
        peers.shuffle(&mut OsRng);
        peers.truncate(n);

        Ok(peers)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id. If there are less than N known peers, this will _always_ return true
    pub fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError> {
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
    ) -> Result<NodeDistance, PeerManagerError> {
        if n == 0 {
            return Ok(NodeDistance::max_distance());
        }

        let mut dists = Vec::new();
        self.peer_db
            .for_each_ok(|(_, peer)| {
                if peer.features != features || peer.is_banned() || peer.is_offline() {
                    return IterationResult::Continue;
                }
                dists.push(region_node_id.distance(&peer.node_id));
                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        if dists.is_empty() {
            return Ok(NodeDistance::max_distance());
        }

        // If we have less than `n` matching peers in our threshold group, the threshold should be max
        if dists.len() < n {
            return Ok(NodeDistance::max_distance());
        }

        dists.sort();
        dists.truncate(n);
        Ok(dists.pop().expect("dists cannot be empty at this point"))
    }

    /// Unban the peer
    pub fn unban_peer(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_key = *self
            .node_id_index
            .get(node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .expect("public_key_index is out of sync with peer db");

        if peer.banned_until.is_some() {
            peer.unban();
            self.peer_db
                .insert(peer_key, peer)
                .map_err(PeerManagerError::DatabaseError)?;
        }
        Ok(())
    }

    /// Ban the peer for the given duration
    pub fn ban_peer(
        &mut self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let id = *self
            .public_key_index
            .get(public_key)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.ban_peer_by_id(id, duration, reason)
    }

    /// Ban the peer for the given duration
    pub fn ban_peer_by_node_id(
        &mut self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let id = *self
            .node_id_index
            .get(node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.ban_peer_by_id(id, duration, reason)
    }

    fn ban_peer_by_id(&mut self, id: PeerId, duration: Duration, reason: String) -> Result<NodeId, PeerManagerError> {
        let mut peer: Peer = self
            .peer_db
            .get(&id)
            .map_err(PeerManagerError::DatabaseError)?
            .expect("index are out of sync with peer db");
        peer.ban_for(duration, reason);
        let node_id = peer.node_id.clone();
        self.peer_db.insert(id, peer).map_err(PeerManagerError::DatabaseError)?;
        Ok(node_id)
    }

    pub fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let peer = self
            .find_by_node_id(node_id)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(peer.is_banned())
    }

    /// This will store metadata inside of the metadata field in the peer provided by the nodeID.
    /// It will return None if the value was empty and the old value if the value was updated
    pub fn set_peer_metadata(
        &self,
        node_id: &NodeId,
        key: u8,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, PeerManagerError> {
        let peer_key = *self
            .node_id_index
            .get(node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let mut peer: Peer = self
            .peer_db
            .get(&peer_key)
            .map_err(PeerManagerError::DatabaseError)?
            .expect("node_id_index is out of sync with peer db");
        let result = peer.set_metadata(key, data);
        self.peer_db
            .insert(peer_key, peer)
            .map_err(PeerManagerError::DatabaseError)?;
        Ok(result)
    }
}

#[allow(clippy::from_over_into)]
impl Into<CommsDatabase> for PeerStorage<CommsDatabase> {
    fn into(self) -> CommsDatabase {
        self.peer_db
    }
}

fn is_active_peer(peer: &Peer, features: Option<PeerFeatures>, excluded_peers: &[NodeId]) -> bool {
    features.map(|f| peer.features == f).unwrap_or(true) &&
        !excluded_peers.contains(&peer.node_id) &&
        !peer.is_banned() &&
        peer.deleted_at.is_none() &&
        peer.last_seen_since().is_some() &&
        peer.last_seen_since().expect("Last seen to exist") <= Duration::from_secs(PEER_ACTIVE_WITHIN_DURATION)
}

#[cfg(test)]
mod test {
    use std::{borrow::BorrowMut, iter::repeat_with};

    use chrono::NaiveDateTime;
    use multiaddr::Multiaddr;
    use rand::Rng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    use super::*;
    use crate::{
        net_address::{MultiaddrWithStats, MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{peer::PeerFlags, PeerFeatures},
    };

    #[test]
    fn test_restore() {
        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address1 = "/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/5.6.7.8/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/5.6.7.8/tcp/7000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address1], &PeerAddressSource::Config);
        net_addresses.add_address(&net_address2, &PeerAddressSource::Config);
        net_addresses.add_address(&net_address3, &PeerAddressSource::Config);
        let peer1 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address4 = "/ip4/9.10.11.12/tcp/7000".parse::<Multiaddr>().unwrap();
        let net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address4], &PeerAddressSource::Config);
        let peer2: Peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address5 = "/ip4/13.14.15.16/tcp/6000".parse::<Multiaddr>().unwrap();
        let net_address6 = "/ip4/17.18.19.20/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address5], &PeerAddressSource::Config);
        net_addresses.add_address(&net_address6, &PeerAddressSource::Config);
        let peer3 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
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

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_add_delete_find_peer() {
        let mut peer_storage = PeerStorage::new_indexed(HashmapDatabase::new()).unwrap();

        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address1 = "/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/5.6.7.8/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/5.6.7.8/tcp/7000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address1], &PeerAddressSource::Config);
        net_addresses.add_address(&net_address2, &PeerAddressSource::Config);
        net_addresses.add_address(&net_address3, &PeerAddressSource::Config);
        let peer1 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address4 = "/ip4/9.10.11.12/tcp/7000".parse::<Multiaddr>().unwrap();
        let net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address4], &PeerAddressSource::Config);
        let peer2: Peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let net_address5 = "/ip4/13.14.15.16/tcp/6000".parse::<Multiaddr>().unwrap();
        let net_address6 = "/ip4/17.18.19.20/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address5], &PeerAddressSource::Config);
        net_addresses.add_address(&net_address6, &PeerAddressSource::Config);
        let peer3 = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );
        // Test adding and searching for peers
        assert!(peer_storage.add_peer(peer1.clone()).is_ok());
        assert!(peer_storage.add_peer(peer2.clone()).is_ok());
        assert!(peer_storage.add_peer(peer3.clone()).is_ok());

        assert_eq!(peer_storage.peer_db.len().unwrap(), 3);

        assert_eq!(
            peer_storage
                .find_by_public_key(&peer1.public_key)
                .unwrap()
                .unwrap()
                .public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage
                .find_by_public_key(&peer2.public_key)
                .unwrap()
                .unwrap()
                .public_key,
            peer2.public_key
        );
        assert_eq!(
            peer_storage
                .find_by_public_key(&peer3.public_key)
                .unwrap()
                .unwrap()
                .public_key,
            peer3.public_key
        );

        assert_eq!(
            peer_storage.find_by_node_id(&peer1.node_id).unwrap().unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer2.node_id).unwrap().unwrap().node_id,
            peer2.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer3.node_id).unwrap().unwrap().node_id,
            peer3.node_id
        );

        peer_storage.find_by_public_key(&peer1.public_key).unwrap().unwrap();
        peer_storage.find_by_public_key(&peer2.public_key).unwrap().unwrap();
        peer_storage.find_by_public_key(&peer3.public_key).unwrap().unwrap();

        // Test delete of border case peer
        assert!(peer_storage.delete_peer(&peer3.node_id).is_ok());

        // It is a logical delete, so there should still be 3 peers in the db
        assert_eq!(peer_storage.peer_db.len().unwrap(), 3);

        assert_eq!(
            peer_storage
                .find_by_public_key(&peer1.public_key)
                .unwrap()
                .unwrap()
                .public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage
                .find_by_public_key(&peer2.public_key)
                .unwrap()
                .unwrap()
                .public_key,
            peer2.public_key
        );
        assert!(peer_storage
            .find_by_public_key(&peer3.public_key)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some());

        assert_eq!(
            peer_storage.find_by_node_id(&peer1.node_id).unwrap().unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_by_node_id(&peer2.node_id).unwrap().unwrap().node_id,
            peer2.node_id
        );
        assert!(peer_storage
            .find_by_node_id(&peer3.node_id)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some());
    }

    fn create_test_peer(features: PeerFeatures, ban: bool) -> Peer {
        let mut rng = rand::rngs::OsRng;

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);

        let mut net_addresses = MultiaddressesWithStats::from_addresses_with_source(vec![], &PeerAddressSource::Config);

        // Create 1 to 4 random addresses
        for _i in 1..=rand::thread_rng().gen_range(1..4) {
            let n = [
                rand::thread_rng().gen_range(1..9),
                rand::thread_rng().gen_range(1..9),
                rand::thread_rng().gen_range(1..9),
                rand::thread_rng().gen_range(1..9),
            ];
            let net_address = format!("/ip4/{}.{}.{}.{}/tcp/{0}{1}{2}{3}", n[0], n[1], n[2], n[3],)
                .parse::<Multiaddr>()
                .unwrap();
            net_addresses.add_address(&net_address, &PeerAddressSource::Config);
        }

        let mut peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            features,
            Default::default(),
            Default::default(),
        );
        if ban {
            peer.ban_for(Duration::from_secs(600), "".to_string());
        }
        peer
    }

    #[test]
    fn test_in_network_region() {
        let mut peer_storage = PeerStorage::new_indexed(HashmapDatabase::new()).unwrap();

        let mut nodes = repeat_with(|| create_test_peer(PeerFeatures::COMMUNICATION_NODE, false))
            .take(5)
            .chain(repeat_with(|| create_test_peer(PeerFeatures::COMMUNICATION_CLIENT, false)).take(4))
            .collect::<Vec<_>>();

        for p in &nodes {
            peer_storage.add_peer(p.clone()).unwrap();
        }

        let main_peer_node_id = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false).node_id;

        nodes.sort_by(|a, b| {
            a.node_id
                .distance(&main_peer_node_id)
                .cmp(&b.node_id.distance(&main_peer_node_id))
        });

        let close_node = &nodes.first().unwrap().node_id;
        let far_node = &nodes.last().unwrap().node_id;

        let is_in_region = peer_storage
            .in_network_region(&main_peer_node_id, &main_peer_node_id, 1)
            .unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage
            .in_network_region(close_node, &main_peer_node_id, 1)
            .unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage.in_network_region(far_node, &main_peer_node_id, 9).unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage.in_network_region(far_node, &main_peer_node_id, 3).unwrap();
        assert!(!is_in_region);
    }

    #[test]
    fn discovery_syncing_returns_correct_peers() {
        let mut peer_storage = PeerStorage::new_indexed(HashmapDatabase::new()).unwrap();
        #[allow(clippy::cast_possible_wrap)] // Won't wrap around, numbers are static
        let a_week_ago = Utc::now().timestamp() - (PEER_ACTIVE_WITHIN_DURATION + 60) as i64; // A week ago + a minute

        let never_seen_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let banned_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, true);

        let mut not_active_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let address = not_active_peer.addresses.best().unwrap();
        let mut address = MultiaddrWithStats::new(address.address().clone(), PeerAddressSource::Config);
        address.mark_last_attempted(NaiveDateTime::from_timestamp_opt(a_week_ago, 0).unwrap());
        not_active_peer
            .addresses
            .merge(&MultiaddressesWithStats::from(vec![address]));

        let mut good_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let good_addresses = good_peer.addresses.borrow_mut();
        let good_address = good_addresses.addresses()[0].address().clone();
        good_addresses.mark_last_seen_now(&good_address);

        assert!(peer_storage.add_peer(never_seen_peer).is_ok());
        assert!(peer_storage.add_peer(not_active_peer).is_ok());
        assert!(peer_storage.add_peer(banned_peer).is_ok());
        assert!(peer_storage.add_peer(good_peer).is_ok());

        assert_eq!(peer_storage.all().unwrap().len(), 4);
        assert_eq!(
            peer_storage
                .discovery_syncing(100, &[], Some(PeerFeatures::COMMUNICATION_NODE))
                .unwrap()
                .len(),
            1
        );
    }
}

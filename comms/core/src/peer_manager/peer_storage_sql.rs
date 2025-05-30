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

use std::{cmp::min, time::Duration};

use log::*;
use multiaddr::Multiaddr;

use crate::{
    net_address::PeerAddressSource,
    peer_manager::{
        database::{PeerDatabaseSql, ThisPeerIdentity},
        peer::Peer,
        peer_id::PeerId,
        NodeDistance,
        NodeId,
        PeerFeatures,
        PeerManagerError,
    },
    types::{CommsDatabase, CommsPublicKey},
};

const LOG_TARGET: &str = "comms::peer_manager::peer_storage_sql";
// The maximum number of peers to return in peer manager
const PEER_MANAGER_SYNC_PEERS: usize = 100;
// The maximum amount of time a peer can be inactive before being considered stale:
// ((5 days, 24h, 60m, 60s)/2 = 2.5 days)
pub const STALE_PEER_THRESHOLD_DURATION: Duration = Duration::from_secs(5 * 24 * 60 * 60 / 2);
// Wallet peer connections are not verified in the way node peer connections are, thus a stale wallet connection may be
// totally valid, just not verified. Any stale wallet peers that are not neighbours will be deleted.
const MAX_NEIGHBOUR_WALLET_PEER_COUNT: usize = 25;

/// PeerStorageSql provides a mechanism to keep a datastore and a local copy of all peers in sync and allow fast
/// searches using the node_id, public key or net_address of a peer.
#[derive(Clone)]
pub struct PeerStorageSql {
    peer_db: PeerDatabaseSql,
}

impl PeerStorageSql {
    /// Constructs a new PeerStorageSql, with indexes populated from the given datastore
    pub fn new_indexed(database: PeerDatabaseSql) -> Result<PeerStorageSql, PeerManagerError> {
        trace!(
            target: LOG_TARGET,
            "Peer storage is initialized. {} total entries.",
            database.size(),
        );

        Ok(PeerStorageSql { peer_db: database })
    }

    /// Get this peer's identity
    pub fn this_peer_identity(&self) -> ThisPeerIdentity {
        self.peer_db.this_peer_identity()
    }

    /// Get the size of the database
    pub fn count(&self) -> usize {
        self.peer_db.size()
    }

    /// Adds or updates a peer and sets the last connection as successful.
    /// If the peer is marked as offline, it will be unmarked.
    pub fn add_or_update_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        Ok(self.peer_db.add_or_update_peer(peer)?)
    }

    /// Adds a peer an online peer if the peer does not already exist. When a peer already
    /// exists, the stored version will be replaced with the newly provided peer.
    pub fn add_or_update_online_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: &NodeId,
        addresses: &[Multiaddr],
        peer_features: &PeerFeatures,
        source: &PeerAddressSource,
    ) -> Result<Peer, PeerManagerError> {
        Ok(self
            .peer_db
            .add_or_update_online_peer(pubkey, node_id, addresses, peer_features, source)?)
    }

    /// The peer with the specified node id will be soft deleted (marked as deleted)
    pub fn soft_delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_db.soft_delete_peer(node_id)?;
        Ok(())
    }

    /// Find the peer with the provided NodeID
    pub fn get_peer_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_peer_by_node_id(node_id)?)
    }

    /// Get all peers based on a list of their node_ids
    pub fn get_peers_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_peers_by_node_ids(node_ids)?)
    }

    /// Get all peers based on a list of their node_ids
    pub fn get_peer_public_keys_by_node_ids(
        &self,
        node_ids: &[NodeId],
    ) -> Result<Vec<CommsPublicKey>, PeerManagerError> {
        Ok(self.peer_db.get_peer_public_keys_by_node_ids(node_ids)?)
    }

    /// Get all banned peers
    pub fn get_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_banned_peers()?)
    }

    pub fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.find_all_peers_match_partial_key(partial)?)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_peer_by_public_key(public_key)?)
    }

    /// Check if a peer exist using the specified public_key
    pub fn exists_public_key(&self, public_key: &CommsPublicKey) -> Result<bool, PeerManagerError> {
        if let Ok(val) = self.peer_db.peer_exists_by_public_key(public_key) {
            Ok(val.is_some())
        } else {
            Ok(false)
        }
    }

    /// Check if a peer exist using the specified node_id
    pub fn exists_node_id(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        if let Ok(val) = self.peer_db.peer_exists_by_node_id(node_id) {
            Ok(val.is_some())
        } else {
            Ok(false)
        }
    }

    /// Return the peer by corresponding to the provided NodeId if it is not banned
    pub fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Peer, PeerManagerError> {
        let peer = self
            .get_peer_by_node_id(node_id)?
            .ok_or(PeerManagerError::peer_not_found(node_id))?;

        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer)
        }
    }

    /// Return the peer by corresponding to the provided public key if it is not banned
    pub fn direct_identity_public_key(&self, public_key: &CommsPublicKey) -> Result<Peer, PeerManagerError> {
        let peer = self
            .find_by_public_key(public_key)?
            .ok_or(PeerManagerError::peer_not_found(&NodeId::from_public_key(public_key)))?;

        if peer.is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(peer)
        }
    }

    /// Return all peers, optionally filtering on supplied feature
    pub fn all(&self, features: Option<PeerFeatures>) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_all_peers(features)?)
    }

    /// Return "good" peers for syncing
    /// Criteria:
    ///  - Peer is not banned
    ///  - Peer has been seen within a defined time span (within the threshold)
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
            n = PEER_MANAGER_SYNC_PEERS;
        } else {
            n = min(n, PEER_MANAGER_SYNC_PEERS);
        }

        Ok(self
            .peer_db
            .get_n_random_active_peers(n, excluded_peers, features, Some(STALE_PEER_THRESHOLD_DURATION))?)
    }

    /// Compile a list of all known peers
    pub fn get_not_banned_or_deleted_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self
            .peer_db
            .get_n_not_banned_or_deleted_peers(PEER_MANAGER_SYNC_PEERS)?)
    }

    /// Compile a list of closest `n` active peers
    pub fn closest_n_active_peers(
        &self,
        region_node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        exclusion_distance: Option<NodeDistance>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_closest_n_active_peers(
            region_node_id,
            n,
            excluded_peers,
            features,
            stale_peer_threshold,
            exclude_if_all_address_failed,
            exclusion_distance,
        )?)
    }

    pub fn get_seed_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_seed_peers()?)
    }

    /// Delete all stale peers, removing them from the database and returning their node_ids
    pub fn hard_delete_all_stale_peers(&self) -> Result<Vec<NodeId>, PeerManagerError> {
        Ok(self
            .peer_db
            .hard_delete_all_stale_peers(STALE_PEER_THRESHOLD_DURATION, MAX_NEIGHBOUR_WALLET_PEER_COUNT)?)
    }

    /// Compile a random list of communication node peers of size _n_ that are not banned or offline
    pub fn random_peers(&self, n: usize, exclude_peers: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_n_random_peers(n, exclude_peers)?)
    }

    /// Get the closest `n` not failed, banned or deleted peers, ordered by their distance to the given node ID.
    pub fn get_closest_n_good_standing_peers(
        &self,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        Ok(self.peer_db.get_closest_n_good_standing_peers(n, features)?)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id. If there are less than N known peers, this will _always_ return true
    pub fn in_network_region(&self, node_id: &NodeId, n: usize) -> Result<bool, PeerManagerError> {
        let region_node_id = self.this_peer_identity().node_id;
        let region_node_distance = region_node_id.distance(node_id);
        let node_threshold = self.calc_region_threshold(n, PeerFeatures::COMMUNICATION_NODE)?;
        // Is node ID in the base node threshold?
        if region_node_distance <= node_threshold {
            return Ok(true);
        }
        let client_threshold = self.calc_region_threshold(n, PeerFeatures::COMMUNICATION_CLIENT)?; // Is node ID in the base client threshold?
        Ok(region_node_distance <= client_threshold)
    }

    /// Calculate the threshold for the region specified by region_node_id.
    pub fn calc_region_threshold(&self, n: usize, features: PeerFeatures) -> Result<NodeDistance, PeerManagerError> {
        let region_node_id = self.this_peer_identity().node_id;
        if n == 0 {
            return Ok(NodeDistance::max_distance());
        }

        let closest_peers = self.peer_db.get_closest_n_good_standing_peer_node_ids(n, features)?;
        let mut dists = Vec::new();
        for node_id in closest_peers {
            dists.push(region_node_id.distance(&node_id));
        }

        if dists.is_empty() {
            return Ok(NodeDistance::max_distance());
        }

        // If we have less than `n` matching peers in our threshold group, the threshold should be max
        if dists.len() < n {
            return Ok(NodeDistance::max_distance());
        }

        Ok(dists.pop().expect("dists cannot be empty at this point"))
    }

    /// Unban the peer
    pub fn unban_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let _node_id = self.peer_db.reset_banned(node_id)?;
        Ok(())
    }

    /// Unban the peer
    pub fn unban_all_peers(&self) -> Result<usize, PeerManagerError> {
        let number_unbanned = self.peer_db.reset_all_banned()?;
        Ok(number_unbanned)
    }

    pub fn reset_offline_non_wallet_peers(&self) -> Result<usize, PeerManagerError> {
        let number_offline = self.peer_db.reset_offline_non_wallet_peers()?;
        Ok(number_offline)
    }

    /// Ban the peer for the given duration
    pub fn ban_peer(
        &self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let node_id = NodeId::from_key(public_key);
        self.peer_db
            .set_banned(&node_id, duration, reason)?
            .ok_or(PeerManagerError::peer_not_found(&NodeId::from_public_key(public_key)))
    }

    /// Ban the peer for the given duration
    pub fn ban_peer_by_node_id(
        &self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        self.peer_db
            .set_banned(node_id, duration, reason)?
            .ok_or(PeerManagerError::peer_not_found(node_id))
    }

    pub fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let peer = self
            .get_peer_by_node_id(node_id)?
            .ok_or(PeerManagerError::peer_not_found(node_id))?;
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
        Ok(self.peer_db.set_metadata(node_id, key, data)?)
    }
}

#[allow(clippy::from_over_into)]
impl Into<CommsDatabase> for PeerStorageSql {
    fn into(self) -> CommsDatabase {
        self.peer_db
    }
}

#[cfg(test)]
mod test {
    use std::{borrow::BorrowMut, iter::repeat_with};

    use chrono::{DateTime, Utc};
    use multiaddr::Multiaddr;
    use rand::Rng;
    use tari_common_sqlite::connection::DbConnection;

    use super::*;
    use crate::{
        net_address::{MultiaddrWithStats, MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{database::MIGRATIONS, peer::PeerFlags},
    };

    fn get_peer_db_sql_test_db() -> Result<PeerDatabaseSql, PeerManagerError> {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        Ok(PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(PeerFeatures::COMMUNICATION_NODE, false),
        )?)
    }

    fn get_peer_storage_sql_test_db() -> Result<PeerStorageSql, PeerManagerError> {
        PeerStorageSql::new_indexed(get_peer_db_sql_test_db()?)
    }

    #[test]
    fn test_restore() {
        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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

        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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

        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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
        let mut db = Some(get_peer_db_sql_test_db().unwrap());
        {
            let peer_storage = db.take().unwrap();

            // Test adding and searching for peers
            assert!(peer_storage.add_or_update_peer(peer1.clone()).is_ok());
            assert!(peer_storage.add_or_update_peer(peer2.clone()).is_ok());
            assert!(peer_storage.add_or_update_peer(peer3.clone()).is_ok());

            assert_eq!(peer_storage.size(), 3);
            assert!(peer_storage.get_peer_by_public_key(&peer1.public_key).is_ok());
            assert!(peer_storage.get_peer_by_public_key(&peer2.public_key).is_ok());
            assert!(peer_storage.get_peer_by_public_key(&peer3.public_key).is_ok());
            db = Some(peer_storage);
        }
        // Restore from existing database
        let peer_storage = PeerStorageSql::new_indexed(db.take().unwrap()).unwrap();

        assert_eq!(peer_storage.peer_db.size(), 3);
        assert!(peer_storage.find_by_public_key(&peer1.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer2.public_key).is_ok());
        assert!(peer_storage.find_by_public_key(&peer3.public_key).is_ok());
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_add_delete_find_peer() {
        let peer_storage = get_peer_storage_sql_test_db().unwrap();

        // Create Peers
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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

        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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

        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
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
        peer_storage.add_or_update_peer(peer1.clone()).unwrap(); // assert!(peer_storage.add_or_update_peer(peer1.clone()).is_ok());
        assert!(peer_storage.add_or_update_peer(peer2.clone()).is_ok());
        assert!(peer_storage.add_or_update_peer(peer3.clone()).is_ok());

        assert_eq!(peer_storage.peer_db.size(), 3);

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
            peer_storage
                .get_peer_by_node_id(&peer1.node_id)
                .unwrap()
                .unwrap()
                .node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage
                .get_peer_by_node_id(&peer2.node_id)
                .unwrap()
                .unwrap()
                .node_id,
            peer2.node_id
        );
        assert_eq!(
            peer_storage
                .get_peer_by_node_id(&peer3.node_id)
                .unwrap()
                .unwrap()
                .node_id,
            peer3.node_id
        );

        peer_storage.find_by_public_key(&peer1.public_key).unwrap().unwrap();
        peer_storage.find_by_public_key(&peer2.public_key).unwrap().unwrap();
        peer_storage.find_by_public_key(&peer3.public_key).unwrap().unwrap();

        // Test delete of border case peer
        assert!(peer_storage.soft_delete_peer(&peer3.node_id).is_ok());

        // It is a logical delete, so there should still be 3 peers in the db
        assert_eq!(peer_storage.peer_db.size(), 3);

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
            peer_storage
                .get_peer_by_node_id(&peer1.node_id)
                .unwrap()
                .unwrap()
                .node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage
                .get_peer_by_node_id(&peer2.node_id)
                .unwrap()
                .unwrap()
                .node_id,
            peer2.node_id
        );
        assert!(peer_storage
            .get_peer_by_node_id(&peer3.node_id)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some());
    }

    fn create_test_peer(features: PeerFeatures, ban: bool) -> Peer {
        let mut rng = rand::rngs::OsRng;

        let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);

        let mut net_addresses = MultiaddressesWithStats::from_addresses_with_source(vec![], &PeerAddressSource::Config);

        // Create 1 to 4 random addresses
        for _i in 1..=rand::thread_rng().gen_range(1..4) {
            let n = [
                rand::thread_rng().gen_range(1..255),
                rand::thread_rng().gen_range(1..255),
                rand::thread_rng().gen_range(1..255),
                rand::thread_rng().gen_range(1..255),
                rand::thread_rng().gen_range(5000..9000),
            ];
            let net_address = format!("/ip4/{}.{}.{}.{}/tcp/{}", n[0], n[1], n[2], n[3], n[4])
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
        let peer_storage = get_peer_storage_sql_test_db().unwrap();

        let mut nodes = repeat_with(|| create_test_peer(PeerFeatures::COMMUNICATION_NODE, false))
            .take(5)
            .chain(repeat_with(|| create_test_peer(PeerFeatures::COMMUNICATION_CLIENT, false)).take(4))
            .collect::<Vec<_>>();

        for p in &nodes {
            peer_storage.add_or_update_peer(p.clone()).unwrap();
        }

        let main_peer_node_id = peer_storage.this_peer_identity().node_id;

        nodes.sort_by(|a, b| {
            a.node_id
                .distance(&main_peer_node_id)
                .cmp(&b.node_id.distance(&main_peer_node_id))
        });

        let db_nodes = peer_storage.peer_db.get_all_peers(None).unwrap();
        assert_eq!(db_nodes.len(), 9);

        let close_node = &nodes.first().unwrap().node_id;
        let far_node = &nodes.last().unwrap().node_id;

        let is_in_region = peer_storage.in_network_region(&main_peer_node_id, 1).unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage.in_network_region(close_node, 1).unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage.in_network_region(far_node, 9).unwrap();
        assert!(is_in_region);

        let is_in_region = peer_storage.in_network_region(far_node, 3).unwrap();
        assert!(!is_in_region);
    }

    #[test]
    fn get_just_seeds() {
        let peer_storage = get_peer_storage_sql_test_db().unwrap();

        let seeds = repeat_with(|| {
            let mut peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
            peer.add_flags(PeerFlags::SEED);
            peer
        })
        .take(5)
        .collect::<Vec<_>>();

        for p in &seeds {
            peer_storage.add_or_update_peer(p.clone()).unwrap();
        }

        let nodes = repeat_with(|| create_test_peer(PeerFeatures::COMMUNICATION_NODE, false))
            .take(5)
            .collect::<Vec<_>>();

        for p in &nodes {
            peer_storage.add_or_update_peer(p.clone()).unwrap();
        }
        let retrieved_seeds = peer_storage.get_seed_peers().unwrap();
        assert_eq!(retrieved_seeds.len(), seeds.len());
        for seed in seeds {
            assert!(retrieved_seeds.iter().any(|p| p.node_id == seed.node_id));
        }
    }

    #[test]
    fn discovery_syncing_returns_correct_peers() {
        let peer_storage = get_peer_storage_sql_test_db().unwrap();

        // Threshold duration + a minute
        #[allow(clippy::cast_possible_wrap)] // Won't wrap around, numbers are static
        let above_the_threshold = Utc::now().timestamp() - (STALE_PEER_THRESHOLD_DURATION.as_secs() + 60) as i64;

        let never_seen_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let banned_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, true);

        let mut not_active_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let address = not_active_peer.addresses.best().unwrap();
        let mut address = MultiaddrWithStats::new(address.address().clone(), PeerAddressSource::Config);
        address.mark_last_attempted(DateTime::from_timestamp(above_the_threshold, 0).unwrap().naive_utc());
        not_active_peer
            .addresses
            .merge(&MultiaddressesWithStats::from(vec![address]));

        let mut good_peer = create_test_peer(PeerFeatures::COMMUNICATION_NODE, false);
        let good_addresses = good_peer.addresses.borrow_mut();
        let good_address = good_addresses.addresses()[0].address().clone();
        good_addresses.mark_last_seen_now(&good_address);

        assert!(peer_storage.add_or_update_peer(never_seen_peer).is_ok());
        assert!(peer_storage.add_or_update_peer(not_active_peer).is_ok());
        assert!(peer_storage.add_or_update_peer(banned_peer).is_ok());
        assert!(peer_storage.add_or_update_peer(good_peer).is_ok());

        assert_eq!(peer_storage.all(None).unwrap().len(), 4);
        assert_eq!(
            peer_storage
                .discovery_syncing(100, &[], Some(PeerFeatures::COMMUNICATION_NODE))
                .unwrap()
                .len(),
            1
        );
    }
}

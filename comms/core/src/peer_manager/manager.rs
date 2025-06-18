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

use std::{fmt, time::Duration};

use multiaddr::Multiaddr;

#[cfg(feature = "metrics")]
use crate::peer_manager::metrics;
use crate::{
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        peer::Peer,
        peer_id::PeerId,
        peer_storage_sql::PeerStorageSql,
        NodeDistance,
        NodeId,
        PeerFeatures,
        PeerManagerError,
        ThisPeerIdentity,
    },
    types::{CommsDatabase, CommsPublicKey},
};

/// The PeerManager provides functionality to add, find and delete peers. It wraps synchronous
/// WAL-enabled SQLite database access and provides an async interface to the rest of the code base.
#[derive(Clone)]
pub struct PeerManager {
    // yo dawg, I heard you like wrappers, so I wrapped your wrapper in a wrapper so you can wrap while you wrap
    peer_storage_sql: PeerStorageSql,
}

impl PeerManager {
    /// Constructs a new empty PeerManager
    pub fn new(database: CommsDatabase) -> Result<PeerManager, PeerManagerError> {
        let peer_storage_sql = PeerStorageSql::new_indexed(database)?;

        Ok(Self { peer_storage_sql })
    }

    /// Get this peer's identity
    pub fn this_peer_identity(&self) -> ThisPeerIdentity {
        self.peer_storage_sql.this_peer_identity()
    }

    /// Get the number of peers in the PeerManager - any error will translate to a size of zero
    pub async fn count(&self) -> usize {
        self.peer_storage_sql.count()
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub async fn add_or_update_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        let peer_id = self.peer_storage_sql.add_or_update_peer(peer)?;
        #[cfg(feature = "metrics")]
        {
            let count = self.count().await;
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(peer_id)
    }

    /// The peer with the specified node id will be soft deleted (marked as deleted)
    pub async fn soft_delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage_sql.soft_delete_peer(node_id)?;
        #[cfg(feature = "metrics")]
        {
            let count = self.count().await;
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(())
    }

    /// Get all peers based on a list of their node_ids
    pub async fn get_peers_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_peers_by_node_ids(node_ids)
    }

    /// Get all peers based on a list of their node_ids
    pub async fn get_peer_public_keys_by_node_ids(
        &self,
        node_ids: &[NodeId],
    ) -> Result<Vec<CommsPublicKey>, PeerManagerError> {
        self.peer_storage_sql.get_peer_public_keys_by_node_ids(node_ids)
    }

    /// Get all banned peers
    pub async fn get_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_banned_peers()
    }

    /// Find the peer with the provided NodeID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_peer_by_node_id(node_id)
    }

    /// gets all seed peers
    pub async fn get_seed_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_seed_peers()
    }

    /// Find the peer with the provided PublicKey
    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        self.peer_storage_sql.find_by_public_key(public_key)
    }

    /// Find the peer with the provided substring. This currently only compares the given bytes to the NodeId
    pub async fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.find_all_starts_with(partial)
    }

    /// Check if a peer exist using the specified public_key
    pub async fn exists(&self, public_key: &CommsPublicKey) -> Result<bool, PeerManagerError> {
        self.peer_storage_sql.exists_public_key(public_key)
    }

    /// Check if a peer exist using the specified node_id
    pub async fn exists_node_id(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        self.peer_storage_sql.exists_node_id(node_id)
    }

    /// Returns all peers
    pub async fn all(&self, features: Option<PeerFeatures>) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.all(features)
    }

    /// Get available dial candidates that are communication nodes, not banned, not deleted,
    /// and not in the excluded node IDs list
    pub async fn get_available_dial_candidates(
        &self,
        exclude_node_ids: &[NodeId],
        limit: Option<usize>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql
            .get_available_dial_candidates(exclude_node_ids, limit)
    }

    /// Return "good" peers for syncing
    /// Criteria:
    ///  - Peer is not banned
    ///  - Peer has been seen within a defined time span (1 week)
    ///  - Only returns a maximum number of syncable peers (corresponds with the max possible number of requestable
    ///    peers to sync)
    pub async fn discovery_syncing(
        &self,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.discovery_syncing(n, excluded_peers, features)
    }

    /// Adds or updates a peer and sets the last connection as successful.
    /// If the peer is marked as offline, it will be unmarked.
    pub async fn add_or_update_online_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: &NodeId,
        addresses: &[Multiaddr],
        peer_features: &PeerFeatures,
        source: &PeerAddressSource,
    ) -> Result<Peer, PeerManagerError> {
        self.peer_storage_sql
            .add_or_update_online_peer(pubkey, node_id, addresses, peer_features, source)
    }

    /// Get a peer matching the given node ID
    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        match self.peer_storage_sql.direct_identity_node_id(node_id) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFound(_)) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Get a peer matching the given public key
    pub async fn direct_identity_public_key(
        &self,
        public_key: &CommsPublicKey,
    ) -> Result<Option<Peer>, PeerManagerError> {
        match self.peer_storage_sql.direct_identity_public_key(public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFound(_)) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Fetch all peers (except banned ones)
    pub async fn get_not_banned_or_deleted_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_not_banned_or_deleted_peers()
    }

    /// Fetch n nearest active neighbours. If features are supplied, the function will return the closest peers matching
    /// that feature
    pub async fn closest_n_active_peers(
        &self,
        region_node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        exclusion_distance: Option<NodeDistance>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.closest_n_active_peers(
            region_node_id,
            n,
            excluded_peers,
            features,
            stale_peer_threshold,
            exclude_if_all_address_failed,
            exclusion_distance,
        )
    }

    /// Get the closest `n` not failed, banned or deleted peers, ordered by their distance to the given node ID.
    pub async fn closest_n_good_standing_peers(
        &self,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_closest_n_good_standing_peers(n, features)
    }

    /// Fetch n random peers that are Communication Nodes
    pub async fn random_peers(&self, n: usize, excluded: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.random_peers(n, excluded)
    }

    /// Calculate the region threshold for a given number of peers and features
    pub async fn calc_region_threshold(
        &self,
        n: usize,
        features: PeerFeatures,
    ) -> Result<NodeDistance, PeerManagerError> {
        self.peer_storage_sql.calc_region_threshold(n, features)
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage_sql.unban_peer(node_id)
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_all_peers(&self) -> Result<usize, PeerManagerError> {
        self.peer_storage_sql.unban_all_peers()
    }

    pub async fn reset_offline_non_wallet_peers(&self) -> Result<usize, PeerManagerError> {
        self.peer_storage_sql.reset_offline_non_wallet_peers()
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer(
        &self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        self.peer_storage_sql.ban_peer(public_key, duration, reason)
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer_by_node_id(
        &self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        self.peer_storage_sql.ban_peer_by_node_id(node_id, duration, reason)
    }

    /// Get the ban status of a peer
    pub async fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        self.peer_storage_sql.is_peer_banned(node_id)
    }

    /// Get the peer's features
    pub async fn get_peer_features(&self, node_id: &NodeId) -> Result<PeerFeatures, PeerManagerError> {
        let peer = self
            .find_by_node_id(node_id)
            .await?
            .ok_or(PeerManagerError::peer_not_found(node_id))?;
        Ok(peer.features)
    }

    /// Get a peer's multiaddresses
    pub async fn get_peer_multi_addresses(
        &self,
        node_id: &NodeId,
    ) -> Result<MultiaddressesWithStats, PeerManagerError> {
        let peer = self
            .find_by_node_id(node_id)
            .await?
            .ok_or(PeerManagerError::peer_not_found(node_id))?;
        Ok(peer.addresses)
    }

    /// Get multiple peers' multiaddresses
    pub async fn get_peers_multi_addresses(
        &self,
        node_ids: &[NodeId],
    ) -> Result<Vec<(NodeId, MultiaddressesWithStats)>, PeerManagerError> {
        if node_ids.is_empty() {
            return Err(PeerManagerError::ProcessError(
                "NodeId list cannot be empty".to_string(),
            ));
        }
        let peers = self.get_peers_by_node_ids(node_ids).await?;
        if peers.is_empty() {
            return Err(PeerManagerError::peers_not_found(node_ids));
        }
        let results = peers.into_iter().map(|p| (p.node_id, p.addresses)).collect::<Vec<_>>();
        Ok(results)
    }

    /// This will store metadata inside of the metadata field in the peer provided by the nodeID.
    /// It will return None if the value was empty and the old value if the value was updated
    pub async fn set_peer_metadata(
        &self,
        node_id: &NodeId,
        key: u8,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, PeerManagerError> {
        self.peer_storage_sql.set_peer_metadata(node_id, key, data)
    }
}

impl fmt::Debug for PeerManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PeerManager { peer_storage: ... }")
    }
}

#[cfg(test)]
pub fn create_test_peer(ban_flag: bool, features: PeerFeatures) -> Peer {
    use std::borrow::BorrowMut;

    use rand::{rngs::OsRng, Rng};

    use crate::peer_manager::PeerFlags;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut OsRng);
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
    if ban_flag {
        peer.ban_for(Duration::from_secs(1000), "".to_string());
    }

    let good_addresses = peer.addresses.borrow_mut();
    let good_address = good_addresses.addresses()[0].address().clone();
    good_addresses.mark_last_seen_now(&good_address);

    peer
}

#[cfg(test)]
mod test {
    use tari_common_sqlite::connection::DbConnection;

    use super::*;
    use crate::peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        STALE_PEER_THRESHOLD_DURATION,
    };

    fn create_peer_manager() -> PeerManager {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        PeerManager::new(peers_db).unwrap()
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = create_peer_manager();
        let mut test_peers = vec![create_test_peer(true, PeerFeatures::COMMUNICATION_NODE)];
        // Create 20 peers were the 1st and last one is bad
        assert!(peer_manager
            .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
            .await
            .is_ok());
        for _i in 0..18 {
            test_peers.push(create_test_peer(false, PeerFeatures::COMMUNICATION_NODE));
            assert!(peer_manager
                .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok());
        }
        test_peers.push(create_test_peer(true, PeerFeatures::COMMUNICATION_NODE));
        assert!(peer_manager
            .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
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
        let unmanaged_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        assert!(peer_manager
            .direct_identity_node_id(&unmanaged_peer.node_id)
            .await
            .unwrap()
            .is_none());

        // Test Flood
        let selected_peers = peer_manager.get_not_banned_or_deleted_peers().await.unwrap();
        assert_eq!(selected_peers.len(), 18);
        for peer_identity in &selected_peers {
            assert!(!peer_manager
                .find_by_node_id(&peer_identity.node_id)
                .await
                .unwrap()
                .unwrap()
                .is_banned(),);
        }

        // Test Closest - No exclusions
        let selected_peers = peer_manager
            .closest_n_active_peers(
                &unmanaged_peer.node_id,
                3,
                &[],
                None,
                Some(STALE_PEER_THRESHOLD_DURATION),
                true,
                None,
            )
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
                assert!(unused_dist >= selected_dist);
            }
        }

        // Test Closest - With an exclusion
        let excluded_peers = vec![selected_peers[0].node_id.clone()];
        let selected_peers = peer_manager
            .closest_n_active_peers(
                &unmanaged_peer.node_id,
                3,
                &excluded_peers,
                None,
                Some(STALE_PEER_THRESHOLD_DURATION),
                true,
                None,
            )
            .await
            .unwrap();
        assert_eq!(selected_peers.len(), 3);
        // Remove current identity nodes from test peers
        let mut unused_peers: Vec<Peer> = Vec::new();
        for peer in &test_peers {
            let unused = !selected_peers.iter().any(|peer_identity| {
                peer.node_id == peer_identity.node_id || peer.is_banned() || excluded_peers.contains(&peer.node_id)
            });
            if unused {
                unused_peers.push(peer.clone());
            }
        }

        // Check that none of the remaining unused peers have smaller distances compared to the selected peers
        for peer_identity in &selected_peers {
            let selected_dist = unmanaged_peer.node_id.distance(&peer_identity.node_id);
            for unused_peer in &unused_peers {
                let unused_dist = unmanaged_peer.node_id.distance(&unused_peer.node_id);
                assert!(unused_dist >= selected_dist);
            }
            assert!(!excluded_peers.contains(&peer_identity.node_id));
        }

        // Test Random
        let identities1 = peer_manager.random_peers(10, &[]).await.unwrap();
        let identities2 = peer_manager.random_peers(10, &[]).await.unwrap();
        assert_ne!(identities1, identities2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_calc_region_threshold() {
        let n = 5;
        // Create peer manager with random peers
        let peer_manager = create_peer_manager();
        let network_region_node_id = peer_manager.peer_storage_sql.this_peer_identity().node_id;
        let mut test_peers = (0..10)
            .map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_NODE))
            .chain((0..10).map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT)))
            .collect::<Vec<_>>();

        for p in &test_peers {
            peer_manager.add_or_update_peer(p.clone()).await.unwrap();
        }

        test_peers.sort_by(|a, b| {
            let a_dist = network_region_node_id.distance(&a.node_id);
            let b_dist = network_region_node_id.distance(&b.node_id);
            a_dist.partial_cmp(&b_dist).unwrap()
        });

        let node_region_threshold = peer_manager
            .calc_region_threshold(n, PeerFeatures::COMMUNICATION_NODE)
            .await
            .unwrap();

        // First 5 base nodes should be within the region
        for peer in test_peers
            .iter()
            .filter(|p| p.features == PeerFeatures::COMMUNICATION_NODE)
            .take(n)
        {
            assert!(peer.node_id.distance(&network_region_node_id) <= node_region_threshold);
        }

        // Next 5 should not be in the region
        for peer in test_peers
            .iter()
            .filter(|p| p.features == PeerFeatures::COMMUNICATION_NODE)
            .skip(n)
        {
            assert!(peer.node_id.distance(&network_region_node_id) >= node_region_threshold);
        }

        let node_region_threshold = peer_manager
            .calc_region_threshold(n, PeerFeatures::COMMUNICATION_CLIENT)
            .await
            .unwrap();

        // First 5 clients should be in region
        for peer in test_peers
            .iter()
            .filter(|p| p.features == PeerFeatures::COMMUNICATION_CLIENT)
            .take(5)
        {
            assert!(peer.node_id.distance(&network_region_node_id) <= node_region_threshold);
        }

        // Next 5 should not be in the region
        for peer in test_peers
            .iter()
            .filter(|p| p.features == PeerFeatures::COMMUNICATION_CLIENT)
            .skip(5)
        {
            assert!(peer.node_id.distance(&network_region_node_id) >= node_region_threshold);
        }
    }

    #[tokio::test]
    async fn test_closest_peers() {
        let n = 5;
        // Create peer manager with random peers
        let peer_manager = create_peer_manager();
        let network_region_node_id = peer_manager.this_peer_identity().node_id;
        let test_peers = (0..10)
            .map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_NODE))
            .chain((0..10).map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT)))
            .collect::<Vec<_>>();

        for p in &test_peers {
            peer_manager.add_or_update_peer(p.clone()).await.unwrap();
        }

        for features in &[PeerFeatures::COMMUNICATION_NODE, PeerFeatures::COMMUNICATION_CLIENT] {
            let node_threshold = peer_manager.calc_region_threshold(n, *features).await.unwrap();

            let closest = peer_manager.closest_n_good_standing_peers(n, *features).await.unwrap();

            assert!(closest
                .iter()
                .all(|p| network_region_node_id.distance(&p.node_id) <= node_threshold));
        }
    }

    #[tokio::test]
    async fn test_add_or_update_online_peer() {
        let peer_manager = create_peer_manager();
        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);

        peer_manager.add_or_update_peer(peer.clone()).await.unwrap();

        let peer = peer_manager
            .add_or_update_online_peer(
                &peer.public_key,
                &peer.node_id,
                &[],
                &peer.features,
                &PeerAddressSource::Config,
            )
            .await
            .unwrap();

        assert!(!peer.is_offline());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_concurrent_add_or_update_and_get_closest_peers() {
        let peer_manager = create_peer_manager();
        let num_peers = 75;
        let num_write_tasks = 20;
        let num_read_tasks = 1500;
        let n = 100;

        // Spawn tasks to concurrently add peers and update their stats
        let add_tasks: Vec<_> = (0..num_write_tasks)
            .map(|_| {
                let peer_manager = peer_manager.clone();
                tokio::spawn(async move {
                    let mut peers_to_update_last_seen = Vec::new();
                    let mut peers_to_set_metadata = Vec::new();
                    for i in 0..num_peers {
                        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
                        if i % 7 == 0 {
                            peers_to_update_last_seen.push(peer.clone());
                        }
                        if i % 11 == 0 {
                            peers_to_set_metadata.push(peer.clone());
                        }
                        peers_to_update_last_seen.push(peer.clone());
                        peer_manager.add_or_update_peer(peer).await.unwrap();
                        tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    }
                    for peer in &mut peers_to_update_last_seen {
                        let addresses = peer.addresses.addresses().to_vec();
                        peer.addresses.mark_last_seen_now(addresses[0].address());
                        peer_manager.add_or_update_peer(peer.clone()).await.unwrap();
                        tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    }
                    for (key, peer) in peers_to_set_metadata.iter().enumerate() {
                        peer_manager
                            .set_peer_metadata(
                                &peer.node_id,
                                u8::try_from(key % usize::from(u8::MAX)).unwrap_or_default(),
                                vec![1, 2, 3],
                            )
                            .await
                            .unwrap();
                        tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    }
                    Ok::<_, PeerManagerError>(())
                    // println!("Added {} peers", num_peers);
                })
            })
            .collect();

        // Spawn tasks to concurrently fetch closest peers
        let get_tasks: Vec<_> = (0..num_read_tasks)
            .map(|_| {
                let peer_manager = peer_manager.clone();
                tokio::spawn(async move {
                    let region_node_id = peer_manager.this_peer_identity().node_id;
                    tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    let _closest_peers = peer_manager
                        .closest_n_active_peers(
                            &region_node_id,
                            n,
                            &[],
                            Some(PeerFeatures::COMMUNICATION_NODE),
                            None,
                            false,
                            None,
                        )
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    let _total_peers = peer_manager.count().await;
                    // println!("Total peers: {}, Closest peers: {}", _total_peers, _closest_peers.len());
                    Ok::<_, PeerManagerError>(())
                })
            })
            .collect();

        // Wait for all tasks to complete
        let all_tasks = add_tasks.into_iter().chain(get_tasks);

        for (i, task) in all_tasks.enumerate() {
            match task.await {
                Ok(Ok(_)) => { /* success */ },
                Ok(Err(e)) => panic!("Task {i} failed with PeerManagerError: {e:?}"),
                Err(e) => panic!("Task {i} panicked: {e:?}"),
            }
        }

        // Do one final read
        tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
        let region_node_id = peer_manager.this_peer_identity().node_id;
        let closest_peers = peer_manager
            .closest_n_active_peers(
                &region_node_id,
                n,
                &[],
                Some(PeerFeatures::COMMUNICATION_NODE),
                None,
                false,
                None,
            )
            .await
            .unwrap();
        let total_peers = peer_manager.count().await;
        // println!("Total peers: {}, Closest peers: {}", total_peers, closest_peers.len());
        assert_eq!(total_peers, num_peers * num_write_tasks);
        assert_eq!(closest_peers.len(), n);
    }
}

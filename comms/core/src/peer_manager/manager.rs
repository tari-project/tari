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
        peer::{Peer, PeerFlags},
        peer_id::PeerId,
        peer_storage_sql::PeerStorageSql,
        NodeDistance,
        NodeId,
        PeerFeatures,
        PeerManagerError,
    },
    types::{CommsDatabase, CommsPublicKey},
};

/// The PeerManager consist of a routing table of previously discovered peers.
/// It also provides functionality to add, find and delete peers.
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

    /// Get the number of peers in the PeerManager - any error will translate to a size of zero
    pub async fn count(&self) -> usize {
        let peer_manager = self.clone();
        (tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.count()).await).unwrap_or_default()
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub async fn add_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        let peer_manager = self.clone();
        let peer_id = tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.add_peer(peer)).await??;
        #[cfg(feature = "metrics")]
        {
            let count = self.count().await;
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(peer_id)
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub async fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.delete_peer(&node_id)).await??;
        #[cfg(feature = "metrics")]
        {
            let count = self.count().await;
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(())
    }

    /// Delete all stale peers, removing them from the database and returning their node_ids
    pub async fn delete_all_stale_peers(&self, self_node_id: &NodeId) -> Result<Vec<NodeId>, PeerManagerError> {
        let peer_manager = self.clone();
        let self_node_id = self_node_id.clone();
        let deleted_peers =
            tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.delete_all_stale_peers(&self_node_id))
                .await??;
        Ok(deleted_peers)
    }

    /// Get all peers based on a list of their node_ids
    pub async fn get_peers_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let node_ids = node_ids.to_vec();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.get_peers_by_node_ids(&node_ids)).await?
    }

    /// Get all banned peers
    pub async fn get_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.get_banned_peers()).await?
    }

    /// Find the peer with the provided NodeID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.find_by_node_id(&node_id)).await?
    }

    /// gets all seed peers
    pub async fn get_seed_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.get_seed_peers()).await?
    }

    /// Find the peer with the provided PublicKey
    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let public_key = public_key.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.find_by_public_key(&public_key)).await?
    }

    /// Find the peer with the provided substring. This currently only compares the given bytes to the NodeId
    pub async fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let partial = partial.to_vec();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.find_all_starts_with(&partial)).await?
    }

    /// Check if a peer exist using the specified public_key
    pub async fn exists(&self, public_key: &CommsPublicKey) -> Result<bool, PeerManagerError> {
        let peer_manager = self.clone();
        let public_key = public_key.clone();
        Ok(tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.exists_public_key(&public_key)).await?)
    }

    /// Check if a peer exist using the specified node_id
    pub async fn exists_node_id(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        Ok(tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.exists_node_id(&node_id)).await?)
    }

    /// Returns all peers
    pub async fn all(&self, features: Option<PeerFeatures>) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.all(features)).await?
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
        let peer_manager = self.clone();
        let excluded_peers = excluded_peers.to_vec();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .discovery_syncing(n, &excluded_peers, features)
        })
        .await?
    }

    /// Adds or updates a peer and sets the last connection as successful.
    /// If the peer is marked as offline, it will be unmarked.
    pub async fn add_or_update_online_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: NodeId,
        addresses: Vec<Multiaddr>,
        peer_features: PeerFeatures,
        source: &PeerAddressSource,
    ) -> Result<Peer, PeerManagerError> {
        match self.find_by_public_key(pubkey).await {
            Ok(Some(mut peer)) => {
                peer.addresses.update_addresses(&addresses, source);
                peer.features = peer_features;
                self.add_peer(peer.clone()).await?;
                Ok(peer)
            },
            Ok(None) => {
                self.add_peer(Peer::new(
                    pubkey.clone(),
                    node_id,
                    MultiaddressesWithStats::from_addresses_with_source(addresses, source),
                    PeerFlags::default(),
                    peer_features,
                    Default::default(),
                    Default::default(),
                ))
                .await?;

                self.find_by_public_key(pubkey)
                    .await?
                    .ok_or(PeerManagerError::PeerNotFoundError)
            },
            Err(err) => Err(err),
        }
    }

    pub async fn update_peer_address_latency_and_last_seen(
        &self,
        pubkey: &CommsPublicKey,
        address: &Multiaddr,
        latency: Option<Duration>,
    ) -> Result<(), PeerManagerError> {
        match self.find_by_public_key(pubkey).await {
            Ok(Some(mut peer)) => {
                if let Some(val) = latency {
                    peer.addresses.update_latency(address, val);
                }
                peer.addresses.mark_last_seen_now(address);
                self.add_peer(peer.clone()).await?;
                Ok(())
            },
            Ok(None) => Err(PeerManagerError::PeerNotFoundError),
            Err(err) => Err(err),
        }
    }

    /// Get a peer matching the given node ID
    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(
            move || match peer_manager.peer_storage_sql.direct_identity_node_id(&node_id) {
                Ok(peer) => Ok(Some(peer)),
                Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
                Err(err) => Err(err),
            },
        )
        .await?
    }

    /// Get a peer matching the given public key
    pub async fn direct_identity_public_key(
        &self,
        public_key: &CommsPublicKey,
    ) -> Result<Option<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let public_key = public_key.clone();
        tokio::task::spawn_blocking(move || {
            match peer_manager.peer_storage_sql.direct_identity_public_key(&public_key) {
                Ok(peer) => Ok(Some(peer)),
                Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
                Err(err) => Err(err),
            }
        })
        .await?
    }

    /// Fetch all peers (except banned ones)
    pub async fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.flood_peers()).await?
    }

    /// Fetch n nearest active neighbours. If features are supplied, the function will return the closest peers matching
    /// that feature
    pub async fn closest_n_active_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        exclusion_distance: Option<NodeDistance>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        let excluded_peers = excluded_peers.to_vec();
        let exclusion_distance = exclusion_distance.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager.peer_storage_sql.closest_n_active_peers(
                &node_id,
                n,
                &excluded_peers,
                features,
                stale_peer_threshold,
                exclude_if_all_address_failed,
                exclusion_distance,
            )
        })
        .await?
    }

    /// Get the closest `n` not failed, banned or deleted peer node ids, ordered by their distance to the given node ID.
    pub async fn closest_n_good_standing_peer_node_ids(
        &self,
        region_node_id: &NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<NodeId>, PeerManagerError> {
        let peer_manager = self.clone();
        let region_node_id = region_node_id.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .get_closest_n_good_standing_peer_node_ids(&region_node_id, n, features)
        })
        .await?
    }

    /// Get the closest `n` not failed, banned or deleted peers, ordered by their distance to the given node ID.
    pub async fn closest_n_good_standing_peers(
        &self,
        region_node_id: &NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let region_node_id = region_node_id.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .get_closest_n_good_standing_peers(&region_node_id, n, features)
        })
        .await?
    }

    /// Fetch n random peers that are Communication Nodes
    pub async fn random_peers(&self, n: usize, excluded: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        let peer_manager = self.clone();
        let excluded = excluded.to_vec();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.random_peers(n, &excluded)).await?
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id
    pub async fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError> {
        let peer_manager = self.clone();
        let region_node_id = region_node_id.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .in_network_region(&node_id, &region_node_id, n)
        })
        .await?
    }

    pub async fn calc_region_threshold(
        &self,
        region_node_id: &NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<NodeDistance, PeerManagerError> {
        let peer_manager = self.clone();
        let region_node_id = region_node_id.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .calc_region_threshold(&region_node_id, n, features)
        })
        .await?
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.unban_peer(&node_id)).await?
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_all_peers(&self) -> Result<usize, PeerManagerError> {
        let peer_manager = self.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.unban_all_peers()).await?
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer(
        &self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let peer_manager = self.clone();
        let public_key = public_key.clone();
        let reason = reason.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.ban_peer(&public_key, duration, reason))
            .await?
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer_by_node_id(
        &self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        let reason = reason.clone();
        tokio::task::spawn_blocking(move || {
            peer_manager
                .peer_storage_sql
                .ban_peer_by_node_id(&node_id, duration, reason)
        })
        .await?
    }

    pub async fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.is_peer_banned(&node_id)).await?
    }

    // TODO: This function is still hugely inefficient as it retrieve all peers and then perform an update operation -
    // TODO: it should be split into smaller targeted queries - investigate caller function use case for this.
    pub async fn update_each<F>(&self, f: &mut F, features: Option<PeerFeatures>) -> Result<usize, PeerManagerError>
    where F: FnMut(Peer) -> Option<Peer> {
        let mut peers_to_update = Vec::new();

        let all_peers = self.all(features).await?;
        for peer in all_peers {
            if let Some(updated_peer) = (f)(peer) {
                peers_to_update.push(updated_peer);
            }
        }

        let updated_count = peers_to_update.len();
        for peer in peers_to_update {
            self.add_peer(peer).await?;
        }

        Ok(updated_count)
    }

    pub async fn get_peer_features(&self, node_id: &NodeId) -> Result<PeerFeatures, PeerManagerError> {
        let peer = self
            .find_by_node_id(node_id)
            .await?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(peer.features)
    }

    pub async fn get_peer_multi_addresses(
        &self,
        node_id: &NodeId,
    ) -> Result<MultiaddressesWithStats, PeerManagerError> {
        let peer = self
            .find_by_node_id(node_id)
            .await?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(peer.addresses)
    }

    /// This will store metadata inside of the metadata field in the peer provided by the nodeID.
    /// It will return None if the value was empty and the old value if the value was updated
    pub async fn set_peer_metadata(
        &self,
        node_id: &NodeId,
        key: u8,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, PeerManagerError> {
        let peer_manager = self.clone();
        let node_id = node_id.clone();
        let data = data.clone();
        tokio::task::spawn_blocking(move || peer_manager.peer_storage_sql.set_peer_metadata(&node_id, key, data))
            .await?
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
    use std::iter;

    use rand::{distributions::Alphanumeric, Rng};
    use tari_common_sqlite::connection::DbConnection;

    use super::*;
    use crate::peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        STALE_PEER_THRESHOLD_DURATION,
    };

    fn random_name() -> String {
        let mut rng = rand::thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric) as char)
            .take(8)
            .collect::<String>()
    }

    fn create_peer_manager() -> PeerManager {
        let db_connection = DbConnection::connect_memory_and_migrate(random_name(), MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(db_connection);
        PeerManager::new(peers_db).unwrap()
    }

    #[tokio::test]
    async fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = create_peer_manager();
        let mut test_peers = vec![create_test_peer(true, PeerFeatures::COMMUNICATION_NODE)];
        // Create 20 peers were the 1st and last one is bad
        assert!(peer_manager
            .add_peer(test_peers[test_peers.len() - 1].clone())
            .await
            .is_ok());
        for _i in 0..18 {
            test_peers.push(create_test_peer(false, PeerFeatures::COMMUNICATION_NODE));
            assert!(peer_manager
                .add_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok());
        }
        test_peers.push(create_test_peer(true, PeerFeatures::COMMUNICATION_NODE));
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
        let unmanaged_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        assert!(peer_manager
            .direct_identity_node_id(&unmanaged_peer.node_id)
            .await
            .unwrap()
            .is_none());

        // Test Flood
        let selected_peers = peer_manager.flood_peers().await.unwrap();
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
        let network_region_node_id = create_test_peer(false, Default::default()).node_id;
        let mut test_peers = (0..10)
            .map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_NODE))
            .chain((0..10).map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT)))
            .collect::<Vec<_>>();

        for p in &test_peers {
            peer_manager.add_peer(p.clone()).await.unwrap();
        }

        test_peers.sort_by(|a, b| {
            let a_dist = network_region_node_id.distance(&a.node_id);
            let b_dist = network_region_node_id.distance(&b.node_id);
            a_dist.partial_cmp(&b_dist).unwrap()
        });

        let node_region_threshold = peer_manager
            .calc_region_threshold(&network_region_node_id, n, PeerFeatures::COMMUNICATION_NODE)
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
            .calc_region_threshold(&network_region_node_id, n, PeerFeatures::COMMUNICATION_CLIENT)
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
        let network_region_node_id = create_test_peer(false, Default::default()).node_id;
        let test_peers = (0..10)
            .map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_NODE))
            .chain((0..10).map(|_| create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT)))
            .collect::<Vec<_>>();

        for p in &test_peers {
            peer_manager.add_peer(p.clone()).await.unwrap();
        }

        for features in &[PeerFeatures::COMMUNICATION_NODE, PeerFeatures::COMMUNICATION_CLIENT] {
            let node_threshold = peer_manager
                .calc_region_threshold(&network_region_node_id, n, *features)
                .await
                .unwrap();

            let closest = peer_manager
                .closest_n_good_standing_peers(&network_region_node_id, n, *features)
                .await
                .unwrap();

            assert!(closest
                .iter()
                .all(|p| network_region_node_id.distance(&p.node_id) <= node_threshold));
        }
    }

    #[tokio::test]
    async fn test_add_or_update_online_peer() {
        let peer_manager = create_peer_manager();
        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);

        peer_manager.add_peer(peer.clone()).await.unwrap();

        let peer = peer_manager
            .add_or_update_online_peer(
                &peer.public_key,
                peer.node_id,
                vec![],
                peer.features,
                &PeerAddressSource::Config,
            )
            .await
            .unwrap();

        assert!(!peer.is_offline());
    }
}

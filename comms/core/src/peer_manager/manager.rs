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

use std::{fmt, fs::File, time::Duration};

use multiaddr::Multiaddr;
use tari_storage::{lmdb_store::LMDBDatabase, CachedStore, IterationResult};
use tokio::sync::RwLock;

#[cfg(feature = "metrics")]
use crate::peer_manager::metrics;
use crate::{
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        migrations,
        peer::{Peer, PeerFlags},
        peer_id::PeerId,
        peer_storage::PeerStorage,
        wrapper::KeyValueWrapper,
        NodeDistance,
        NodeId,
        PeerFeatures,
        PeerManagerError,
        PeerQuery,
    },
    types::{CommsDatabase, CommsPublicKey},
};

/// The PeerManager consist of a routing table of previously discovered peers.
/// It also provides functionality to add, find and delete peers.
pub struct PeerManager {
    // yo dawg, I heard you like wrappers, so I wrapped your wrapper in a wrapper so you can wrap while you wrap
    peer_storage: RwLock<PeerStorage<CachedStore<PeerId, Peer, KeyValueWrapper<CommsDatabase>>>>,
    _file_lock: Option<File>,
}

impl PeerManager {
    /// Constructs a new empty PeerManager
    pub fn new(database: CommsDatabase, file_lock: Option<File>) -> Result<PeerManager, PeerManagerError> {
        let storage = PeerStorage::new_indexed(CachedStore::new(KeyValueWrapper::new(database)))?;
        Ok(Self {
            peer_storage: RwLock::new(storage),
            _file_lock: file_lock,
        })
    }

    /// Migrate the peer database, this only applies to the LMDB database
    pub fn migrate_lmdb(database: &LMDBDatabase) -> Result<(), PeerManagerError> {
        migrations::migrate(database).map_err(|err| PeerManagerError::MigrationError(err.to_string()))
    }

    pub async fn count(&self) -> usize {
        self.peer_storage.read().await.count()
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub async fn add_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        let mut lock = self.peer_storage.write().await;
        let peer_id = lock.add_peer(peer)?;
        #[cfg(feature = "metrics")]
        {
            let count = lock.count();
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(peer_id)
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub async fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let mut lock = self.peer_storage.write().await;
        lock.delete_peer(node_id)?;
        #[cfg(feature = "metrics")]
        {
            let count = lock.count();
            #[allow(clippy::cast_possible_wrap)]
            metrics::peer_list_size().set(count as i64);
        }
        Ok(())
    }

    /// Performs the given [PeerQuery].
    ///
    /// [PeerQuery]: crate::peer_manager::PeerQuery
    pub async fn perform_query(&self, peer_query: PeerQuery<'_>) -> Result<Vec<Peer>, PeerManagerError> {
        let lock = self.peer_storage.read().await;
        lock.perform_query(peer_query)
    }

    /// Find the peer with the provided NodeID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        self.peer_storage.read().await.find_by_node_id(node_id)
    }

    /// Find the peer with the provided PublicKey
    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        self.peer_storage.read().await.find_by_public_key(public_key)
    }

    /// Find the peer with the provided substring. This currently only compares the given bytes to the NodeId
    pub async fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.read().await.find_all_starts_with(partial)
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
        self.peer_storage
            .read()
            .await
            .discovery_syncing(n, excluded_peers, features)
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

    /// Get a peer matching the given node ID
    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        match self.peer_storage.read().await.direct_identity_node_id(node_id) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Get a peer matching the given public key
    pub async fn direct_identity_public_key(
        &self,
        public_key: &CommsPublicKey,
    ) -> Result<Option<Peer>, PeerManagerError> {
        match self.peer_storage.read().await.direct_identity_public_key(public_key) {
            Ok(peer) => Ok(Some(peer)),
            Err(PeerManagerError::PeerNotFoundError) | Err(PeerManagerError::BannedPeer) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Fetch all peers (except banned ones)
    pub async fn flood_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.read().await.flood_peers()
    }

    pub async fn for_each<F>(&self, f: F) -> Result<(), PeerManagerError>
    where F: FnMut(Peer) -> IterationResult {
        self.peer_storage.read().await.for_each(f)
    }

    /// Fetch n nearest neighbours. If features are supplied, the function will return the closest peers matching that
    /// feature
    pub async fn closest_peers(
        &self,
        node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage
            .read()
            .await
            .closest_peers(node_id, n, excluded_peers, features)
    }

    pub async fn mark_last_seen(&self, node_id: &NodeId, addr: &Multiaddr) -> Result<(), PeerManagerError> {
        let mut lock = self.peer_storage.write().await;
        let mut peer = lock
            .find_by_node_id(node_id)?
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        let source = peer
            .addresses
            .iter()
            .find(|a| a.address() == addr)
            .map(|a| a.source().clone())
            .ok_or_else(|| PeerManagerError::AddressNotFoundError {
                address: addr.clone(),
                node_id: node_id.clone(),
            })?;
        // if we have an address, update it
        peer.addresses.add_address(addr, &source);
        peer.addresses.mark_last_seen_now(addr);
        lock.add_peer(peer)?;
        Ok(())
    }

    /// Fetch n random peers
    pub async fn random_peers(&self, n: usize, excluded: &[NodeId]) -> Result<Vec<Peer>, PeerManagerError> {
        // Send to a random set of peers of size n that are Communication Nodes
        self.peer_storage.read().await.random_peers(n, excluded)
    }

    /// Check if a specific node_id is in the network region of the N nearest neighbours of the region specified by
    /// region_node_id
    pub async fn in_network_region(
        &self,
        node_id: &NodeId,
        region_node_id: &NodeId,
        n: usize,
    ) -> Result<bool, PeerManagerError> {
        self.peer_storage
            .read()
            .await
            .in_network_region(node_id, region_node_id, n)
    }

    pub async fn calc_region_threshold(
        &self,
        region_node_id: &NodeId,
        n: usize,
        features: PeerFeatures,
    ) -> Result<NodeDistance, PeerManagerError> {
        let lock = self.peer_storage.read().await;
        lock.calc_region_threshold(region_node_id, n, features)
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage.write().await.unban_peer(node_id)
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer(
        &self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        self.peer_storage.write().await.ban_peer(public_key, duration, reason)
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer_by_node_id(
        &self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        self.peer_storage
            .write()
            .await
            .ban_peer_by_node_id(node_id, duration, reason)
    }

    pub async fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        self.peer_storage.read().await.is_peer_banned(node_id)
    }

    pub async fn update_each<F>(&self, mut f: F) -> Result<usize, PeerManagerError>
    where F: FnMut(Peer) -> Option<Peer> {
        let mut lock = self.peer_storage.write().await;
        let mut peers_to_update = Vec::new();
        lock.for_each(|peer| {
            if let Some(peer) = (f)(peer) {
                peers_to_update.push(peer);
            }
            IterationResult::Continue
        })?;

        let updated_count = peers_to_update.len();
        for p in peers_to_update {
            lock.add_peer(p)?;
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

    /// This will store metadata inside of the metadata field in the peer provided by the nodeID.
    /// It will return None if the value was empty and the old value if the value was updated
    pub async fn set_peer_metadata(
        &self,
        node_id: &NodeId,
        key: u8,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, PeerManagerError> {
        self.peer_storage.write().await.set_peer_metadata(node_id, key, data)
    }
}

impl fmt::Debug for PeerManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PeerManager { peer_storage: ... }")
    }
}

#[cfg(test)]
mod test {
    use std::borrow::BorrowMut;

    use rand::{rngs::OsRng, Rng};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    use super::*;
    use crate::{
        net_address::MultiaddressesWithStats,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
            PeerFeatures,
        },
    };

    fn create_test_peer(ban_flag: bool, features: PeerFeatures) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
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
        if ban_flag {
            peer.ban_for(Duration::from_secs(1000), "".to_string());
        }

        let good_addresses = peer.addresses.borrow_mut();
        let good_address = good_addresses.addresses()[0].address().clone();
        good_addresses.mark_last_seen_now(&good_address);

        peer
    }

    #[tokio::test]
    async fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = PeerManager::new(HashmapDatabase::new(), None).unwrap();
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
            .closest_peers(&unmanaged_peer.node_id, 3, &[], None)
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
            .closest_peers(&unmanaged_peer.node_id, 3, &excluded_peers, None)
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
        let peer_manager = PeerManager::new(HashmapDatabase::new(), None).unwrap();
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
        let peer_manager = PeerManager::new(HashmapDatabase::new(), None).unwrap();
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
                .peer_storage
                .read()
                .await
                .calc_region_threshold(&network_region_node_id, n, *features)
                .unwrap();

            let closest = peer_manager
                .closest_peers(&network_region_node_id, n, &[], Some(*features))
                .await
                .unwrap();

            assert!(closest
                .iter()
                .all(|p| network_region_node_id.distance(&p.node_id) <= node_threshold));
        }
    }

    #[tokio::test]
    async fn test_add_or_update_online_peer() {
        let peer_manager = PeerManager::new(HashmapDatabase::new(), None).unwrap();
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

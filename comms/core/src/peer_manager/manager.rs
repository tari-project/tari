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
        NodeId,
        PeerFeatures,
        PeerFlags,
        PeerManagerError,
        ThisPeerIdentity,
        peer::Peer,
        peer_id::PeerId,
        peer_storage_sql::PeerStorageSql,
    },
    types::{CommsDatabase, CommsPublicKey, TransportProtocol},
};

/// The PeerManager provides functionality to add, find and delete peers. It wraps synchronous
/// WAL-enabled SQLite database access and provides an async interface to the rest of the code base.
#[derive(Clone)]
pub struct PeerManager {
    // yo dawg, I heard you like wrappers, so I wrapped your wrapper in a wrapper so you can wrap while you wrap
    peer_storage_sql: PeerStorageSql,
    transport_protocols: Vec<TransportProtocol>,
}

impl PeerManager {
    /// Constructs a new empty PeerManager
    pub fn new(
        database: CommsDatabase,
        transport_protocols: Vec<TransportProtocol>,
    ) -> Result<PeerManager, PeerManagerError> {
        let peer_storage_sql = PeerStorageSql::new_indexed(database)?;

        Ok(Self {
            peer_storage_sql,
            transport_protocols,
        })
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

    /// Get available dial candidates that are communication nodes, not banned, not deleted, reachable
    /// optionally not failed, optionally at random, and not in the excluded node IDs list
    pub async fn get_available_dial_candidates(
        &self,
        exclude_node_ids: &[NodeId],
        limit: Option<usize>,
        exclude_failed: bool,
        randomize: bool,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql.get_available_dial_candidates(
            exclude_node_ids,
            limit,
            &self.transport_protocols,
            exclude_failed,
            randomize,
        )
    }

    /// Return "good" peers for syncing
    /// Criteria:
    ///  - Peer is not banned
    ///  - Peer has been seen within a defined time span (1 week)
    ///  - Returns at most `max_n` peers (the caller's configured serve cap); a `max_n` of 0 falls back to the peer
    ///    manager default
    pub async fn discovery_syncing(
        &self,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        external_addresses_only: bool,
        max_n: usize,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage_sql
            .discovery_syncing(n, excluded_peers, features, external_addresses_only, max_n)
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

    /// Fetch n random peers that are Communication Nodes and have at least one external address
    pub async fn random_peers(
        &self,
        n: usize,
        excluded: &[NodeId],
        flags: Option<PeerFlags>,
        known_good: bool,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        let mut peers =
            self.peer_storage_sql
                .random_peers(n, excluded, flags, &self.transport_protocols, known_good)?;
        if known_good && peers.len() < n {
            // The fallback must also exclude what the first query already returned: a known-good peer
            // satisfies the relaxed query too, so without this it is selected twice and the caller
            // dials the same peer more than once while believing it reached `n` distinct peers.
            let mut excluded = excluded.to_vec();
            excluded.extend(peers.iter().map(|peer| peer.node_id.clone()));
            let mut additional = self.peer_storage_sql.random_peers(
                n.checked_sub(peers.len()).unwrap_or(1),
                &excluded,
                flags,
                &self.transport_protocols,
                false,
            )?;
            peers.append(&mut additional);
        }
        Ok(peers)
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

    use rand::RngExt;

    use crate::peer_manager::PeerFlags;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let node_id = NodeId::from_key(&pk);

    // Create 1 to 4 random addresses
    let mut addresses = Vec::new();
    for _i in 1..=rand::rng().random_range(1..4) {
        let n = [
            // Use a range that is always globally routable according to the
            // address classifier; internal-address tests add their own cases.
            rand::rng().random_range(11..100),
            rand::rng().random_range(1..255),
            rand::rng().random_range(1..255),
            rand::rng().random_range(1..255),
            rand::rng().random_range(5000..9000),
        ];
        let address = format!("/ip4/{}.{}.{}.{}/tcp/{}", n[0], n[1], n[2], n[3], n[4])
            .parse::<Multiaddr>()
            .unwrap();
        addresses.push(address);
    }
    let net_addresses = MultiaddressesWithStats::from_addresses_with_source(
        addresses.clone(),
        &create_peer_address_source_with_claim(addresses, features),
    );

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
    let good_address = good_addresses.addresses().first().unwrap().address().clone();
    good_addresses.mark_last_seen_now(&good_address);

    peer
}

/// Generate a random, syntactically valid Tor v3 onion hostname:
///  - 56 chars, base32 alphabet [a-z2-7], lowercase.
#[cfg(test)]
fn random_onion3_host() -> String {
    use rand::distr::Uniform;

    const LEN: usize = 56;
    // RFC4648 base32 alphabet as used by onion v3 (lowercase).
    const B32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

    let mut rng = rand::rng();
    let dist = Uniform::new(0, B32.len()).unwrap();

    let mut s = String::with_capacity(LEN);
    for _ in 0..LEN {
        use rand::RngExt;

        let idx = rng.sample(dist);
        s.push(*B32.get(idx).expect("Index out of bounds") as char);
    }
    s
}

#[cfg(test)]
pub fn create_test_peer_with_onion_address(ban_flag: bool, features: PeerFeatures) -> Peer {
    use std::borrow::BorrowMut;

    use rand::RngExt;

    use crate::peer_manager::PeerFlags;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let node_id = NodeId::from_key(&pk);

    // Create 1 to 4 random onion addresses
    let mut addresses = Vec::new();
    for _i in 1..=rand::rng().random_range(1..4) {
        use std::str::FromStr;

        let host = random_onion3_host();
        let port = rand::rng().random_range(1024..=65535);
        let addr_str = format!("/onion3/{}:{}", host, port);
        let address = Multiaddr::from_str(&addr_str).expect("valid onion3 multiaddr");
        addresses.push(address);
    }
    let net_addresses = MultiaddressesWithStats::from_addresses_with_source(
        addresses.clone(),
        &create_peer_address_source_with_claim(addresses, features),
    );

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
    let good_address = good_addresses.addresses().first().unwrap().address().clone();
    good_addresses.mark_last_seen_now(&good_address);

    peer
}

#[cfg(test)]
pub fn create_test_peer_add_internal_addresses(ban_flag: bool, features: PeerFeatures) -> Peer {
    let mut peer = create_test_peer(ban_flag, features);
    add_internal_addresses(&mut peer);

    peer
}

#[cfg(test)]
pub fn create_test_peer_internal_addresses_only(ban_flag: bool, features: PeerFeatures) -> Peer {
    use crate::peer_manager::PeerFlags;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rand::rng());
    let node_id = NodeId::from_key(&pk);

    let mut peer = Peer::new(
        pk,
        node_id,
        MultiaddressesWithStats::default(),
        PeerFlags::default(),
        features,
        Default::default(),
        Default::default(),
    );
    if ban_flag {
        peer.ban_for(Duration::from_secs(1000), "".to_string());
    }
    add_internal_addresses(&mut peer);

    peer
}

#[cfg(test)]
fn add_internal_addresses(peer: &mut Peer) {
    use rand::{RngExt, prelude::SliceRandom};

    let mut addresses = Vec::new();
    // IPv4 Loopback
    let address_1 = format!(
        "/ip4/127.{}.{}.{}/tcp/{}",
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(9000..9100)
    )
    .parse::<Multiaddr>()
    .unwrap();
    addresses.push(address_1);
    // IPv4 Unspecified
    let address_2 = format!("/ip4/0.0.0.0/tcp/{}", rand::rng().random_range(9100..9200))
        .parse::<Multiaddr>()
        .unwrap();
    addresses.push(address_2);
    // IPv4 Private
    let address_3 = format!(
        "/ip4/10.{}.{}.{}/tcp/{}",
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(9200..9300)
    )
    .parse::<Multiaddr>()
    .unwrap();
    addresses.push(address_3);
    // IPv4 Private
    let address_4 = format!(
        "/ip4/172.{}.{}.{}/tcp/{}",
        rand::rng().random_range(16..=31),
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(9300..9400)
    )
    .parse::<Multiaddr>()
    .unwrap();
    addresses.push(address_4);
    // IPv4 Private
    let address_5 = format!(
        "/ip4/192.168.{}.{}/tcp/{}",
        rand::rng().random_range(0..255),
        rand::rng().random_range(0..255),
        rand::rng().random_range(9400..9500)
    )
    .parse::<Multiaddr>()
    .unwrap();
    addresses.push(address_5);
    // IPv6 Loopback
    let address_6 = format!("/ip6/::1/tcp/{}", rand::rng().random_range(9500..9600))
        .parse::<Multiaddr>()
        .unwrap();
    addresses.push(address_6);
    // IPv6 Unspecified
    let address_7 = format!("/ip6/::/tcp/{}", rand::rng().random_range(9600..9700))
        .parse::<Multiaddr>()
        .unwrap();
    addresses.push(address_7);
    addresses.shuffle(&mut rand::rng());

    // Do not create a new PeerAddressSource with PeerIdentityClaim - use PeerAddressSource::Config - otherwise the
    // previous claims and associated addresses will be discarded.
    peer.addresses
        .add_or_update_addresses(&addresses, &PeerAddressSource::Config);
}

#[cfg(test)]
pub fn create_peer_address_source_with_claim(
    addresses: Vec<Multiaddr>,
    peer_features: PeerFeatures,
) -> PeerAddressSource {
    use chrono::Utc;
    use tari_crypto::keys::SecretKey;

    use crate::{
        peer_manager::{IdentitySignature, PeerIdentityClaim},
        types::CommsSecretKey,
    };

    fn create_identity_signature(addresses: &[Multiaddr], peer_features: PeerFeatures) -> IdentitySignature {
        let secret = CommsSecretKey::random(&mut rand::rng());
        let public_key = CommsPublicKey::from_secret_key(&secret);
        let updated_at = Utc::now();
        let identity = IdentitySignature::sign_new(&secret, peer_features, addresses, updated_at);
        assert!(
            identity.is_valid(&public_key, peer_features, addresses).unwrap(),
            "Signature is not valid"
        );
        identity
    }

    PeerAddressSource::FromPeerConnection {
        peer_identity_claim: PeerIdentityClaim {
            addresses: addresses.clone(),
            features: peer_features,
            signature: create_identity_signature(&addresses, peer_features),
        },
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use chrono::{DateTime, Utc};
    use tari_common_sqlite::connection::DbConnection;

    use super::*;
    use crate::peer_manager::database::{MIGRATIONS, PeerDatabaseSql};

    fn create_peer_manager() -> PeerManager {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        PeerManager::new(peers_db, TransportProtocol::get_all()).unwrap()
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn test_get_broadcast_identities() {
        // Create peer manager with random peers
        let peer_manager = create_peer_manager();
        let mut test_peers = vec![create_test_peer(true, PeerFeatures::COMMUNICATION_NODE)];
        // Create 20 peers were the 1st and last one is bad
        assert!(
            peer_manager
                .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok()
        );
        for _i in 0..18 {
            test_peers.push(create_test_peer(false, PeerFeatures::COMMUNICATION_NODE));
            assert!(
                peer_manager
                    .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
                    .await
                    .is_ok()
            );
        }
        test_peers.push(create_test_peer(true, PeerFeatures::COMMUNICATION_NODE));
        assert!(
            peer_manager
                .add_or_update_peer(test_peers[test_peers.len() - 1].clone())
                .await
                .is_ok()
        );

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
        assert!(
            peer_manager
                .direct_identity_node_id(&unmanaged_peer.node_id)
                .await
                .unwrap()
                .is_none()
        );

        // Test Flood
        let selected_peers = peer_manager.get_not_banned_or_deleted_peers().await.unwrap();
        assert_eq!(selected_peers.len(), 18);
        for peer_identity in &selected_peers {
            assert!(
                !peer_manager
                    .find_by_node_id(&peer_identity.node_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_banned(),
            );
        }

        // Test Random
        let identities1 = peer_manager.random_peers(10, &[], None, false).await.unwrap();
        let identities2 = peer_manager.random_peers(10, &[], None, false).await.unwrap();
        assert_ne!(identities1, identities2);
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

    async fn validate_claim_bump_by_newer(
        peer_manager: &PeerManager,
        update_peer: &Peer,
        previous_claim_time: Option<DateTime<Utc>>,
        expected_count: usize,
    ) -> DateTime<Utc> {
        let peer_from_db = peer_manager
            .find_by_node_id(&update_peer.node_id)
            .await
            .unwrap()
            .unwrap();
        let newest_time = peer_from_db.addresses.newest_claim_updated_at().unwrap();

        if let Some(prev_time) = previous_claim_time {
            assert!(
                newest_time > prev_time,
                "New claim time was not newer than previous claim time"
            );
        }

        for addr in peer_from_db.addresses.addresses() {
            let claim_time = match addr.source() {
                PeerAddressSource::FromPeerConnection { peer_identity_claim } => {
                    peer_identity_claim.signature.updated_at()
                },
                _ => panic!("Expected FromPeerConnection source for address: {}", addr.address()),
            };
            assert_eq!(
                claim_time, newest_time,
                "Address claim time inconsistent among addresses"
            );
        }

        assert_eq!(peer_manager.count().await, expected_count, "Peer count mismatch");

        let mut expected_addresses = update_peer
            .addresses
            .addresses()
            .iter()
            .map(|a| a.address().clone())
            .collect::<Vec<_>>();
        let mut addresses_from_db = peer_from_db
            .addresses
            .addresses()
            .iter()
            .map(|a| a.address().clone())
            .collect::<Vec<_>>();
        expected_addresses.sort();
        addresses_from_db.sort();
        assert_eq!(expected_addresses, addresses_from_db);

        newest_time
    }

    #[tokio::test]
    async fn it_correctly_merges_old_and_new_address_claims() {
        // Create a PeerManager and a test peer
        let peer_manager = create_peer_manager();
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);

        // Add the original peer to the database
        peer_manager.add_or_update_peer(peer.clone()).await.unwrap();

        // Verify that the peer from the database has consistent claim timestamps
        let claim_1_time = validate_claim_bump_by_newer(&peer_manager, &peer, None, 1).await;

        // Create a new claim with the same addresses but a new timestamp
        tokio::time::sleep(Duration::from_millis(150)).await; // Sleep to ensure different timestamp
        let peer_addresses = peer
            .addresses
            .addresses()
            .iter()
            .map(|a| a.address().clone())
            .collect::<Vec<_>>();
        let peer_address_source = create_peer_address_source_with_claim(peer_addresses.clone(), peer.features);
        // Update the peer's addresses with a new claim
        peer.addresses
            .add_or_update_addresses(&peer_addresses, &peer_address_source);

        // Update the peer in the database
        peer_manager.add_or_update_peer(peer.clone()).await.unwrap();

        // Assert that the updated peer from the database has the new claim timestamp and that it is consistent
        let claim_2_time = validate_claim_bump_by_newer(&peer_manager, &peer, Some(claim_1_time), 1).await;

        // Create a total new set addresses and claim with a new timestamp for the same peer
        tokio::time::sleep(Duration::from_millis(150)).await; // Sleep to ensure different timestamp
        let mut update_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        update_peer.node_id = peer.node_id.clone();
        update_peer.public_key = peer.public_key.clone();

        // Add the updated peer to the database
        peer_manager.add_or_update_peer(update_peer.clone()).await.unwrap();

        // Assert that the database still contains only one peer and that the peer from the database has the new claim
        // timestamp and that it is consistent
        let _claim_3_time = validate_claim_bump_by_newer(&peer_manager, &update_peer, Some(claim_2_time), 1).await;

        // Update the peer in the database with the old peer (which has an older claim timestamp)
        // This should NOT update the claim timestamp
        peer_manager.add_or_update_peer(peer.clone()).await.unwrap();

        // Assert that the database still contains only one peer with the latest claim and addresses
        let _claim_3_time = validate_claim_bump_by_newer(&peer_manager, &update_peer, Some(claim_2_time), 1).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_concurrent_add_or_update_and_get_random_peers() {
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
                })
            })
            .collect();

        // Spawn tasks to concurrently fetch random peers
        let get_tasks: Vec<_> = (0..num_read_tasks)
            .map(|_| {
                let peer_manager = peer_manager.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    let _random_peers = peer_manager.random_peers(n, &[], None, false).await.unwrap();
                    tokio::time::sleep(Duration::from_micros(rand::random::<u64>() % 100)).await;
                    let _total_peers = peer_manager.count().await;
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
        let random_peers = peer_manager.random_peers(n, &[], None, false).await.unwrap();
        let total_peers = peer_manager.count().await;
        assert_eq!(total_peers, num_peers * num_write_tasks);
        assert!(random_peers.len() <= n);
    }
}

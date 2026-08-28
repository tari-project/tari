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
        blocking_storage::BlockingPeerStorage,
        peer::Peer,
        peer_id::PeerId,
        peer_storage_sql::PeerStorageSql,
    },
    types::{CommsDatabase, CommsPublicKey, TransportProtocol},
};

/// The PeerManager provides functionality to add, find and delete peers. It wraps synchronous
/// WAL-enabled SQLite database access and provides an async interface to the rest of the code base.
///
/// Every database call is dispatched to tokio's blocking thread pool by [`BlockingPeerStorage`], so
/// a contended peer database can never park a tokio worker thread. See that type's documentation for
/// why that matters.
#[derive(Clone)]
pub struct PeerManager {
    // yo dawg, I heard you like wrappers, so I wrapped your wrapper in a wrapper so you can wrap while you wrap
    peer_storage: BlockingPeerStorage,
    transport_protocols: Vec<TransportProtocol>,
}

/// How long an actor's main loop should be prepared to wait on a peer database lookup before
/// treating it as sheddable.
///
/// `ConnectionManager`, `Dialer` and `ConnectivityManager` all hit the peer database from their
/// single request loop. Even though the query itself now runs on the blocking pool, a caller that
/// awaits it without bound still stops servicing every other request behind it. Anything slower
/// than this is not worth the head-of-line blocking: the dial it was for is already stale.
pub const PEER_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// `PRAGMA busy_timeout` for the peer database.
///
/// Deliberately far below the shared 60s default. The peer database is written on every dial result,
/// so a dial storm produces a sustained stream of writes against SQLite's single writer. A 60s lock
/// wait means one contended call occupies a thread for a minute — a tokio worker before this was
/// moved to the blocking pool, and a blocking-pool thread after. Neither is affordable, and the
/// answer is never worth waiting a minute for: the dial the query was for is long dead by then.
/// Callers above this shed at [`PEER_LOOKUP_TIMEOUT`] anyway.
///
/// This is scoped to the peer database only. Databases where losing a write is worse than waiting —
/// the wallet in particular — keep the 60s default.
pub const PEER_DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(10);

impl PeerManager {
    /// Constructs a new empty PeerManager
    pub fn new(
        database: CommsDatabase,
        transport_protocols: Vec<TransportProtocol>,
    ) -> Result<PeerManager, PeerManagerError> {
        let peer_storage_sql = PeerStorageSql::new_indexed(database)?;

        Ok(Self {
            peer_storage: BlockingPeerStorage::new(peer_storage_sql),
            transport_protocols,
        })
    }

    /// Get this peer's identity
    pub fn this_peer_identity(&self) -> ThisPeerIdentity {
        self.peer_storage.this_peer_identity()
    }

    /// Get the number of peers in the PeerManager - any error will translate to a size of zero
    pub async fn count(&self) -> usize {
        self.peer_storage.call(|s| Ok(s.count())).await.unwrap_or_default()
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub async fn add_or_update_peer(&self, peer: Peer) -> Result<PeerId, PeerManagerError> {
        let peer_id = self.peer_storage.call(move |s| s.add_or_update_peer(peer)).await?;
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
        let node_id = node_id.clone();
        self.peer_storage.call(move |s| s.soft_delete_peer(&node_id)).await?;
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
        let node_ids = node_ids.to_vec();
        self.peer_storage
            .call(move |s| s.get_peers_by_node_ids(&node_ids))
            .await
    }

    /// Get all peers based on a list of their node_ids
    pub async fn get_peer_public_keys_by_node_ids(
        &self,
        node_ids: &[NodeId],
    ) -> Result<Vec<CommsPublicKey>, PeerManagerError> {
        let node_ids = node_ids.to_vec();
        self.peer_storage
            .call(move |s| s.get_peer_public_keys_by_node_ids(&node_ids))
            .await
    }

    /// Get all banned peers
    pub async fn get_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.call(PeerStorageSql::get_banned_peers).await
    }

    /// Find the peer with the provided NodeID
    pub async fn find_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage.call(move |s| s.get_peer_by_node_id(&node_id)).await
    }

    /// gets all seed peers
    pub async fn get_seed_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.call(PeerStorageSql::get_seed_peers).await
    }

    /// Find the peer with the provided PublicKey
    pub async fn find_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, PeerManagerError> {
        let public_key = public_key.clone();
        self.peer_storage.call(move |s| s.find_by_public_key(&public_key)).await
    }

    /// Find the peer with the provided substring. This currently only compares the given bytes to the NodeId
    pub async fn find_all_starts_with(&self, partial: &[u8]) -> Result<Vec<Peer>, PeerManagerError> {
        let partial = partial.to_vec();
        self.peer_storage.call(move |s| s.find_all_starts_with(&partial)).await
    }

    /// Check if a peer exist using the specified public_key
    pub async fn exists(&self, public_key: &CommsPublicKey) -> Result<bool, PeerManagerError> {
        let public_key = public_key.clone();
        self.peer_storage.call(move |s| s.exists_public_key(&public_key)).await
    }

    /// Check if a peer exist using the specified node_id
    pub async fn exists_node_id(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage.call(move |s| s.exists_node_id(&node_id)).await
    }

    /// Returns all peers
    pub async fn all(&self, features: Option<PeerFeatures>) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage.call(move |s| s.all(features)).await
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
        let exclude_node_ids = exclude_node_ids.to_vec();
        let transport_protocols = self.transport_protocols.clone();
        self.peer_storage
            .call(move |s| {
                s.get_available_dial_candidates(
                    &exclude_node_ids,
                    limit,
                    &transport_protocols,
                    exclude_failed,
                    randomize,
                )
            })
            .await
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
        let excluded_peers = excluded_peers.to_vec();
        self.peer_storage
            .call(move |s| s.discovery_syncing(n, &excluded_peers, features, external_addresses_only, max_n))
            .await
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
        let pubkey = pubkey.clone();
        let node_id = node_id.clone();
        let addresses = addresses.to_vec();
        let peer_features = *peer_features;
        let source = source.clone();
        self.peer_storage
            .call(move |s| s.add_or_update_online_peer(&pubkey, &node_id, &addresses, &peer_features, &source))
            .await
    }

    /// Get a peer matching the given node ID
    pub async fn direct_identity_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage
            .call(move |s| match s.direct_identity_node_id(&node_id) {
                Ok(peer) => Ok(Some(peer)),
                Err(PeerManagerError::PeerNotFound(_)) | Err(PeerManagerError::BannedPeer) => Ok(None),
                Err(err) => Err(err),
            })
            .await
    }

    /// Get a peer matching the given public key
    pub async fn direct_identity_public_key(
        &self,
        public_key: &CommsPublicKey,
    ) -> Result<Option<Peer>, PeerManagerError> {
        let public_key = public_key.clone();
        self.peer_storage
            .call(move |s| match s.direct_identity_public_key(&public_key) {
                Ok(peer) => Ok(Some(peer)),
                Err(PeerManagerError::PeerNotFound(_)) | Err(PeerManagerError::BannedPeer) => Ok(None),
                Err(err) => Err(err),
            })
            .await
    }

    /// Fetch all peers (except banned ones)
    pub async fn get_not_banned_or_deleted_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        self.peer_storage
            .call(PeerStorageSql::get_not_banned_or_deleted_peers)
            .await
    }

    /// Fetch n random peers that are Communication Nodes and have at least one external address
    pub async fn random_peers(
        &self,
        n: usize,
        excluded: &[NodeId],
        flags: Option<PeerFlags>,
        known_good: bool,
    ) -> Result<Vec<Peer>, PeerManagerError> {
        let excluded = excluded.to_vec();
        let transport_protocols = self.transport_protocols.clone();
        self.peer_storage
            .call(move |s| {
                let mut peers = s.random_peers(n, &excluded, flags, &transport_protocols, known_good)?;
                if known_good && peers.len() < n {
                    // The fallback must also exclude what the first query already returned: a known-good peer
                    // satisfies the relaxed query too, so without this it is selected twice and the caller
                    // dials the same peer more than once while believing it reached `n` distinct peers.
                    let mut excluded = excluded.clone();
                    excluded.extend(peers.iter().map(|peer| peer.node_id.clone()));
                    let mut additional = s.random_peers(
                        n.checked_sub(peers.len()).unwrap_or(1),
                        &excluded,
                        flags,
                        &transport_protocols,
                        false,
                    )?;
                    peers.append(&mut additional);
                }
                Ok(peers)
            })
            .await
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage.call(move |s| s.unban_peer(&node_id)).await
    }

    /// Unbans the peer if it is banned. This function is idempotent.
    pub async fn unban_all_peers(&self) -> Result<usize, PeerManagerError> {
        self.peer_storage.call(PeerStorageSql::unban_all_peers).await
    }

    pub async fn reset_offline_non_wallet_peers(&self) -> Result<usize, PeerManagerError> {
        self.peer_storage
            .call(PeerStorageSql::reset_offline_non_wallet_peers)
            .await
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer(
        &self,
        public_key: &CommsPublicKey,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let public_key = public_key.clone();
        self.peer_storage
            .call(move |s| s.ban_peer(&public_key, duration, reason))
            .await
    }

    /// Ban the peer for a length of time specified by the duration
    pub async fn ban_peer_by_node_id(
        &self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<NodeId, PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage
            .call(move |s| s.ban_peer_by_node_id(&node_id, duration, reason))
            .await
    }

    /// Get the ban status of a peer
    pub async fn is_peer_banned(&self, node_id: &NodeId) -> Result<bool, PeerManagerError> {
        let node_id = node_id.clone();
        self.peer_storage.call(move |s| s.is_peer_banned(&node_id)).await
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
        let node_id = node_id.clone();
        self.peer_storage
            .call(move |s| s.set_peer_metadata(&node_id, key, data))
            .await
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
    use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};

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

    /// A peer manager plus a second handle on the same database, so a test can take SQLite's write
    /// lock out from under it and create genuine contention.
    ///
    /// The busy timeout is deliberately short: a test that trips the fallback should fail fast
    /// rather than sit on the shared 60s default.
    fn create_contended_peer_manager() -> (PeerManager, DbConnection, tempfile::TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_url = DbConnectionUrl::File(temp_dir.path().join("contended_peers.db"));
        let db_connection =
            DbConnection::connect_and_migrate_with_busy_timeout(&db_url, MIGRATIONS, Some(6), Duration::from_secs(5))
                .unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection.clone(),
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        let peer_manager = PeerManager::new(peers_db, TransportProtocol::get_all()).unwrap();
        (peer_manager, db_connection, temp_dir)
    }

    /// Regression test for the peer-database worker-thread starvation bug.
    ///
    /// `PeerManager` methods used to be `async fn`s with no await points that called straight into
    /// diesel/r2d2. A contended peer database therefore blocked the tokio *worker thread* running
    /// the caller, not merely the calling task, and under sustained write pressure the worker pool
    /// drained one thread at a time until the comms actors stopped being scheduled at all.
    ///
    /// The runtime here has exactly one worker, so any peer database call that blocks it is
    /// immediately visible: the concurrently spawned ticker stops making progress.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn peer_database_writes_do_not_park_the_runtime_worker() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        use diesel::connection::SimpleConnection;

        let (peer_manager, db_connection, _temp_dir) = create_contended_peer_manager();

        // Take SQLite's single write lock on a separate connection. Every peer write issued below
        // now parks inside SQLite until this is released.
        let mut lock_holder = db_connection.get_pooled_connection().unwrap();
        lock_holder.batch_execute("BEGIN IMMEDIATE;").unwrap();

        // A plain async task that only ever needs the worker thread for a few microseconds at a
        // time. It is the canary: if the worker is parked, it stops counting.
        let progress = Arc::new(AtomicUsize::new(0));
        let ticker = tokio::spawn({
            let progress = progress.clone();
            async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    progress.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        let writers = (0..4)
            .map(|_| {
                let peer_manager = peer_manager.clone();
                let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
                tokio::spawn(async move { peer_manager.add_or_update_peer(peer).await })
            })
            .collect::<Vec<_>>();

        tokio::time::sleep(Duration::from_millis(300)).await;
        let ticks = progress.load(Ordering::Relaxed);
        assert!(
            ticks > 10,
            "the runtime worker was parked by the peer database: the canary task only ran {ticks} time(s) in 300ms              while 4 peer writes were contended"
        );

        // Release the write lock and confirm the writes actually landed - shedding the wait must not
        // mean losing the write.
        lock_holder.batch_execute("COMMIT;").unwrap();
        drop(lock_holder);
        for writer in writers {
            writer.await.unwrap().unwrap();
        }
        ticker.abort();
    }

    /// Structural guard: the peer database must never be touched from a tokio worker thread.
    ///
    /// # Why this parses instead of pattern-matching
    ///
    /// Four successive review rounds broke a text-scanning version of this guard, each time by a
    /// different route, and each time with the same symptom: the scan silently checked less than it
    /// appeared to and still reported `ok`. Line-oriented matching missed rustfmt-wrapped calls; an
    /// unbounded region false-positived on test code; a `}`-at-column-zero bound was truncated by a
    /// multi-line string literal; a `#[cfg(test)]` bound took the *first* match and so skipped the
    /// 258 lines between manager.rs's module-scope test helpers and its actual test module, hiding a
    /// violation in a second, non-contiguous `impl PeerManager` block.
    ///
    /// Every one of those is a boundary guessed from bytes. They are not five bugs, they are one
    /// bug with five faces, and patching the sixth face was not going to end. So the boundary now
    /// comes from the grammar: [`syn::parse_file`] gives real items, real attributes and real
    /// expressions. A `#[cfg(test)]` item is skipped because it *is* a test item, not because it
    /// follows some byte offset; a string literal is a `Lit` and can never be mistaken for a field
    /// access; and non-contiguous impl blocks anywhere in the file are visited like any other.
    ///
    /// The two behavioural tests above (`peer_database_writes_do_not_park_the_runtime_worker` and
    /// `select_connections_is_served_while_the_peer_database_is_slow`) prove the runtime
    /// consequence. This one prevents the regression from being reintroduced at the source level,
    /// where it is cheap to catch.
    mod storage_access {
        use syn::{
            Attribute,
            Expr,
            File,
            ImplItem,
            ImplItemFn,
            Item,
            Member,
            visit::{self, Visit},
        };

        /// True for the canonical `#[cfg(test)]`.
        ///
        /// Anything else - `#[cfg(all(test))]`, `#[cfg(feature = "...")]` - reads as false, so the
        /// item is *scanned* rather than skipped. That is the safe direction: an unrecognised
        /// attribute spelling can only ever cause an over-strict failure, never a silent hole.
        fn is_cfg_test(attrs: &[Attribute]) -> bool {
            attrs.iter().any(|attr| {
                if !attr.path().is_ident("cfg") {
                    return false;
                }
                let mut mentions_test = false;
                let _result = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("test") {
                        mentions_test = true;
                    }
                    Ok(())
                });
                mentions_test
            })
        }

        /// Attributes of the item kinds that can contain expressions. Kinds that cannot are given an
        /// empty slice, which at worst means visiting something harmless.
        fn item_attrs(item: &Item) -> &[Attribute] {
            match item {
                Item::Const(i) => &i.attrs,
                Item::Enum(i) => &i.attrs,
                Item::Fn(i) => &i.attrs,
                Item::Impl(i) => &i.attrs,
                Item::Macro(i) => &i.attrs,
                Item::Mod(i) => &i.attrs,
                Item::Static(i) => &i.attrs,
                Item::Struct(i) => &i.attrs,
                Item::Trait(i) => &i.attrs,
                Item::Type(i) => &i.attrs,
                Item::Union(i) => &i.attrs,
                Item::Use(i) => &i.attrs,
                _ => &[],
            }
        }

        /// How a `self.<field>` access is used.
        #[derive(Debug, Default)]
        pub(super) struct FieldUsage {
            /// Every `self.<field>` expression found, however it is used.
            pub(super) accesses: usize,
            /// The method invoked, for those accesses that are a method-call receiver.
            pub(super) methods: Vec<String>,
        }

        /// Walks production items looking for `self.<field>`.
        struct FieldVisitor<'a> {
            field: &'a str,
            usage: FieldUsage,
        }

        /// True when `expr` is exactly `self.<field>`.
        fn is_self_field(expr: &Expr, field: &str) -> bool {
            let Expr::Field(access) = expr else {
                return false;
            };
            let Expr::Path(base) = access.base.as_ref() else {
                return false;
            };
            let Member::Named(name) = &access.member else {
                return false;
            };
            base.path.is_ident("self") && name == field
        }

        impl<'ast> Visit<'ast> for FieldVisitor<'_> {
            fn visit_item(&mut self, item: &'ast Item) {
                if is_cfg_test(item_attrs(item)) {
                    return;
                }
                visit::visit_item(self, item);
            }

            fn visit_impl_item(&mut self, item: &'ast ImplItem) {
                let attrs = match item {
                    ImplItem::Const(i) => &i.attrs,
                    ImplItem::Fn(i) => &i.attrs,
                    ImplItem::Type(i) => &i.attrs,
                    ImplItem::Macro(i) => &i.attrs,
                    _ => &[][..],
                };
                if is_cfg_test(attrs) {
                    return;
                }
                visit::visit_impl_item(self, item);
            }

            fn visit_expr(&mut self, expr: &'ast Expr) {
                match expr {
                    // A method call directly on `self.<field>`: record which method.
                    Expr::MethodCall(call) if is_self_field(&call.receiver, self.field) => {
                        self.usage.methods.push(call.method.to_string());
                    },
                    // Any other appearance of `self.<field>` is counted by `accesses` below and will
                    // fail the "every access is a permitted method call" balance check - a borrow
                    // stashed in a local, say, and called synchronously later.
                    _ => {},
                }
                if is_self_field(expr, self.field) {
                    self.usage.accesses = self.usage.accesses.saturating_add(1);
                }
                visit::visit_expr(self, expr);
            }
        }

        /// Find every use of `self.<field>` in the file's production items.
        pub(super) fn field_usage(file: &File, field: &str) -> FieldUsage {
            let mut visitor = FieldVisitor {
                field,
                usage: FieldUsage::default(),
            };
            visitor.visit_file(file);
            visitor.usage
        }

        /// The named inherent method of the named type, if the file declares one.
        pub(super) fn inherent_method<'a>(file: &'a File, type_name: &str, method: &str) -> Option<&'a ImplItemFn> {
            file.items.iter().find_map(|item| {
                let Item::Impl(block) = item else {
                    return None;
                };
                if block.trait_.is_some() || is_cfg_test(&block.attrs) {
                    return None;
                }
                let syn::Type::Path(path) = block.self_ty.as_ref() else {
                    return None;
                };
                if path.path.segments.last().is_none_or(|seg| seg.ident != type_name) {
                    return None;
                }
                block.items.iter().find_map(|impl_item| match impl_item {
                    ImplItem::Fn(func) if func.sig.ident == method => Some(func),
                    _ => None,
                })
            })
        }

        /// Counts invocations of a bare function-path such as the closure parameter `f`.
        struct CallCounter<'a> {
            callee: &'a str,
            count: usize,
        }

        impl<'ast> Visit<'ast> for CallCounter<'_> {
            fn visit_expr(&mut self, expr: &'ast Expr) {
                if let Expr::Call(call) = expr &&
                    let Expr::Path(path) = call.func.as_ref() &&
                    path.path.is_ident(self.callee)
                {
                    self.count = self.count.saturating_add(1);
                }
                visit::visit_expr(self, expr);
            }
        }

        fn count_calls_in_block(block: &syn::Block, callee: &str) -> usize {
            let mut counter = CallCounter { callee, count: 0 };
            counter.visit_block(block);
            counter.count
        }

        fn count_calls_in_expr(expr: &Expr, callee: &str) -> usize {
            let mut counter = CallCounter { callee, count: 0 };
            counter.visit_expr(expr);
            counter.count
        }

        /// Every invocation of `callee` inside `func`, and how many of those sit inside a closure
        /// handed as the first argument to `dispatcher`.
        pub(super) fn dispatch_balance(func: &ImplItemFn, callee: &str, dispatcher: &str) -> (usize, usize) {
            let total = count_calls_in_block(&func.block, callee);

            struct DispatcherVisitor<'a> {
                dispatcher: &'a str,
                callee: &'a str,
                dispatched: usize,
            }

            impl<'ast> Visit<'ast> for DispatcherVisitor<'_> {
                fn visit_expr(&mut self, expr: &'ast Expr) {
                    if let Expr::Call(call) = expr &&
                        let Expr::Path(path) = call.func.as_ref() &&
                        path.path
                            .segments
                            .last()
                            .is_some_and(|seg| seg.ident == self.dispatcher) &&
                        let Some(Expr::Closure(closure)) = call.args.first()
                    {
                        self.dispatched = self
                            .dispatched
                            .saturating_add(count_calls_in_expr(closure.body.as_ref(), self.callee));
                    }
                    visit::visit_expr(self, expr);
                }
            }

            let mut visitor = DispatcherVisitor {
                dispatcher,
                callee,
                dispatched: 0,
            };
            visitor.visit_block(&func.block);
            (total, visitor.dispatched)
        }
    }

    /// Assert that `self.<field>` is only ever used as a receiver of one of `permitted`.
    fn assert_field_only_reached_via(source: &str, file: &str, field: &str, permitted: &[&str]) -> usize {
        let parsed = syn::parse_file(source).unwrap_or_else(|err| panic!("{file} must parse: {err}"));
        let usage = storage_access::field_usage(&parsed, field);

        let offending = usage
            .methods
            .iter()
            .filter(|method| !permitted.contains(&method.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            offending.is_empty(),
            "{file}: `self.{field}` is used as the receiver of {offending:?}, but only {permitted:?} are permitted. \
             Anything else runs SQLite on the caller's tokio worker thread."
        );

        // Every access must be one of those method calls. A `self.<field>` that is borrowed into a
        // local, passed to a function or returned would not appear in `methods`, and is caught here.
        assert_eq!(
            usage.accesses,
            usage.methods.len(),
            "{file}: {} of the {} `self.{field}` access(es) are not direct method calls. The handle must not escape - \
             once it does, nothing constrains how it is used.",
            usage.accesses.saturating_sub(usage.methods.len()),
            usage.accesses,
        );

        usage.accesses
    }

    /// The peer database must never be touched from a tokio worker thread.
    ///
    /// Two halves, because the invariant has two halves. The compiler enforces the first - only
    /// `BlockingPeerStorage` can reach `PeerStorageSql`, it lives in another module and has no
    /// accessor - but nothing stops a `PeerManager` method reaching it through some future
    /// synchronous passthrough, and nothing stops `BlockingPeerStorage::call` being rewritten to
    /// invoke the closure inline. Both are checked here, structurally. See [`storage_access`].
    #[test]
    fn no_peer_manager_method_performs_synchronous_storage_io() {
        const MANAGER_SOURCE: &str = include_str!("manager.rs");
        const BLOCKING_STORAGE_SOURCE: &str = include_str!("blocking_storage.rs");

        // Half one: every peer storage access in production `manager.rs` goes through the blocking
        // dispatcher, or through the documented I/O-free identity accessor. Position in the file is
        // irrelevant - a second, non-contiguous `impl PeerManager` is visited like any other item.
        let inspected = assert_field_only_reached_via(MANAGER_SOURCE, "manager.rs", "peer_storage", &[
            "call",
            "this_peer_identity",
        ]);
        // Anti-vacuity floor: a renamed field would leave the guard inspecting nothing while still
        // reporting success. There are around thirty accesses today.
        assert!(
            inspected >= 20,
            "manager.rs: the guard found only {inspected} `self.peer_storage` access(es), so it is no longer guarding \
             anything meaningful. Check that the field name still matches the source."
        );

        // Half two: nothing in production `blocking_storage.rs` reaches the database synchronously.
        let inspected = assert_field_only_reached_via(BLOCKING_STORAGE_SOURCE, "blocking_storage.rs", "storage", &[
            // The handle moved into the blocking closure, and the documented exception that
            // reads an in-memory field and performs no I/O.
            "clone",
            "this_peer_identity",
        ]);
        assert!(
            inspected > 0,
            "blocking_storage.rs: the guard found no `self.storage` accesses at all, so the field name no longer \
             matches the source and this guard is inert."
        );

        // And the dispatch itself: every invocation of the caller's closure `f` must happen inside a
        // closure handed to `spawn_blocking`. Asserting on the call graph rather than on token order
        // is what makes a decoy `spawn_blocking(|| {})` sitting in front of an inline `f(&storage)`
        // fail - the decoy's closure does not invoke `f`, so the two counts do not balance.
        let parsed = syn::parse_file(BLOCKING_STORAGE_SOURCE).expect("blocking_storage.rs must parse");
        let call = storage_access::inherent_method(&parsed, "BlockingPeerStorage", "call")
            .expect("blocking_storage.rs must declare `BlockingPeerStorage::call`");
        let (total, dispatched) = storage_access::dispatch_balance(call, "f", "spawn_blocking");
        assert!(
            total > 0,
            "`BlockingPeerStorage::call` never invokes the caller's closure `f`; this guard is inert."
        );
        assert_eq!(
            total, dispatched,
            "`BlockingPeerStorage::call` invokes the caller's closure `f` {total} time(s) but only {dispatched} of \
             those are inside a closure passed to `spawn_blocking`. The rest run SQLite on the caller's tokio worker \
             thread."
        );
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

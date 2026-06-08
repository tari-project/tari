//  Copyright 2020, The Tari Project
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

use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
    time::Duration,
};

use primitive_types::U512;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{PeerConnection, peer_manager::NodeId};

use crate::{base_node::chain_metadata_service::PeerChainMetadata, common::rolling_avg::RollingAverageTime};

#[derive(Debug, Clone)]
pub struct SyncPeer {
    peer_metadata: PeerChainMetadata,
    avg_latency: RollingAverageTime,
    /// Optional pre-dialled connection that travels with this sync peer across header_sync →
    /// block_sync → horizon_state_sync. Populated when entering header sync (typically as a
    /// [`tari_comms::RefKind::Strong`] handle so it remains pinned for the full sync). Cloning
    /// the SyncPeer preserves the underlying connection's strength because
    /// [`PeerConnection::clone`] is Arc-like.
    connection: Option<PeerConnection>,
}

impl SyncPeer {
    pub fn node_id(&self) -> &NodeId {
        self.peer_metadata.node_id()
    }

    pub fn claimed_chain_metadata(&self) -> &ChainMetadata {
        self.peer_metadata.claimed_chain_metadata()
    }

    pub fn claimed_difficulty(&self) -> U512 {
        self.peer_metadata.claimed_chain_metadata().accumulated_difficulty()
    }

    pub fn latency(&self) -> Option<Duration> {
        self.peer_metadata.latency()
    }

    pub(super) fn set_latency(&mut self, latency: Duration) -> &mut Self {
        self.peer_metadata.set_latency(latency);
        self
    }

    pub fn items_per_second(&self) -> Option<f64> {
        self.avg_latency.calc_samples_per_second()
    }

    pub(super) fn add_sample(&mut self, time: Duration) -> &mut Self {
        self.avg_latency.add_sample(time);
        self
    }

    pub fn calc_avg_latency(&self) -> Option<Duration> {
        self.avg_latency.calculate_average()
    }

    /// Returns the connection held by this peer, if any.
    pub fn connection(&self) -> Option<&PeerConnection> {
        self.connection.as_ref()
    }

    /// Stores a pre-dialled `PeerConnection` on this sync peer. Typically called once at the
    /// entry to header sync with a [`tari_comms::RefKind::Strong`] handle; that strong handle
    /// then propagates with the SyncPeer into block_sync and horizon_state_sync, pinning the
    /// connection for the full sync cycle.
    pub fn set_connection(&mut self, connection: PeerConnection) {
        self.connection = Some(connection);
    }

    /// Drops any stored connection for this peer (releasing the strong ref if one was held).
    /// Use when a peer is being removed from the sync set, e.g. after a failed attempt.
    pub fn clear_connection(&mut self) {
        self.connection = None;
    }

    /// If the stored connection is currently a Weak handle (or no connection is attached),
    /// upgrade it to Strong in place. If it's already Strong this is a no-op. The upgrade is
    /// cheap and never touches the connectivity actor — it just clones the existing handle.
    ///
    /// Returns `true` if a Strong handle is attached after this call (either pre-existing or
    /// newly upgraded), `false` if no connection was attached. Callers that get `false` should
    /// dial a fresh Strong connection via the connectivity layer.
    pub fn ensure_strong_connection(&mut self) -> bool {
        match self.connection.take() {
            Some(conn) if conn.is_strong() => {
                self.connection = Some(conn);
                true
            },
            Some(conn) => {
                // Upgrade: clone_strong bumps the shared strong-counter, then we drop the old
                // weak handle. Net result: +1 strong slot held by this SyncPeer.
                self.connection = Some(conn.clone_strong());
                true
            },
            None => false,
        }
    }

    /// If the stored connection is currently a Strong handle, replace it with a Weak clone.
    /// The connection itself remains attached so later stages can reuse it without redialing,
    /// but the strong-count is released — letting background reapers reclaim the peer if no
    /// other strong holder exists. No-op when the stored handle is already Weak or absent.
    pub fn downgrade_connection(&mut self) {
        match self.connection.take() {
            Some(conn) if conn.is_strong() => {
                // clone_weak does not touch the counter; dropping `conn` (the strong handle)
                // releases its slot. Net result: -1 strong slot, connection still attached.
                self.connection = Some(conn.clone_weak());
            },
            other => {
                self.connection = other;
            },
        }
    }
}

impl From<PeerChainMetadata> for SyncPeer {
    fn from(peer_metadata: PeerChainMetadata) -> Self {
        Self {
            peer_metadata,
            avg_latency: RollingAverageTime::new(20),
            connection: None,
        }
    }
}

impl Display for SyncPeer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node ID: {}, Chain metadata: {}, Latency: {}",
            self.node_id(),
            self.claimed_chain_metadata(),
            self.latency()
                .map(|d| format!("{d:.2?}"))
                .unwrap_or_else(|| "--".to_string())
        )
    }
}

impl PartialEq for SyncPeer {
    fn eq(&self, other: &Self) -> bool {
        self.node_id() == other.node_id()
    }
}
impl Eq for SyncPeer {}

impl Ord for SyncPeer {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut result = other
            .peer_metadata
            .claimed_chain_metadata()
            .accumulated_difficulty()
            .cmp(&self.peer_metadata.claimed_chain_metadata().accumulated_difficulty());
        if result == Ordering::Equal {
            match (self.latency(), other.latency()) {
                (None, None) => result = Ordering::Equal,
                // No latency goes to the end
                (Some(_), None) => result = Ordering::Less,
                (None, Some(_)) => result = Ordering::Greater,
                (Some(la), Some(lb)) => result = la.cmp(&lb),
            }
        }
        result
    }
}

impl PartialOrd for SyncPeer {
    fn partial_cmp(&self, other: &SyncPeer) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::time::Duration;

    use tari_common_types::chain_metadata::ChainMetadata;

    use super::*;

    mod connection_attachment {
        use tari_common_types::types::FixedHash;
        use tari_comms::{
            RefKind,
            test_utils::mocks::create_dummy_peer_connection,
            types::{CommsPublicKey, CommsSecretKey},
        };
        use tari_crypto::keys::SecretKey;

        use super::*;

        fn peer_with_id() -> SyncPeer {
            let sk = CommsSecretKey::random(&mut rand::rng());
            let pk = CommsPublicKey::from_secret_key(&sk);
            let node_id = NodeId::from_key(&pk);
            PeerChainMetadata::new(
                node_id,
                ChainMetadata::new(0, FixedHash::zero(), 0, 0, U512::from(1), 0).unwrap(),
                None,
            )
            .into()
        }

        #[test]
        fn set_connection_stores_handle_and_clone_preserves_strength() {
            let mut peer = peer_with_id();
            assert!(peer.connection().is_none());

            let (raw_conn, _rx) = create_dummy_peer_connection(peer.node_id().clone());
            let strong = raw_conn.clone_strong();
            assert_eq!(strong.strong_count(), 1);

            peer.set_connection(strong);
            assert!(peer.connection().is_some());
            assert_eq!(peer.connection().unwrap().strong_count(), 1);

            // Cloning the SyncPeer clones the underlying strong handle (Arc-like): counter bumps.
            let peer_clone = peer.clone();
            assert_eq!(peer.connection().unwrap().strong_count(), 2);
            drop(peer_clone);
            assert_eq!(peer.connection().unwrap().strong_count(), 1);

            // Weak clone used inside attempt loops does not bump the counter.
            let weak = peer.connection().unwrap().clone_with(RefKind::Weak);
            assert_eq!(peer.connection().unwrap().strong_count(), 1);
            drop(weak);
            assert_eq!(peer.connection().unwrap().strong_count(), 1);

            // Releasing the SyncPeer entirely drops the only Strong handle.
            peer.clear_connection();
            assert!(peer.connection().is_none());
            // raw_conn (weak) observes the release via the shared counter.
            assert_eq!(raw_conn.strong_count(), 0);
        }

        #[test]
        fn downgrade_connection_releases_strong_but_keeps_handle() {
            let mut peer = peer_with_id();
            let (raw_conn, _rx) = create_dummy_peer_connection(peer.node_id().clone());
            peer.set_connection(raw_conn.clone_strong());
            assert_eq!(raw_conn.strong_count(), 1);

            peer.downgrade_connection();
            // Connection still attached for reuse, but strong-count is back to zero.
            assert!(peer.connection().is_some());
            assert!(!peer.connection().unwrap().is_strong());
            assert_eq!(raw_conn.strong_count(), 0);
        }

        #[test]
        fn downgrade_connection_is_noop_for_weak_or_none() {
            let mut peer = peer_with_id();
            // None case
            peer.downgrade_connection();
            assert!(peer.connection().is_none());

            // Weak case
            let (raw_conn, _rx) = create_dummy_peer_connection(peer.node_id().clone());
            peer.set_connection(raw_conn.clone_weak());
            assert!(!peer.connection().unwrap().is_strong());
            peer.downgrade_connection();
            assert!(peer.connection().is_some());
            assert!(!peer.connection().unwrap().is_strong());
            assert_eq!(raw_conn.strong_count(), 0);
        }

        #[test]
        fn ensure_strong_connection_upgrades_weak_in_place() {
            let mut peer = peer_with_id();
            let (raw_conn, _rx) = create_dummy_peer_connection(peer.node_id().clone());
            peer.set_connection(raw_conn.clone_weak());
            assert_eq!(raw_conn.strong_count(), 0);

            let upgraded = peer.ensure_strong_connection();
            assert!(upgraded);
            assert!(peer.connection().unwrap().is_strong());
            assert_eq!(raw_conn.strong_count(), 1);

            // Calling again is idempotent — already-strong handles aren't double-counted.
            let upgraded_again = peer.ensure_strong_connection();
            assert!(upgraded_again);
            assert_eq!(raw_conn.strong_count(), 1);
        }

        #[test]
        fn ensure_strong_connection_returns_false_when_no_connection() {
            let mut peer = peer_with_id();
            assert!(!peer.ensure_strong_connection());
        }
    }

    mod sort_by_latency {
        use tari_common_types::types::FixedHash;
        use tari_comms::types::{CommsPublicKey, CommsSecretKey};
        use tari_crypto::keys::SecretKey;

        use super::*;

        // Helper function to generate a peer with a given latency
        fn generate_peer(latency: Option<usize>, accumulated_difficulty: Option<U512>) -> SyncPeer {
            let sk = CommsSecretKey::random(&mut rand::rng());
            let pk = CommsPublicKey::from_secret_key(&sk);
            let node_id = NodeId::from_key(&pk);
            let latency_option = latency.map(|latency| Duration::from_millis(latency as u64));
            let peer_accumulated_difficulty = match accumulated_difficulty {
                Some(v) => v,
                None => 1.into(),
            };
            PeerChainMetadata::new(
                node_id,
                ChainMetadata::new(0, FixedHash::zero(), 0, 0, peer_accumulated_difficulty, 0).unwrap(),
                latency_option,
            )
            .into()
        }

        #[test]
        fn it_sorts_by_latency() {
            const DISTINCT_LATENCY: usize = 5;

            // Generate a list of peers with latency, adding duplicates
            let mut peers = (0..2 * DISTINCT_LATENCY)
                .map(|latency| generate_peer(Some(latency % DISTINCT_LATENCY), None))
                .collect::<Vec<SyncPeer>>();

            // Add peers with no latency in a few places
            peers.insert(0, generate_peer(None, None));
            peers.insert(DISTINCT_LATENCY, generate_peer(None, None));
            peers.push(generate_peer(None, None));

            // Sort the list; because difficulty is identical, it should sort by latency
            peers.sort();

            // Confirm that the sorted latency is correct: numerical ordering, then `None`
            for (i, peer) in peers[..2 * DISTINCT_LATENCY].iter().enumerate() {
                assert_eq!(peer.latency(), Some(Duration::from_millis((i as u64) / 2)));
            }
            for _ in 0..3 {
                assert_eq!(peers.pop().unwrap().latency(), None);
            }
        }

        #[test]
        fn it_sorts_by_pow() {
            let mut peers = Vec::new();

            let mut pow = U512::from(1);
            let new_peer = generate_peer(Some(1), Some(pow));
            peers.push(new_peer);
            pow = U512::from(100);
            let new_peer = generate_peer(Some(100), Some(pow));
            peers.push(new_peer);
            pow = U512::from(1000);
            let new_peer = generate_peer(Some(1000), Some(pow));
            peers.push(new_peer);

            // Sort the list;
            peers.sort();

            assert_eq!(
                peers[0].peer_metadata.claimed_chain_metadata().accumulated_difficulty(),
                1000.into()
            );
            assert_eq!(
                peers[1].peer_metadata.claimed_chain_metadata().accumulated_difficulty(),
                100.into()
            );
            assert_eq!(
                peers[2].peer_metadata.claimed_chain_metadata().accumulated_difficulty(),
                1.into()
            );
        }
    }
}

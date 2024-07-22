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

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::peer_manager::NodeId;

use crate::{base_node::chain_metadata_service::PeerChainMetadata, common::rolling_avg::RollingAverageTime};

#[derive(Debug, Clone)]
pub struct SyncPeer {
    peer_metadata: PeerChainMetadata,
    avg_latency: RollingAverageTime,
}

impl SyncPeer {
    pub fn node_id(&self) -> &NodeId {
        self.peer_metadata.node_id()
    }

    pub fn claimed_chain_metadata(&self) -> &ChainMetadata {
        self.peer_metadata.claimed_chain_metadata()
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
}

impl From<PeerChainMetadata> for SyncPeer {
    fn from(peer_metadata: PeerChainMetadata) -> Self {
        Self {
            peer_metadata,
            avg_latency: RollingAverageTime::new(20),
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
                .map(|d| format!("{:.2?}", d))
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
        let mut result = self
            .peer_metadata
            .claimed_chain_metadata()
            .accumulated_difficulty()
            .cmp(&other.peer_metadata.claimed_chain_metadata().accumulated_difficulty());
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
    use std::time::Duration;

    use rand::rngs::OsRng;
    use tari_common_types::chain_metadata::ChainMetadata;

    use super::*;

    mod sort_by_latency {
        use tari_common_types::types::FixedHash;
        use tari_comms::types::{CommsPublicKey, CommsSecretKey};
        use tari_crypto::keys::{PublicKey, SecretKey};

        use super::*;

        // Helper function to generate a peer with a given latency
        fn generate_peer(latency: Option<usize>) -> SyncPeer {
            let sk = CommsSecretKey::random(&mut OsRng);
            let pk = CommsPublicKey::from_secret_key(&sk);
            let node_id = NodeId::from_key(&pk);
            let latency_option = latency.map(|latency| Duration::from_millis(latency as u64));
            PeerChainMetadata::new(
                node_id,
                ChainMetadata::new(0, FixedHash::zero(), 0, 0, 1.into(), 0).unwrap(),
                latency_option,
            )
            .into()
        }

        #[test]
        fn it_sorts_by_latency() {
            const DISTINCT_LATENCY: usize = 5;

            // Generate a list of peers with latency, adding duplicates
            let mut peers = (0..2 * DISTINCT_LATENCY)
                .map(|latency| generate_peer(Some(latency % DISTINCT_LATENCY)))
                .collect::<Vec<SyncPeer>>();

            // Add peers with no latency in a few places
            peers.insert(0, generate_peer(None));
            peers.insert(DISTINCT_LATENCY, generate_peer(None));
            peers.push(generate_peer(None));

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
    }
}

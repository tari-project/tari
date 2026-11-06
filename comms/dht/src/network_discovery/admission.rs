//  Copyright 2026, The Tari Project
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

//! Per-source admission of new peer records. Join gossip and OnConnect peer sync each keep their own budget, so
//! that a Join flood relayed by honest neighbours cannot stop OnConnect between them.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tari_comms::peer_manager::NodeId;

use crate::{
    network_discovery::{NetworkDiscoveryConfig, NetworkDiscoveryError, discovering::MAX_HOSTILE_RELAYED_CLAIMS},
    peer_validator::DhtPeerValidatorError,
};

/// A budget shared by the concurrent tasks of one path (the DHT handler's Join gossip, or network discovery).
pub(crate) type SharedSourceBudget = Arc<Mutex<SourceBudget>>;

/// When each directly connected source last had new peer records stored, within `source_budget_window`. Identities
/// are free, so without this one source could fill the peer DB with fresh records and churn honest ones out.
#[derive(Debug, Default)]
pub(crate) struct SourceBudget {
    stored_at: HashMap<NodeId, VecDeque<Instant>>,
    /// The new peers charged within the window, so that a peer delivered by several sources at once is charged once
    charged_peers: HashMap<NodeId, Instant>,
    last_sweep: Option<Instant>,
}

impl SourceBudget {
    /// Returns true if `source` may still have a new peer record stored at `now`. Charges nothing.
    pub fn has_headroom(&self, source: &NodeId, now: Instant, config: &NetworkDiscoveryConfig) -> bool {
        let window = config.source_budget_window;
        let in_window = self.stored_at.get(source).map_or(0, |times| {
            times
                .iter()
                .filter(|at| now.saturating_duration_since(**at) < window)
                .count()
        });
        in_window < config.max_stored_per_source
    }

    /// Charges `source` for storing the new peer `peer` at `now`. Returns false, and charges nothing, if `source` has
    /// already had `max_stored_per_source` records stored within the last `source_budget_window`. A peer already
    /// charged within the window (delivered by another source at the same time) is admitted without a charge.
    pub fn try_charge(
        &mut self,
        source: &NodeId,
        peer: &NodeId,
        now: Instant,
        config: &NetworkDiscoveryConfig,
    ) -> bool {
        let window = config.source_budget_window;
        self.sweep(now, window);
        if self
            .charged_peers
            .get(peer)
            .is_some_and(|at| now.saturating_duration_since(*at) < window)
        {
            return true;
        }
        let times = self.stored_at.entry(source.clone()).or_default();
        while times
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= window)
        {
            times.pop_front();
        }
        if times.len() >= config.max_stored_per_source {
            return false;
        }
        times.push_back(now);
        self.charged_peers.insert(peer.clone(), now);
        true
    }

    /// Forgets expired charges at most once per window, which keeps both maps to the charges of about two windows.
    fn sweep(&mut self, now: Instant, window: Duration) {
        if self
            .last_sweep
            .is_some_and(|at| now.saturating_duration_since(at) < window)
        {
            return;
        }
        self.last_sweep = Some(now);
        let fresh = |at: &Instant| now.saturating_duration_since(*at) < window;
        self.stored_at.retain(|_, times| {
            times.retain(fresh);
            !times.is_empty()
        });
        self.charged_peers.retain(|_, at| fresh(at));
    }
}

/// Counts the claims a source relayed that only a broken or hostile node could produce.
#[derive(Debug, Default)]
pub(super) struct HostileClaims {
    count: usize,
}

impl HostileClaims {
    /// Records a relayed claim that failed validation. Fails with `TooManyInvalidPeersReceived`, a ban offence, once
    /// more than `MAX_HOSTILE_RELAYED_CLAIMS` claims with an invalid identity signature have been recorded. Other
    /// failures (no usable addresses, over this node's limits) are not counted.
    pub fn record(&mut self, err: &DhtPeerValidatorError) -> Result<(), NetworkDiscoveryError> {
        if err.is_ban_offence() {
            self.count = self.count.saturating_add(1);
            if self.count > MAX_HOSTILE_RELAYED_CLAIMS {
                return Err(NetworkDiscoveryError::TooManyInvalidPeersReceived);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tari_comms::peer_validator::PeerValidatorError;

    use super::*;
    use crate::test_utils::make_node_identity;

    fn node_id() -> NodeId {
        NodeId::from_public_key(make_node_identity().public_key())
    }

    #[test]
    fn a_source_gets_its_own_budget_that_frees_up_as_the_window_slides() {
        let config = NetworkDiscoveryConfig::default();
        let mut budget = SourceBudget::default();
        let source = node_id();
        let other = node_id();
        let start = Instant::now();

        for i in 0..50u64 {
            assert!(budget.has_headroom(&source, start, &config));
            assert!(budget.try_charge(&source, &node_id(), start + Duration::from_secs(i), &config));
        }
        let later = start + Duration::from_secs(60);
        assert!(!budget.has_headroom(&source, later, &config));
        assert!(!budget.try_charge(&source, &node_id(), later, &config));
        assert!(budget.try_charge(&other, &node_id(), later, &config));

        // The first charge leaves the window, freeing exactly one slot
        let window_end = start + config.source_budget_window;
        assert!(budget.has_headroom(&source, window_end, &config));
        assert!(budget.try_charge(&source, &node_id(), window_end, &config));
        assert!(!budget.try_charge(&source, &node_id(), window_end, &config));
        assert!(budget.try_charge(&source, &node_id(), window_end + Duration::from_secs(1), &config));
    }

    #[test]
    fn a_peer_delivered_by_several_sources_at_once_is_charged_once() {
        let config = NetworkDiscoveryConfig {
            max_stored_per_source: 1,
            ..Default::default()
        };
        let mut budget = SourceBudget::default();
        let (first, second) = (node_id(), node_id());
        let peer = node_id();
        let now = Instant::now();

        assert!(budget.try_charge(&first, &peer, now, &config));
        assert!(budget.try_charge(&second, &peer, now, &config));
        // The second source was not charged
        assert!(budget.try_charge(&second, &node_id(), now, &config));
        assert!(!budget.try_charge(&first, &node_id(), now, &config));
    }

    #[test]
    fn expired_charges_are_swept() {
        let config = NetworkDiscoveryConfig::default();
        let mut budget = SourceBudget::default();
        let start = Instant::now();
        for _ in 0..10 {
            budget.try_charge(&node_id(), &node_id(), start, &config);
        }
        let later = start + config.source_budget_window;
        budget.try_charge(&node_id(), &node_id(), later, &config);
        assert_eq!(budget.stored_at.len(), 1);
        assert_eq!(budget.charged_peers.len(), 1);
    }

    #[test]
    fn only_invalid_signatures_count_as_hostile() {
        let mut hostile = HostileClaims::default();
        let unusable = DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoUsableAddresses {
            peer: NodeId::default(),
        });
        for _ in 0..10 {
            hostile.record(&unusable).unwrap();
        }
        let forged = DhtPeerValidatorError::ValidatorError(PeerValidatorError::InvalidPeerSignature {
            peer: NodeId::default(),
        });
        for _ in 0..MAX_HOSTILE_RELAYED_CLAIMS {
            hostile.record(&forged).unwrap();
        }
        assert!(matches!(
            hostile.record(&forged),
            Err(NetworkDiscoveryError::TooManyInvalidPeersReceived)
        ));
    }
}

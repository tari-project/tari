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

//! Per-source admission of new peer records, shared by Join gossip and OnConnect peer sync.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Instant,
};

use tari_comms::peer_manager::NodeId;

use crate::{
    network_discovery::{NetworkDiscoveryConfig, NetworkDiscoveryError, discovering::MAX_HOSTILE_RELAYED_CLAIMS},
    peer_validator::DhtPeerValidatorError,
};

/// The budget shared by the DHT handler (Join gossip) and network discovery (OnConnect).
pub(crate) type SharedSourceBudget = Arc<Mutex<SourceBudget>>;

/// When each directly connected source last had new peer records stored, within `source_budget_window`. Identities
/// are free, so without this one source could fill the peer DB with fresh records and churn honest ones out.
#[derive(Debug, Default)]
pub(crate) struct SourceBudget {
    stored_at: HashMap<NodeId, VecDeque<Instant>>,
}

impl SourceBudget {
    /// Charges `source` one new peer record at `now`. Returns false, and charges nothing, if `source` has already had
    /// `max_stored_per_source` records stored within the last `source_budget_window`.
    pub fn try_charge(&mut self, source: &NodeId, now: Instant, config: &NetworkDiscoveryConfig) -> bool {
        let window = config.source_budget_window;
        self.stored_at.retain(|_, times| {
            while times
                .front()
                .is_some_and(|at| now.saturating_duration_since(*at) >= window)
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        let times = self.stored_at.entry(source.clone()).or_default();
        if times.len() >= config.max_stored_per_source {
            return false;
        }
        times.push_back(now);
        true
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

    #[test]
    fn a_source_gets_its_own_budget_that_frees_up_as_the_window_slides() {
        let config = NetworkDiscoveryConfig::default();
        let mut budget = SourceBudget::default();
        let source = NodeId::from_public_key(make_node_identity().public_key());
        let other = NodeId::from_public_key(make_node_identity().public_key());
        let start = Instant::now();

        for i in 0..50u64 {
            assert!(budget.try_charge(&source, start + Duration::from_secs(i), &config));
        }
        let later = start + Duration::from_secs(60);
        assert!(!budget.try_charge(&source, later, &config));
        assert!(budget.try_charge(&other, later, &config));

        // The first charge leaves the window, freeing exactly one slot
        let window_end = start + config.source_budget_window;
        assert!(budget.try_charge(&source, window_end, &config));
        assert!(!budget.try_charge(&source, window_end, &config));
        assert!(budget.try_charge(&source, window_end + Duration::from_secs(1), &config));
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

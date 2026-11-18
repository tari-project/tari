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

use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use tari_comms::types::CommsPublicKey;

/// What to do with a valid Join
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JoinAdmission {
    Propagate,
    /// Its claim is no newer than one already stored or propagated
    Stale,
    /// A Join from the same origin was propagated within `join_cooldown_interval`
    Cooldown,
}

#[derive(Debug)]
struct Propagated {
    at: Instant,
    seq: u64,
    claim_updated_at: DateTime<Utc>,
}

/// The Join origins this node re-propagated, when, and the claim each carried. An origin is relayed at most once per
/// `join_cooldown_interval`, and only with a claim newer than any it was relayed with before. Entries outlive the
/// cooldown so that an old Join cannot be replayed once it has passed; only capacity eviction drops them, the least
/// recently propagated first. Every entry has the same cooldown, so that is always one whose cooldown has passed, if
/// there is one.
#[derive(Debug, Default)]
pub(super) struct JoinCooldown {
    propagated: HashMap<CommsPublicKey, Propagated>,
    /// The origins in the order they were last propagated. The sequence number orders propagations at the same
    /// instant.
    by_age: BTreeMap<(Instant, u64), CommsPublicKey>,
    next_seq: u64,
}

impl JoinCooldown {
    /// Decides whether to propagate a Join from `origin` whose newest claim is `incoming`, given the newest claim
    /// already stored for it, and records a propagation. Past `capacity` origins, the least recently propagated is
    /// forgotten.
    pub fn admit(
        &mut self,
        origin: &CommsPublicKey,
        incoming: Option<DateTime<Utc>>,
        stored: Option<DateTime<Utc>>,
        now: Instant,
        interval: Duration,
        capacity: usize,
    ) -> JoinAdmission {
        let entry = self.propagated.get(origin);
        let known = stored.max(entry.map(|entry| entry.claim_updated_at));
        let Some(incoming) = incoming.filter(|incoming| known.is_none_or(|known| *incoming > known)) else {
            return JoinAdmission::Stale;
        };
        if entry.is_some_and(|entry| now.saturating_duration_since(entry.at) < interval) {
            return JoinAdmission::Cooldown;
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        if let Some(previous) = self.propagated.insert(origin.clone(), Propagated {
            at: now,
            seq,
            claim_updated_at: incoming,
        }) {
            self.by_age.remove(&(previous.at, previous.seq));
        }
        self.by_age.insert((now, seq), origin.clone());
        while self.propagated.len() > capacity {
            let Some((_, oldest)) = self.by_age.pop_first() else {
                break;
            };
            self.propagated.remove(&oldest);
        }
        JoinAdmission::Propagate
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use super::*;
    use crate::test_utils::make_node_identity;

    const INTERVAL: Duration = Duration::from_secs(600);

    fn secs(n: i64) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(n, 0)
    }

    #[test]
    fn a_newer_claim_is_propagated_once_per_interval_and_a_replay_never() {
        let mut gate = JoinCooldown::default();
        let origin = make_node_identity().public_key().clone();
        let now = Instant::now();
        let admit =
            |gate: &mut JoinCooldown, incoming, stored, at| gate.admit(&origin, incoming, stored, at, INTERVAL, 10);

        assert_eq!(admit(&mut gate, secs(10), None, now), JoinAdmission::Propagate);
        assert_eq!(admit(&mut gate, secs(10), None, now), JoinAdmission::Stale);
        // Not newer than what is stored
        assert_eq!(admit(&mut gate, secs(11), secs(11), now), JoinAdmission::Stale);
        assert_eq!(admit(&mut gate, secs(11), None, now), JoinAdmission::Cooldown);

        let later = now + INTERVAL;
        // The cooldown has passed, but the last propagated claim is remembered
        assert_eq!(admit(&mut gate, secs(10), None, later), JoinAdmission::Stale);
        assert_eq!(admit(&mut gate, secs(11), None, later), JoinAdmission::Propagate);
        assert_eq!(admit(&mut gate, None, None, later + INTERVAL), JoinAdmission::Stale);
    }

    #[test]
    fn the_least_recently_propagated_origin_is_forgotten_past_capacity() {
        let mut gate = JoinCooldown::default();
        let origins = (0..5)
            .map(|_| make_node_identity().public_key().clone())
            .collect::<Vec<_>>();
        let start = Instant::now();
        for (i, origin) in (0u64..).zip(&origins) {
            gate.admit(origin, secs(10), None, start + Duration::from_secs(i), INTERVAL, 3);
        }
        assert_eq!(gate.propagated.len(), 3);
        assert_eq!(gate.by_age.len(), 3);
        assert!(!gate.propagated.contains_key(&origins[0]));
        assert!(!gate.propagated.contains_key(&origins[1]));
        assert!(origins[2..].iter().all(|origin| gate.propagated.contains_key(origin)));
    }

    /// Past capacity an origin whose cooldown has passed goes before one still in its cooldown, even if the latter
    /// was added first
    #[test]
    fn an_expired_origin_is_forgotten_before_a_live_one() {
        let mut gate = JoinCooldown::default();
        let origins = (0..3)
            .map(|_| make_node_identity().public_key().clone())
            .collect::<Vec<_>>();
        let start = Instant::now();
        gate.admit(&origins[0], secs(10), None, start, INTERVAL, 2);
        gate.admit(&origins[1], secs(10), None, start + Duration::from_secs(1), INTERVAL, 2);
        // origins[0] is propagated again after its cooldown: it is now the newest, and origins[1] has expired too
        let later = start + INTERVAL + Duration::from_secs(1);
        assert_eq!(
            gate.admit(&origins[0], secs(11), None, later, INTERVAL, 2),
            JoinAdmission::Propagate
        );
        gate.admit(&origins[2], secs(10), None, later, INTERVAL, 2);

        assert!(!gate.propagated.contains_key(&origins[1]));
        assert!(gate.propagated.contains_key(&origins[0]));
        assert!(gate.propagated.contains_key(&origins[2]));
        assert_eq!(gate.by_age.len(), 2);
    }
}

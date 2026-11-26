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
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
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

/// The origins whose Discovery requests this node answered and when, plus origins currently being processed. An origin
/// is answered at most once per `discovery_cooldown_interval`, even when its identity claim has not changed: a known
/// peer cannot force repeated peer-DB writes or outbound Discovery responses by streaming Discovery requests through
/// relays (including concurrent bursts before the first DB lookup completes), while an honest peer rediscovering this
/// node with an unchanged claim is answered again once the cooldown passes. Past `capacity` origins, the least
/// recently answered is forgotten first.
#[derive(Debug, Default)]
pub(super) struct DiscoveryCooldown {
    answered: HashMap<CommsPublicKey, (Instant, u64)>,
    /// The origins in the order they were last answered. The sequence number orders answers at the same instant.
    by_age: BTreeMap<(Instant, u64), CommsPublicKey>,
    /// Origins with an inbound Discovery currently in flight between the early gate and completion/rejection.
    in_flight: HashSet<CommsPublicKey>,
    next_seq: u64,
}

/// Per-origin in-progress reservation held while validating and storing an inbound Discovery. Dropping it without
/// calling [`Self::commit`] releases the in-flight reservation without starting a cooldown.
pub(super) struct DiscoveryLease {
    gate: Arc<Mutex<DiscoveryCooldown>>,
    origin: Option<CommsPublicKey>,
}

impl DiscoveryLease {
    /// Releases the in-flight reservation and records an answered Discovery cooldown for the origin.
    pub fn commit(mut self, now: Instant, capacity: usize) {
        let Some(origin) = self.origin.take() else {
            return;
        };
        let mut gate = match self.gate.lock() {
            Ok(gate) => gate,
            Err(poisoned) => poisoned.into_inner(),
        };
        gate.in_flight.remove(&origin);
        gate.record_answer(&origin, now, capacity);
    }
}

impl Drop for DiscoveryLease {
    fn drop(&mut self) {
        if let Some(origin) = self.origin.take() {
            let mut gate = match self.gate.lock() {
                Ok(gate) => gate,
                Err(poisoned) => poisoned.into_inner(),
            };
            gate.in_flight.remove(&origin);
        }
    }
}

impl DiscoveryCooldown {
    /// Reserves `origin` for processing unless it is currently in cooldown or already has a Discovery in flight.
    pub fn try_begin(
        gate: &Arc<Mutex<Self>>,
        origin: &CommsPublicKey,
        now: Instant,
        interval: Duration,
    ) -> Option<DiscoveryLease> {
        let Ok(mut guard) = gate.lock() else {
            return None;
        };
        if guard.in_cooldown(origin, now, interval) || !guard.in_flight.insert(origin.clone()) {
            return None;
        }
        drop(guard);
        Some(DiscoveryLease {
            gate: Arc::clone(gate),
            origin: Some(origin.clone()),
        })
    }

    /// Returns true if a Discovery from `origin` was answered within `interval` of `now`.
    pub fn in_cooldown(&self, origin: &CommsPublicKey, now: Instant, interval: Duration) -> bool {
        self.answered
            .get(origin)
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) < interval)
    }

    /// Records a Discovery answer for `origin` at `now`. Past `capacity` origins, the least recently answered is
    /// forgotten.
    pub fn record_answer(&mut self, origin: &CommsPublicKey, now: Instant, capacity: usize) {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        if let Some(previous) = self.answered.insert(origin.clone(), (now, seq)) {
            self.by_age.remove(&previous);
        }
        self.by_age.insert((now, seq), origin.clone());
        while self.answered.len() > capacity {
            let Some((_, oldest)) = self.by_age.pop_first() else {
                break;
            };
            self.answered.remove(&oldest);
        }
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

    #[test]
    fn an_origin_is_answered_once_per_interval_and_again_after_expiry() {
        let gate = Arc::new(Mutex::new(DiscoveryCooldown::default()));
        let origin = make_node_identity().public_key().clone();
        let other = make_node_identity().public_key().clone();
        let interval = Duration::from_secs(60);
        let start = Instant::now();

        assert!(!gate.lock().unwrap().in_cooldown(&origin, start, interval));
        DiscoveryCooldown::try_begin(&gate, &origin, start, interval)
            .expect("first discovery admitted")
            .commit(start, 10);
        assert!(gate.lock().unwrap().in_cooldown(&origin, start, interval));
        let within = start + Duration::from_secs(59);
        assert!(gate.lock().unwrap().in_cooldown(&origin, within, interval));
        assert!(DiscoveryCooldown::try_begin(&gate, &origin, within, interval).is_none());
        DiscoveryCooldown::try_begin(&gate, &other, start + Duration::from_secs(1), interval)
            .expect("other origin admitted")
            .commit(start + Duration::from_secs(1), 10);

        let expired = start + interval;
        assert!(!gate.lock().unwrap().in_cooldown(&origin, expired, interval));
        DiscoveryCooldown::try_begin(&gate, &origin, expired, interval)
            .expect("admitted after expiry")
            .commit(expired, 10);
    }

    #[test]
    fn discovery_cooldown_forgets_least_recently_answered_origin_past_capacity() {
        let mut gate = DiscoveryCooldown::default();
        let origins = (0..3)
            .map(|_| make_node_identity().public_key().clone())
            .collect::<Vec<_>>();
        let interval = Duration::from_secs(60);
        let start = Instant::now();

        gate.record_answer(&origins[0], start, 2);
        gate.record_answer(&origins[1], start + Duration::from_secs(1), 2);
        let later = start + interval + Duration::from_secs(1);
        gate.record_answer(&origins[0], later, 2);
        gate.record_answer(&origins[2], later, 2);

        assert!(!gate.answered.contains_key(&origins[1]));
        assert!(gate.answered.contains_key(&origins[0]));
        assert!(gate.answered.contains_key(&origins[2]));
        assert_eq!(gate.answered.len(), 2);
        assert_eq!(gate.by_age.len(), 2);
    }

    #[test]
    fn in_flight_guard_blocks_concurrent_requests_and_releases_without_cooldown_on_drop() {
        let gate = Arc::new(Mutex::new(DiscoveryCooldown::default()));
        let origin = make_node_identity().public_key().clone();
        let interval = Duration::from_secs(60);
        let start = Instant::now();

        let lease = DiscoveryCooldown::try_begin(&gate, &origin, start, interval).expect("first request admitted");
        assert!(DiscoveryCooldown::try_begin(&gate, &origin, start, interval).is_none());

        // Dropping without commit releases the in-flight reservation without starting a cooldown
        drop(lease);
        assert!(!gate.lock().unwrap().in_cooldown(&origin, start, interval));

        let lease = DiscoveryCooldown::try_begin(&gate, &origin, start, interval).expect("re-admitted after drop");
        lease.commit(start, 10);
        assert!(DiscoveryCooldown::try_begin(&gate, &origin, start, interval).is_none());
        DiscoveryCooldown::try_begin(&gate, &origin, start + interval, interval)
            .expect("re-admitted after expiry")
            .commit(start + interval, 10);
    }
}

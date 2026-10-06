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

//! Fixed bounds on the number of inbound gossip messages (transactions or blocks) that a service has accepted but not
//! yet finished handling, per source peer and in total. Each accepted message holds a [PendingGuard] for as long as
//! its handling task runs. A peer at its own cap has further messages dropped (and logged at a limited rate) instead of
//! queueing without limit, without affecting other peers; the total cap is a backstop that is only reached if many
//! peers flood at once. Dropping never bans: an honest but busy peer can hit a cap too.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use log::*;
use tari_comms::peer_manager::NodeId;

/// How often, at most, a summary of dropped messages is logged
const DROP_LOG_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Default)]
struct PendingCounts {
    by_peer: HashMap<NodeId, usize>,
    total: usize,
}

fn lock(counts: &Mutex<PendingCounts>) -> MutexGuard<'_, PendingCounts> {
    // The counts are always left consistent, so a poisoned lock is still usable
    counts.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct PendingByPeer {
    counts: Arc<Mutex<PendingCounts>>,
    max_per_peer: usize,
    max_total: usize,
    kind: &'static str,
    log_target: &'static str,
    dropped_since_log: u64,
    last_logged: Option<Instant>,
}

/// Held by the task handling one accepted message; releases the message's slot when dropped (including when the task
/// fails or panics).
#[must_use]
pub(crate) struct PendingGuard {
    counts: Arc<Mutex<PendingCounts>>,
    peer: NodeId,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        let mut counts = lock(&self.counts);
        counts.total = counts.total.saturating_sub(1);
        if let Some(count) = counts.by_peer.get_mut(&self.peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.by_peer.remove(&self.peer);
            }
        }
    }
}

impl PendingByPeer {
    pub fn new(max_per_peer: usize, max_total: usize, kind: &'static str, log_target: &'static str) -> Self {
        Self {
            counts: Arc::new(Mutex::new(PendingCounts::default())),
            max_per_peer,
            max_total,
            kind,
            log_target,
            dropped_since_log: 0,
            last_logged: None,
        }
    }

    /// Try to accept an inbound message from `peer`. Returns a guard to be held by the task handling the message, or
    /// `None` if `peer` (or every peer together) already has too many messages pending, in which case the message must
    /// be dropped.
    pub fn try_accept(&mut self, peer: &NodeId) -> Option<PendingGuard> {
        {
            let mut counts = lock(&self.counts);
            let peer_count = counts.by_peer.get(peer).copied().unwrap_or(0);
            if peer_count < self.max_per_peer && counts.total < self.max_total {
                counts.by_peer.insert(peer.clone(), peer_count.saturating_add(1));
                counts.total = counts.total.saturating_add(1);
                return Some(PendingGuard {
                    counts: self.counts.clone(),
                    peer: peer.clone(),
                });
            }
        }
        self.dropped_since_log = self.dropped_since_log.saturating_add(1);
        if self.last_logged.is_none_or(|t| t.elapsed() >= DROP_LOG_INTERVAL) {
            debug!(
                target: self.log_target,
                "Too many inbound {} messages pending (at most {} per peer, {} in total); dropped {} message(s) since \
                 the last report, most recently from peer {}",
                self.kind,
                self.max_per_peer,
                self.max_total,
                self.dropped_since_log,
                peer
            );
            self.dropped_since_log = 0;
            self.last_logged = Some(Instant::now());
        }
        None
    }

    /// The number of messages pending from `peer`
    #[cfg(test)]
    pub fn pending_for(&self, peer: &NodeId) -> usize {
        lock(&self.counts).by_peer.get(peer).copied().unwrap_or(0)
    }

    /// The number of messages pending from all peers
    #[cfg(test)]
    pub fn pending_total(&self) -> usize {
        lock(&self.counts).total
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn node_id(n: u8) -> NodeId {
        NodeId::from_key(&vec![n; 32])
    }

    #[test]
    fn a_peer_at_its_cap_does_not_block_other_peers() {
        let mut pending = PendingByPeer::new(2, 10, "test", "test");
        let (a, b) = (node_id(1), node_id(2));
        let a1 = pending.try_accept(&a).unwrap();
        let a2 = pending.try_accept(&a).unwrap();
        // Peer a is at its cap: its next message is dropped ...
        assert!(pending.try_accept(&a).is_none());
        // ... while peer b is still accepted
        let b1 = pending.try_accept(&b).unwrap();
        assert_eq!(pending.pending_for(&a), 2);
        assert_eq!(pending.pending_for(&b), 1);
        assert_eq!(pending.pending_total(), 3);

        drop(a1);
        let a3 = pending.try_accept(&a).unwrap();
        drop((a2, a3, b1));
        assert_eq!(pending.pending_for(&a), 0);
        assert_eq!(pending.pending_for(&b), 0);
        assert_eq!(pending.pending_total(), 0);
    }

    #[test]
    fn the_total_cap_is_a_backstop() {
        let mut pending = PendingByPeer::new(2, 3, "test", "test");
        let guards = (1..=3)
            .map(|n| pending.try_accept(&node_id(n)).unwrap())
            .collect::<Vec<_>>();
        assert!(pending.try_accept(&node_id(4)).is_none());
        drop(guards);
        assert_eq!(pending.pending_total(), 0);
        assert!(pending.try_accept(&node_id(4)).is_some());
    }

    #[tokio::test]
    async fn a_panicking_task_releases_its_slot() {
        let mut pending = PendingByPeer::new(1, 10, "test", "test");
        let peer = node_id(1);
        let guard = pending.try_accept(&peer).unwrap();
        let result = tokio::spawn(async move {
            let _guard = guard;
            panic!("handler panicked");
        })
        .await;
        assert!(result.unwrap_err().is_panic());
        assert_eq!(pending.pending_for(&peer), 0);
        assert!(pending.try_accept(&peer).is_some());
    }
}

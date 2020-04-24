// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use chrono::{NaiveDateTime, Utc};
use rand::{rngs::OsRng, seq::SliceRandom};
use std::time::Duration;
use tari_comms::peer_manager::NodeId;

pub struct PeerPool {
    last_updated: Option<NaiveDateTime>,
    node_ids: Vec<NodeId>,
    stale_interval: Duration,
}

impl PeerPool {
    pub fn new(stale_interval: Duration) -> Self {
        Self {
            last_updated: None,
            node_ids: Vec::default(),
            stale_interval,
        }
    }

    pub fn len(&self) -> usize {
        self.node_ids.len()
    }

    pub fn is_stale(&self) -> bool {
        self.last_updated
            .map(|dt| {
                let chrono_dt = chrono::Duration::from_std(self.stale_interval)
                    .expect("PeerPool::stale_interval is too large (overflows chrono::Duration::from_std)");
                dt.checked_add_signed(chrono_dt)
                    .map(|dt| dt < Utc::now().naive_utc())
                    .expect("PeerPool::stale_interval is too large (overflows i32 when added to NaiveDateTime)")
            })
            .unwrap_or(true)
    }

    pub fn set_node_ids(&mut self, node_ids: Vec<NodeId>) {
        self.node_ids = node_ids;
        self.last_updated = Some(Utc::now().naive_utc());
    }

    pub fn remove(&mut self, node_id: &NodeId) -> Option<NodeId> {
        let pos = self.node_ids.iter().position(|n| n == node_id)?;
        Some(self.node_ids.remove(pos))
    }

    pub fn push(&mut self, node_id: NodeId) {
        self.node_ids.push(node_id)
    }

    pub fn node_ids(&self) -> &[NodeId] {
        &self.node_ids
    }

    pub fn sample(&self, n: usize) -> Vec<&NodeId> {
        self.node_ids.choose_multiple(&mut OsRng, n).collect()
    }

    pub fn contains(&self, node_id: &NodeId) -> bool {
        self.node_ids.iter().any(|n| n == node_id)
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::make_node_id;
    use std::iter::repeat_with;

    #[test]
    fn is_stale() {
        let mut pool = PeerPool::new(Duration::from_secs(100));
        assert_eq!(pool.is_stale(), true);
        pool.set_node_ids(vec![]);
        assert_eq!(pool.is_stale(), false);
        pool.last_updated = Some(
            Utc::now()
                .naive_utc()
                .checked_sub_signed(chrono::Duration::from_std(Duration::from_secs(101)).unwrap())
                .unwrap(),
        );
        assert_eq!(pool.is_stale(), true);
    }

    #[test]
    fn sample() {
        let mut pool = PeerPool::new(Duration::from_secs(100));
        let node_ids = repeat_with(make_node_id).take(10).collect::<Vec<_>>();
        pool.set_node_ids(node_ids.clone());
        let mut sample = pool.sample(4);
        assert_eq!(sample.len(), 4);
        node_ids.into_iter().for_each(|node_id| {
            if let Some(pos) = sample.iter().position(|n| *n == &node_id) {
                sample.remove(pos);
            }
        });
        assert_eq!(sample.len(), 0);
    }
}

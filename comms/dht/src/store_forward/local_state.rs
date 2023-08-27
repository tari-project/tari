//  Copyright 2021, The Taiji Project
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
    collections::{hash_map::Entry, HashMap},
    time::{Duration, Instant},
};

use taiji_comms::peer_manager::NodeId;

/// Keeps track of the current pending SAF requests.
#[derive(Debug, Clone, Default)]
pub(crate) struct SafLocalState {
    inflight_saf_requests: HashMap<NodeId, (usize, Instant)>,
}

impl SafLocalState {
    pub fn register_inflight_requests(&mut self, peers: &[NodeId]) {
        peers
            .iter()
            .for_each(|peer| self.register_inflight_request(peer.clone()));
    }

    pub fn register_inflight_request(&mut self, peer: NodeId) {
        match self.inflight_saf_requests.entry(peer) {
            Entry::Occupied(mut entry) => {
                let (count, _) = *entry.get();
                *entry.get_mut() = (count + 1, Instant::now());
            },
            Entry::Vacant(entry) => {
                entry.insert((1, Instant::now()));
            },
        }
    }

    pub fn mark_infight_response_received(&mut self, peer: NodeId) -> Option<Duration> {
        match self.inflight_saf_requests.entry(peer) {
            Entry::Occupied(mut entry) => {
                let (count, ts) = *entry.get();
                let reduced_count = count - 1;
                if reduced_count > 0 {
                    *entry.get_mut() = (reduced_count, ts);
                } else {
                    entry.remove();
                }
                Some(ts.elapsed())
            },
            Entry::Vacant(_) => None,
        }
    }

    pub fn garbage_collect(&mut self, older_than: Duration) {
        self.inflight_saf_requests = self
            .inflight_saf_requests
            .drain()
            .filter(|(_, (_, i))| i.elapsed() <= older_than)
            .collect();
    }
}

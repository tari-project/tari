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
use std::time::Duration;
use tari_comms::peer_manager::{NodeId, Peer};

pub struct Neighbours {
    last_updated: Option<NaiveDateTime>,
    peers: Vec<Peer>,
    stale_interval: Duration,
}

impl Neighbours {
    pub fn new(stale_interval: Duration) -> Self {
        Self {
            last_updated: None,
            peers: Vec::default(),
            stale_interval,
        }
    }

    pub fn is_fresh(&self) -> bool {
        self.last_updated
            .map(|dt| {
                let chrono_dt = chrono::Duration::from_std(self.stale_interval)
                    .expect("Neighbours::stale_interval is too large (overflows chrono::Duration::from_std)");
                dt.checked_add_signed(chrono_dt)
                    .map(|dt| dt < Utc::now().naive_utc())
                    .expect("Neighbours::stale_interval is too large (overflows i32 when added to NaiveDateTime)")
            })
            .unwrap_or(false)
    }

    pub fn set_peers(&mut self, peers: Vec<Peer>) {
        self.peers = peers;
        self.last_updated = Some(Utc::now().naive_utc());
    }

    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }

    pub fn contains(&self, node_id: &NodeId) -> bool {
        self.peers.iter().map(|p| &p.node_id).any(|n| n == node_id)
    }
}

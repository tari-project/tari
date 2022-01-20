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
    fmt::{Display, Formatter},
    time::Duration,
};

use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::peer_manager::NodeId;

use crate::{base_node::chain_metadata_service::PeerChainMetadata, common::rolling_vec::RollingVec};

#[derive(Debug, Clone)]
pub struct SyncPeer {
    peer_metadata: PeerChainMetadata,
    samples: RollingVec<Duration>,
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
        if self.samples.is_empty() {
            return None;
        }

        let total_time = self.samples.iter().sum::<Duration>();
        Some((self.samples.len() as f64 / total_time.as_micros() as f64) * 1_000_000.0)
    }

    pub(super) fn add_sample(&mut self, time: Duration) -> &mut Self {
        self.samples.push(time);
        self
    }
}

impl From<PeerChainMetadata> for SyncPeer {
    fn from(peer_metadata: PeerChainMetadata) -> Self {
        Self {
            peer_metadata,
            samples: RollingVec::new(20),
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

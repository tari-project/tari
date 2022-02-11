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

use std::time::Duration;

use tari_comms::peer_manager::NodeId;

/// Configuration for liveness service
#[derive(Debug, Clone)]
pub struct LivenessConfig {
    /// The interval to send Ping messages, or None to disable periodic pinging (default: None (disabled))
    pub auto_ping_interval: Option<Duration>,
    /// Number of peers to ping per round, excluding monitored peers (Default: 8)
    pub num_peers_per_round: usize,
    /// Peers to include in every auto ping round (Default: <empty>)
    pub monitored_peers: Vec<NodeId>,
    /// Number of ping failures to tolerate before disconnecting the peer. A value of zero disables this feature.
    pub max_allowed_ping_failures: usize,
}

impl Default for LivenessConfig {
    fn default() -> Self {
        Self {
            auto_ping_interval: None,
            num_peers_per_round: 8,
            monitored_peers: Default::default(),
            max_allowed_ping_failures: 2,
        }
    }
}

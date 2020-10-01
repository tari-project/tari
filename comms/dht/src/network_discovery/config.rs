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

use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct NetworkDiscoveryConfig {
    /// True to enable network discovery, false to disable it.
    /// Default: true
    pub enabled: bool,
    /// A threshold for the minimum number of peers this node should ideally be aware of. If below this threshold a
    /// more "aggressive" strategy is employed.
    /// Default: 50
    pub min_desired_peers: usize,
    /// The period to wait once the number of rounds given by `idle_after_num_rounds` has completed.
    /// Default: 30 mins
    pub idle_period: Duration,
    /// The minimum number of network discovery rounds to perform before idling (going to sleep). If there are less
    /// than `min_desired_peers` then the actual number of rounds performed will exceed this value. Default: 10
    pub idle_after_num_rounds: usize,
    /// Time to idle after a failed round.
    /// Default: 5 secs
    pub on_failure_idle_period: Duration,
    /// The maximum number of sync peer to select for each round. The selection strategy varies depending on the
    /// current state.
    /// Default: 5
    pub max_sync_peers: usize,
}

impl Default for NetworkDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_desired_peers: 50,
            idle_period: Duration::from_secs(30 * 60),
            idle_after_num_rounds: 10,
            on_failure_idle_period: Duration::from_secs(5),
            max_sync_peers: 5,
        }
    }
}

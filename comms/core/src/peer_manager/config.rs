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

/// Peer manager configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerManagerConfig {
    /// The most peers with addresses (not counting soft-deleted ones) kept in the peer database. Above this, garbage
    /// collection removes peers that have never been reached on any address, oldest first. Seed peers, peers with a
    /// configured address and peers with an active ban are never removed. Peers without any address (mostly rows that
    /// only record a handshake ban) are capped separately at a fifth of this, soonest-ending ban removed first.
    /// Default: 50,000
    pub max_peer_rows: usize,
    /// The most addresses taken from any one network group (IPv4 /16, IPv6 /32, an onion or DNS name) when selecting
    /// dial candidates, so that one operator cannot fill the candidate list. 0 disables the cap.
    /// Default: 4
    pub max_candidates_per_netgroup: usize,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            max_peer_rows: 50_000,
            max_candidates_per_netgroup: 4,
        }
    }
}

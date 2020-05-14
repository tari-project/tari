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

use crate::services::liveness::state::Metadata;

pub use crate::proto::liveness::{PingPong, PingPongMessage};
use rand::{rngs::OsRng, RngCore};

impl PingPongMessage {
    pub fn new(ping_pong: PingPong, nonce: u64, metadata: Metadata, useragent: String) -> Self {
        PingPongMessage {
            ping_pong: ping_pong as i32,
            nonce,
            metadata: metadata.into(),
            useragent,
        }
    }

    /// Construct a ping message with metadata
    pub fn ping_with_metadata(metadata: Metadata, useragent: String) -> Self {
        let nonce = OsRng.next_u64();
        Self::new(PingPong::Ping, nonce, metadata, useragent)
    }

    /// Construct a pong message with metadata
    pub fn pong_with_metadata(nonce: u64, metadata: Metadata, useragent: String) -> Self {
        Self::new(PingPong::Pong, nonce, metadata, useragent)
    }

    /// Return the kind of PingPong message. Either a ping or pong.
    pub fn kind(&self) -> Option<PingPong> {
        PingPong::from_i32(self.ping_pong)
    }
}

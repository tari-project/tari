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

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::fmt;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct PeerFeatures: u64 {
        const NONE = 0b0000_0000;
        const MESSAGE_PROPAGATION = 0b0000_0001;
        const DHT_STORE_FORWARD = 0b0000_0010;

        const COMMUNICATION_NODE = Self::MESSAGE_PROPAGATION.bits | Self::DHT_STORE_FORWARD.bits;
        const COMMUNICATION_CLIENT = Self::NONE.bits;
    }
}

impl PeerFeatures {
    #[inline]
    pub fn is_client(self) -> bool {
        self == PeerFeatures::COMMUNICATION_CLIENT
    }

    #[inline]
    pub fn is_node(self) -> bool {
        self == PeerFeatures::COMMUNICATION_NODE
    }
}

impl Default for PeerFeatures {
    fn default() -> Self {
        PeerFeatures::NONE
    }
}

impl fmt::Display for PeerFeatures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

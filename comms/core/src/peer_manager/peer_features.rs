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

use std::fmt;

use bitflags::bitflags;

bitflags! {
    /// Peer feature flags. These advertised the capabilities of peer nodes.
    #[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PeerFeatures: u32 {
        /// No capabilities
        const NONE = 0b0000_0000;
        /// Node is able to propagate messages
        const MESSAGE_PROPAGATION = 0b0000_0001;
        /// Node offers store and forward functionality
        const DHT_STORE_FORWARD = 0b0000_0010;

        /// Node is a communication node (typically a base layer node)
        const COMMUNICATION_NODE = Self::MESSAGE_PROPAGATION.bits() | Self::DHT_STORE_FORWARD.bits();
        /// Node is a network client
        const COMMUNICATION_CLIENT = Self::NONE.bits();
    }
}

impl PeerFeatures {
    /// Returns true if these flags represent a COMMUNICATION_CLIENT.
    #[inline]
    pub fn is_client(self) -> bool {
        self == PeerFeatures::COMMUNICATION_CLIENT
    }

    /// Returns true if these flags represent a COMMUNICATION_NODE.
    #[inline]
    pub fn is_node(self) -> bool {
        self == PeerFeatures::COMMUNICATION_NODE
    }

    /// Returns a human-readable string that represents these flags.
    pub fn as_role_str(self) -> &'static str {
        match self {
            PeerFeatures::COMMUNICATION_NODE => "node",
            PeerFeatures::COMMUNICATION_CLIENT => "client",
            _ => "unknown",
        }
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

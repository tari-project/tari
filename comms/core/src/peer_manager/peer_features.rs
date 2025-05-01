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
use serde::{Deserialize, Serialize};

bitflags! {
    /// Peer feature flags. These advertised the capabilities of peer nodes.
    #[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PeerFeatures: u8 {
        /// No capabilities
        const NONE = 0b0000_0000;
        /// Node is able to propagate messages
        const MESSAGE_PROPAGATION = 0b0000_0001;
        /// Legacy compatibility for nodes that used to offer store and forward functionality; this is no longer used,
        /// but it will change the node's public key if it is not present.
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

    /// Creates a new Option<Self> instance from a u32 value. It truncates a u32 to its least significant 8 bits and
    /// then returns 'PeerFeatures::from_bits(u8_val)'.
    pub fn from_bits_u32_truncate(value: u32) -> Option<Self> {
        PeerFeatures::from_bits(u8::try_from(value & 0b1111_1111).expect("will not fail"))
    }

    /// Returns the bits of the PeerFeatures as an i32
    pub fn to_i32(&self) -> i32 {
        i32::from_le_bytes([self.bits(), 0, 0, 0])
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

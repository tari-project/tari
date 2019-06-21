// Copyright 2019. The Tari Project
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

use serde::{Deserialize, Serialize};

/// Reduce repetitive boilerplate by defining a `is_xxx_message() -> bool` function for each class of message
macro_rules! is_type {
    ($m:ident, $f:ident) => {
        pub fn $f(&self) -> bool {
            self.0 >= $m::START_RANGE && self.0 <= $m::END_RANGE
        }
    }
}

/// A tari message type is an immutable 8-bit unsigned integer indicating the type of message being received or sent
/// over the network. Details are in
/// [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messagetype).
#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Debug)]
pub struct TariMessageType(u8);

#[allow(non_snake_case, non_upper_case_globals)]
pub mod NetMessage {
    pub(super) const START_RANGE: u8 = 1;
    pub(super) const END_RANGE: u8 = 4; // Can be extended to 32
    /// Message sent when a request to establish a peer connection has been accepted
    pub const Accept: u8 = 1;
    pub const Join: u8 = 2;
    pub const Discover: u8 = 3;
    pub const PingPong: u8 = 4;
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod PeerMessage {
    pub(super) const START_RANGE: u8 = 33;
    pub(super) const END_RANGE: u8 = 33; // Can be extended to 64
    pub const Connect: u8 = 33;
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod BlockchainMessage {
    pub(super) const START_RANGE: u8 = 65;
    pub(super) const END_RANGE: u8 = 65; // Can be extended to 96
    pub const NewBlock: u8 = 65;
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod ValidatorNodeMessage {
    pub(super) const START_RANGE: u8 = 97;
    pub(super) const END_RANGE: u8 = 97; // Can be extended to 224
    pub const Instruction: u8 = 97;
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod ExtendedMessage {
    pub(super) const START_RANGE: u8 = 225;
    pub(super) const END_RANGE: u8 = 226; // Can be extended to 255
    pub const Text: u8 = 225;
    pub const TextAck: u8 = 226;
}

impl TariMessageType {
    is_type!(NetMessage, is_net_message);

    is_type!(PeerMessage, is_peer_message);

    is_type!(BlockchainMessage, is_blockchain_message);

    is_type!(ValidatorNodeMessage, is_vn_message);

    pub fn new(value: u8) -> TariMessageType {
        TariMessageType(value)
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn is_known_message(&self) -> bool {
        self.is_net_message() || self.is_peer_message() || self.is_blockchain_message() || self.is_vn_message()
    }
}

impl From<u8> for TariMessageType {
    fn from(v: u8) -> Self {
        TariMessageType::new(v)
    }
}

#[derive(Deserialize, Serialize)]
pub struct TariMessageHeader {
    pub version: u8,
    pub message_type: TariMessageType,
}

pub struct TariMessage {
    pub header: TariMessageHeader,
    pub body: Vec<u8>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn message_type_definition() {
        // When constructing messages, we want to use human-readable definitions as defined in RFC-0172
        let t = TariMessageType::new(PeerMessage::Connect);
        assert_eq!(t.value(), 33);
        let t = TariMessageType::new(ValidatorNodeMessage::Instruction);
        assert_eq!(t.value(), 97);
        let t = TariMessageType::new(BlockchainMessage::NewBlock);
        assert_eq!(t.value(), 65);
    }

    #[test]
    fn create_message() {
        // When reading from the wire, the message type will be a byte value
        let t = TariMessageType::from(3);
        assert_eq!(t.value(), NetMessage::Discover);
        assert!(t.is_net_message());
        assert!(t.is_known_message());
    }

    #[test]
    fn unknown_message_type() {
        let t = TariMessageType::from(30);
        assert_eq!(t.is_known_message(), false);
    }
}

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

use tari_comms::{message::MessageHeader, peer_manager::Peer};
use tari_comms_dht::envelope::DhtMessageHeader;
use tari_utilities::message_format::{MessageFormat, MessageFormatError};

/// A domain-level message
pub struct PeerMessage<MType> {
    /// Serialized message data
    pub body: Vec<u8>,
    /// Domain message header
    pub message_header: MessageHeader<MType>,
    /// The message envelope header
    pub dht_header: DhtMessageHeader,
    /// The connected peer which sent this message
    pub source_peer: Peer,
}

impl<MType> PeerMessage<MType> {
    pub fn new(
        message: Vec<u8>,
        message_header: MessageHeader<MType>,
        dht_header: DhtMessageHeader,
        source_peer: Peer,
    ) -> Self
    {
        Self {
            body: message,
            message_header,
            dht_header,
            source_peer,
        }
    }

    pub fn deserialize_message<T>(&self) -> Result<T, MessageFormatError>
    where T: MessageFormat {
        T::from_binary(&self.body)
    }
}

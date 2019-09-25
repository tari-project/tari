//  Copyright 2019 The Tari Project
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

use crate::{
    message::{Frame, MessageEnvelopeHeader},
    peer_manager::Peer,
};
use serde::{Deserialize, Serialize};

/// The InboundMessage is the container that will be dispatched to the domain handlers. It contains the received
/// message and source identity after the comms level envelope has been removed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InboundMessage {
    /// The message envelope header
    pub envelope_header: MessageEnvelopeHeader,
    /// The connected peer which sent this message
    pub source_peer: Peer,
    /// The version of the incoming message envelope
    pub version: u8,
    /// The raw message envelope
    pub body: Frame,
}

impl InboundMessage {
    /// Construct a new InboundMessage that consist of the peer connection information and the received message
    /// header and body
    pub fn new(source_peer: Peer, envelope_header: MessageEnvelopeHeader, version: u8, body: Frame) -> Self {
        Self {
            source_peer,
            envelope_header,
            version,
            body,
        }
    }
}

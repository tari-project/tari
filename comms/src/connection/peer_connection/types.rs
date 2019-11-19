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

use crate::connection::{
    net_address::ip::SocketAddress,
    peer_connection::control::ThreadControlMessenger,
    PeerConnectionError,
};
use std::{sync::Arc, thread::JoinHandle};

pub type PeerConnectionJoinHandle = JoinHandle<Result<(), PeerConnectionError>>;

/// Represents messages that must be sent to a PeerConnection.
pub enum PeerConnectionProtocolMessage {
    /// Sent to establish the identity frame for a PeerConnection. This must be sent by an
    /// Outbound connection to an Inbound connection before any other communication occurs.
    Identify = 0,
    /// A peer message to be forwarded to the message sink (the IMS)
    Message = 1,
    /// Ping test
    Ping = 2,
    /// Requests to this connection are denied
    Deny = 3,
    /// Any other message is invalid and is discarded
    Invalid,
}

impl From<u8> for PeerConnectionProtocolMessage {
    fn from(val: u8) -> Self {
        match val {
            0 => PeerConnectionProtocolMessage::Identify,
            1 => PeerConnectionProtocolMessage::Message,
            2 => PeerConnectionProtocolMessage::Ping,
            _ => PeerConnectionProtocolMessage::Invalid,
        }
    }
}

pub struct ConnectionInfo {
    pub(super) control_messenger: Arc<ThreadControlMessenger>,
    pub(super) connected_address: Option<SocketAddress>,
}

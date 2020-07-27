// Copyright 2020, The Tari Project
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

use crate::{connection_manager::PeerConnectionError, peer_manager::PeerManagerError, protocol::ProtocolError};
use futures::channel::mpsc;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InboundMessagingError {
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Failed to decode message: {0}")]
    MessageDecodeError(#[from] prost::DecodeError),
}

#[derive(Debug, Error)]
pub enum MessagingProtocolError {
    #[error("Failed to send message")]
    MessageSendFailed,
    #[error("ProtocolError: {0}")]
    ProtocolError(#[from] ProtocolError),
    #[error("PeerConnectionError: {0}")]
    PeerConnectionError(#[from] PeerConnectionError),
    #[error("Failed to dial peer")]
    PeerDialFailed,
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Sender error: {0}")]
    SenderError(#[from] mpsc::SendError),
    #[error("Stream closed due to inactivity")]
    Inactivity,
}

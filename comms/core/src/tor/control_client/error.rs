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
use std::io;

use thiserror::Error;
use tokio_util::codec::LinesCodecError;

use super::parsers::ParseError;

#[derive(Debug, Error)]
pub enum TorClientError {
    #[error("Failed to read/write line to socket. The maximum line length was exceeded.")]
    MaxLineLengthExceeded,
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Command failed: {0}")]
    TorCommandFailed(String),
    #[error("Tor control port connection unexpectedly closed")]
    UnexpectedEof,
    #[error("Parse error: {0}")]
    ParseError(#[from] ParseError),
    #[error("The server returned no response")]
    ServerNoResponse,
    #[error("Server did not return a ServiceID for ADD_ONION command")]
    AddOnionNoServiceId,
    #[error("The given service id was invalid")]
    InvalidServiceId,
    #[error("Onion address is exists")]
    OnionAddressCollision,
    #[error("Response returned an no value for key")]
    KeyValueNoValue,
    #[error("The command sender disconnected")]
    CommandSenderDisconnected,
}

impl From<LinesCodecError> for TorClientError {
    fn from(err: LinesCodecError) -> Self {
        use LinesCodecError::{Io, MaxLineLengthExceeded};
        match err {
            MaxLineLengthExceeded => TorClientError::MaxLineLengthExceeded,
            Io(err) => TorClientError::Io(err),
        }
    }
}

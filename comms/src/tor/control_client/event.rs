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

use super::response::ResponseLine;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ControlEventError {
    #[error("Received an empty response")]
    EmptyResponse,
    #[error("Received invalid event data")]
    InvalidEventData,
}

#[derive(Debug, Clone)]
pub enum TorControlEvent {
    NetworkLivenessUp,
    NetworkLivenessDown,
    TorControlDisconnected,
    Unsupported(String),
}

impl TorControlEvent {
    pub fn try_from_response(resp: ResponseLine) -> Result<Self, ControlEventError> {
        debug_assert!(resp.is_event());

        let mut parts = resp.value.splitn(2, ' ');
        let event_type = parts.next().ok_or_else(|| ControlEventError::EmptyResponse)?;

        match event_type {
            "NETWORK_LIVENESS" => {
                let up_or_down = parts.next().ok_or_else(|| ControlEventError::InvalidEventData)?;

                match up_or_down.trim() {
                    "UP" => Ok(TorControlEvent::NetworkLivenessUp),
                    "DOWN" => Ok(TorControlEvent::NetworkLivenessDown),
                    _ => Err(ControlEventError::InvalidEventData),
                }
            },
            s => Ok(TorControlEvent::Unsupported(s.to_owned())),
        }
    }
}

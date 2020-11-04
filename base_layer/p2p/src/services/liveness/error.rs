// Copyright 2019 The Tari Project
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

use tari_comms::{connectivity::ConnectivityError, message::MessageError};
use tari_comms_dht::{outbound::DhtOutboundError, DhtActorError};
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LivenessError {
    #[error("DHT outbound error: `{0}`")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Connectivity error: `{0}`")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("DHT actor error: `{0}`")]
    DhtActorError(#[from] DhtActorError),
    #[error("Failed to send a pong message")]
    SendPongFailed,
    #[error("Failed to send a ping message")]
    SendPingFailed,
    #[error("Occurs when a message cannot deserialize into a PingPong message: `{0}`")]
    MessageError(#[from] MessageError),
    #[error("The Handle repsonse was not what was expected for this request")]
    UnexpectedApiResponse,
    #[error("An error has occurred reading from the event subscriber stream")]
    EventStreamError,
    #[error("Transport channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Ping pong type was invalid or unrecognised")]
    InvalidPingPongType,
    #[error("NodeId does not exist")]
    NodeIdDoesNotExist,
}

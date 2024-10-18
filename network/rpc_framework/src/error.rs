//  Copyright 2020, The Tari Project
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

use std::io;

use prost::DecodeError;
use thiserror::Error;

use super::{handshake::RpcHandshakeError, server::RpcServerError, RpcStatus};
use crate::{optional::OrOptional, proto as rpc_proto};

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("Failed to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("The client connection is closed")]
    ClientClosed,
    #[error("Request failed: {0}")]
    RequestFailed(#[from] RpcStatus),
    #[error("Remote peer unexpectedly closed the RPC connection")]
    ServerClosedRequest,
    #[error("Request cancelled")]
    RequestCancelled,
    #[error("Response did not match the request ID (expected {expected} actual {actual})")]
    ResponseIdDidNotMatchRequest { expected: u16, actual: u16 },
    #[error("Client internal error: {0}")]
    ClientInternalError(String),
    #[error("Handshake error: {0}")]
    HandshakeError(#[from] RpcHandshakeError),
    #[error("Server error: {0}")]
    ServerError(#[from] RpcServerError),
    #[error("Reply from peer timed out")]
    ReplyTimeout,
    #[error("Received an invalid ping response")]
    InvalidPingResponse,
    #[error("Unexpected ACK response. This is likely because of a previous ACK timeout")]
    UnexpectedAckResponse,
    #[error("Remote peer attempted to send more than {expected} payload chunks")]
    RemotePeerExceededMaxChunkCount { expected: usize },
    #[error("Request body was too large. Expected <= {expected} but got {got}")]
    MaxRequestSizeExceeded { got: usize, expected: usize },
}

impl RpcError {
    pub fn client_internal_error<T: ToString>(err: &T) -> Self {
        RpcError::ClientInternalError(err.to_string())
    }

    /// Returns true if the server directly caused the error, otherwise false
    pub fn is_caused_by_server(&self) -> bool {
        match self {
            RpcError::ReplyTimeout |
            RpcError::DecodeError(_) |
            RpcError::RemotePeerExceededMaxChunkCount { .. } |
            RpcError::HandshakeError(RpcHandshakeError::DecodeError(_)) |
            RpcError::HandshakeError(RpcHandshakeError::ServerClosedRequest) |
            RpcError::HandshakeError(RpcHandshakeError::Rejected(_)) |
            RpcError::HandshakeError(RpcHandshakeError::TimedOut) |
            RpcError::ServerClosedRequest |
            RpcError::UnexpectedAckResponse |
            RpcError::ResponseIdDidNotMatchRequest { .. } => true,

            // Some of these may be caused by the server, but not with 100% certainty
            RpcError::RequestFailed(_) |
            RpcError::Io(_) |
            RpcError::ClientClosed |
            RpcError::RequestCancelled |
            RpcError::ClientInternalError(_) |
            RpcError::ServerError(_) |
            RpcError::InvalidPingResponse |
            RpcError::MaxRequestSizeExceeded { .. } |
            RpcError::HandshakeError(RpcHandshakeError::Io(_)) |
            RpcError::HandshakeError(RpcHandshakeError::ClientNoSupportedVersion) |
            RpcError::HandshakeError(RpcHandshakeError::ClientClosed) => false,
        }
    }
}

#[derive(Debug, Error, Clone, Copy)]
pub enum HandshakeRejectReason {
    #[error("protocol version not supported")]
    UnsupportedVersion,
    #[error("no more RPC sessions available")]
    NoSessionsAvailable,
    #[error("protocol not supported")]
    ProtocolNotSupported,
    #[error("unknown protocol error: {0}")]
    Unknown(&'static str),
}

impl HandshakeRejectReason {
    pub fn as_i32(&self) -> i32 {
        rpc_proto::rpc_session_reply::HandshakeRejectReason::from(*self) as i32
    }

    pub fn from_i32(v: i32) -> Option<Self> {
        rpc_proto::rpc_session_reply::HandshakeRejectReason::try_from(v)
            .ok()
            .map(Into::into)
    }
}

impl From<rpc_proto::rpc_session_reply::HandshakeRejectReason> for HandshakeRejectReason {
    fn from(reason: rpc_proto::rpc_session_reply::HandshakeRejectReason) -> Self {
        #[allow(clippy::enum_glob_use)]
        use rpc_proto::rpc_session_reply::HandshakeRejectReason::*;
        match reason {
            UnsupportedVersion => HandshakeRejectReason::UnsupportedVersion,
            NoSessionsAvailable => HandshakeRejectReason::NoSessionsAvailable,
            ProtocolNotSupported => HandshakeRejectReason::ProtocolNotSupported,
            Unknown => HandshakeRejectReason::Unknown("reject reason is not known"),
        }
    }
}

impl From<HandshakeRejectReason> for rpc_proto::rpc_session_reply::HandshakeRejectReason {
    fn from(reason: HandshakeRejectReason) -> Self {
        #[allow(clippy::enum_glob_use)]
        use rpc_proto::rpc_session_reply::HandshakeRejectReason::*;
        match reason {
            HandshakeRejectReason::UnsupportedVersion => UnsupportedVersion,
            HandshakeRejectReason::NoSessionsAvailable => NoSessionsAvailable,
            HandshakeRejectReason::ProtocolNotSupported => ProtocolNotSupported,
            HandshakeRejectReason::Unknown(_) => Unknown,
        }
    }
}

impl<T> OrOptional<T> for Result<T, RpcError> {
    type Error = RpcError;

    fn or_optional(self) -> Result<Option<T>, Self::Error> {
        self.map(Some).or_else(|err| {
            if let RpcError::RequestFailed(ref status) = err {
                if status.is_not_found() {
                    Ok(None)
                } else {
                    Err(err)
                }
            } else {
                Err(err)
            }
        })
    }
}

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

use super::RpcStatus;
use crate::{
    connectivity::ConnectivityError,
    peer_manager::PeerManagerError,
    proto::rpc as rpc_proto,
    PeerConnectionError,
};
use prost::DecodeError;
use std::io;
use thiserror::Error;

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
    #[error("Maximum number of concurrent RPC sessions reached")]
    MaximumConcurrencyReached,
    #[error("Service not found for protocol `{0}`")]
    ProtocolServiceNotFound(String),
    #[error("Remote peer unexpectedly closed the RPC connection")]
    ServerClosedRequest,
    #[error("Request cancelled")]
    RequestCancelled,
    #[error("Client internal error: {0}")]
    ClientInternalError(String),
    #[error("RPC handshake timed out")]
    HandshakeTimedOut,
    #[error("RPC handshake failed: The client does not support any RPC protocol version supported by this node")]
    HandshakeClientNoSupportedVersion,
    #[error("RPC handshake was explicitly rejected: {0}")]
    HandshakeRejected(#[from] HandshakeRejectReason),
    #[error("Peer connection error: {0}")]
    PeerConnectionError(#[from] PeerConnectionError),
    #[error("Peer manager error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Connectivity error: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error(transparent)]
    UnknownError(#[from] anyhow::Error),
}

impl RpcError {
    pub fn client_internal_error<T: ToString>(err: T) -> Self {
        RpcError::ClientInternalError(err.to_string())
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
        rpc_proto::rpc_session_reply::HandshakeRejectReason::from_i32(v).map(Into::into)
    }
}

impl From<rpc_proto::rpc_session_reply::HandshakeRejectReason> for HandshakeRejectReason {
    fn from(reason: rpc_proto::rpc_session_reply::HandshakeRejectReason) -> Self {
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
        use rpc_proto::rpc_session_reply::HandshakeRejectReason::*;
        match reason {
            HandshakeRejectReason::UnsupportedVersion => UnsupportedVersion,
            HandshakeRejectReason::NoSessionsAvailable => NoSessionsAvailable,
            HandshakeRejectReason::ProtocolNotSupported => ProtocolNotSupported,
            HandshakeRejectReason::Unknown(_) => Unknown,
        }
    }
}

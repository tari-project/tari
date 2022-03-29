//  Copyright 2021, The Tari Project
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
use tokio::sync::oneshot;

use crate::{proto, protocol::rpc::handshake::RpcHandshakeError};

#[derive(Debug, thiserror::Error)]
pub enum RpcServerError {
    #[error("Failed to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Maximum number of RPC sessions reached")]
    MaximumSessionsReached,
    #[error("Internal service request canceled")]
    RequestCanceled,
    #[error("Stream was closed by remote")]
    StreamClosedByRemote,
    #[error("Handshake error: {0}")]
    HandshakeError(#[from] RpcHandshakeError),
    #[error("Service not found for protocol `{0}`")]
    ProtocolServiceNotFound(String),
    #[error("Unexpected incoming message")]
    UnexpectedIncomingMessage(proto::rpc::RpcRequest),
    #[error("Unexpected incoming MALFORMED message")]
    UnexpectedIncomingMessageMalformed,
    #[error("Client interrupted stream")]
    ClientInterruptedStream,
    #[error("Service call exceeded deadline")]
    ServiceCallExceededDeadline,
    #[error("Stream read exceeded deadline")]
    ReadStreamExceededDeadline,
}

impl From<oneshot::error::RecvError> for RpcServerError {
    fn from(_: oneshot::error::RecvError) -> Self {
        RpcServerError::RequestCanceled
    }
}

impl RpcServerError {
    pub fn to_debug_string(&self) -> String {
        use RpcServerError::*;
        match self {
            DecodeError(_) => "DecodeError".to_string(),
            Io(err) => {
                format!("Io({:?})", err.kind())
            },
            HandshakeError(_) => "HandshakeError".to_string(),
            ProtocolServiceNotFound(_) => "ProtocolServiceNotFound".to_string(),
            UnexpectedIncomingMessage(_) => "UnexpectedIncomingMessage".to_string(),
            err => {
                format!("{:?}", err)
            },
        }
    }
}

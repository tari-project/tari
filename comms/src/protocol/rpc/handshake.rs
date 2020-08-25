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

use crate::{framing::CanonicalFraming, message::MessageExt, proto, protocol::rpc::RpcError};
use futures::{AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use prost::Message;
use std::time::Duration;
use tokio::time;

const LOG_TARGET: &str = "comms::rpc::handshake";

/// Supported RPC protocol versions.
/// Currently only v0 is supported
const SUPPORTED_VERSIONS: &[u32] = &[0];

/// Handshake protocol
pub struct Handshake<'a, T> {
    framed: &'a mut CanonicalFraming<T>,
}

impl<'a, T> Handshake<'a, T>
where T: AsyncRead + AsyncWrite + Unpin
{
    pub fn new(framed: &'a mut CanonicalFraming<T>) -> Self {
        Self { framed }
    }

    /// Server-side handshake protocol
    pub async fn perform_server_handshake(&mut self) -> Result<u32, RpcError> {
        let result = time::timeout(Duration::from_secs(10), self.framed.next()).await;
        match result {
            Ok(Some(Ok(msg))) => {
                let msg = proto::rpc::RpcSession::decode(&mut msg.freeze())?;
                let version = SUPPORTED_VERSIONS.iter().find(|v| msg.supported_versions.contains(v));
                if let Some(version) = version {
                    debug!(target: LOG_TARGET, "Server accepted version {}", version);
                    let reply = proto::rpc::RpcSessionReply {
                        session_result: Some(proto::rpc::rpc_session_reply::SessionResult::AcceptedVersion(*version)),
                    };
                    self.framed.send(reply.to_encoded_bytes().into()).await?;
                    return Ok(*version);
                }

                let reply = proto::rpc::RpcSessionReply {
                    session_result: Some(proto::rpc::rpc_session_reply::SessionResult::Rejected(true)),
                };
                self.framed.send(reply.to_encoded_bytes().into()).await?;
                Err(RpcError::NegotiationClientNoSupportedVersion)
            },
            Ok(Some(Err(err))) => Err(err.into()),
            Ok(None) => Err(RpcError::ClientClosed),
            Err(_elapsed) => Err(RpcError::NegotiationTimedOut),
        }
    }

    /// Client-side handshake protocol
    pub async fn perform_client_handshake(&mut self) -> Result<(), RpcError> {
        let msg = proto::rpc::RpcSession {
            supported_versions: SUPPORTED_VERSIONS.to_vec(),
        };
        self.framed.send(msg.to_encoded_bytes().into()).await?;
        let result = time::timeout(Duration::from_secs(10), self.framed.next()).await;
        match result {
            Ok(Some(Ok(msg))) => {
                let msg = proto::rpc::RpcSessionReply::decode(&mut msg.freeze())?;
                let version = msg
                    .accepted_version()
                    .ok_or_else(|| RpcError::NegotiationServerNoSupportedVersion)?;
                debug!(target: LOG_TARGET, "Server accepted version {}", version);
                Ok(())
            },
            Ok(Some(Err(err))) => Err(err.into()),
            Ok(None) => Err(RpcError::ServerClosedRequest),
            Err(_) => Err(RpcError::NegotiationTimedOut),
        }
    }
}

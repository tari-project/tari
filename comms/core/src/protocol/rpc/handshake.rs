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

use std::{io, time::Duration};

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use prost::{DecodeError, Message};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time,
};
use tracing::{debug, error, span, warn, Instrument, Level};

use crate::{framing::CanonicalFraming, message::MessageExt, proto, protocol::rpc::error::HandshakeRejectReason};

const LOG_TARGET: &str = "comms::rpc::handshake";

/// Supported RPC protocol versions.
/// Currently only v0 is supported
pub(super) const SUPPORTED_RPC_VERSIONS: &[u32] = &[0];

#[derive(Debug, thiserror::Error)]
pub enum RpcHandshakeError {
    #[error("Failed to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("The client does not support any RPC protocol version supported by this node")]
    ClientNoSupportedVersion,
    #[error("Remote peer unexpectedly closed the RPC connection")]
    ServerClosedRequest,
    #[error("RPC handshake timed out")]
    TimedOut,
    #[error("RPC handshake was explicitly rejected: {0}")]
    Rejected(#[from] HandshakeRejectReason),
    #[error("The client connection is closed")]
    ClientClosed,
}

/// Handshake protocol
pub struct Handshake<'a, T> {
    framed: &'a mut CanonicalFraming<T>,
    timeout: Option<Duration>,
}

impl<'a, T> Handshake<'a, T>
where T: AsyncRead + AsyncWrite + Unpin
{
    /// Create a Handshake using the given framing and no timeout. To set a timeout, use `with_timeout`.
    pub fn new(framed: &'a mut CanonicalFraming<T>) -> Self {
        Self { framed, timeout: None }
    }

    /// Set the length of time that a client/server should wait for the other side to respond before timing out.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Server-side handshake protocol
    #[tracing::instrument(level="trace", name = "rpc::server::perform_server_handshake", skip(self), err, fields(comms.direction="inbound"))]
    pub async fn perform_server_handshake(&mut self) -> Result<u32, RpcHandshakeError> {
        match self.recv_next_frame().await {
            Ok(Some(Ok(msg))) => {
                let msg = proto::rpc::RpcSession::decode(&mut msg.freeze())?;
                let version = SUPPORTED_RPC_VERSIONS
                    .iter()
                    .find(|v| msg.supported_versions.contains(v));
                if let Some(version) = version {
                    debug!(target: LOG_TARGET, "Server accepted version: {}", version);
                    let reply = proto::rpc::RpcSessionReply {
                        session_result: Some(proto::rpc::rpc_session_reply::SessionResult::AcceptedVersion(*version)),
                        ..Default::default()
                    };
                    let span = span!(Level::INFO, "rpc::server::handshake::send_accept_version_reply");
                    self.framed
                        .send(reply.to_encoded_bytes().into())
                        .instrument(span)
                        .await?;
                    return Ok(*version);
                }

                let span = span!(Level::INFO, "rpc::server::handshake::send_rejection");
                self.reject_with_reason(HandshakeRejectReason::UnsupportedVersion)
                    .instrument(span)
                    .await?;
                Err(RpcHandshakeError::ClientNoSupportedVersion)
            },
            Ok(Some(Err(err))) => {
                error!(target: LOG_TARGET, "Error during handshake: {}", err);
                Err(err.into())
            },
            Ok(None) => {
                error!(target: LOG_TARGET, "Error during handshake, client closed connection");
                Err(RpcHandshakeError::ClientClosed)
            },
            Err(_) => {
                error!(target: LOG_TARGET, "Error during handshake, timed out");
                Err(RpcHandshakeError::TimedOut)
            },
        }
    }

    pub async fn reject_with_reason(&mut self, reject_reason: HandshakeRejectReason) -> Result<(), RpcHandshakeError> {
        warn!(target: LOG_TARGET, "Rejecting handshake because {}", reject_reason);
        let reply = proto::rpc::RpcSessionReply {
            session_result: Some(proto::rpc::rpc_session_reply::SessionResult::Rejected(true)),
            reject_reason: reject_reason.as_i32(),
        };
        self.framed.send(reply.to_encoded_bytes().into()).await?;
        self.framed.close().await?;
        Ok(())
    }

    /// Client-side handshake protocol
    #[tracing::instrument(name = "rpc::client::perform_client_handshake", skip(self), err, fields(comms.direction="outbound"))]
    pub async fn perform_client_handshake(&mut self) -> Result<(), RpcHandshakeError> {
        let msg = proto::rpc::RpcSession {
            supported_versions: SUPPORTED_RPC_VERSIONS.to_vec(),
        };
        let payload = msg.to_encoded_bytes();
        debug!(target: LOG_TARGET, "Sending client handshake ({} bytes)", payload.len());
        // It is possible that the server rejects the session and closes the substream before we've had a chance to send
        // anything. Rather than returning an IO error, let's ignore the send error and see if we can receive anything,
        // or return an IO error similarly to what send would have done.
        if let Err(err) = self.framed.send(payload.into()).await {
            warn!(
                target: LOG_TARGET,
                "IO error when sending new session handshake to peer: {}", err
            );
        }
        self.framed.flush().await?;
        match self.recv_next_frame().await {
            Ok(Some(Ok(msg))) => {
                let msg = proto::rpc::RpcSessionReply::decode(&mut msg.freeze())?;
                let version = msg.result()?;
                debug!(target: LOG_TARGET, "Server accepted version {}", version);
                Ok(())
            },
            Ok(Some(Err(err))) => {
                error!(target: LOG_TARGET, "Error during handshake: {}", err);
                Err(err.into())
            },
            Ok(None) => {
                error!(target: LOG_TARGET, "Error during handshake, server closed connection");
                Err(RpcHandshakeError::ServerClosedRequest)
            },
            Err(_) => {
                error!(target: LOG_TARGET, "Error during handshake, timed out");
                Err(RpcHandshakeError::TimedOut)
            },
        }
    }

    #[tracing::instrument(name = "rpc::receive_handshake_reply", skip(self), err)]
    async fn recv_next_frame(&mut self) -> Result<Option<Result<BytesMut, io::Error>>, time::error::Elapsed> {
        match self.timeout {
            Some(timeout) => time::timeout(timeout, self.framed.next()).await,
            None => Ok(self.framed.next().await),
        }
    }
}

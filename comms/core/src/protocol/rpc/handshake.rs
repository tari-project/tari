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

use std::{cmp, io, time::Duration};

use bytes::BytesMut;
use futures::{SinkExt, StreamExt};
use log::info;
use prost::{DecodeError, Message};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time,
};
use tracing::{Instrument, Level, debug, error, span, warn};

use crate::{framing::CanonicalFraming, message::MessageExt, proto, protocol::rpc::error::HandshakeRejectReason};

const LOG_TARGET: &str = "comms::rpc::handshake";

/// Supported RPC protocol versions.
/// Currently only v0 is supported
pub(super) const SUPPORTED_RPC_VERSIONS: &[u32] = &[0];

/// The largest handshake frame either side will read. A handshake carries a short list of protocol versions (or the
/// reply to one), so anything near this size is not a real handshake. The codec's frame limit is lowered to this while
/// a handshake frame is read, so a larger declared length is rejected before any buffer is reserved for it (the RPC
/// framing otherwise allows 8 MiB).
pub(super) const MAX_HANDSHAKE_FRAME_SIZE: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum RpcHandshakeError {
    #[error("Failed to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("IO Error: {0}")]
    Io(io::Error),
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
    #[error("Handshake frame was larger than the {max} byte limit")]
    FrameTooLarge { max: usize },
}

impl From<io::Error> for RpcHandshakeError {
    fn from(err: io::Error) -> Self {
        // The codec rejects a frame whose declared length is over its limit with this error, before reading it
        if err
            .get_ref()
            .is_some_and(|inner| inner.is::<tokio_util::codec::LengthDelimitedCodecError>())
        {
            return RpcHandshakeError::FrameTooLarge {
                max: MAX_HANDSHAKE_FRAME_SIZE,
            };
        }
        RpcHandshakeError::Io(err)
    }
}

fn send_timed_out() -> RpcHandshakeError {
    RpcHandshakeError::Io(io::Error::new(
        io::ErrorKind::TimedOut,
        "timed out sending a handshake frame",
    ))
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
    pub async fn perform_server_handshake(&mut self) -> Result<u32, RpcHandshakeError> {
        match self.recv_next_frame().await {
            Ok(Some(Ok(msg))) => {
                let msg = proto::rpc::RpcSession::decode(&mut msg.freeze())?;
                let version = SUPPORTED_RPC_VERSIONS
                    .iter()
                    .find(|v| msg.supported_versions.contains(v));
                if let Some(version) = version {
                    debug!(target: LOG_TARGET, "Local server accepted version: {}", version);
                    let reply = proto::rpc::RpcSessionReply {
                        session_result: Some(proto::rpc::rpc_session_reply::SessionResult::AcceptedVersion(*version)),
                        ..Default::default()
                    };
                    let span = span!(Level::INFO, "rpc::server::handshake::send_accept_version_reply");
                    self.send_bounded(reply.to_encoded_bytes().into())
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
                info!(target: LOG_TARGET, "Error during handshake: {err}");
                Err(err.into())
            },
            Ok(None) => {
                info!(target: LOG_TARGET, "Error during handshake, client closed connection");
                Err(RpcHandshakeError::ClientClosed)
            },
            Err(_) => {
                info!(target: LOG_TARGET, "Error during handshake, timed out");
                Err(RpcHandshakeError::TimedOut)
            },
        }
    }

    pub async fn reject_with_reason(&mut self, reject_reason: HandshakeRejectReason) -> Result<(), RpcHandshakeError> {
        // Debug, not warn: the server logs (and rate-limits) the reasons it rejects sessions
        debug!(target: LOG_TARGET, "Rejecting handshake because {}", reject_reason);
        let reply = proto::rpc::RpcSessionReply {
            session_result: Some(proto::rpc::rpc_session_reply::SessionResult::Rejected(true)),
            reject_reason: reject_reason.as_i32(),
        };
        self.send_bounded(reply.to_encoded_bytes().into()).await?;
        match self.timeout {
            Some(timeout) => time::timeout(timeout, self.framed.close())
                .await
                .map_err(|_| send_timed_out())??,
            None => self.framed.close().await?,
        }
        Ok(())
    }

    /// Sends a handshake frame, within the timeout if one is set. A send that does not finish in time is an IO error.
    async fn send_bounded(&mut self, frame: bytes::Bytes) -> Result<(), RpcHandshakeError> {
        match self.timeout {
            Some(timeout) => time::timeout(timeout, self.framed.send(frame))
                .await
                .map_err(|_| send_timed_out())??,
            None => self.framed.send(frame).await?,
        }
        Ok(())
    }

    /// Client-side handshake protocol
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
                debug!(target: LOG_TARGET, "Remote server accepted version {}", version);
                Ok(())
            },
            Ok(Some(Err(err))) => {
                error!(target: LOG_TARGET, "Error during handshake: {}", err);
                Err(err.into())
            },
            Ok(None) => {
                warn!(target: LOG_TARGET, "Error during handshake, server closed connection");
                Err(RpcHandshakeError::ServerClosedRequest)
            },
            Err(_) => {
                error!(target: LOG_TARGET, "Error during handshake, timed out");
                Err(RpcHandshakeError::TimedOut)
            },
        }
    }

    /// Reads the next (handshake) frame with the codec's frame limit lowered to [MAX_HANDSHAKE_FRAME_SIZE], restoring
    /// the previous limit afterwards
    async fn recv_next_frame(&mut self) -> Result<Option<Result<BytesMut, io::Error>>, time::error::Elapsed> {
        let previous_limit = self.framed.codec().max_frame_length();
        self.framed
            .codec_mut()
            .set_max_frame_length(cmp::min(previous_limit, MAX_HANDSHAKE_FRAME_SIZE));
        let result = match self.timeout {
            Some(timeout) => time::timeout(timeout, self.framed.next()).await,
            None => Ok(self.framed.next().await),
        };
        self.framed.codec_mut().set_max_frame_length(previous_limit);
        result
    }
}

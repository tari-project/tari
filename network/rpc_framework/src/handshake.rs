//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{io, time::Duration};

use bytes::BytesMut;
use futures::{AsyncRead, AsyncWrite, SinkExt, StreamExt};
use prost::{DecodeError, Message};
use tokio::time;
use tracing::{debug, error, span, warn, Instrument, Level};

use crate::{error::HandshakeRejectReason, framing::CanonicalFraming, proto};

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
    pub async fn perform_server_handshake(&mut self) -> Result<u32, RpcHandshakeError> {
        match self.recv_next_frame().await {
            Ok(Some(Ok(msg))) => {
                let msg = proto::RpcSession::decode(&mut msg.freeze())?;
                let version = SUPPORTED_RPC_VERSIONS
                    .iter()
                    .find(|v| msg.supported_versions.contains(v));
                if let Some(version) = version {
                    debug!(target: LOG_TARGET, "Server accepted version: {}", version);
                    // let reply = proto::RpcSessionReply {
                    //     session_result: Some(proto::rpc_session_reply::SessionResult::AcceptedVersion(*version)),
                    //     ..Default::default()
                    // };
                    // let span = span!(Level::INFO, "rpc::server::handshake::send_accept_version_reply");
                    // self.framed.send(reply.encode_to_vec().into()).instrument(span).await?;
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
        let reply = proto::RpcSessionReply {
            session_result: Some(proto::rpc_session_reply::SessionResult::Rejected(true)),
            reject_reason: reject_reason.as_i32(),
        };
        self.framed.send(reply.encode_to_vec().into()).await?;
        self.framed.close().await?;
        Ok(())
    }

    /// Client-side handshake protocol
    pub async fn perform_client_handshake(&mut self) -> Result<(), RpcHandshakeError> {
        let msg = proto::RpcSession {
            supported_versions: SUPPORTED_RPC_VERSIONS.to_vec(),
        };
        let payload = msg.encode_to_vec();
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
        // match self.recv_next_frame().await {
        //     Ok(Some(Ok(msg))) => {
        //         let msg = proto::RpcSessionReply::decode(&mut msg.freeze())?;
        //         let version = msg.result()?;
        //         debug!(target: LOG_TARGET, "Server accepted version {}", version);
        //         Ok(())
        //     },
        //     Ok(Some(Err(err))) => {
        //         error!(target: LOG_TARGET, "Error during handshake: {}", err);
        //         Err(err.into())
        //     },
        //     Ok(None) => {
        //         error!(target: LOG_TARGET, "Error during handshake, server closed connection");
        //         Err(RpcHandshakeError::ServerClosedRequest)
        //     },
        //     Err(_) => {
        //         error!(target: LOG_TARGET, "Error during handshake, timed out");
        //         Err(RpcHandshakeError::TimedOut)
        //     },
        // }
        Ok(())
    }

    async fn recv_next_frame(&mut self) -> Result<Option<Result<BytesMut, io::Error>>, time::error::Elapsed> {
        match self.timeout {
            Some(timeout) => time::timeout(timeout, self.framed.next()).await,
            None => Ok(self.framed.next().await),
        }
    }
}

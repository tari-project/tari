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
use std::{convert::TryFrom, io, time::Duration};

use bytes::Bytes;
use log::*;
use prost::Message;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time,
};

use crate::{
    bans::{BAN_DURATION_LONG, BAN_DURATION_SHORT},
    message::MessageExt,
    peer_manager::NodeIdentity,
    proto::identity::PeerIdentityMsg,
    protocol::{NodeNetworkInfo, ProtocolId},
};

const LOG_TARGET: &str = "comms::protocol::identity";

const MAX_IDENTITY_PROTOCOL_MSG_SIZE: u16 = 1024;

/// Perform the identity exchange protocol.
///
/// This occurs on each new connection. Identity data is sent immediately by both the initiator and responder, therefore
/// this protocol has a half RTT.
///
/// ```text
/// [initiator]   (simultaneous)   [responder]
///   |  ---------[identity]--------> |
///   |  <---------[identity]-------- |
/// ```
pub async fn identity_exchange<'p, TSocket, P>(
    node_identity: &NodeIdentity,
    our_supported_protocols: P,
    network_info: NodeNetworkInfo,
    socket: &mut TSocket,
) -> Result<PeerIdentityMsg, IdentityProtocolError>
where
    TSocket: AsyncRead + AsyncWrite + Unpin,
    P: IntoIterator<Item = &'p ProtocolId>,
{
    let supported_protocols = our_supported_protocols.into_iter().map(|p| p.to_vec()).collect();

    // Send this node's identity
    let msg_bytes = PeerIdentityMsg {
        addresses: node_identity.public_addresses().iter().map(|a| a.to_vec()).collect(),
        features: node_identity.features().bits(),
        supported_protocols,
        user_agent: network_info.user_agent,
        identity_signature: node_identity.identity_signature_read().as_ref().map(Into::into),
    }
    .to_encoded_bytes();

    write_protocol_frame(socket, network_info.major_version, &msg_bytes).await?;
    socket.flush().await?;

    // Receive the connecting node's identity
    let (_, msg_bytes) = time::timeout(
        Duration::from_secs(10),
        read_protocol_frame(socket, network_info.major_version),
    )
    .await??;

    debug!(
        target: LOG_TARGET,
        "Identity message received {} bytes",
        msg_bytes.len()
    );
    let identity_msg = PeerIdentityMsg::decode(Bytes::from(msg_bytes))?;
    Ok(identity_msg)
}

async fn read_protocol_frame<S: AsyncRead + Unpin>(
    socket: &mut S,
    max_supported_version: u8,
) -> Result<(u8, Vec<u8>), IdentityProtocolError> {
    let mut buf = [0u8; 3];
    socket.read_exact(&mut buf).await?;
    let version = buf[0];
    if version > max_supported_version {
        return Err(IdentityProtocolError::UnsupportedProtocolVersion {
            max_supported_version,
            provided_version: version,
        });
    }

    let buf = [buf[1], buf[2]];
    let len = u16::from_le_bytes(buf);
    if len > MAX_IDENTITY_PROTOCOL_MSG_SIZE {
        return Err(IdentityProtocolError::MaxMsgSizeExceeded {
            expected: MAX_IDENTITY_PROTOCOL_MSG_SIZE,
            got: len,
        });
    }

    let len = len as usize;
    let mut msg = vec![0u8; len];
    socket.read_exact(&mut msg).await?;
    Ok((version, msg))
}

async fn write_protocol_frame<S: AsyncWrite + Unpin>(
    socket: &mut S,
    version: u8,
    msg_bytes: &[u8],
) -> Result<(), IdentityProtocolError> {
    if msg_bytes.len() > MAX_IDENTITY_PROTOCOL_MSG_SIZE as usize {
        return Err(IdentityProtocolError::InvariantError(format!(
            "Sending identity protocol message of size {}, greater than {} bytes. This is a protocol violation",
            msg_bytes.len(),
            MAX_IDENTITY_PROTOCOL_MSG_SIZE
        )));
    }

    let len = u16::try_from(msg_bytes.len()).map_err(|_| {
        IdentityProtocolError::InvariantError(
            "This node attempted to send a message of size greater than u16::MAX".to_string(),
        )
    })?;
    let version_bytes = [version];
    let len_bytes = len.to_le_bytes();

    trace!(
        target: LOG_TARGET,
        "Writing {} bytes",
        len_bytes.len() + msg_bytes.len() + 1
    );
    socket.write_all(&version_bytes[..]).await?;
    socket.write_all(&len_bytes[..]).await?;
    socket.write_all(msg_bytes).await?;
    Ok(())
}

/// Error type for the identity protocol
#[derive(Debug, Error, Clone)]
pub enum IdentityProtocolError {
    #[error("IoError: {0}")]
    IoError(String),
    #[error("Possible bug: InvariantError {0}")]
    InvariantError(String),
    #[error("ProtobufDecodeError: {0}")]
    ProtobufDecodeError(String),
    #[error("Peer unexpectedly closed the connection")]
    PeerUnexpectedCloseConnection,
    #[error("Timeout waiting for peer to send identity information")]
    Timeout,
    #[error(
        "Unsupported protocol version. Max supported version: {max_supported_version}, provided version: \
         {provided_version}"
    )]
    UnsupportedProtocolVersion {
        max_supported_version: u8,
        provided_version: u8,
    },
    #[error("Max identity protocol message size exceeded. Expected <= {expected} got {got}")]
    MaxMsgSizeExceeded { expected: u16, got: u16 },
}

impl IdentityProtocolError {
    pub fn as_ban_duration(&self) -> Option<Duration> {
        match self {
            // Don't ban
            IdentityProtocolError::InvariantError(_) | IdentityProtocolError::IoError(_) => None,
            // Long bans
            IdentityProtocolError::ProtobufDecodeError(_) | IdentityProtocolError::MaxMsgSizeExceeded { .. } => {
                Some(BAN_DURATION_LONG)
            },
            // Short bans
            IdentityProtocolError::PeerUnexpectedCloseConnection |
            IdentityProtocolError::UnsupportedProtocolVersion { .. } |
            IdentityProtocolError::Timeout => Some(BAN_DURATION_SHORT),
        }
    }
}

impl From<time::error::Elapsed> for IdentityProtocolError {
    fn from(_: time::error::Elapsed) -> Self {
        IdentityProtocolError::Timeout
    }
}

impl From<io::Error> for IdentityProtocolError {
    fn from(err: io::Error) -> Self {
        IdentityProtocolError::IoError(err.to_string())
    }
}

impl From<prost::DecodeError> for IdentityProtocolError {
    fn from(err: prost::DecodeError) -> Self {
        IdentityProtocolError::ProtobufDecodeError(err.to_string())
    }
}

#[cfg(test)]
mod test {
    use futures::{future, StreamExt};

    use crate::{
        peer_manager::PeerFeatures,
        protocol::{IdentityProtocolError, NodeNetworkInfo},
        test_utils::node_identity::build_node_identity,
        transports::{MemoryTransport, Transport},
    };

    #[tokio::test]
    async fn identity_exchange() {
        let transport = MemoryTransport;
        let addr = "/memory/0".parse().unwrap();
        let (mut listener, addr) = transport.listen(&addr).await.unwrap();

        let (out_sock, in_sock) = future::join(transport.dial(&addr), listener.next()).await;

        let mut out_sock = out_sock.unwrap();
        let (mut in_sock, _) = in_sock.unwrap().unwrap();

        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);

        let (result1, result2) = future::join(
            super::identity_exchange(
                &node_identity1,
                &[],
                NodeNetworkInfo {
                    minor_version: 1,
                    ..Default::default()
                },
                &mut in_sock,
            ),
            super::identity_exchange(
                &node_identity2,
                &[],
                NodeNetworkInfo {
                    minor_version: 2,
                    ..Default::default()
                },
                &mut out_sock,
            ),
        )
        .await;

        // Test node 1 gets node 2's details and vice versa
        let identity2 = result1.unwrap();
        let identity1 = result2.unwrap();

        assert_eq!(identity1.features, node_identity1.features().bits());
        assert_eq!(
            identity1.addresses,
            node_identity1
                .public_addresses()
                .iter()
                .map(|a| a.to_vec())
                .collect::<Vec<_>>()
        );

        assert_eq!(identity2.features, node_identity2.features().bits());
        assert_eq!(
            identity2.addresses,
            node_identity2
                .public_addresses()
                .iter()
                .map(|a| a.to_vec())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn fail_cases() {
        let transport = MemoryTransport;
        let addr = "/memory/0".parse().unwrap();
        let (mut listener, addr) = transport.listen(&addr).await.unwrap();

        let (out_sock, in_sock) = future::join(transport.dial(&addr), listener.next()).await;

        let mut out_sock = out_sock.unwrap();
        let (mut in_sock, _) = in_sock.unwrap().unwrap();

        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);

        let (result1, result2) = future::join(
            super::identity_exchange(
                &node_identity1,
                &[],
                NodeNetworkInfo {
                    major_version: 0,
                    ..Default::default()
                },
                &mut in_sock,
            ),
            super::identity_exchange(
                &node_identity2,
                &[],
                NodeNetworkInfo {
                    major_version: 1,
                    ..Default::default()
                },
                &mut out_sock,
            ),
        )
        .await;

        let err = result1.unwrap_err();
        assert!(matches!(err, IdentityProtocolError::UnsupportedProtocolVersion { .. }));

        // Passes because older versions are supported
        result2.unwrap();
    }
}

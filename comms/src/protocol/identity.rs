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
use crate::{
    connection_manager::ConnectionDirection,
    message::MessageExt,
    peer_manager::NodeIdentity,
    proto::identity::PeerIdentityMsg,
    protocol::{NodeNetworkInfo, ProtocolError, ProtocolId, ProtocolNegotiation},
};
use futures::{SinkExt, StreamExt};
use log::*;
use prost::Message;
use std::{io, time::Duration};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing;

pub static IDENTITY_PROTOCOL: ProtocolId = ProtocolId::from_static(b"t/identity/1.0");
const LOG_TARGET: &str = "comms::protocol::identity";

#[tracing::instrument(skip(socket, our_supported_protocols), err)]
pub async fn identity_exchange<'p, TSocket, P>(
    node_identity: &NodeIdentity,
    direction: ConnectionDirection,
    our_supported_protocols: P,
    network_info: NodeNetworkInfo,
    mut socket: TSocket,
) -> Result<PeerIdentityMsg, IdentityProtocolError>
where
    TSocket: AsyncRead + AsyncWrite + Unpin,
    P: IntoIterator<Item = &'p ProtocolId>,
{
    // Negotiate the identity protocol
    let mut negotiation = ProtocolNegotiation::new(&mut socket);
    let proto = match direction {
        ConnectionDirection::Outbound => {
            debug!(
                target: LOG_TARGET,
                "[ThisNode={}] Starting Outbound identity exchange with peer.",
                node_identity.node_id().short_str()
            );
            negotiation
                .negotiate_protocol_outbound_optimistic(&IDENTITY_PROTOCOL.clone())
                .await?
        },
        ConnectionDirection::Inbound => {
            debug!(
                target: LOG_TARGET,
                "[ThisNode={}] Starting Inbound identity exchange with peer.",
                node_identity.node_id().short_str()
            );
            negotiation
                .negotiate_protocol_inbound(&[IDENTITY_PROTOCOL.clone()])
                .await?
        },
    };

    debug_assert_eq!(proto, IDENTITY_PROTOCOL);

    // Create length-delimited frame codec
    let framed = Framed::new(socket, LengthDelimitedCodec::new());
    let (mut sink, mut stream) = framed.split();

    let supported_protocols = our_supported_protocols.into_iter().map(|p| p.to_vec()).collect();

    // Send this node's identity
    let msg_bytes = PeerIdentityMsg {
        addresses: vec![node_identity.public_address().to_vec()],
        features: node_identity.features().bits(),
        supported_protocols,
        major: network_info.major_version,
        minor: network_info.minor_version,
        user_agent: network_info.user_agent,
    }
    .to_encoded_bytes();

    sink.send(msg_bytes.into()).await?;
    sink.close().await?;

    // Receive the connecting nodes identity
    let msg_bytes = time::timeout(Duration::from_secs(10), stream.next())
        .await?
        .ok_or(IdentityProtocolError::PeerUnexpectedCloseConnection)??;
    let identity_msg = PeerIdentityMsg::decode(msg_bytes)?;

    if identity_msg.major != network_info.major_version {
        warn!(
            target: LOG_TARGET,
            "Peer sent mismatching major protocol version '{}'. This node has version '{}.{}'",
            identity_msg.major,
            network_info.major_version,
            network_info.minor_version
        );
        return Err(IdentityProtocolError::ProtocolVersionMismatch);
    }

    Ok(identity_msg)
}

#[derive(Debug, Error, Clone)]
pub enum IdentityProtocolError {
    #[error("IoError: {0}")]
    IoError(String),
    #[error("ProtocolError: {0}")]
    ProtocolError(String),
    #[error("ProtobufDecodeError: {0}")]
    ProtobufDecodeError(String),
    #[error("Failed to encode protobuf message")]
    ProtobufEncodingError,
    #[error("Peer unexpectedly closed the connection")]
    PeerUnexpectedCloseConnection,
    #[error("Timeout waiting for peer to send identity information")]
    Timeout,
    #[error("Protocol version mismatch")]
    ProtocolVersionMismatch,
}

impl From<time::error::Elapsed> for IdentityProtocolError {
    fn from(_: time::error::Elapsed) -> Self {
        IdentityProtocolError::Timeout
    }
}

impl From<ProtocolError> for IdentityProtocolError {
    fn from(err: ProtocolError) -> Self {
        IdentityProtocolError::ProtocolError(err.to_string())
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
    use crate::{
        connection_manager::ConnectionDirection,
        peer_manager::PeerFeatures,
        protocol::{IdentityProtocolError, NodeNetworkInfo},
        runtime,
        test_utils::node_identity::build_node_identity,
        transports::{MemoryTransport, Transport},
    };
    use futures::{future, StreamExt};

    #[runtime::test]
    async fn identity_exchange() {
        let transport = MemoryTransport;
        let addr = "/memory/0".parse().unwrap();
        let (mut listener, addr) = transport.listen(addr).await.unwrap();

        let (out_sock, in_sock) = future::join(transport.dial(addr), listener.next()).await;

        let out_sock = out_sock.unwrap();
        let (in_sock, _) = in_sock.unwrap().unwrap();

        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);

        let (result1, result2) = future::join(
            super::identity_exchange(
                &node_identity1,
                ConnectionDirection::Inbound,
                &[],
                NodeNetworkInfo {
                    minor_version: 1,
                    ..Default::default()
                },
                in_sock,
            ),
            super::identity_exchange(
                &node_identity2,
                ConnectionDirection::Outbound,
                &[],
                NodeNetworkInfo {
                    minor_version: 2,
                    ..Default::default()
                },
                out_sock,
            ),
        )
        .await;

        // Test node 1 gets node 2's details and vice versa
        let identity2 = result1.unwrap();
        let identity1 = result2.unwrap();

        assert_eq!(identity1.features, node_identity1.features().bits());
        assert_eq!(identity1.addresses, vec![node_identity1.public_address().to_vec()]);

        assert_eq!(identity2.features, node_identity2.features().bits());
        assert_eq!(identity2.addresses, vec![node_identity2.public_address().to_vec()]);
    }

    #[runtime::test]
    async fn fail_cases() {
        let transport = MemoryTransport;
        let addr = "/memory/0".parse().unwrap();
        let (mut listener, addr) = transport.listen(addr).await.unwrap();

        let (out_sock, in_sock) = future::join(transport.dial(addr), listener.next()).await;

        let out_sock = out_sock.unwrap();
        let (in_sock, _) = in_sock.unwrap().unwrap();

        let node_identity1 = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let node_identity2 = build_node_identity(PeerFeatures::COMMUNICATION_CLIENT);

        let (result1, result2) = future::join(
            super::identity_exchange(
                &node_identity1,
                ConnectionDirection::Inbound,
                &[],
                NodeNetworkInfo {
                    major_version: 0,
                    ..Default::default()
                },
                in_sock,
            ),
            super::identity_exchange(
                &node_identity2,
                ConnectionDirection::Outbound,
                &[],
                NodeNetworkInfo {
                    major_version: 1,
                    ..Default::default()
                },
                out_sock,
            ),
        )
        .await;

        let err = result1.unwrap_err();
        assert!(matches!(err, IdentityProtocolError::ProtocolVersionMismatch));

        let err = result2.unwrap_err();
        assert!(matches!(err, IdentityProtocolError::ProtocolVersionMismatch));
    }
}

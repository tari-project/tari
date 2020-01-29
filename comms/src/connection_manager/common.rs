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
    connection::ConnectionDirection,
    connection_manager::error::ConnectionManagerError,
    multiplexing::Yamux,
    peer_manager::{AsyncPeerManager, NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    proto::identity::PeerIdentityMsg,
    protocol,
    types::CommsPublicKey,
};
use futures::StreamExt;
use log::*;
use std::sync::Arc;
use tari_utilities::ByteArray;

const LOG_TARGET: &str = "comms::connection_manager::common";

pub async fn perform_identity_exchange(
    muxer: &mut Yamux,
    node_identity: Arc<NodeIdentity>,
    direction: ConnectionDirection,
) -> Result<PeerIdentityMsg, ConnectionManagerError>
{
    let mut control = muxer.get_yamux_control();
    let stream = match direction {
        ConnectionDirection::Inbound => muxer
            .incoming_mut()
            .next()
            .await
            .ok_or(ConnectionManagerError::IncomingListenerStreamClosed)??,
        ConnectionDirection::Outbound => control.open_stream().await?,
    };

    debug!(target: LOG_TARGET, "{} substream opened to peer", direction);

    let peer_identity = protocol::identity_exchange(node_identity, direction, stream).await?;
    Ok(peer_identity)
}

/// Validate the node id against the given public key. Returns true if this is a valid base node
/// node id, otherwise false.
pub fn is_valid_base_node_node_id(node_id: &NodeId, public_key: &CommsPublicKey) -> bool {
    match NodeId::from_key(public_key) {
        Ok(expected_node_id) => &expected_node_id == node_id,
        Err(_) => false,
    }
}

/// Validate the peer identity info.
///
/// The following process is used to validate the peer:
/// 1. Check the offered node identity is a valid base node identity (TODO: This won't work for DAN nodes)
/// 1. Check if we know the peer, if so, is the peer banned, if so, return an error
/// 1. Check that the offered addresses are valid
/// 1. Update or add the peer, returning it's NodeId
pub async fn validate_and_add_peer_from_peer_identity(
    peer_manager: &AsyncPeerManager,
    authenticated_public_key: CommsPublicKey,
    peer_identity: PeerIdentityMsg,
) -> Result<NodeId, ConnectionManagerError>
{
    // Validate the given node id for base nodes
    // TODO: This is technically a domain-level rule
    let peer_node_id =
        NodeId::from_bytes(&peer_identity.node_id).map_err(|_| ConnectionManagerError::PeerIdentityInvalidNodeId)?;
    if !is_valid_base_node_node_id(&peer_node_id, &authenticated_public_key) {
        return Err(ConnectionManagerError::PeerIdentityInvalidNodeId);
    }

    // Check if we know the peer and if it is banned
    let maybe_peer = match peer_manager.find_by_public_key(&authenticated_public_key).await {
        Ok(peer) if peer.is_banned() => return Err(ConnectionManagerError::PeerBanned),
        Ok(peer) => Some(peer),
        Err(err) if err.is_peer_not_found() => None,
        Err(err) => return Err(err.into()),
    };

    let addresses = peer_identity
        .addresses
        .into_iter()
        .filter_map(|addr_str| addr_str.parse().ok())
        .collect::<Vec<_>>();
    if addresses.len() == 0 {
        return Err(ConnectionManagerError::PeerIdentityNoValidAddresses);
    }

    // Add or update the peer
    match maybe_peer {
        Some(peer) => {
            let mut conn_stats = peer.connection_stats;
            conn_stats.set_connection_success();
            peer_manager
                .update_peer(
                    &authenticated_public_key,
                    Some(peer_node_id.clone()),
                    Some(addresses),
                    None,
                    Some(PeerFeatures::from_bits_truncate(peer_identity.features)),
                    Some(conn_stats),
                )
                .await?;
        },
        None => {
            let mut new_peer = Peer::new(
                authenticated_public_key,
                peer_node_id.clone(),
                addresses.into(),
                PeerFlags::empty(),
                PeerFeatures::from_bits_truncate(peer_identity.features),
            );
            new_peer.connection_stats.set_connection_success();
            peer_manager.add_peer(new_peer).await?;
        },
    }

    Ok(peer_node_id)
}

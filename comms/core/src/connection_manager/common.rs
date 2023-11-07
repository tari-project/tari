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

use std::{
    convert::{TryFrom, TryInto},
    time::Duration,
};

use log::*;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    connection_manager::error::ConnectionManagerError,
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerIdentityClaim, PeerManagerError},
    peer_validator::{validate_peer_identity_claim, PeerValidatorConfig, PeerValidatorError},
    proto::identity::PeerIdentityMsg,
    protocol,
    protocol::{NodeNetworkInfo, ProtocolId},
    types::CommsPublicKey,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connection_manager::common";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPeerIdentityExchange {
    pub claim: PeerIdentityClaim,
    pub metadata: PeerIdentityMetadata,
}

impl ValidatedPeerIdentityExchange {
    // getters
    pub fn peer_features(&self) -> PeerFeatures {
        self.claim.features
    }

    pub fn supported_protocols(&self) -> &[ProtocolId] {
        &self.metadata.supported_protocols
    }

    pub fn user_agent(&self) -> &str {
        self.metadata.user_agent.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerIdentityMetadata {
    pub user_agent: String,
    pub supported_protocols: Vec<ProtocolId>,
}

/// Performs the identity exchange protocol on the given socket.
pub(super) async fn perform_identity_exchange<
    'p,
    P: IntoIterator<Item = &'p ProtocolId>,
    TSocket: AsyncRead + AsyncWrite + Unpin,
>(
    socket: &mut TSocket,
    node_identity: &NodeIdentity,
    our_supported_protocols: P,
    network_info: NodeNetworkInfo,
) -> Result<PeerIdentityMsg, ConnectionManagerError> {
    let peer_identity =
        protocol::identity_exchange(node_identity, our_supported_protocols, network_info, socket).await?;

    Ok(peer_identity)
}

/// Validate the peer identity info.
///
/// The following process is used to validate the peer:
/// 1. Check the offered node identity is a valid base node identity
/// 1. Check if we know the peer, if so, is the peer banned, if so, return an error
/// 1. Check that the offered addresses are valid
///
/// If the `allow_test_addrs` parameter is true, loopback, local link and other addresses normally not considered valid
/// for p2p comms will be accepted.
pub(super) fn validate_peer_identity_message(
    config: &PeerValidatorConfig,
    authenticated_public_key: &CommsPublicKey,
    peer_identity_msg: PeerIdentityMsg,
) -> Result<ValidatedPeerIdentityExchange, ConnectionManagerError> {
    let PeerIdentityMsg {
        addresses,
        features,
        supported_protocols,
        user_agent,
        identity_signature,
    } = peer_identity_msg;

    // Perform basic length checks before parsing
    if supported_protocols.len() > config.max_supported_protocols {
        return Err(PeerValidatorError::PeerIdentityTooManyProtocols {
            length: supported_protocols.len(),
            max: config.max_supported_protocols,
        }
        .into());
    }

    if let Some(proto) = supported_protocols
        .iter()
        .find(|p| p.len() > config.max_protocol_id_length)
    {
        return Err(PeerValidatorError::PeerIdentityProtocolIdTooLong {
            length: proto.len(),
            max: config.max_protocol_id_length,
        }
        .into());
    }

    if addresses.is_empty() {
        return Err(PeerValidatorError::PeerIdentityNoAddresses.into());
    }

    if addresses.len() > config.max_permitted_peer_addresses_per_claim {
        return Err(PeerValidatorError::PeerIdentityTooManyAddresses {
            length: addresses.len(),
            max: config.max_permitted_peer_addresses_per_claim,
        }
        .into());
    }

    if user_agent.as_bytes().len() > config.max_user_agent_byte_length {
        return Err(PeerValidatorError::PeerIdentityUserAgentTooLong {
            length: user_agent.as_bytes().len(),
            max: config.max_user_agent_byte_length,
        }
        .into());
    }

    let supported_protocols = supported_protocols.into_iter().map(ProtocolId::from).collect();

    let addresses = addresses
        .into_iter()
        .map(Multiaddr::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PeerManagerError::MultiaddrError(e.to_string()))?;

    let peer_identity_claim = PeerIdentityClaim {
        addresses,
        features: PeerFeatures::from_bits(features).ok_or(PeerManagerError::InvalidPeerFeatures { bits: features })?,
        signature: identity_signature
            .ok_or(PeerManagerError::MissingIdentitySignature)?
            .try_into()?,
    };

    validate_peer_identity_claim(config, authenticated_public_key, &peer_identity_claim)?;

    Ok(ValidatedPeerIdentityExchange {
        claim: peer_identity_claim,
        metadata: PeerIdentityMetadata {
            user_agent,
            supported_protocols,
        },
    })
}

/// Validate the peer identity info.
///
/// The following process is used to validate the peer:
/// 1. Check the offered node identity is a valid base node identity
/// 1. Check if we know the peer, if so, is the peer banned, if so, return an error
/// 1. Check that the offered addresses are valid
/// 1. Update or add the peer, returning it's NodeId
///
/// If the `allow_test_addrs` parameter is true, loopback, local link and other addresses normally not considered valid
/// for p2p comms will be accepted.
pub(super) fn create_or_update_peer_from_validated_peer_identity(
    known_peer: Option<Peer>,
    authenticated_public_key: CommsPublicKey,
    peer_identity: &ValidatedPeerIdentityExchange,
) -> Peer {
    let peer_node_id = NodeId::from_public_key(&authenticated_public_key);

    // Note: the peer will be merged in the db if it already exists
    match known_peer {
        Some(mut peer) => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' already exists in peer list. Updating.",
                peer.node_id.short_str()
            );
            peer.addresses
                .update_addresses(&peer_identity.claim.addresses, &PeerAddressSource::FromPeerConnection {
                    peer_identity_claim: peer_identity.claim.clone(),
                });

            peer.addresses
                .mark_all_addresses_as_last_seen_now(&peer_identity.claim.addresses);

            peer.features = peer_identity.claim.features;
            peer.supported_protocols = peer_identity.metadata.supported_protocols.clone();
            peer.user_agent = peer_identity.metadata.user_agent.clone();

            peer
        },
        None => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' does not exist in peer list. Adding.",
                peer_node_id.short_str()
            );
            Peer::new(
                authenticated_public_key,
                peer_node_id,
                MultiaddressesWithStats::from_addresses_with_source(
                    peer_identity.claim.addresses.clone(),
                    &PeerAddressSource::FromPeerConnection {
                        peer_identity_claim: peer_identity.claim.clone(),
                    },
                ),
                PeerFlags::empty(),
                peer_identity.peer_features(),
                peer_identity.supported_protocols().to_vec(),
                peer_identity.user_agent().to_string(),
            )
        },
    }
}

pub(super) async fn find_unbanned_peer(
    peer_manager: &PeerManager,
    authenticated_public_key: &CommsPublicKey,
) -> Result<Option<Peer>, ConnectionManagerError> {
    match peer_manager.find_by_public_key(authenticated_public_key).await {
        Ok(Some(peer)) if peer.is_banned() => Err(ConnectionManagerError::PeerBanned),
        Ok(peer) => Ok(peer),
        Err(err) => Err(err.into()),
    }
}

pub(super) async fn ban_on_offence<T>(
    peer_manager: &PeerManager,
    authenticated_public_key: &CommsPublicKey,
    result: Result<T, ConnectionManagerError>,
) -> Result<T, ConnectionManagerError> {
    match result {
        Ok(t) => Ok(t),
        Err(ConnectionManagerError::PeerValidationError(e)) => {
            maybe_ban(peer_manager, authenticated_public_key, e.as_ban_duration(), e).await
        },
        Err(ConnectionManagerError::IdentityProtocolError(e)) => {
            maybe_ban(peer_manager, authenticated_public_key, e.as_ban_duration(), e).await
        },
        Err(err) => Err(err),
    }
}

async fn maybe_ban<T, E: ToString + Into<ConnectionManagerError>>(
    peer_manager: &PeerManager,
    authenticated_public_key: &CommsPublicKey,
    ban_duration: Option<Duration>,
    err: E,
) -> Result<T, ConnectionManagerError> {
    if let Some(ban_duration) = ban_duration {
        if let Err(pe) = peer_manager
            .ban_peer(authenticated_public_key, ban_duration, err.to_string())
            .await
        {
            error!(target: LOG_TARGET, "Failed to ban peer due to internal error: {}. Original ban error: {}", pe, err.to_string());
        }
    }

    Err(err.into())
}

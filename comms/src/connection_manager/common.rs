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

use super::types::ConnectionDirection;
use crate::{
    connection_manager::error::ConnectionManagerError,
    multiaddr::{Multiaddr, Protocol},
    multiplexing::Yamux,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    proto::identity::PeerIdentityMsg,
    protocol,
    protocol::ProtocolId,
    types::CommsPublicKey,
    PeerManager,
};
use futures::StreamExt;
use log::*;
use std::sync::Arc;
use tari_crypto::tari_utilities::ByteArray;

const LOG_TARGET: &str = "comms::connection_manager::common";

pub async fn perform_identity_exchange<'p, P: IntoIterator<Item = &'p ProtocolId>>(
    muxer: &mut Yamux,
    node_identity: &NodeIdentity,
    direction: ConnectionDirection,
    our_supported_protocols: P,
) -> Result<PeerIdentityMsg, ConnectionManagerError>
{
    let mut control = muxer.get_yamux_control();
    let stream = match direction {
        ConnectionDirection::Inbound => muxer
            .incoming_mut()
            .next()
            .await
            .ok_or_else(|| ConnectionManagerError::IncomingListenerStreamClosed)?,
        ConnectionDirection::Outbound => control.open_stream().await?,
    };

    debug!(target: LOG_TARGET, "{} substream opened to peer", direction);

    let peer_identity = protocol::identity_exchange(node_identity, direction, our_supported_protocols, stream).await?;
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
///
/// If the `allow_test_addrs` parameter is true, loopback, local link and other addresses normally not considered valid
/// for p2p comms will be accepted.
pub async fn validate_and_add_peer_from_peer_identity(
    peer_manager: &PeerManager,
    authenticated_public_key: CommsPublicKey,
    peer_identity: PeerIdentityMsg,
    allow_test_addrs: bool,
) -> Result<Arc<Peer>, ConnectionManagerError>
{
    // let peer_manager = peer_manager.inner();
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

    // TODO: #banheuristic
    validate_peer_addresses(&addresses, allow_test_addrs)?;

    if addresses.is_empty() {
        return Err(ConnectionManagerError::PeerIdentityNoValidAddresses);
    }

    let supported_protocols = peer_identity
        .supported_protocols
        .into_iter()
        .map(bytes::Bytes::from)
        .collect::<Vec<_>>();

    // Add or update the peer
    match maybe_peer {
        Some(peer) => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' already exists in peer list. Updating.",
                peer.node_id.short_str()
            );
            let mut conn_stats = peer.connection_stats;
            conn_stats.set_connection_success();
            peer_manager
                .update_peer(
                    &authenticated_public_key,
                    Some(peer_node_id.clone()),
                    Some(addresses),
                    None,
                    None,
                    Some(false),
                    Some(PeerFeatures::from_bits_truncate(peer_identity.features)),
                    Some(conn_stats),
                    Some(supported_protocols),
                )
                .await?;
        },
        None => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' does not exist in peer list. Adding.",
                peer_node_id.short_str()
            );
            let mut new_peer = Peer::new(
                authenticated_public_key.clone(),
                peer_node_id.clone(),
                addresses.into(),
                PeerFlags::empty(),
                PeerFeatures::from_bits_truncate(peer_identity.features),
                &supported_protocols,
            );
            new_peer.connection_stats.set_connection_success();
            peer_manager.add_peer(new_peer).await?;
        },
    }

    let peer = Arc::new(peer_manager.find_by_node_id(&peer_node_id).await?);

    Ok(peer)
}

pub fn validate_peer_addresses<A: AsRef<[Multiaddr]>>(
    addresses: A,
    allow_test_addrs: bool,
) -> Result<(), ConnectionManagerError>
{
    for addr in addresses.as_ref() {
        validate_address(addr, allow_test_addrs)?;
    }
    Ok(())
}

pub fn validate_address(addr: &Multiaddr, allow_test_addrs: bool) -> Result<(), ConnectionManagerError> {
    let mut addr_iter = addr.iter();
    let proto = addr_iter
        .next()
        .ok_or_else(|| ConnectionManagerError::InvalidMultiaddr("Multiaddr was empty".to_string()))?;

    let expect_end_of_address = |mut iter: multiaddr::Iter<'_>| match iter.next() {
        Some(p) => Err(ConnectionManagerError::InvalidMultiaddr(format!(
            "Unexpected multiaddress component '{}'",
            p
        ))),
        None => Ok(()),
    };

    match proto {
        Protocol::Dns4(_) | Protocol::Dns6(_) | Protocol::Dnsaddr(_) => {
            let tcp = addr_iter.next().ok_or_else(|| {
                ConnectionManagerError::InvalidMultiaddr("Address does not include a TCP port".to_string())
            })?;

            validate_tcp_port(tcp)?;

            expect_end_of_address(addr_iter)
        },

        Protocol::Ip4(addr)
            if !allow_test_addrs && (addr.is_loopback() || addr.is_link_local() || addr.is_unspecified()) =>
        {
            Err(ConnectionManagerError::InvalidMultiaddr(
                "Non-global IP addresses are invalid".to_string(),
            ))
        },
        Protocol::Ip6(addr)
            if !allow_test_addrs && (addr.is_loopback() || addr.is_unicast_link_local() || addr.is_unspecified()) =>
        {
            Err(ConnectionManagerError::InvalidMultiaddr(
                "Non-global IP addresses are invalid".to_string(),
            ))
        },
        Protocol::Ip4(_) | Protocol::Ip6(_) => {
            let tcp = addr_iter.next().ok_or_else(|| {
                ConnectionManagerError::InvalidMultiaddr("Address does not include a TCP port".to_string())
            })?;

            validate_tcp_port(tcp)?;
            expect_end_of_address(addr_iter)
        },
        Protocol::Memory(0) => Err(ConnectionManagerError::InvalidMultiaddr(
            "Cannot connect to a zero memory port".to_string(),
        )),
        Protocol::Memory(_) if allow_test_addrs => expect_end_of_address(addr_iter),
        Protocol::Memory(_) => Err(ConnectionManagerError::InvalidMultiaddr(
            "Memory addresses are invalid".to_string(),
        )),
        // Zero-port onions should have already failed when parsing. Keep these checks here just in case.
        Protocol::Onion(_, 0) => Err(ConnectionManagerError::InvalidMultiaddr(
            "A zero onion port is not valid in the onion spec".to_string(),
        )),
        Protocol::Onion3(addr) if addr.port() == 0 => Err(ConnectionManagerError::InvalidMultiaddr(
            "A zero onion port is not valid in the onion spec".to_string(),
        )),
        Protocol::Onion(_, _) | Protocol::Onion3(_) => expect_end_of_address(addr_iter),
        p => Err(ConnectionManagerError::InvalidMultiaddr(format!(
            "Unsupported address type '{}'",
            p
        ))),
    }
}

fn validate_tcp_port(expected_tcp: Protocol) -> Result<(), ConnectionManagerError> {
    match expected_tcp {
        Protocol::Tcp(0) => Err(ConnectionManagerError::InvalidMultiaddr(
            "Cannot connect to a zero TCP port".to_string(),
        )),
        Protocol::Tcp(_) => Ok(()),
        p => Err(ConnectionManagerError::InvalidMultiaddr(format!(
            "Expected TCP address component but got '{}'",
            p
        ))),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use multiaddr::multiaddr;

    #[test]
    fn validate_address_strict() {
        let valid = [
            multiaddr!(Ip4([172, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(1u16)),
            "/onion/aaimaq4ygg2iegci:1234".parse().unwrap(),
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com"), Tcp(1u16)),
        ];

        let invalid = &[
            multiaddr!(Ip4([127, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([169, 254, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 0, 0, 1])),
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Memory(1234u64)),
            multiaddr!(Memory(0u64)),
        ];

        validate_peer_addresses(valid, false).unwrap();
        for addr in invalid {
            validate_address(addr, false).unwrap_err();
        }
    }

    #[test]
    fn validate_address_allow_test_addrs() {
        let valid = [
            multiaddr!(Ip4([127, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([169, 254, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(1u16)),
            "/onion/aaimaq4ygg2iegci:1234".parse().unwrap(),
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com"), Tcp(1u16)),
            multiaddr!(Memory(1234u64)),
        ];

        let invalid = &[
            multiaddr!(Ip4([172, 0, 0, 1])),
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Memory(0u64)),
        ];

        validate_peer_addresses(valid, true).unwrap();
        for addr in invalid {
            validate_address(addr, true).unwrap_err();
        }
    }
}

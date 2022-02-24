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

use std::{convert::TryFrom, net::Ipv6Addr};

use log::*;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    connection_manager::error::ConnectionManagerError,
    multiaddr::{Multiaddr, Protocol},
    peer_manager::{IdentitySignature, NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    proto,
    proto::identity::PeerIdentityMsg,
    protocol,
    protocol::{NodeNetworkInfo, ProtocolId},
    types::CommsPublicKey,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connection_manager::common";

/// The maximum size of the peer's user agent string. If the peer sends a longer string it is truncated.
const MAX_USER_AGENT_LEN: usize = 100;

pub async fn perform_identity_exchange<
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
/// 1. Check the offered node identity is a valid base node identity (TODO: This won't work for DAN nodes)
/// 1. Check if we know the peer, if so, is the peer banned, if so, return an error
/// 1. Check that the offered addresses are valid
/// 1. Update or add the peer, returning it's NodeId
///
/// If the `allow_test_addrs` parameter is true, loopback, local link and other addresses normally not considered valid
/// for p2p comms will be accepted.
pub async fn validate_and_add_peer_from_peer_identity(
    peer_manager: &PeerManager,
    known_peer: Option<Peer>,
    authenticated_public_key: CommsPublicKey,
    mut peer_identity: PeerIdentityMsg,
    dialed_addr: Option<&Multiaddr>,
    allow_test_addrs: bool,
) -> Result<(NodeId, Vec<ProtocolId>), ConnectionManagerError> {
    let peer_node_id = NodeId::from_public_key(&authenticated_public_key);
    let addresses = peer_identity
        .addresses
        .into_iter()
        .filter_map(|addr_bytes| Multiaddr::try_from(addr_bytes).ok())
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

    peer_identity.user_agent.truncate(MAX_USER_AGENT_LEN);

    // Add or update the peer
    let peer = match known_peer {
        Some(mut peer) => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' already exists in peer list. Updating.",
                peer.node_id.short_str()
            );
            peer.connection_stats.set_connection_success();
            peer.addresses = addresses.into();
            peer.set_offline(false);
            if let Some(addr) = dialed_addr {
                peer.addresses.mark_last_seen_now(addr);
            }
            peer.features = PeerFeatures::from_bits_truncate(peer_identity.features);
            peer.supported_protocols = supported_protocols.clone();
            peer.user_agent = peer_identity.user_agent;
            if let Some(identity_signature) = peer_identity.identity_signature {
                add_valid_identity_signature_to_peer(&mut peer, identity_signature)?;
            }
            peer
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
                supported_protocols.clone(),
                peer_identity.user_agent,
            );
            new_peer.connection_stats.set_connection_success();
            // TODO(testnetreset): Require an identity signature once majority nodes are upgraded
            if let Some(identity_sig) = peer_identity.identity_signature {
                add_valid_identity_signature_to_peer(&mut new_peer, identity_sig)?;
            }
            if let Some(addr) = dialed_addr {
                new_peer.addresses.mark_last_seen_now(addr);
            }
            new_peer
        },
    };

    peer_manager.add_peer(peer).await?;

    Ok((peer_node_id, supported_protocols))
}

fn add_valid_identity_signature_to_peer(
    peer: &mut Peer,
    identity_sig: proto::identity::IdentitySignature,
) -> Result<(), ConnectionManagerError> {
    let identity_sig =
        IdentitySignature::try_from(identity_sig).map_err(|_| ConnectionManagerError::PeerIdentityInvalidSignature)?;

    if !identity_sig.is_valid_for_peer(peer) {
        warn!(
            target: LOG_TARGET,
            "Peer {} sent invalid identity signature", peer.node_id
        );
        return Err(ConnectionManagerError::PeerIdentityInvalidSignature);
    }

    peer.identity_signature = Some(identity_sig);
    Ok(())
}

pub async fn find_unbanned_peer(
    peer_manager: &PeerManager,
    authenticated_public_key: &CommsPublicKey,
) -> Result<Option<Peer>, ConnectionManagerError> {
    match peer_manager.find_by_public_key(authenticated_public_key).await {
        Ok(Some(peer)) if peer.is_banned() => Err(ConnectionManagerError::PeerBanned),
        Ok(peer) => Ok(peer),
        Err(err) => Err(err.into()),
    }
}

pub fn validate_peer_addresses<'a, A: IntoIterator<Item = &'a Multiaddr>>(
    addresses: A,
    allow_test_addrs: bool,
) -> Result<(), ConnectionManagerError> {
    let mut has_address = false;
    for addr in addresses.into_iter() {
        has_address = true;
        validate_address(addr, allow_test_addrs)?;
    }
    if !has_address {
        return Err(ConnectionManagerError::PeerIdentityNoAddresses);
    }
    Ok(())
}

fn validate_address(addr: &Multiaddr, allow_test_addrs: bool) -> Result<(), ConnectionManagerError> {
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

    /// Returns [true] if the address is a unicast link-local address (fe80::/10).
    #[inline]
    const fn is_unicast_link_local(addr: &Ipv6Addr) -> bool {
        (addr.segments()[0] & 0xffc0) == 0xfe80
    }

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
            if !allow_test_addrs && (addr.is_loopback() || is_unicast_link_local(&addr) || addr.is_unspecified()) =>
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
    use multiaddr::multiaddr;

    use super::*;

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

        validate_peer_addresses(&valid, false).unwrap();
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

        validate_peer_addresses(&valid, true).unwrap();
        for addr in invalid {
            validate_address(addr, true).unwrap_err();
        }
    }
}

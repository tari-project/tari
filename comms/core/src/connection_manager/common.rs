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

use std::{convert::TryInto, net::Ipv6Addr};

use digest::Digest;
use log::*;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    connection_manager::error::ConnectionManagerError,
    multiaddr::{Multiaddr, Protocol},
    net_address::{MultiaddrWithStats, MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFlags, PeerIdentityClaim},
    protocol,
    protocol::{NodeNetworkInfo, ProtocolId},
    types::CommsPublicKey,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connection_manager::common";

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
) -> Result<PeerIdentityClaim, ConnectionManagerError> {
    let peer_identity =
        protocol::identity_exchange(node_identity, our_supported_protocols, network_info, socket).await?;

    Ok(peer_identity.try_into()?)
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
pub(super) async fn validate_peer_identity(
    authenticated_public_key: &CommsPublicKey,
    peer_identity: &PeerIdentityClaim,
    allow_test_addrs: bool,
) -> Result<(), ConnectionManagerError> {
    validate_addresses(&peer_identity.addresses, allow_test_addrs)?;
    if peer_identity.addresses.is_empty() {
        return Err(ConnectionManagerError::PeerIdentityNoAddresses);
    }

    if !peer_identity.signature.is_valid(
        authenticated_public_key,
        peer_identity.features,
        &peer_identity.addresses,
    ) {
        return Err(ConnectionManagerError::PeerIdentityInvalidSignature);
    }

    Ok(())
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
pub(super) async fn validate_and_add_peer_from_peer_identity(
    peer_manager: &PeerManager,
    known_peer: Option<Peer>,
    authenticated_public_key: CommsPublicKey,
    peer_identity: &PeerIdentityClaim,
    allow_test_addrs: bool,
) -> Result<NodeId, ConnectionManagerError> {
    let peer_node_id = NodeId::from_public_key(&authenticated_public_key);

    let addresses = MultiaddressesWithStats::from_addresses_with_source(
        peer_identity.addresses.clone(),
        &PeerAddressSource::FromPeerConnection {
            peer_identity_claim: peer_identity.clone(),
        },
    );
    validate_addresses_and_source(&addresses, &authenticated_public_key, allow_test_addrs)?;

    // Note: the peer will be merged in the db if it already exists
    let peer = match known_peer {
        Some(mut peer) => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' already exists in peer list. Updating.",
                peer.node_id.short_str()
            );
            peer.addresses
                .update_addresses(&peer_identity.addresses, &PeerAddressSource::FromPeerConnection {
                    peer_identity_claim: peer_identity.clone(),
                });

            peer.features = peer_identity.features;
            peer.supported_protocols = peer_identity.supported_protocols();
            peer.user_agent = peer_identity.user_agent().unwrap_or_default();

            peer
        },
        None => {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' does not exist in peer list. Adding.",
                peer_node_id.short_str()
            );
            Peer::new(
                authenticated_public_key.clone(),
                peer_node_id.clone(),
                MultiaddressesWithStats::from_addresses_with_source(
                    peer_identity.addresses.clone(),
                    &PeerAddressSource::FromPeerConnection {
                        peer_identity_claim: peer_identity.clone(),
                    },
                ),
                PeerFlags::empty(),
                peer_identity.features,
                peer_identity.supported_protocols(),
                peer_identity.user_agent().unwrap_or_default(),
            )
        },
    };

    peer_manager.add_peer(peer).await?;

    Ok(peer_node_id)
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

/// Checks that the given peer addresses are well-formed and valid. If allow_test_addrs is false, all localhost and
/// memory addresses will be rejected. Also checks that the source (signature of the address) is correct
pub fn validate_addresses_and_source(
    addresses: &MultiaddressesWithStats,
    public_key: &CommsPublicKey,
    allow_test_addrs: bool,
) -> Result<(), ConnectionManagerError> {
    for addr in addresses.addresses() {
        validate_address_and_source(public_key, addr, allow_test_addrs)?;
    }

    Ok(())
}

/// Checks that the given peer addresses are well-formed and valid. If allow_test_addrs is false, all localhost and
/// memory addresses will be rejected.
pub fn validate_addresses(addresses: &[Multiaddr], allow_test_addrs: bool) -> Result<(), ConnectionManagerError> {
    for addr in addresses {
        validate_address(addr, allow_test_addrs)?;
    }

    Ok(())
}

pub fn validate_address_and_source(
    public_key: &CommsPublicKey,
    addr: &MultiaddrWithStats,
    allow_test_addrs: bool,
) -> Result<(), ConnectionManagerError> {
    match addr.source {
        PeerAddressSource::Config => (),
        _ => {
            let claim = addr
                .source
                .peer_identity_claim()
                .ok_or(ConnectionManagerError::PeerIdentityInvalidSignature)?;
            if !claim.signature.is_valid(public_key, claim.features, &claim.addresses) {
                return Err(ConnectionManagerError::PeerIdentityInvalidSignature);
            }
            if !claim.addresses.contains(addr.address()) {
                return Err(ConnectionManagerError::PeerIdentityInvalidSignature);
            }
        },
    }
    validate_address(addr.address(), allow_test_addrs)?;
    Ok(())
}

fn validate_address(addr: &Multiaddr, allow_test_addrs: bool) -> Result<(), ConnectionManagerError> {
    let mut addr_iter = addr.iter();
    let proto = addr_iter
        .next()
        .ok_or_else(|| ConnectionManagerError::InvalidMultiaddr("Multiaddr was empty".to_string()))?;

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
        Protocol::Onion(_, _) => Err(ConnectionManagerError::OnionV2NotSupported),
        Protocol::Onion3(addr) => {
            expect_end_of_address(addr_iter)?;
            validate_onion3_address(&addr)
        },
        p => Err(ConnectionManagerError::InvalidMultiaddr(format!(
            "Unsupported address type '{}'",
            p
        ))),
    }
}

fn expect_end_of_address(mut iter: multiaddr::Iter<'_>) -> Result<(), ConnectionManagerError> {
    match iter.next() {
        Some(p) => Err(ConnectionManagerError::InvalidMultiaddr(format!(
            "Unexpected multiaddress component '{}'",
            p
        ))),
        None => Ok(()),
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

/// Validates the onion3 version and checksum as per https://github.com/torproject/torspec/blob/main/rend-spec-v3.txt#LL2258C6-L2258C6
fn validate_onion3_address(addr: &multiaddr::Onion3Addr<'_>) -> Result<(), ConnectionManagerError> {
    const ONION3_PUBKEY_SIZE: usize = 32;
    const ONION3_CHECKSUM_SIZE: usize = 2;

    let (pub_key, checksum_version) = addr.hash().split_at(ONION3_PUBKEY_SIZE);
    let (checksum, version) = checksum_version.split_at(ONION3_CHECKSUM_SIZE);

    if version != b"\x03" {
        return Err(ConnectionManagerError::InvalidMultiaddr(
            "Invalid version in onion address".to_string(),
        ));
    }

    let calculated_checksum = sha3::Sha3_256::new()
        .chain_update(".onion checksum")
        .chain_update(pub_key)
        .chain_update(version)
        .finalize();

    if calculated_checksum[..2] != *checksum {
        return Err(ConnectionManagerError::InvalidMultiaddr(
            "Invalid checksum in onion address".to_string(),
        ));
    }

    Ok(())
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
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com"), Tcp(1u16)),
        ];

        let invalid = &[
            "/onion/aaimaq4ygg2iegci:1234".parse().unwrap(),
            multiaddr!(Ip4([127, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([169, 254, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 0, 0, 1])),
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Memory(1234u64)),
            multiaddr!(Memory(0u64)),
        ];

        validate_addresses(&valid, false).unwrap();
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
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com"), Tcp(1u16)),
            multiaddr!(Memory(1234u64)),
        ];

        let invalid = &[
            "/onion/aaimaq4ygg2iegci:1234".parse().unwrap(),
            multiaddr!(Ip4([172, 0, 0, 1])),
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Memory(0u64)),
        ];

        validate_addresses(&valid, true).unwrap();
        for addr in invalid {
            validate_address(addr, true).unwrap_err();
        }
    }

    #[test]
    fn validate_onion3_checksum() {
        let valid: Multiaddr = "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
            .parse()
            .unwrap();

        validate_address(&valid, false).unwrap();

        // Change one byte
        let invalid: Multiaddr = "/onion3/www6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
            .parse()
            .unwrap();

        validate_address(&invalid, false).unwrap_err();

        // Randomly generated
        let invalid: Multiaddr = "/onion3/pd6sf3mqkkkfrn4rk5odgcr2j5sn7m523a4tm7pzpuotk2b7rpuhaeym:1234"
            .parse()
            .unwrap();

        let err = validate_address(&invalid, false).unwrap_err();
        assert!(matches!(err, ConnectionManagerError::InvalidMultiaddr(_)));
    }
}

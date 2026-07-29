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

use digest::Digest;
use log::debug;

use crate::{
    multiaddr::{Multiaddr, Protocol},
    net_address::is_external_address,
    peer_manager::{NodeId, PeerIdentityClaim},
    peer_validator::{PeerValidatorConfig, error::PeerValidatorError},
    types::CommsPublicKey,
};

const LOG_TARGET: &str = "comms::peer_validator";

/// Checks that the given peer addresses are well-formed and valid. If test addresses are not allowed, local, private
/// and memory addresses are rejected.
pub fn validate_addresses(config: &PeerValidatorConfig, addresses: &[Multiaddr]) -> Result<(), PeerValidatorError> {
    if addresses.is_empty() {
        debug!(target: LOG_TARGET, "validate_addresses - no addresses to validate.");
        return Ok(());
    }

    validate_address_count(config, addresses)?;
    for addr in addresses {
        validate_address(addr, config.allow_test_addresses)?;
    }

    Ok(())
}

fn validate_address_count(config: &PeerValidatorConfig, addresses: &[Multiaddr]) -> Result<(), PeerValidatorError> {
    if addresses.len() > config.max_permitted_peer_addresses_per_claim {
        return Err(PeerValidatorError::PeerIdentityTooManyAddresses {
            length: addresses.len(),
            max: config.max_permitted_peer_addresses_per_claim,
        });
    }
    Ok(())
}

pub fn find_most_recent_claim<'a, I: IntoIterator<Item = &'a PeerIdentityClaim>>(
    claims: I,
) -> Option<&'a PeerIdentityClaim> {
    claims.into_iter().max_by_key(|c| c.signature.updated_at())
}

pub fn validate_peer_identity_claim(
    config: &PeerValidatorConfig,
    public_key: &CommsPublicKey,
    claim: &PeerIdentityClaim,
) -> Result<(), PeerValidatorError> {
    validate_addresses(config, &claim.addresses)?;
    validate_peer_identity_claim_signature(public_key, claim)
}

fn validate_peer_identity_claim_signature(
    public_key: &CommsPublicKey,
    claim: &PeerIdentityClaim,
) -> Result<(), PeerValidatorError> {
    if let Ok(true) = claim.is_valid(public_key) {
        Ok(())
    } else {
        Err(PeerValidatorError::InvalidPeerSignature {
            peer: NodeId::from_public_key(public_key),
        })
    }
}

/// Verifies a signed claim and returns only the addresses permitted by the
/// validator configuration.
///
/// Discovery can safely retain a peer that advertises both reachable and local
/// addresses: the signature is checked against the original, unmodified claim,
/// then invalid addresses are omitted from the peer database.
pub fn validate_and_filter_peer_identity_claim_addresses(
    config: &PeerValidatorConfig,
    public_key: &CommsPublicKey,
    claim: &PeerIdentityClaim,
) -> Result<Vec<Multiaddr>, PeerValidatorError> {
    validate_address_count(config, &claim.addresses)?;
    for addr in &claim.addresses {
        // Reject malformed or unsupported addresses before the more expensive
        // signature verification. Passing `true` permits well-formed local
        // transports for the filtering step.
        validate_address(addr, true)?;
    }
    validate_peer_identity_claim_signature(public_key, claim)?;

    if config.allow_test_addresses {
        return Ok(claim.addresses.clone());
    }

    let mut valid_addresses = Vec::with_capacity(claim.addresses.len());
    for addr in &claim.addresses {
        // Structural validation has already succeeded above. This second pass
        // applies the production-only address policy and filters local/test
        // transports without invalidating the signature over the original claim.
        match validate_address(addr, false) {
            Ok(()) => valid_addresses.push(addr.clone()),
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Discarding disallowed address '{}' from a valid peer claim: {}",
                    addr,
                    err
                );
            },
        }
    }

    Ok(valid_addresses)
}

fn validate_address(addr: &Multiaddr, allow_test_addrs: bool) -> Result<(), PeerValidatorError> {
    let mut addr_iter = addr.iter();
    let proto = addr_iter
        .next()
        .ok_or_else(|| PeerValidatorError::InvalidMultiaddr("Multiaddr was empty".to_string()))?;

    match proto {
        Protocol::Dns4(_) | Protocol::Dns6(_) | Protocol::Dnsaddr(_) => {
            let tcp = addr_iter.next().ok_or_else(|| {
                PeerValidatorError::InvalidMultiaddr("Address does not include a TCP port".to_string())
            })?;

            validate_tcp_port(tcp)?;
            expect_end_of_address(addr_iter)?;
            if !allow_test_addrs && !is_external_address(addr) {
                return Err(PeerValidatorError::InvalidMultiaddr(
                    "Non-global DNS addresses are invalid".to_string(),
                ));
            }
            Ok(())
        },

        Protocol::Ip4(_) | Protocol::Ip6(_) => {
            let tcp = addr_iter.next().ok_or_else(|| {
                PeerValidatorError::InvalidMultiaddr("Address does not include a TCP port".to_string())
            })?;

            validate_tcp_port(tcp)?;
            expect_end_of_address(addr_iter)?;
            if !allow_test_addrs && !is_external_address(addr) {
                return Err(PeerValidatorError::InvalidMultiaddr(
                    "Non-global IP addresses are invalid".to_string(),
                ));
            }
            Ok(())
        },
        Protocol::Memory(0) => Err(PeerValidatorError::InvalidMultiaddr(
            "Cannot connect to a zero memory port".to_string(),
        )),
        Protocol::Memory(_) if allow_test_addrs => expect_end_of_address(addr_iter),
        Protocol::Memory(_) => Err(PeerValidatorError::InvalidMultiaddr(
            "Memory addresses are invalid".to_string(),
        )),
        // Zero-port onions should have already failed when parsing. Keep these checks here just in case.
        Protocol::Onion(_, 0) => Err(PeerValidatorError::InvalidMultiaddr(
            "A zero onion port is not valid in the onion spec".to_string(),
        )),
        Protocol::Onion3(addr) if addr.port() == 0 => Err(PeerValidatorError::InvalidMultiaddr(
            "A zero onion port is not valid in the onion spec".to_string(),
        )),
        Protocol::Onion(_, _) => Err(PeerValidatorError::OnionV2NotSupported),
        Protocol::Onion3(addr) => {
            expect_end_of_address(addr_iter)?;
            validate_onion3_address(&addr)
        },
        p => Err(PeerValidatorError::InvalidMultiaddr(format!(
            "Unsupported address type '{p}'"
        ))),
    }
}

fn expect_end_of_address(mut iter: multiaddr::Iter<'_>) -> Result<(), PeerValidatorError> {
    match iter.next() {
        Some(p) => Err(PeerValidatorError::InvalidMultiaddr(format!(
            "Unexpected multiaddress component '{p}'"
        ))),
        None => Ok(()),
    }
}

fn validate_tcp_port(expected_tcp: Protocol) -> Result<(), PeerValidatorError> {
    match expected_tcp {
        Protocol::Tcp(0) => Err(PeerValidatorError::InvalidMultiaddr(
            "Cannot connect to a zero TCP port".to_string(),
        )),
        Protocol::Tcp(_) => Ok(()),
        p => Err(PeerValidatorError::InvalidMultiaddr(format!(
            "Expected TCP address component but got '{p}'"
        ))),
    }
}

/// Validates the onion3 version and checksum as per https://github.com/torproject/torspec/blob/main/rend-spec-v3.txt#LL2258C6-L2258C6
fn validate_onion3_address(addr: &multiaddr::Onion3Addr<'_>) -> Result<(), PeerValidatorError> {
    const ONION3_PUBKEY_SIZE: usize = 32;
    const ONION3_CHECKSUM_SIZE: usize = 2;

    let (pub_key, checksum_version) = addr
        .hash()
        .split_at_checked(ONION3_PUBKEY_SIZE)
        .ok_or(PeerValidatorError::InvalidMultiaddr("Unable to split data".to_string()))?;
    let (checksum, version) = checksum_version
        .split_at_checked(ONION3_CHECKSUM_SIZE)
        .ok_or(PeerValidatorError::InvalidMultiaddr("Unable to split data".to_string()))?;

    if version != b"\x03" {
        return Err(PeerValidatorError::InvalidMultiaddr(
            "Invalid version in onion address".to_string(),
        ));
    }

    let calculated_checksum = sha3::Sha3_256::new()
        .chain_update(".onion checksum")
        .chain_update(pub_key)
        .chain_update(version)
        .finalize();

    if *calculated_checksum.get(..2).expect("Index should be valid") != *checksum {
        return Err(PeerValidatorError::InvalidMultiaddr(
            "Invalid checksum in onion address".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
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
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Ip4([127, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([10, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 16, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([192, 168, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([169, 254, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0xfc00, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0xfe80, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            "/ip6/::ffff:127.0.0.1/tcp/1".parse().unwrap(),
            "/ip6/::ffff:192.168.0.1/tcp/1".parse().unwrap(),
            multiaddr!(Dns4("localhost"), Tcp(1u16)),
            multiaddr!(Dns4("node.internal"), Tcp(1u16)),
            multiaddr!(Dns6("printer.local"), Tcp(1u16)),
            multiaddr!(Dnsaddr("home.arpa"), Tcp(1u16)),
            multiaddr!(Memory(1234u64)),
            multiaddr!(Memory(0u64)),
        ];

        for addr in valid {
            validate_address(&addr, false).unwrap();
        }
        for addr in invalid {
            validate_address(addr, false).unwrap_err();
        }
    }

    #[test]
    fn validate_address_allow_test_addrs() {
        let valid = [
            multiaddr!(Ip4([127, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([169, 254, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([10, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 16, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([192, 168, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip4([172, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0xfc00, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            multiaddr!(Ip6([0xfe80, 0, 0, 0, 0, 0, 0, 1]), Tcp(1u16)),
            "/ip6/::ffff:127.0.0.1/tcp/1".parse().unwrap(),
            "/ip6/::ffff:192.168.0.1/tcp/1".parse().unwrap(),
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com"), Tcp(1u16)),
            multiaddr!(Dns4("localhost"), Tcp(1u16)),
            multiaddr!(Dns4("node.internal"), Tcp(1u16)),
            multiaddr!(Dns6("printer.local"), Tcp(1u16)),
            multiaddr!(Dnsaddr("home.arpa"), Tcp(1u16)),
            multiaddr!(Memory(1234u64)),
        ];

        let invalid = &[
            "/onion/aaimaq4ygg2iegci:1234".parse().unwrap(),
            multiaddr!(Ip4([172, 0, 0, 1])),
            "/onion/aaimaq4ygg2iegci:1234/http".parse().unwrap(),
            multiaddr!(Dnsaddr("mike-magic-nodes.com")),
            multiaddr!(Memory(0u64)),
        ];

        for addr in valid {
            validate_address(&addr, true).unwrap();
        }
        for addr in invalid {
            validate_address(addr, true).unwrap_err();
        }
    }

    #[test]
    fn malformed_local_dns_reports_the_structural_error_first() {
        let address = multiaddr!(Dns4("localhost"));
        let err = validate_address(&address, false).unwrap_err();

        assert!(matches!(
            err,
            PeerValidatorError::InvalidMultiaddr(message) if message == "Address does not include a TCP port"
        ));
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
        assert!(matches!(err, PeerValidatorError::InvalidMultiaddr(_)));
    }
}

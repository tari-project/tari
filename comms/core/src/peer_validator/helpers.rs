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

use std::str::FromStr;

use digest::Digest;
use log::debug;

use crate::{
    multiaddr::{Multiaddr, Protocol},
    net_address::is_external_address,
    peer_manager::{NodeId, PeerIdentityClaim, PeerManagerError},
    peer_validator::{PeerValidatorConfig, error::PeerValidatorError},
    types::CommsPublicKey,
};

const LOG_TARGET: &str = "comms::peer_validator";

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

fn validate_peer_identity_claim_signature(
    public_key: &CommsPublicKey,
    claim: &PeerIdentityClaim,
) -> Result<(), PeerValidatorError> {
    claim.verify(public_key).map_err(|err| match err {
        PeerManagerError::IdentitySignatureFromFuture => PeerValidatorError::PeerSignatureFromFuture {
            peer: NodeId::from_public_key(public_key),
        },
        _ => PeerValidatorError::InvalidPeerSignature {
            peer: NodeId::from_public_key(public_key),
        },
    })
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
    // Do not skip authentication when every address will be filtered. The
    // handshake and existing-peer paths may retain an authenticated claim with
    // an empty permitted set, and accepting it without this check would allow
    // unsigned identity metadata. DHT message rate limiting bounds the cost of
    // repeatedly submitting validly structured all-local claims.
    validate_peer_identity_claim_signature(public_key, claim)?;

    let mut valid_addresses = Vec::with_capacity(claim.addresses.len());
    for addr in &claim.addresses {
        // Structural validation has already succeeded above. This second pass
        // filters addresses without invalidating the signature over the original
        // claim: DNS names that are not ordinary host names (older nodes accept
        // and relay them, so they are not a sign of hostility), and, in
        // production, local/test transports.
        let permitted = match validate_dns_name_of(addr) {
            Ok(()) if config.allow_test_addresses => Ok(()),
            Ok(()) => validate_address(addr, false),
            Err(err) => Err(err),
        };
        match permitted {
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

/// Checks the shape of an address without the local-address policy: an IP or DNS address followed by exactly a
/// non-zero TCP port, an onion3 address on its own, or a non-zero memory port, and an ordinary DNS host name. Local and
/// test addresses pass. An address that fails is either refused (a malformed one) or dropped (an unusual DNS name) by
/// claim validation, so it must never be stored from or relayed in a claim.
pub fn validate_address_shape(addr: &Multiaddr) -> Result<(), PeerValidatorError> {
    validate_address(addr, true)?;
    validate_dns_name_of(addr)
}

/// Applies [`validate_dns_name`] to the address's DNS name, if it has one.
fn validate_dns_name_of(addr: &Multiaddr) -> Result<(), PeerValidatorError> {
    match addr.iter().next() {
        Some(Protocol::Dns4(name)) | Some(Protocol::Dns6(name)) | Some(Protocol::Dnsaddr(name)) => {
            validate_dns_name(&name)
        },
        _ => Ok(()),
    }
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
            // The address is stored and relayed in its text form too, which must read back as exactly the address
            // that was signed. A DNS name containing `/` does not; no honest node produces one.
            let reparsed = Multiaddr::from_str(&addr.to_string())
                .map_err(|_| PeerValidatorError::InvalidMultiaddr("DNS address does not round trip".to_string()))?;
            if reparsed != *addr {
                return Err(PeerValidatorError::InvalidMultiaddr(
                    "DNS address does not round trip".to_string(),
                ));
            }
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
        // Only the protocol name: the component's value is the peer's and is not echoed into logs
        p => Err(PeerValidatorError::InvalidMultiaddr(format!(
            "Unsupported address type '{}'",
            p.tag()
        ))),
    }
}

/// The longest DNS name, in bytes, without the optional trailing dot (RFC 1035).
const MAX_DNS_NAME_LENGTH: usize = 253;
/// The longest single DNS label, in bytes (RFC 1035).
const MAX_DNS_LABEL_LENGTH: usize = 63;

/// Checks that a claimed DNS name is an ordinary host name.
///
/// Only letter-digit-hyphen labels are allowed, and a name whose last label is numeric is refused: names such as
/// `127.1` or `0x7f000001` are resolved as IP addresses and would bypass the IP address checks. Older nodes accept
/// and relay any name, so a failure here only drops the address ([`PeerValidatorError::UnusableDnsName`] is not a ban
/// offence).
fn validate_dns_name(name: &str) -> Result<(), PeerValidatorError> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > MAX_DNS_NAME_LENGTH {
        return Err(PeerValidatorError::UnusableDnsName(
            "DNS name is empty or too long".to_string(),
        ));
    }

    let mut last_label = "";
    for label in name.split('.') {
        if label.is_empty() || label.len() > MAX_DNS_LABEL_LENGTH {
            return Err(PeerValidatorError::UnusableDnsName(
                "DNS label is empty or too long".to_string(),
            ));
        }
        if !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(PeerValidatorError::UnusableDnsName(
                "DNS label contains characters other than letters, digits and '-'".to_string(),
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(PeerValidatorError::UnusableDnsName(
                "DNS label starts or ends with '-'".to_string(),
            ));
        }
        last_label = label;
    }

    let last_label = last_label.to_ascii_lowercase();
    let is_decimal = last_label.bytes().all(|b| b.is_ascii_digit());
    let is_hex = last_label
        .strip_prefix("0x")
        .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_hexdigit()));
    if is_decimal || is_hex {
        return Err(PeerValidatorError::UnusableDnsName(
            "Numeric DNS names are invalid".to_string(),
        ));
    }

    Ok(())
}

fn expect_end_of_address(mut iter: multiaddr::Iter<'_>) -> Result<(), PeerValidatorError> {
    match iter.next() {
        Some(p) => Err(PeerValidatorError::InvalidMultiaddr(format!(
            "Unexpected multiaddress component '{}'",
            p.tag()
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
            "Expected TCP address component but got '{}'",
            p.tag()
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

    fn signed_claim(addresses: Vec<Multiaddr>) -> (CommsPublicKey, PeerIdentityClaim) {
        let node_identity = crate::peer_manager::NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            addresses.clone(),
            crate::peer_manager::PeerFeatures::COMMUNICATION_NODE,
        );
        let claim = PeerIdentityClaim {
            addresses,
            features: crate::peer_manager::PeerFeatures::COMMUNICATION_NODE,
            signature: node_identity.identity_signature_read().clone().unwrap(),
        };
        (node_identity.public_key().clone(), claim)
    }

    /// A claim dated more than a day ahead may come from an honest signer whose clock runs fast, so it is rejected
    /// without a ban.
    #[test]
    fn future_dated_claim_is_rejected_without_a_ban() {
        use tari_crypto::keys::SecretKey;

        use crate::types::CommsSecretKey;

        let config = PeerValidatorConfig::default();
        let (public_key, mut claim) = signed_claim(vec!["/ip4/23.23.23.23/tcp/18189".parse().unwrap()]);
        let secret = CommsSecretKey::random(&mut rand::rng());
        let public_key_future = CommsPublicKey::from_secret_key(&secret);
        claim.signature = crate::peer_manager::IdentitySignature::sign_new(
            &secret,
            claim.features,
            &claim.addresses,
            chrono::Utc::now() + chrono::Duration::days(2),
        );
        let err = validate_and_filter_peer_identity_claim_addresses(&config, &public_key_future, &claim).unwrap_err();
        assert!(
            matches!(err, PeerValidatorError::PeerSignatureFromFuture { .. }),
            "{err:?}"
        );
        assert!(!err.is_ban_offence());

        // A forged claim cannot avoid the ban by being future-dated
        let err = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap_err();
        assert!(
            matches!(err, PeerValidatorError::InvalidPeerSignature { .. }),
            "{err:?}"
        );
        assert!(err.is_ban_offence());
    }

    /// A `/` in a DNS name changes what the address's text form reads back as. No honest node produces one, so it
    /// stays a ban offence.
    #[test]
    fn slash_in_dns_label_is_rejected_as_invalid_multiaddr() {
        let config = PeerValidatorConfig::default();
        let good: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        for name in ["evil/x", "a/tcp/80/ip4/10.0.0.1"] {
            let (public_key, claim) = signed_claim(vec![good.clone(), multiaddr!(Dns4(name), Tcp(1u16))]);
            let err = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap_err();
            assert!(
                matches!(err, PeerValidatorError::InvalidMultiaddr(_)),
                "{name}: {err:?}"
            );
            // Already a long-ban offence; no new ban type
            assert!(err.is_ban_offence());
        }

        let (public_key, claim) = signed_claim(vec![multiaddr!(Dns4("ok.example.com"), Tcp(1u16))]);
        let permitted = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap();
        assert_eq!(permitted, claim.addresses);
    }

    /// Numeric names resolve as IP addresses and would bypass the IP checks, so the address is dropped. Older nodes
    /// accept and relay them, so the claim is not refused and nobody is banned for it.
    #[test]
    fn numeric_dns_label_is_rejected() {
        let config = PeerValidatorConfig::default();
        let good: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        for name in ["127.1", "0x7f000001", "1.2.3.4", "example.123"] {
            let numeric = multiaddr!(Dns4(name), Tcp(1u16));
            let (public_key, claim) = signed_claim(vec![good.clone(), numeric.clone()]);
            let permitted = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap();
            assert_eq!(permitted, vec![good.clone()], "{name}");

            let err = validate_address_shape(&numeric).unwrap_err();
            assert!(matches!(err, PeerValidatorError::UnusableDnsName(_)), "{name}");
            assert!(!err.is_ban_offence());
        }
    }

    #[test]
    fn unusual_dns_name_is_filtered_not_banned() {
        let good: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let unusual: Multiaddr = "/dns4/a_b.example.com/tcp/1".parse().unwrap();
        for allow_test_addresses in [false, true] {
            let config = PeerValidatorConfig {
                allow_test_addresses,
                ..Default::default()
            };
            let (public_key, claim) = signed_claim(vec![good.clone(), unusual.clone()]);
            let permitted = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap();
            assert_eq!(permitted, vec![good.clone()]);
            // The signature was checked over the unmodified claim
            assert_eq!(claim.addresses, vec![good.clone(), unusual.clone()]);

            // A claim with only such a name is valid but has nothing usable, which is not a ban offence either
            let (public_key, claim) = signed_claim(vec![unusual.clone()]);
            let permitted = validate_and_filter_peer_identity_claim_addresses(&config, &public_key, &claim).unwrap();
            assert!(permitted.is_empty());
        }
        let err = validate_address_shape(&unusual).unwrap_err();
        assert!(!err.is_ban_offence());
    }

    #[test]
    fn dns_label_shape_is_enforced() {
        let too_long_label = "a".repeat(64);
        let too_long_name = vec!["a".repeat(63); 5].join(".");
        let invalid = [
            "",
            ".",
            "a..b",
            "-a.com",
            "a-.com",
            "a_b.com",
            "ex ample.com",
            "exämple.com",
            too_long_label.as_str(),
            too_long_name.as_str(),
        ];
        for name in invalid {
            let address = multiaddr!(Dns4(name), Tcp(1u16));
            let err = validate_address_shape(&address).unwrap_err();
            assert!(!err.is_ban_offence(), "{name}: {err:?}");
        }

        let valid = [
            "example.com",
            "example.com.",
            "a-b.example.com",
            "x1.y2.z3",
            "localhost",
        ];
        for name in valid {
            validate_address_shape(&multiaddr!(Dns4(name), Tcp(1u16))).unwrap();
        }
    }

    #[test]
    fn error_messages_do_not_echo_address_values() {
        let address: Multiaddr = "/ip4/23.23.23.23/udp/7777".parse().unwrap();
        let message = validate_address_shape(&address).unwrap_err().to_string();
        assert!(message.contains("udp"), "{message}");
        assert!(!message.contains("7777"), "{message}");

        let address = multiaddr!(Dns4("example.com"), Tcp(1u16), Dns4("secret-value"));
        let message = validate_address_shape(&address).unwrap_err().to_string();
        assert!(!message.contains("secret-value"), "{message}");

        let address: Multiaddr = "/unix/secret-path".parse().unwrap();
        let message = validate_address_shape(&address).unwrap_err().to_string();
        assert!(!message.contains("secret-path"), "{message}");
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

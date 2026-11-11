//  Copyright 2021, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    str::FromStr,
};

use anyhow::anyhow;
use log::*;
use serde::{Deserialize, Serialize};
use tari_common::DnsNameServer;
use tari_comms::{
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::{CommsPublicKey, UncompressedCommsPublicKey},
};
use tari_utilities::hex::Hex;

use super::dns::DnsClientError;
use crate::dns::DnsClient;

const LOG_TARGET: &str = "tari::p2p::dns::client";

#[derive(Clone)]
pub struct DnsSeedResolver {
    client: DnsClient,
}

impl DnsSeedResolver {
    /// Connect to DNS host with DNSSEC protection using the default root DNSKEY public keys as trust anchors.
    ///
    /// Records that cannot be proven authentic are rejected, so a resolution either yields validated seed peers or
    /// an error - never unvalidated peers.
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub fn connect_secure(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let client = DnsClient::connect_secure(name_server)?;
        Ok(Self { client })
    }

    /// Connect without DNSSEC protection
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let client = DnsClient::connect(name_server)?;
        Ok(Self { client })
    }

    /// Resolves DNS TXT records and parses them into [`SeedPeer`]s.
    ///
    /// Example TXT record:
    /// ```text
    /// 06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234
    /// ```
    pub async fn resolve(&mut self, addr: &str) -> Result<Vec<SeedPeer>, DnsClientError> {
        let records = self.client.query_txt(addr).await?;
        trace!(target: LOG_TARGET, "DNS records: {:?}", records);
        let peers: Vec<_> = records
            .into_iter()
            .filter_map(|txt| {
                txt.parse()
                    .inspect_err(|err| {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to parse DNS seed peer string: {txt}. Error: {err}"
                        );
                    })
                    .ok()
            })
            .collect();
        trace!(target: LOG_TARGET, "Seed peers: {:?}", peers.iter().map(|p| format!("{}", p)).collect::<Vec<_>>());
        Ok(peers)
    }
}

/// Parsed information from a DNS seed record
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SeedPeer {
    pub public_key: CommsPublicKey,
    pub addresses: Vec<Multiaddr>,
}

impl SeedPeer {
    pub fn new(public_key: CommsPublicKey, addresses: Vec<Multiaddr>) -> Self {
        Self { public_key, addresses }
    }

    #[inline]
    pub fn derive_node_id(&self) -> NodeId {
        NodeId::from_public_key(&self.public_key)
    }
}

impl TryFrom<String> for SeedPeer {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

const MAX_SEED_PEER_STR_LEN: usize = 1024;
const MAX_SEED_ADDRESSES: usize = 10;
const MAX_SEED_SPLIT_ATTEMPTS: usize = 20;

fn parse_seed_addresses(mut rest: &str) -> Result<Vec<Multiaddr>, anyhow::Error> {
    if rest.is_empty() {
        return Err(anyhow!("Invalid address string: empty address"));
    }
    if rest.len() > MAX_SEED_PEER_STR_LEN {
        return Err(anyhow!(
            "Invalid address string: exceeds maximum length of {MAX_SEED_PEER_STR_LEN} bytes"
        ));
    }
    if rest.ends_with("::") {
        return Err(anyhow!("Invalid address string: trailing '::' delimiter"));
    }
    let mut addresses = Vec::new();
    let mut split_attempts: usize = 0;
    loop {
        if addresses.len() >= MAX_SEED_ADDRESSES {
            return Err(anyhow!(
                "Invalid address string: exceeds maximum of {MAX_SEED_ADDRESSES} addresses"
            ));
        }
        let mut split_at = None;
        for (idx, _) in rest.char_indices() {
            if let Some(remainder) = rest[idx..].strip_prefix("::") &&
                remainder.starts_with('/')
            {
                let candidate = &rest[..idx];
                if !candidate.is_empty() {
                    split_attempts = split_attempts.saturating_add(1);
                    if split_attempts > MAX_SEED_SPLIT_ATTEMPTS {
                        return Err(anyhow!("Invalid address string: too many '::' split attempts"));
                    }
                    if let Ok(addr) = Multiaddr::from_str(candidate) &&
                        !addr.is_empty()
                    {
                        split_at = Some((addr, remainder));
                        break;
                    }
                }
            }
        }
        if let Some((addr, remainder)) = split_at {
            addresses.push(addr);
            rest = remainder;
        } else {
            let addr = Multiaddr::from_str(rest).map_err(|e| anyhow!("Invalid address string:{e}"))?;
            if addr.is_empty() {
                return Err(anyhow!("Invalid address string: empty address"));
            }
            addresses.push(addr);
            break;
        }
    }
    Ok(addresses)
}

impl FromStr for SeedPeer {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_SEED_PEER_STR_LEN {
            return Err(anyhow!(
                "Invalid seed peer string: exceeds maximum length of {MAX_SEED_PEER_STR_LEN} bytes"
            ));
        }
        let (part_a, part_b) = s
            .split_once("::")
            .ok_or_else(|| anyhow!("Invalid seed peer string, missing '::' delimiter"))?;
        let public_key = UncompressedCommsPublicKey::from_hex(part_a)
            .ok()
            .ok_or_else(|| anyhow!("Invalid public key string"))?;
        let addresses = parse_seed_addresses(part_b)?;
        Ok(SeedPeer {
            public_key: CommsPublicKey::new_from_pk(public_key),
            addresses,
        })
    }
}

impl Display for SeedPeer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}::{}",
            self.public_key.to_hex(),
            self.addresses
                .iter()
                .map(|ma| ma.to_string())
                .collect::<Vec<_>>()
                .join("::")
        )
    }
}

impl From<SeedPeer> for String {
    fn from(s: SeedPeer) -> Self {
        s.to_string()
    }
}

impl From<SeedPeer> for Peer {
    fn from(seed: SeedPeer) -> Self {
        let node_id = seed.derive_node_id();
        Peer::new(
            seed.public_key,
            node_id,
            MultiaddressesWithStats::from_addresses_with_source(seed.addresses, &PeerAddressSource::Config),
            PeerFlags::default(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        )
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    mod peer_seed {
        use std::time::{Duration, Instant};

        use super::*;

        #[test]
        fn it_parses_single_address() {
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000";
            let seed = SeedPeer::from_str(sample).unwrap();
            assert_eq!(
                seed.public_key.to_hex(),
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
            );
            assert_eq!(seed.addresses.len(), 1);
            assert_eq!(seed.addresses[0].to_string(), "/ip4/127.0.0.1/tcp/8000");
        }

        #[test]
        fn it_parses_multiple_addresses_and_roundtrips_display() {
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::/\
                          ip6/::1/tcp/8001::/ip6/2001:db8::/tcp/8002::/onion3/\
                          bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234";
            let seed = SeedPeer::from_str(sample).unwrap();
            assert_eq!(seed.addresses.len(), 4);
            assert_eq!(seed.addresses[0].to_string(), "/ip4/127.0.0.1/tcp/8000");
            assert_eq!(seed.addresses[1].to_string(), "/ip6/::1/tcp/8001");
            assert_eq!(seed.addresses[2].to_string(), "/ip6/2001:db8::/tcp/8002");
            assert_eq!(
                seed.addresses[3].to_string(),
                "/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234"
            );
            let formatted = seed.to_string();
            assert_eq!(formatted, sample);
            assert_eq!(SeedPeer::from_str(&formatted).unwrap(), seed);
        }

        #[test]
        fn it_errors_if_empty_or_blank() {
            SeedPeer::from_str("").unwrap_err();
            SeedPeer::from_str(" ").unwrap_err();
        }

        #[test]
        fn it_errors_if_not_a_seed_peer() {
            SeedPeer::from_str("nonsensical::garbage").unwrap_err();
        }

        #[test]
        fn it_errors_invalid_public_key() {
            let sample = "16e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000";
            SeedPeer::from_str(sample).unwrap_err();
        }

        #[test]
        fn it_errors_invalid_address() {
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/invalid/tcp/8000";
            SeedPeer::from_str(sample).unwrap_err();
            let empty_addr = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::";
            SeedPeer::from_str(empty_addr).unwrap_err();
            let trailing_delim =
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::";
            SeedPeer::from_str(trailing_delim).unwrap_err();
        }

        #[test]
        fn it_rejects_overlong_and_crafted_p2p_colon_payloads_quickly() {
            let pk = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a";

            let start = Instant::now();
            let overlong_colons = format!("{pk}::/p2p/{}{}", "2".repeat(3000), "::".repeat(1500));
            SeedPeer::from_str(&overlong_colons).unwrap_err();

            let under_cap_colons = format!("{pk}::/p2p/{}{}", "2".repeat(400), "::".repeat(200));
            assert!(under_cap_colons.len() <= MAX_SEED_PEER_STR_LEN);
            SeedPeer::from_str(&under_cap_colons).unwrap_err();

            let under_cap_slash_colons = format!("{pk}::/p2p/{}{}", "2".repeat(400), "::/".repeat(150));
            assert!(under_cap_slash_colons.len() <= MAX_SEED_PEER_STR_LEN);
            SeedPeer::from_str(&under_cap_slash_colons).unwrap_err();

            let too_many_addrs = format!(
                "{pk}::{}",
                (0..=MAX_SEED_ADDRESSES)
                    .map(|i| format!("/ip4/127.0.0.1/tcp/{}", 8000 + i))
                    .collect::<Vec<_>>()
                    .join("::")
            );
            SeedPeer::from_str(&too_many_addrs).unwrap_err();
            assert!(start.elapsed() < Duration::from_millis(100));
        }
    }

    mod peer_seed_resolver {
        use super::*;

        #[tokio::test]
        #[ignore = "Useful for developer testing but will fail unless the DNS has TXT records setup correctly."]
        async fn it_returns_seeds_from_real_address() {
            let mut resolver = DnsSeedResolver::connect(DnsNameServer::System).unwrap();
            let seeds = resolver.resolve("seeds.nextnet.tari.com").await.unwrap();
            println!("{seeds:?}");
            assert!(!seeds.is_empty());
        }
    }
}

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
    utils::text::sanitize_for_display,
};
use tari_utilities::hex::Hex;

use super::dns::DnsClientError;
use crate::dns::DnsClient;

const LOG_TARGET: &str = "tari::p2p::dns::client";

/// The longest part of an unparsable seed record that is logged, in bytes. Unless DNSSEC is enabled, which it is not by
/// default, the record is whatever the name server answered.
const MAX_LOGGED_SEED_RECORD_BYTES: usize = 512;

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
                        warn!(target: LOG_TARGET, "{}", unparsable_seed_record_message(&txt, err));
                    })
                    .ok()
            })
            .collect();
        trace!(target: LOG_TARGET, "Seed peers: {:?}", peers.iter().map(|p| format!("{}", p)).collect::<Vec<_>>());
        Ok(peers)
    }
}

/// The log text for a TXT record that is not a seed peer. The parse error is sanitised as well as the record, because
/// it can quote part of the record (an unknown multiaddr protocol name, for example).
fn unparsable_seed_record_message(txt: &str, err: &anyhow::Error) -> String {
    format!(
        "Failed to parse DNS seed peer string: {}. Error: {}",
        sanitize_for_display(txt, MAX_LOGGED_SEED_RECORD_BYTES),
        sanitize_for_display(&err.to_string(), MAX_LOGGED_SEED_RECORD_BYTES)
    )
}

/// Parsed information from a DNS seed record
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl FromStr for SeedPeer {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (part_a, part_b) = s
            .split_once("::")
            .ok_or_else(|| anyhow!("Invalid seed peer string, missing '::' delimiter"))?;
        let public_key = UncompressedCommsPublicKey::from_hex(part_a)
            .ok()
            .ok_or_else(|| anyhow!("Invalid public key string"))?;
        let addresses = match Multiaddr::from_str(part_b) {
            Ok(addr) => {
                if addr.is_empty() {
                    return Err(anyhow!("Empty address in seed peer string"));
                }
                vec![addr]
            },
            Err(first_err) => {
                let addrs = part_b
                    .split("::")
                    .map(|part| {
                        let addr =
                            Multiaddr::from_str(part).map_err(|_| anyhow!("Invalid address string:{first_err}"))?;
                        if addr.is_empty() {
                            return Err(anyhow!("Empty address in seed peer string"));
                        }
                        Ok(addr)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if addrs.is_empty() {
                    return Err(anyhow!("Empty address in seed peer string"));
                }
                addrs
            },
        };
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
        }

        #[test]
        fn it_parses_multiple_addresses_and_ipv6() {
            let multi = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::/\
                         ip4/127.0.0.1/tcp/8001";
            let seed = SeedPeer::from_str(multi).unwrap();
            assert_eq!(seed.addresses.len(), 2);
            assert_eq!(seed.to_string(), multi);

            let ipv6 = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip6/::1/tcp/8000";
            let seed_v6 = SeedPeer::from_str(ipv6).unwrap();
            assert_eq!(seed_v6.addresses.len(), 1);
            assert_eq!(seed_v6.addresses[0].to_string(), "/ip6/::1/tcp/8000");
        }
    }

    mod unparsable_seed_record_message {
        use super::*;

        const KEY: &str = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a";

        fn assert_display_safe(message: &str) {
            for c in ['\u{1b}', '\r', '\n', '\u{0}', '\u{2028}'] {
                assert!(!message.contains(c), "{c:?} reached the log: {message:?}");
            }
        }

        /// A TXT record that is not a seed peer is logged, and it is whatever the name server answered.
        #[test]
        fn record_is_sanitised_before_it_is_logged() {
            let record = "\r\u{1b}[2K\u{1b}[32m12:00 INFO  Seed peers verified\u{1b}[8m\n\u{0}\u{2028}";
            let err = SeedPeer::from_str(record).unwrap_err();

            let message = unparsable_seed_record_message(record, &err);

            assert_display_safe(&message);
            assert!(
                message.contains("[2K[32m12:00 INFO  Seed peers verified[8m"),
                "{message}"
            );
        }

        /// The parse error quotes an unknown multiaddr protocol name, so it carries the record's bytes too.
        #[test]
        fn parse_error_is_sanitised_before_it_is_logged() {
            let record = format!("{KEY}::/\u{1b}[2J\rforged/tcp/18189");
            let err = SeedPeer::from_str(&record).unwrap_err();
            assert!(err.to_string().contains('\u{1b}'), "{err}");

            let message = unparsable_seed_record_message(&record, &err);

            assert_display_safe(&message);
        }

        #[test]
        fn long_record_is_cut() {
            let record = "x".repeat(4 * MAX_LOGGED_SEED_RECORD_BYTES);
            let err = SeedPeer::from_str(&record).unwrap_err();

            let message = unparsable_seed_record_message(&record, &err);

            assert!(message.len() < 2 * MAX_LOGGED_SEED_RECORD_BYTES, "{}", message.len());
        }

        /// An ordinary bad record reads exactly as before.
        #[test]
        fn ordinary_record_is_unchanged() {
            let record = format!("{KEY}::/ip4/invalid/tcp/8000");
            let err = SeedPeer::from_str(&record).unwrap_err();

            let message = unparsable_seed_record_message(&record, &err);

            assert_eq!(
                message,
                format!("Failed to parse DNS seed peer string: {record}. Error: {err}")
            );
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

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
use serde::{Deserialize, Serialize};
use tari_common::DnsNameServer;
use tari_comms::{
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_utilities::hex::Hex;

use super::dns::DnsClientError;
use crate::dns::{default_trust_anchor, DnsClient};

#[derive(Clone)]
pub struct DnsSeedResolver {
    client: DnsClient,
}

impl DnsSeedResolver {
    /// Connect to DNS host with DNSSEC protection using default root DNSKEY public keys
    /// obtained from root DNS.
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub async fn connect_secure(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let client = DnsClient::connect_secure(name_server, default_trust_anchor()).await?;
        Ok(Self { client })
    }

    /// Connect without DNSSEC protection
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub async fn connect(name_server: DnsNameServer) -> Result<Self, DnsClientError> {
        let client = DnsClient::connect(name_server).await?;
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
        let peers = records.into_iter().filter_map(|txt| txt.parse().ok()).collect();
        Ok(peers)
    }
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
        let mut parts = s.split("::").map(|s| s.trim());
        let public_key = parts
            .next()
            .and_then(|s| CommsPublicKey::from_hex(s).ok())
            .ok_or_else(|| anyhow!("Invalid public key string"))?;
        let addresses = parts.map(Multiaddr::from_str).collect::<Result<Vec<_>, _>>()?;
        if addresses.is_empty() || addresses.iter().any(|a| a.is_empty()) {
            return Err(anyhow!("Empty or invalid address in seed peer string"));
        }
        Ok(SeedPeer { public_key, addresses })
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
            Default::default(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            Default::default(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const TEST_NAME: &str = "test.local.";

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
        fn it_parses_mulitple_addresses() {
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::/\
                          onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234";

            let seed = SeedPeer::from_str(sample).unwrap();
            assert_eq!(
                seed.public_key.to_hex(),
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
            );
            assert_eq!(seed.addresses.len(), 2);
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
        fn it_errors_if_trailing_delim() {
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::";
            SeedPeer::from_str(sample).unwrap_err();
            let sample = "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::";
            SeedPeer::from_str(sample).unwrap_err();
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
        }
    }

    mod peer_seed_resolver {
        use tari_common::configuration::name_server::DEFAULT_DNS_NAME_SERVER;
        use trust_dns_client::{
            proto::{
                op::Query,
                rr::{DNSClass, Name},
                xfer::DnsResponse,
            },
            rr::{rdata, RData, Record, RecordType},
        };

        use super::*;
        use crate::dns::mock;

        #[tokio::test]
        #[ignore = "Useful for developer testing but will fail unless the DNS has TXT records setup correctly."]
        async fn it_returns_seeds_from_real_address() {
            let mut resolver = DnsSeedResolver::connect(DEFAULT_DNS_NAME_SERVER.parse().unwrap())
                .await
                .unwrap();
            let seeds = resolver.resolve("seeds.esmeralda.tari.com").await.unwrap();
            println!("{:?}", seeds);
            assert!(!seeds.is_empty());
        }

        fn create_txt_record(contents: Vec<&str>) -> DnsResponse {
            let mut resp_query = Query::query(Name::from_str(TEST_NAME).unwrap(), RecordType::TXT);
            resp_query.set_query_class(DNSClass::IN);
            let mut record = Record::new();
            record
                .set_record_type(RecordType::TXT)
                .set_data(Some(RData::TXT(rdata::TXT::new(
                    contents.into_iter().map(ToString::to_string).collect(),
                ))));

            mock::message(resp_query, vec![record], vec![], vec![]).into()
        }

        #[tokio::test]
        async fn it_returns_peer_seeds() {
            let records = vec![
                // Multiple addresses(works)
                Ok(create_txt_record(vec![
                    "fab24c542183073996ddf3a6c73ff8b8562fed351d252ec5cb8f269d1ad92f0c::/ip4/127.0.0.1/tcp/8000::/\
                     onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234",
                ])),
                // Misc
                Ok(create_txt_record(vec!["v=spf1 include:_spf.spf.com ~all"])),
                // Single address (works)
                Ok(create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000",
                ])),
                // Single address trailing delim
                Ok(create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::",
                ])),
                // Invalid public key
                Ok(create_txt_record(vec![
                    "07e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000",
                ])),
                // No Address with delim
                Ok(create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::",
                ])),
                // No Address no delim
                Ok(create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a",
                ])),
                // Invalid address
                Ok(create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/invalid:1234",
                ])),
            ];
            let mut resolver = DnsSeedResolver {
                client: DnsClient::connect_mock(records).await.unwrap(),
            };
            let seeds = resolver.resolve(TEST_NAME).await.unwrap();
            assert_eq!(seeds.len(), 2);
            assert_eq!(
                seeds[0].public_key.to_hex(),
                "fab24c542183073996ddf3a6c73ff8b8562fed351d252ec5cb8f269d1ad92f0c"
            );
            assert_eq!(seeds[0].addresses.len(), 2);
            assert_eq!(
                seeds[1].public_key.to_hex(),
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
            );
            assert_eq!(seeds[1].addresses.len(), 1);
        }
    }
}

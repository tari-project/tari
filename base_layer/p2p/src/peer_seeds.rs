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

use super::dns::DnsClientError;
use crate::dns::{default_trust_anchor, DnsClient};
use anyhow::anyhow;
use std::{net::SocketAddr, str::FromStr};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_utilities::hex::Hex;

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
    pub async fn connect_secure(name_server: SocketAddr) -> Result<Self, DnsClientError> {
        let client = DnsClient::connect_secure(name_server, default_trust_anchor()).await?;
        Ok(Self { client })
    }

    /// Connect without DNSSEC protection
    ///
    /// ## Arguments
    /// -`name_server` - the DNS name server to use to resolve records
    pub async fn connect(name_server: SocketAddr) -> Result<Self, DnsClientError> {
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
#[derive(Debug, Clone)]
pub struct SeedPeer {
    pub public_key: CommsPublicKey,
    pub addresses: Vec<Multiaddr>,
}

impl SeedPeer {
    #[inline]
    pub fn get_node_id(&self) -> NodeId {
        NodeId::from_public_key(&self.public_key)
    }
}

impl FromStr for SeedPeer {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split("::").map(|s| s.trim());
        let public_key = parts
            .next()
            .and_then(|s| CommsPublicKey::from_hex(&s).ok())
            .ok_or_else(|| anyhow!("Invalid public key string"))?;
        let addresses = parts.map(Multiaddr::from_str).collect::<Result<Vec<_>, _>>()?;
        if addresses.is_empty() || addresses.iter().any(|a| a.is_empty()) {
            return Err(anyhow!("Empty or invalid address in seed peer string"));
        }
        Ok(SeedPeer { public_key, addresses })
    }
}

impl From<SeedPeer> for Peer {
    fn from(seed: SeedPeer) -> Self {
        let node_id = seed.get_node_id();
        Self::new(
            seed.public_key,
            node_id,
            seed.addresses.into(),
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
    use tari_utilities::hex::Hex;

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

            let seed = SeedPeer::from_str(&sample).unwrap();
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
        use super::*;
        use std::{collections::HashMap, iter::FromIterator};
        use trust_dns_client::rr::{rdata, RData, Record, RecordType};

        #[ignore = "This test requires network IO and is mostly useful during development"]
        #[tokio_macros::test]
        async fn it_returns_an_empty_vec_if_all_seeds_are_invalid() {
            let mut resolver = DnsSeedResolver {
                client: DnsClient::connect("1.1.1.1:53".parse().unwrap()).await.unwrap(),
            };
            let seeds = resolver.resolve("tari.com").await.unwrap();
            assert!(seeds.is_empty());
        }

        fn create_txt_record(contents: Vec<&str>) -> Record {
            let mut record = Record::new();
            record
                .set_record_type(RecordType::TXT)
                .set_rdata(RData::TXT(rdata::TXT::new(
                    contents.into_iter().map(ToString::to_string).collect(),
                )));
            record
        }

        #[tokio_macros::test]
        async fn it_returns_peer_seeds() {
            let records = HashMap::from_iter([("test.local.", vec![
                // Multiple addresses(works)
                create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::/\
                     onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:1234",
                ]),
                // Misc
                create_txt_record(vec!["v=spf1 include:_spf.spf.com ~all"]),
                // Single address (works)
                create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000",
                ]),
                // Single address trailing delim
                create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000::",
                ]),
                // Invalid public key
                create_txt_record(vec![
                    "07e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/ip4/127.0.0.1/tcp/8000",
                ]),
                // No Address with delim
                create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::",
                ]),
                // No Address no delim
                create_txt_record(vec!["06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"]),
                // Invalid address
                create_txt_record(vec![
                    "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a::/onion3/invalid:1234",
                ]),
            ])]);
            let mut resolver = DnsSeedResolver {
                client: DnsClient::connect_mock(records).await.unwrap(),
            };
            let seeds = resolver.resolve("test.local.").await.unwrap();
            assert_eq!(seeds.len(), 2);
            assert_eq!(
                seeds[0].public_key.to_hex(),
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
            );
            assert_eq!(
                seeds[1].public_key.to_hex(),
                "06e98e9c5eb52bd504836edec1878eccf12eb9f26a5fe5ec0e279423156e657a"
            );
            assert_eq!(seeds[0].addresses.len(), 2);
            assert_eq!(seeds[1].addresses.len(), 1);
        }
    }
}

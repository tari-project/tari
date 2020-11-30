//  Copyright 2020, The Tari Project
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

use anyhow::anyhow;
use std::str::FromStr;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_utilities::hex::Hex;

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
        if addresses.is_empty() || addresses.iter().any(|a| a.len() == 0) {
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

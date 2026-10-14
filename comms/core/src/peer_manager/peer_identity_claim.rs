//  Copyright 2022. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use multiaddr::Multiaddr;
use serde_derive::{Deserialize, Serialize};

use crate::{
    peer_manager::{IdentitySignature, PeerFeatures, PeerManagerError},
    proto::identity::PeerIdentityMsg,
    types::CommsPublicKey,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdentityClaim {
    #[serde(with = "claim_addresses_serde")]
    pub addresses: Vec<Multiaddr>,
    pub features: PeerFeatures,
    pub signature: IdentitySignature,
}

impl PeerIdentityClaim {
    pub fn new(addresses: Vec<Multiaddr>, features: PeerFeatures, signature: IdentitySignature) -> Self {
        Self {
            addresses,
            features,
            signature,
        }
    }

    /// Verify the claim's identity signature for the given public key. Returns
    /// `Err(PeerManagerError::InvalidIdentitySignature)` if it is not valid.
    pub fn verify(&self, public_key: &CommsPublicKey) -> Result<(), PeerManagerError> {
        self.signature.verify(public_key, self.features, &self.addresses)
    }
}

/// Serde for the claimed addresses, which are stored in the peer database inside each address's source.
///
/// The text form of a multiaddr is not lossless: a DNS name may contain `/`, which reads back as a different address
/// or not at all, and the claim's signature covers the binary form. Human-readable formats therefore carry each
/// address as the hex of its bytes. Text written by earlier versions (always starting with `/`) is still accepted, and
/// is rewritten as hex the next time the row is saved. Binary formats already used the bytes and are unchanged.
mod claim_addresses_serde {
    use multiaddr::Multiaddr;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error, ser::SerializeSeq};
    use tari_utilities::hex::{from_hex, to_hex};

    pub fn serialize<S: Serializer>(addresses: &[Multiaddr], serializer: S) -> Result<S::Ok, S::Error> {
        if !serializer.is_human_readable() {
            return addresses.serialize(serializer);
        }
        let mut seq = serializer.serialize_seq(Some(addresses.len()))?;
        for address in addresses {
            seq.serialize_element(&to_hex(&address.to_vec()))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Multiaddr>, D::Error> {
        if !deserializer.is_human_readable() {
            return Vec::<Multiaddr>::deserialize(deserializer);
        }
        let items = Vec::<String>::deserialize(deserializer)?;
        let mut addresses = Vec::with_capacity(items.len());
        for item in items {
            let address = if item.starts_with('/') {
                item.parse::<Multiaddr>().map_err(D::Error::custom)?
            } else {
                let bytes = from_hex(&item).map_err(D::Error::custom)?;
                Multiaddr::try_from(bytes).map_err(D::Error::custom)?
            };
            addresses.push(address);
        }
        Ok(addresses)
    }
}

impl TryFrom<PeerIdentityMsg> for PeerIdentityClaim {
    type Error = PeerManagerError;

    fn try_from(value: PeerIdentityMsg) -> Result<Self, Self::Error> {
        let addresses = value
            .addresses
            .into_iter()
            .map(Multiaddr::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PeerManagerError::MultiaddrError(e.to_string()))?;

        if addresses.is_empty() {
            return Err(PeerManagerError::PeerIdentityNoValidAddresses);
        }
        let features = PeerFeatures::from_bits(value.features).ok_or(PeerManagerError::ProtocolError(format!(
            "Invalid message flag, does not match any flags ({})",
            value.features
        )))?;

        if let Some(signature) = value.identity_signature {
            Ok(Self {
                addresses,
                features,
                signature: signature.try_into()?,
            })
        } else {
            Err(PeerManagerError::MissingIdentitySignature)
        }
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use multiaddr::multiaddr;

    use super::*;
    use crate::peer_manager::NodeIdentity;

    fn claim(addresses: Vec<Multiaddr>) -> PeerIdentityClaim {
        let node_identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            addresses.clone(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        PeerIdentityClaim {
            addresses,
            features: PeerFeatures::COMMUNICATION_NODE,
            signature: node_identity.identity_signature_read().clone().unwrap(),
        }
    }

    #[test]
    fn claim_addresses_round_trip_through_json_as_bytes() {
        // A name the text form cannot represent
        let lossy = multiaddr!(Dns4("a/tcp/80/ip4/10.0.0.1"), Tcp(1u16));
        let plain: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let claim = claim(vec![plain.clone(), lossy.clone()]);

        let json = serde_json::to_string(&claim).unwrap();
        assert!(!json.contains("/ip4/"), "{json}");
        let decoded: PeerIdentityClaim = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, claim);
        assert_eq!(decoded.addresses, vec![plain, lossy]);
    }

    #[test]
    fn claim_addresses_accept_the_old_text_form() {
        let address: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let claim = claim(vec![address.clone()]);
        let mut json: serde_json::Value = serde_json::to_value(&claim).unwrap();
        json["addresses"] = serde_json::json!(["/ip4/23.23.23.23/tcp/18189"]);

        let decoded: PeerIdentityClaim = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.addresses, vec![address]);
        assert_eq!(decoded, claim);
    }
}

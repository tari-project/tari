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
    #[serde(serialize_with = "claim_addresses_serde::serialize")]
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

/// Serialization for the claimed addresses, which are stored in the peer database inside each address's source.
///
/// Human-readable formats keep the text form that every version of the node writes and reads, so a node downgraded to
/// an older version can still read every stored claim. The text form is not lossless, though: a DNS name containing
/// `/` reads back as a different address or not at all. Such an address is left out of the stored claim with a
/// warning. Claim validation already rejects these addresses, so this is only a backstop. Binary formats use the
/// address bytes and are unchanged. Deserialization is `Multiaddr`'s own.
mod claim_addresses_serde {
    use std::str::FromStr;

    use log::warn;
    use multiaddr::Multiaddr;
    use serde::{Serialize, Serializer, ser::SerializeSeq};

    const LOG_TARGET: &str = "comms::peer_manager::peer_identity_claim";

    pub fn serialize<S: Serializer>(addresses: &[Multiaddr], serializer: S) -> Result<S::Ok, S::Error> {
        if !serializer.is_human_readable() {
            return addresses.serialize(serializer);
        }
        let mut lossless = Vec::with_capacity(addresses.len());
        for address in addresses {
            let text = address.to_string();
            let reads_back = Multiaddr::from_str(&text).is_ok_and(|parsed| parsed.to_vec() == address.to_vec());
            if reads_back {
                lossless.push(text);
            } else {
                warn!(
                    target: LOG_TARGET,
                    "Leaving claimed address {text:?} out of the stored claim: its text form does not read back as the \
                     same address"
                );
            }
        }
        let mut seq = serializer.serialize_seq(Some(lossless.len()))?;
        for text in &lossless {
            seq.serialize_element(text)?;
        }
        seq.end()
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
    fn claim_addresses_are_stored_as_text() {
        let first: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let second: Multiaddr = "/dns4/node.example.com/tcp/18189".parse().unwrap();
        let claim = claim(vec![first.clone(), second.clone()]);

        let json: serde_json::Value = serde_json::to_value(&claim).unwrap();
        assert_eq!(
            json["addresses"],
            serde_json::json!(["/ip4/23.23.23.23/tcp/18189", "/dns4/node.example.com/tcp/18189"])
        );
        let decoded: PeerIdentityClaim = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, claim);
    }

    #[test]
    fn claim_address_without_a_lossless_text_form_is_not_stored() {
        // Reads back as a different address
        let lossy = multiaddr!(Dns4("a/tcp/80/ip4/10.0.0.1"), Tcp(1u16));
        // Does not read back at all
        let unparsable = multiaddr!(Dns4("evil/x"), Tcp(1u16));
        let plain: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let claim = claim(vec![lossy, plain.clone(), unparsable]);

        let json = serde_json::to_string(&claim).unwrap();
        let decoded: PeerIdentityClaim = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.addresses, vec![plain]);
    }
}

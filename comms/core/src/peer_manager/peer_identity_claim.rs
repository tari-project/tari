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

use borsh::{BorshDeserialize, BorshSerialize};
use multiaddr::Multiaddr;
use serde_derive::{Deserialize, Serialize};
use tari_utilities::ByteArrayError;

use crate::{
    peer_manager::{IdentitySignature, PeerFeatures, PeerManagerError},
    proto::identity::PeerIdentityMsg,
    types::CommsPublicKey,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdentityClaim {
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

    pub fn is_valid(&self, public_key: &CommsPublicKey) -> Result<bool, ByteArrayError> {
        self.signature.is_valid(public_key, self.features, &self.addresses)
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

impl BorshSerialize for PeerIdentityClaim {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let addr_strings: Vec<String> = self.addresses.iter().map(|a| a.to_string()).collect();
        addr_strings.serialize(writer)?;

        self.features.bits().serialize(writer)?;

        self.signature.serialize(writer)?;

        Ok(())
    }
}

impl BorshDeserialize for PeerIdentityClaim {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let addr_strings = Vec::<String>::deserialize_reader(reader)?;
        let addresses = addr_strings
            .into_iter()
            .map(|s| s.parse::<Multiaddr>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let feature_bits = u32::deserialize_reader(reader)?;
        let features = PeerFeatures::from_bits(feature_bits)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid PeerFeatures bits"))?;

        let signature = IdentitySignature::deserialize_reader(reader)?;

        Ok(Self {
            addresses,
            features,
            signature,
        })
    }
}

#[cfg(test)]
pub fn create_test_peer_identity_claim(features: PeerFeatures) -> PeerIdentityClaim {
    use std::str::FromStr;

    use chrono::Utc;
    use multiaddr::Multiaddr;
    use rand::{rngs::OsRng, Rng};
    use tari_crypto::keys::SecretKey;

    use crate::{
        peer_manager::{IdentitySignature, PeerIdentityClaim},
        types::CommsSecretKey,
    };

    let secret = CommsSecretKey::random(&mut OsRng);
    let address_1 = Multiaddr::from_str(&format!(
        "/ip4/127.0.0.1/tcp/{}",
        rand::thread_rng().gen_range(5000..9000)
    ))
    .unwrap();
    let address_2 = Multiaddr::from_str(&format!(
        "/ip4/54.36.113.0/tcp/{}",
        rand::thread_rng().gen_range(5000..9000)
    ))
    .unwrap();
    let address_3 = Multiaddr::from_str(&format!(
        "/ip6/2001:41d0:40d:4300::/tcp/{}",
        rand::thread_rng().gen_range(5000..9000)
    ))
    .unwrap();
    let address_4 = Multiaddr::from_str(&format!(
        "/onion3/bukg4svrs4r3hdtx4s2vle6ekipi4v7bshenfwjalymvax7akivyhkyd:{}",
        rand::thread_rng().gen_range(5000..9000)
    ))
    .unwrap();
    let updated_at = Utc::now();
    let addresses = vec![address_1, address_2, address_3, address_4];
    let signature = IdentitySignature::sign_new(&secret, features, &addresses, updated_at);

    PeerIdentityClaim {
        addresses,
        features,
        signature,
    }
}

#[cfg(test)]
mod test {

    use borsh::{BorshDeserialize, BorshSerialize};

    use crate::peer_manager::{create_test_peer_identity_claim, PeerFeatures, PeerIdentityClaim};

    #[test]
    fn test_borsh_serialize_deserialize() {
        for _i in 0..1000 {
            let claim = create_test_peer_identity_claim(PeerFeatures::COMMUNICATION_NODE);

            // Serialize the claim
            let mut serialized_data = Vec::new();
            BorshSerialize::serialize(&claim, &mut serialized_data).unwrap();

            // Deserialize the claim
            let deserialized_claim = PeerIdentityClaim::deserialize_reader(&mut serialized_data.as_slice()).unwrap();

            // Assert equality
            assert_eq!(
                claim, deserialized_claim,
                "Deserialized object does not match the original"
            );
        }
    }
}

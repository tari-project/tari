// Copyright 2019, The Tari Project
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

use std::{
    convert::{TryFrom, TryInto},
    fmt,
};

use anyhow::anyhow;
use chrono::{DateTime, NaiveDateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
    types::{CommsPublicKey, CommsSecretKey, Signature},
    NodeIdentity,
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_utilities::{hex::Hex, ByteArray, ByteArrayError};
use thiserror::Error;

use crate::{
    proto::dht::{DiscoveryMessage, JoinMessage},
    rpc::{PeerInfo, PeerInfoAddress},
};

pub mod common {
    tari_comms::outdir_include!("tari.dht.common.rs");
}

pub mod envelope {
    tari_comms::outdir_include!("tari.dht.envelope.rs");
}

pub mod dht {
    use super::common;
    tari_comms::outdir_include!("tari.dht.rs");
}

pub mod rpc {
    tari_comms::outdir_include!("tari.dht.rpc.rs");
}

pub mod store_forward {
    tari_comms::outdir_include!("tari.dht.store_forward.rs");
}

pub mod message_header {
    tari_comms::outdir_include!("tari.dht.message_header.rs");
}

//---------------------------------- JoinMessage --------------------------------------------//

impl<T: AsRef<NodeIdentity>> From<T> for JoinMessage {
    fn from(identity: T) -> Self {
        let node_identity = identity.as_ref();
        Self {
            public_key: node_identity.public_key().to_vec(),
            addresses: node_identity.public_addresses().iter().map(|a| a.to_vec()).collect(),
            peer_features: node_identity.features().bits(),
            nonce: OsRng.next_u64(),
            identity_signature: node_identity.identity_signature_read().as_ref().map(Into::into),
        }
    }
}

impl fmt::Display for dht::JoinMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "JoinMessage(PK = {}, Addresses = {:?}, Features = {:?})",
            self.public_key.to_hex(),
            self.addresses,
            PeerFeatures::from_bits_truncate(self.peer_features),
        )
    }
}

//---------------------------------- Rpc Message Conversions --------------------------------------------//

#[derive(Debug, Error, PartialEq)]
enum PeerInfoConvertError {
    #[error("Could not convert into byte array: `{0}`")]
    ByteArrayError(String),
}

impl From<ByteArrayError> for PeerInfoConvertError {
    fn from(e: ByteArrayError) -> Self {
        PeerInfoConvertError::ByteArrayError(e.to_string())
    }
}

impl TryFrom<DiscoveryMessage> for PeerInfo {
    type Error = anyhow::Error;

    fn try_from(value: DiscoveryMessage) -> Result<Self, Self::Error> {
        let identity_signature = value
            .identity_signature
            .ok_or_else(|| anyhow!("DiscoveryMessage missing peer_identity_claim"))?
            .try_into()?;

        let identity_claim = PeerIdentityClaim {
            addresses: value
                .addresses
                .iter()
                .map(|a| Multiaddr::try_from(a.clone()))
                .collect::<Result<_, _>>()?,
            features: PeerFeatures::from_bits_truncate(value.peer_features),
            signature: identity_signature,
            unverified_data: None,
        };

        Ok(Self {
            public_key: RistrettoPublicKey::from_bytes(&value.public_key)
                .map_err(|e| PeerInfoConvertError::ByteArrayError(format!("{}", e)))?,
            addresses: value
                .addresses
                .iter()
                .map(|a| {
                    Ok(PeerInfoAddress {
                        address: Multiaddr::try_from(a.clone())?,
                        peer_identity_claim: identity_claim.clone(),
                    })
                })
                .collect::<Result<_, Self::Error>>()?,
            peer_features: PeerFeatures::from_bits_truncate(value.peer_features),
            supported_protocols: vec![],
            user_agent: "".to_string(),
        })
    }
}

impl From<PeerInfo> for rpc::PeerInfo {
    fn from(value: PeerInfo) -> Self {
        Self {
            public_key: value.public_key.to_vec(),
            addresses: value.addresses.into_iter().map(Into::into).collect(),
            peer_features: value.peer_features.bits(),
            supported_protocols: value
                .supported_protocols
                .into_iter()
                .map(|b| b.as_ref().to_vec())
                .collect(),
            user_agent: value.user_agent,
        }
    }
}

impl From<PeerInfoAddress> for rpc::PeerInfoAddress {
    fn from(value: PeerInfoAddress) -> Self {
        Self {
            address: value.address.to_vec(),
            peer_identity_claim: Some(value.peer_identity_claim.into()),
        }
    }
}

impl From<PeerIdentityClaim> for rpc::PeerIdentityClaim {
    fn from(value: PeerIdentityClaim) -> Self {
        Self {
            addresses: value.addresses.iter().map(|a| a.to_vec()).collect(),
            peer_features: value.features.bits(),
            identity_signature: Some((&value.signature).into()),
        }
    }
}

impl TryInto<PeerInfo> for rpc::PeerInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<PeerInfo, Self::Error> {
        let public_key = CommsPublicKey::from_bytes(&self.public_key)
            .map_err(|e| PeerInfoConvertError::ByteArrayError(format!("{}", e)))?;
        let addresses = self
            .addresses
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let peer_features = PeerFeatures::from_bits_truncate(self.peer_features);
        let supported_protocols = self
            .supported_protocols
            .into_iter()
            .map(|b| b.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PeerInfo {
            public_key,
            addresses,
            peer_features,
            user_agent: self.user_agent,
            supported_protocols,
        })
    }
}

impl TryInto<PeerInfoAddress> for rpc::PeerInfoAddress {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<PeerInfoAddress, Self::Error> {
        let address = Multiaddr::try_from(self.address)?;
        let peer_identity_claim = self
            .peer_identity_claim
            .ok_or_else(|| anyhow::anyhow!("Missing peer identity claim"))?
            .try_into()?;

        Ok(PeerInfoAddress {
            address,
            peer_identity_claim,
        })
    }
}

impl TryInto<PeerIdentityClaim> for rpc::PeerIdentityClaim {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<PeerIdentityClaim, Self::Error> {
        let addresses = self
            .addresses
            .into_iter()
            .filter_map(|addr| Multiaddr::try_from(addr).ok())
            .collect::<Vec<_>>();

        let features = PeerFeatures::from_bits_truncate(self.peer_features);
        let signature = self
            .identity_signature
            .map(TryInto::try_into)
            .ok_or_else(|| anyhow::anyhow!("No signature"))??;
        Ok(PeerIdentityClaim {
            addresses,
            features,
            signature,
            unverified_data: None,
        })
    }
}

impl TryFrom<common::IdentitySignature> for IdentitySignature {
    type Error = anyhow::Error;

    fn try_from(value: common::IdentitySignature) -> Result<Self, Self::Error> {
        let version = u8::try_from(value.version)
            .map_err(|_| anyhow::anyhow!("Invalid peer identity signature version {}", value.version))?;
        let public_nonce = CommsPublicKey::from_bytes(&value.public_nonce)
            .map_err(|e| PeerInfoConvertError::ByteArrayError(format!("{}", e)))?;
        let signature = CommsSecretKey::from_bytes(&value.signature)
            .map_err(|e| PeerInfoConvertError::ByteArrayError(format!("{}", e)))?;
        let updated_at = NaiveDateTime::from_timestamp_opt(value.updated_at, 0)
            .ok_or_else(|| anyhow::anyhow!("updated_at overflowed"))?;
        let updated_at = DateTime::<Utc>::from_utc(updated_at, Utc);

        Ok(Self::new(version, Signature::new(public_nonce, signature), updated_at))
    }
}

impl From<&IdentitySignature> for common::IdentitySignature {
    fn from(identity_sig: &IdentitySignature) -> Self {
        common::IdentitySignature {
            version: u32::from(identity_sig.version()),
            signature: identity_sig.signature().get_signature().to_vec(),
            public_nonce: identity_sig.signature().get_public_nonce().to_vec(),
            updated_at: identity_sig.updated_at().timestamp(),
        }
    }
}

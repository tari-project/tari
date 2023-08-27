//  Copyright 2021, The Taiji Project
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

use chrono::{DateTime, NaiveDateTime, Utc};
use prost::Message;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_crypto::{hashing::DomainSeparatedHasher, keys::PublicKey as PublicKeyTrait};
use tari_utilities::ByteArray;

use super::hashing::{comms_core_peer_manager_domain, CommsCorePeerManagerDomain, IDENTITY_SIGNATURE};
use crate::{
    message::MessageExt,
    multiaddr::Multiaddr,
    peer_manager::{PeerFeatures, PeerManagerError},
    proto,
    types::{CommsChallenge, CommsPublicKey, CommsSecretKey, Signature},
};

/// Signature that secures the peer identity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentitySignature {
    version: u8,
    signature: Signature,
    updated_at: DateTime<Utc>,
}

impl IdentitySignature {
    /// The latest version of the Identity Signature.
    pub const LATEST_VERSION: u8 = 0;

    pub fn new(version: u8, signature: Signature, updated_at: DateTime<Utc>) -> Self {
        Self {
            version,
            signature,
            updated_at,
        }
    }

    pub(crate) fn sign_new<'a, I: IntoIterator<Item = &'a Multiaddr>>(
        secret_key: &CommsSecretKey,
        features: PeerFeatures,
        addresses: I,
        updated_at: DateTime<Utc>,
    ) -> Self {
        let public_key = CommsPublicKey::from_secret_key(secret_key);
        let (secret_nonce, public_nonce) = CommsPublicKey::random_keypair(&mut OsRng);
        let challenge = Self::construct_challenge(
            &public_key,
            &public_nonce,
            Self::LATEST_VERSION,
            features,
            addresses,
            updated_at,
        )
        .finalize();
        let signature = Signature::sign_raw(secret_key, secret_nonce, challenge.as_ref())
            .expect("unreachable panic: challenge hash digest is the correct length");
        Self {
            version: Self::LATEST_VERSION,
            signature,
            updated_at,
        }
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn is_valid<'a, I: IntoIterator<Item = &'a Multiaddr>>(
        &self,
        public_key: &CommsPublicKey,
        features: PeerFeatures,
        addresses: I,
    ) -> bool {
        // A negative timestamp is considered invalid
        if self.updated_at.timestamp() < 0 {
            return false;
        }
        // Do not accept timestamp more than 1 day in the future
        if self.updated_at > Utc::now() + chrono::Duration::days(1) {
            return false;
        }

        let challenge = Self::construct_challenge(
            public_key,
            self.signature.get_public_nonce(),
            self.version,
            features,
            addresses,
            self.updated_at,
        )
        .finalize();
        self.signature.verify_challenge(public_key, challenge.as_ref())
    }

    fn construct_challenge<'a, I: IntoIterator<Item = &'a Multiaddr>>(
        public_key: &CommsPublicKey,
        public_nonce: &CommsPublicKey,
        version: u8,
        features: PeerFeatures,
        addresses: I,
        updated_at: DateTime<Utc>,
    ) -> DomainSeparatedHasher<CommsChallenge, CommsCorePeerManagerDomain> {
        // e = H(P||R||m)
        let challenge = comms_core_peer_manager_domain::<CommsChallenge>(IDENTITY_SIGNATURE)
            .chain(public_key.as_bytes())
            .chain(public_nonce.as_bytes())
            .chain(version.to_le_bytes())
            .chain(u64::try_from(updated_at.timestamp()).unwrap().to_le_bytes())
            .chain(features.bits().to_le_bytes());
        addresses
            .into_iter()
            .fold(challenge, |challenge, addr| challenge.chain(addr))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        proto::identity::IdentitySignature::from(self).to_encoded_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PeerManagerError> {
        let sig = proto::identity::IdentitySignature::decode(bytes)
            .map_err(|_| PeerManagerError::InvalidIdentitySignature)?
            .try_into()?;
        Ok(sig)
    }
}

impl TryFrom<proto::identity::IdentitySignature> for IdentitySignature {
    type Error = PeerManagerError;

    fn try_from(value: proto::identity::IdentitySignature) -> Result<Self, Self::Error> {
        let version = u8::try_from(value.version).map_err(|_| PeerManagerError::InvalidIdentitySignature)?;
        let public_nonce =
            CommsPublicKey::from_bytes(&value.public_nonce).map_err(|_| PeerManagerError::InvalidIdentitySignature)?;
        let signature =
            CommsSecretKey::from_bytes(&value.signature).map_err(|_| PeerManagerError::InvalidIdentitySignature)?;
        let updated_at =
            NaiveDateTime::from_timestamp_opt(value.updated_at, 0).ok_or(PeerManagerError::InvalidIdentitySignature)?;
        let updated_at = DateTime::<Utc>::from_utc(updated_at, Utc);

        Ok(Self {
            version,
            signature: Signature::new(public_nonce, signature),
            updated_at,
        })
    }
}

impl From<&IdentitySignature> for proto::identity::IdentitySignature {
    fn from(identity_sig: &IdentitySignature) -> Self {
        proto::identity::IdentitySignature {
            version: u32::from(identity_sig.version),
            signature: identity_sig.signature.get_signature().to_vec(),
            public_nonce: identity_sig.signature.get_public_nonce().to_vec(),
            updated_at: identity_sig.updated_at.timestamp(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use tari_crypto::keys::{PublicKey, SecretKey};

    use super::*;

    mod is_valid_for_peer {
        use super::*;

        #[test]
        fn it_returns_true_for_valid_signature() {
            let secret = CommsSecretKey::random(&mut OsRng);
            let public_key = CommsPublicKey::from_secret_key(&secret);
            let address = Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap();
            let updated_at = Utc::now();
            let identity =
                IdentitySignature::sign_new(&secret, PeerFeatures::COMMUNICATION_NODE, [&address], updated_at);
            assert!(
                identity.is_valid(&public_key, PeerFeatures::COMMUNICATION_NODE, [&address]),
                "Signature is not valid"
            );
        }

        #[test]
        fn it_returns_false_for_tampered_address() {
            let secret = CommsSecretKey::random(&mut OsRng);
            let public_key = CommsPublicKey::from_secret_key(&secret);
            let address = Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap();
            let updated_at = Utc::now();
            let identity =
                IdentitySignature::sign_new(&secret, PeerFeatures::COMMUNICATION_NODE, [&address], updated_at);

            let tampered = Multiaddr::from_str("/ip4/127.0.0.1/tcp/4321").unwrap();
            assert!(
                !identity.is_valid(&public_key, PeerFeatures::COMMUNICATION_NODE, [&tampered]),
                "Signature is not valid"
            );
        }

        #[test]
        fn it_returns_false_for_tampered_features() {
            let secret = CommsSecretKey::random(&mut OsRng);
            let public_key = CommsPublicKey::from_secret_key(&secret);
            let address = Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap();
            let updated_at = Utc::now();
            let identity =
                IdentitySignature::sign_new(&secret, PeerFeatures::COMMUNICATION_NODE, [&address], updated_at);

            let tampered = PeerFeatures::COMMUNICATION_CLIENT;

            assert!(
                !identity.is_valid(&public_key, tampered, [&address]),
                "Signature is not valid"
            );
        }
    }
}

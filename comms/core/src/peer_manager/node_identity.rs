//  Copyright 2019 The Taiji Project
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
    fmt,
    sync::{RwLock, RwLockReadGuard},
};

use chrono::Utc;
use multiaddr::Multiaddr;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    tari_utilities::hex::serialize_to_hex,
};

use super::node_id::deserialize_node_id_from_hex;
use crate::{
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        identity_signature::IdentitySignature,
        node_id::NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerIdentityClaim,
    },
    types::{CommsPublicKey, CommsSecretKey},
};

/// The public and private identity of this node on the network
#[derive(Serialize, Deserialize)]
pub struct NodeIdentity {
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    node_id: NodeId,
    public_key: CommsPublicKey,
    features: PeerFeatures,
    secret_key: CommsSecretKey,
    public_addresses: RwLock<Vec<Multiaddr>>,
    #[serde(default = "rwlock_none")]
    identity_signature: RwLock<Option<IdentitySignature>>,
}

fn rwlock_none() -> RwLock<Option<IdentitySignature>> {
    RwLock::new(None)
}

impl NodeIdentity {
    /// Create a new NodeIdentity from the provided key pair and control service address
    pub fn new(secret_key: CommsSecretKey, public_addresses: Vec<Multiaddr>, features: PeerFeatures) -> Self {
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key);

        let node_identity = NodeIdentity {
            node_id,
            public_key,
            features,
            secret_key,
            public_addresses: RwLock::new(public_addresses),
            identity_signature: RwLock::new(None),
        };
        node_identity.sign();
        node_identity
    }

    /// Create a new NodeIdentity from the provided key pair and control service address.
    ///
    /// # Unchecked
    /// It is up to the caller to ensure that the given signature is valid for the node identity.
    /// Prefer using NodeIdentity::new over this function.
    pub fn with_signature_unchecked(
        secret_key: CommsSecretKey,
        public_addresses: Vec<Multiaddr>,
        features: PeerFeatures,
        identity_signature: Option<IdentitySignature>,
    ) -> Self {
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key);

        NodeIdentity {
            node_id,
            public_key,
            features,
            secret_key,
            public_addresses: RwLock::new(public_addresses),
            identity_signature: RwLock::new(identity_signature),
        }
    }

    /// Generates a new random NodeIdentity for CommsPublicKey
    pub fn random<R>(rng: &mut R, public_address: Multiaddr, features: PeerFeatures) -> Self
    where R: CryptoRng + Rng {
        let secret_key = CommsSecretKey::random(rng);
        Self::new(secret_key, vec![public_address], features)
    }

    pub fn random_multiple_addresses<R>(rng: &mut R, public_addresses: Vec<Multiaddr>, features: PeerFeatures) -> Self
    where R: CryptoRng + Rng {
        let secret_key = CommsSecretKey::random(rng);
        Self::new(secret_key, public_addresses, features)
    }

    /// Retrieve the publicly accessible address that peers must connect to establish a connection
    pub fn public_addresses(&self) -> Vec<Multiaddr> {
        acquire_read_lock!(self.public_addresses).clone()
    }

    pub fn first_public_address(&self) -> Option<Multiaddr> {
        acquire_read_lock!(self.public_addresses).get(0).cloned()
    }

    /// Modify the public address.
    pub fn add_public_address(&self, address: Multiaddr) {
        let mut must_sign = false;
        {
            let mut lock = acquire_write_lock!(self.public_addresses);
            if !lock.contains(&address) {
                lock.push(address);
                must_sign = true;
            }
        }
        if must_sign {
            self.sign()
        }
    }

    /// Set the available addresses. If none of the addresses have changed, the identity signature remains unchanged.
    pub fn set_public_addresses(&self, addresses: Vec<Multiaddr>) {
        let mut must_sign = false;
        {
            let mut lock = acquire_write_lock!(self.public_addresses);
            if addresses.len() != lock.len() || addresses.iter().any(|a| !lock.contains(a)) {
                lock.clear();
                lock.extend(addresses);
                must_sign = true;
            }
        }
        if must_sign {
            self.sign()
        }
    }

    /// Modify the peer features
    pub fn set_peer_features(&mut self, features: PeerFeatures) {
        let must_sign = features != self.features;
        self.features = features;
        if must_sign {
            self.sign()
        }
    }

    /// This returns a random NodeIdentity for testing purposes. This function can panic. If public_address
    /// is None, 127.0.0.1:9000 will be used (i.e. the caller doesn't care what the control_service_address is).
    #[cfg(test)]
    pub fn random_for_test(public_address: Option<Multiaddr>, features: PeerFeatures) -> Self {
        Self::random(
            &mut rand::rngs::OsRng,
            public_address
                .or_else(|| "/ip4/127.0.0.1/tcp/9000".parse().ok())
                .unwrap(),
            features,
        )
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn public_key(&self) -> &CommsPublicKey {
        &self.public_key
    }

    pub fn secret_key(&self) -> &CommsSecretKey {
        &self.secret_key
    }

    pub fn features(&self) -> PeerFeatures {
        self.features
    }

    pub fn has_peer_features(&self, peer_features: PeerFeatures) -> bool {
        self.features().contains(peer_features)
    }

    pub fn identity_signature_read(&self) -> RwLockReadGuard<'_, Option<IdentitySignature>> {
        acquire_read_lock!(self.identity_signature)
    }

    pub fn is_signed(&self) -> bool {
        self.identity_signature_read().is_some()
    }

    /// Signs the peer using the peer secret key and replaces the peer account signature.
    pub fn sign(&self) {
        let identity_sig = IdentitySignature::sign_new(
            self.secret_key(),
            self.features,
            &*acquire_read_lock!(self.public_addresses),
            Utc::now(),
        );

        *acquire_write_lock!(self.identity_signature) = Some(identity_sig);
    }

    /// Returns a Peer with the same public key, node id, public address and features as represented in this
    /// NodeIdentity. _NOTE: PeerFlags, supported_protocols and user agent are empty._
    pub fn to_peer(&self) -> Peer {
        let peer_identity_claim = PeerIdentityClaim {
            addresses: self.public_addresses(),
            features: self.features,
            signature: IdentitySignature::sign_new(
                &self.secret_key,
                self.features,
                &self.public_addresses(),
                Utc::now(),
            ),
            unverified_data: None,
        };
        let peer = Peer::new(
            self.public_key().clone(),
            self.node_id().clone(),
            MultiaddressesWithStats::from_addresses_with_source(
                self.public_addresses(),
                &PeerAddressSource::FromNodeIdentity { peer_identity_claim },
            ),
            PeerFlags::empty(),
            self.features(),
            Default::default(),
            Default::default(),
        );

        peer
    }
}

impl Clone for NodeIdentity {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id.clone(),
            public_key: self.public_key.clone(),
            features: self.features,
            secret_key: self.secret_key.clone(),
            public_addresses: RwLock::new(self.public_addresses()),
            identity_signature: RwLock::new(self.identity_signature_read().as_ref().cloned()),
        }
    }
}

impl fmt::Display for NodeIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Public Key: {}", self.public_key)?;
        writeln!(f, "Node ID: {}", self.node_id)?;
        writeln!(
            f,
            "Public Addresses: {}",
            acquire_read_lock!(self.public_addresses)
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )?;
        writeln!(f, "Features: {:?}", self.features)?;

        Ok(())
    }
}

impl fmt::Debug for NodeIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeIdentity")
            .field("public_key", &self.public_key)
            .field("node_id", &self.node_id)
            .field("public_address", &self.public_addresses)
            .field("features", &self.features)
            .field("secret_key", &"<secret>")
            .field("identity_signature", &*acquire_read_lock!(self.identity_signature))
            .finish()
    }
}

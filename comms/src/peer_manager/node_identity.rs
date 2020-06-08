//  Copyright 2019 The Tari Project
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

use super::node_id::deserialize_node_id_from_hex;
use crate::{
    peer_manager::{
        node_id::{NodeId, NodeIdError},
        Peer,
        PeerFeatures,
        PeerFlags,
    },
    types::{CommsPublicKey, CommsSecretKey},
};
use multiaddr::Multiaddr;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::RwLock};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    tari_utilities::hex::serialize_to_hex,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeIdentityError {
    #[error("NodeIdError: {0}")]
    NodeIdError(#[from] NodeIdError),
}

/// The public and private identity of this node on the network
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeIdentity {
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    node_id: NodeId,
    public_key: CommsPublicKey,
    features: PeerFeatures,
    secret_key: CommsSecretKey,
    public_address: RwLock<Multiaddr>,
}

impl NodeIdentity {
    /// Create a new NodeIdentity from the provided key pair and control service address
    pub fn new(
        secret_key: CommsSecretKey,
        public_address: Multiaddr,
        features: PeerFeatures,
    ) -> Result<Self, NodeIdentityError>
    {
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            node_id,
            public_key,
            features,
            secret_key,
            public_address: RwLock::new(public_address),
        })
    }

    /// Generates a new random NodeIdentity for CommsPublicKey
    pub fn random<R>(
        rng: &mut R,
        public_address: Multiaddr,
        features: PeerFeatures,
    ) -> Result<Self, NodeIdentityError>
    where
        R: CryptoRng + Rng,
    {
        let secret_key = CommsSecretKey::random(rng);
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            node_id,
            public_key,
            features,
            secret_key,
            public_address: RwLock::new(public_address),
        })
    }

    /// Retrieve the publicly accessible address that peers must connect to establish a connection
    pub fn public_address(&self) -> Multiaddr {
        acquire_read_lock!(self.public_address).clone()
    }

    /// Modify the control_service_address
    pub fn set_public_address(&self, address: Multiaddr) {
        *acquire_write_lock!(self.public_address) = address;
    }

    /// This returns a random NodeIdentity for testing purposes. This function can panic. If public_address
    /// is None, 127.0.0.1:9000 will be used (i.e. the caller doesn't care what the control_service_address is).
    #[cfg(test)]
    pub fn random_for_test(public_address: Option<Multiaddr>, features: PeerFeatures) -> Self {
        Self::random(
            &mut rand::rngs::OsRng,
            public_address.or("/ip4/127.0.0.1/tcp/9000".parse().ok()).unwrap(),
            features,
        )
        .unwrap()
    }

    #[inline]
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    #[inline]
    pub fn public_key(&self) -> &CommsPublicKey {
        &self.public_key
    }

    #[inline]
    pub fn secret_key(&self) -> &CommsSecretKey {
        &self.secret_key
    }

    #[inline]
    pub fn features(&self) -> PeerFeatures {
        self.features
    }

    #[inline]
    pub fn has_peer_features(&self, peer_features: PeerFeatures) -> bool {
        self.features().contains(peer_features)
    }

    /// Returns a Peer with the same public key, node id, public address and features as represented in this
    /// NodeIdentity. _NOTE: PeerFlags and supported_protocols are empty._
    pub fn to_peer(&self) -> Peer {
        Peer::new(
            self.public_key().clone(),
            self.node_id().clone(),
            self.public_address().into(),
            PeerFlags::empty(),
            self.features(),
            &[],
        )
    }
}

impl Clone for NodeIdentity {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id.clone(),
            public_key: self.public_key.clone(),
            features: self.features,
            secret_key: self.secret_key.clone(),
            public_address: RwLock::new(self.public_address()),
        }
    }
}

impl fmt::Display for NodeIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Public Key: {}", self.public_key)?;
        writeln!(f, "Node ID: {}", self.node_id)?;
        writeln!(f, "Public Address: {}", acquire_read_lock!(self.public_address))?;
        writeln!(f, "Features: {:?}", self.features)?;

        Ok(())
    }
}

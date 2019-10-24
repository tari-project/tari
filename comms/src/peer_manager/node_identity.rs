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
    connection::NetAddress,
    peer_manager::{
        node_id::{NodeId, NodeIdError},
        Peer,
        PeerFeature,
        PeerFeatures,
        PeerFlags,
    },
    types::{CommsPublicKey, CommsSecretKey},
};
use derive_error::Error;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_utilities::hex::serialize_to_hex;

#[derive(Debug, Error)]
pub enum NodeIdentityError {
    NodeIdError(NodeIdError),
    /// The Thread Safety has been breached and the data access has become poisoned
    PoisonedAccess,
}

/// Identity of this node
/// # Fields
/// `identity`: The public identity fields for this node
///
/// `secret_key`: The secret key corresponding to the public key of this node
///
/// `control_service_address`: The NetAddress of the local node's Control port
#[derive(Serialize, Deserialize)]
pub struct NodeIdentity {
    pub identity: PeerNodeIdentity,
    pub secret_key: CommsSecretKey,
    control_service_address: RwLock<NetAddress>,
}

impl NodeIdentity {
    /// Create a new NodeIdentity from the provided key pair and control service address
    pub fn new(
        secret_key: CommsSecretKey,
        public_key: CommsPublicKey,
        control_service_address: NetAddress,
        features: PeerFeatures,
    ) -> Result<Self, NodeIdentityError>
    {
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key, features),
            secret_key,
            control_service_address: RwLock::new(control_service_address),
        })
    }

    /// Generates a new random NodeIdentity for CommsPublicKey
    pub fn random<R>(
        rng: &mut R,
        control_service_address: NetAddress,
        features: PeerFeatures,
    ) -> Result<Self, NodeIdentityError>
    where
        R: CryptoRng + Rng,
    {
        let secret_key = CommsSecretKey::random(rng);
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key, features),
            secret_key,
            control_service_address: RwLock::new(control_service_address),
        })
    }

    /// Retrieve the control_service_address
    pub fn control_service_address(&self) -> NetAddress {
        acquire_read_lock!(self.control_service_address).clone()
    }

    /// Modify the control_service_address
    pub fn set_control_service_address(&self, control_service_address: NetAddress) -> Result<(), NodeIdentityError> {
        *self
            .control_service_address
            .write()
            .map_err(|_| NodeIdentityError::PoisonedAccess)? = control_service_address;
        Ok(())
    }

    /// This returns a random NodeIdentity for testing purposes. This function can panic. If a control_service_address
    /// is None, 127.0.0.1:9000 will be used (i.e. the caller doesn't care what the control_service_address is).
    #[cfg(test)]
    pub fn random_for_test(control_service_address: Option<NetAddress>, features: PeerFeatures) -> Self {
        use rand::OsRng;
        Self::random(
            &mut OsRng::new().unwrap(),
            control_service_address.or("127.0.0.1:9000".parse().ok()).unwrap(),
            features,
        )
        .unwrap()
    }

    pub fn node_id(&self) -> &NodeId {
        &self.identity.node_id
    }

    pub fn public_key(&self) -> &CommsPublicKey {
        &self.identity.public_key
    }

    pub fn secret_key(&self) -> &CommsSecretKey {
        &self.secret_key
    }

    pub fn features(&self) -> &PeerFeatures {
        &self.identity.features
    }

    pub fn has_peer_feature(&self, peer_feature: &PeerFeature) -> bool {
        self.features().contains(peer_feature)
    }
}

impl From<NodeIdentity> for Peer {
    fn from(node_identity: NodeIdentity) -> Peer {
        Peer::new(
            node_identity.identity.public_key,
            node_identity.identity.node_id,
            node_identity.control_service_address.read().unwrap().clone().into(),
            PeerFlags::empty(),
            node_identity.identity.features,
        )
    }
}

impl Clone for NodeIdentity {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            secret_key: self.secret_key.clone(),
            control_service_address: RwLock::new(self.control_service_address()),
        }
    }
}

/// The PeerNodeIdentity is a container that stores the public identity (NodeId, Identification Public Key pair) of a
/// single node
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct PeerNodeIdentity {
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    pub public_key: CommsPublicKey,
    pub features: PeerFeatures,
}

impl PeerNodeIdentity {
    /// Construct a new identity for a node that contains its NodeId and identification key pair
    pub fn new(node_id: NodeId, public_key: CommsPublicKey, features: PeerFeatures) -> PeerNodeIdentity {
        PeerNodeIdentity {
            node_id,
            public_key,
            features,
        }
    }
}

/// Construct a PeerNodeIdentity from a Peer
impl From<Peer> for PeerNodeIdentity {
    fn from(peer: Peer) -> Self {
        Self {
            public_key: peer.public_key,
            node_id: peer.node_id,
            features: peer.features,
        }
    }
}

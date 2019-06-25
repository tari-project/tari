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
        PeerFlags,
    },
    types::{CommsPublicKey, CommsSecretKey},
};
use derive_error::Error;
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::serialize::{pubkey_from_hex, secret_from_hex},
};
use tari_utilities::hex::serialize_to_hex;

#[derive(Debug, Error)]
pub enum NodeIdentityError {
    NodeIdError(NodeIdError),
}

/// Identity of this node
/// # Fields
/// `identity`: The public identity fields for this node
///
/// `secret_key`: The secret key corresponding to the public key of this node
///
/// `control_service_address`: The NetAddress of the local node's Control port
#[derive(Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub identity: PeerNodeIdentity<CommsPublicKey>,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "secret_from_hex")]
    pub secret_key: CommsSecretKey,
    pub control_service_address: NetAddress,
}

impl NodeIdentity {
    /// Create a new NodeIdentity from the provided key pair and control service address
    pub fn new(
        secret_key: CommsSecretKey,
        public_key: CommsPublicKey,
        control_service_address: NetAddress,
    ) -> Result<Self, NodeIdentityError>
    {
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key),
            secret_key,
            control_service_address,
        })
    }

    /// Generates a new random NodeIdentity for CommsPublicKey
    pub fn random<R>(rng: &mut R, control_service_address: NetAddress) -> Result<Self, NodeIdentityError>
    where R: CryptoRng + Rng {
        let secret_key = CommsSecretKey::random(rng);
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).map_err(NodeIdentityError::NodeIdError)?;

        Ok(NodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key),
            secret_key,
            control_service_address,
        })
    }

    /// This returns a random NodeIdentity for testing purposes. This function can panic. If a control_service_address
    /// is None, 127.0.0.1:9000 will be used (i.e. the caller doesn't care what the control_service_address is).
    #[cfg(test)]
    pub fn random_for_test(control_service_address: Option<NetAddress>) -> Self {
        use rand::OsRng;
        Self::random(
            &mut OsRng::new().unwrap(),
            control_service_address.or("127.0.0.1:9000".parse().ok()).unwrap(),
        )
        .unwrap()
    }
}

impl From<NodeIdentity> for Peer<CommsPublicKey> {
    fn from(node_identity: NodeIdentity) -> Peer<CommsPublicKey> {
        Peer::new(
            node_identity.identity.public_key,
            node_identity.identity.node_id,
            node_identity.control_service_address.into(),
            PeerFlags::empty(),
        )
    }
}

/// The PeerNodeIdentity is a container that stores the public identity (NodeId, Identification Public Key pair) of a
/// single node
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct PeerNodeIdentity<PK: PublicKey> {
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    #[serde(serialize_with = "serialize_to_hex", bound(serialize = "PK: PublicKey"))]
    #[serde(deserialize_with = "pubkey_from_hex", bound(deserialize = "PK: PublicKey"))]
    pub public_key: PK,
}

impl<PK: PublicKey> PeerNodeIdentity<PK> {
    /// Construct a new identity for a node that contains its NodeId and identification key pair
    pub fn new(node_id: NodeId, public_key: PK) -> PeerNodeIdentity<PK> {
        PeerNodeIdentity { node_id, public_key }
    }
}

/// Construct a PeerNodeIdentity from a Peer
impl<PK: PublicKey> From<Peer<PK>> for PeerNodeIdentity<PK> {
    fn from(peer: Peer<PK>) -> Self {
        Self {
            public_key: peer.public_key,
            node_id: peer.node_id,
        }
    }
}

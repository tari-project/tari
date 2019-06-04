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

use std::sync::{Arc, RwLock};

use crate::{connection::NetAddress, peer_manager::node_id::NodeId, types::CommsPublicKey};

use tari_crypto::keys::PublicKey;
use tari_utilities::Hashable;

pub type CommsNodeIdentity = NodeIdentity<CommsPublicKey>;

lazy_static! {
    static ref GLOBAL_NODE_IDENTITY: RwLock<Option<Arc<CommsNodeIdentity>>> = RwLock::new(None);
}

/// Identity of this node
pub struct NodeIdentity<P: PublicKey> {
    pub identity: PeerNodeIdentity<P>,
    pub secret_key: P::K,
    pub control_service_address: NetAddress,
}

impl<P> NodeIdentity<P>
where P: PublicKey + Hashable
{
    #[cfg(not(test))]
    /// Fetches the NodeIdentity
    pub fn global() -> Option<Arc<CommsNodeIdentity>> {
        let lock = acquire_read_lock!(GLOBAL_NODE_IDENTITY);
        lock.clone()
    }

    #[cfg(test)]
    /// Fetches the test NodeIdentity
    pub fn global() -> Option<Arc<CommsNodeIdentity>> {
        use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
        use tari_utilities::byte_array::ByteArray;

        {
            let lock = acquire_read_lock!(GLOBAL_NODE_IDENTITY);
            if lock.is_some() {
                return lock.clone();
            }
        }

        // Generate a test identity, set it and return it
        let secret_key = RistrettoSecretKey::from_bytes(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
        .unwrap();
        let public_key = RistrettoPublicKey::from_secret_key(&secret_key);
        let node_id = NodeId::from_key(&public_key).unwrap();
        Self::set_global(CommsNodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key),
            secret_key,
            control_service_address: "127.0.0.1:9090".parse::<NetAddress>().unwrap(),
        });
        let lock = acquire_read_lock!(GLOBAL_NODE_IDENTITY);
        lock.clone()
    }

    /// Sets the global node identity.
    pub fn set_global(node_identity: CommsNodeIdentity) {
        let mut lock = acquire_write_lock!(GLOBAL_NODE_IDENTITY);
        *lock = Some(Arc::new(node_identity));
    }
}

/// The NodeIdentity is a container that stores the identity (NodeId, Identification Key pair) of a single node
#[derive(Eq, PartialEq, Debug)]
pub struct PeerNodeIdentity<PubKey> {
    pub node_id: NodeId,
    pub public_key: PubKey,
}

impl<PubKey: PublicKey> PeerNodeIdentity<PubKey> {
    /// Construct a new identity for a node that contains its NodeId and identification key pair
    pub fn new(node_id: NodeId, public_key: PubKey) -> PeerNodeIdentity<PubKey> {
        PeerNodeIdentity { node_id, public_key }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn global() {
        let ident = CommsNodeIdentity::global().unwrap();
        let ident_again = CommsNodeIdentity::global().unwrap();
        assert_eq!(ident.identity, ident_again.identity);
        assert_eq!(ident.secret_key, ident_again.secret_key);
    }
}

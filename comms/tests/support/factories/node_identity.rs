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

use super::{TestFactory, TestFactoryError};
use rand::OsRng;
use tari_comms::{
    connection::NetAddress,
    peer_manager::{NodeId, NodeIdentity, PeerNodeIdentity},
};
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_utilities::Hashable;

pub fn create<PK>() -> NodeIdentityFactory<PK>
where PK: PublicKey {
    NodeIdentityFactory::<PK>::default()
}

#[derive(Default, Clone)]
pub struct NodeIdentityFactory<PK>
where PK: PublicKey
{
    control_service_address: Option<NetAddress>,
    secret_key: Option<PK::K>,
    public_key: Option<PK>,
    node_id: Option<NodeId>,
}

impl<PK> NodeIdentityFactory<PK>
where PK: PublicKey
{
    factory_setter!(
        with_control_service_address,
        control_service_address,
        Option<NetAddress>
    );

    factory_setter!(with_secret_key, secret_key, Option<PK::K>);

    factory_setter!(with_public_key, public_key, Option<PK>);

    factory_setter!(with_node_id, node_id, Option<NodeId>);
}

impl<PK> TestFactory for NodeIdentityFactory<PK>
where
    PK: PublicKey,
    PK: Hashable,
{
    type Object = NodeIdentity<PK>;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        // Generate a test identity, set it and return it
        let secret_key = self
            .secret_key
            .or(Some(PK::K::random(
                &mut OsRng::new().map_err(TestFactoryError::build_failed())?,
            )))
            .unwrap();
        let public_key = self.public_key.or(Some(PK::from_secret_key(&secret_key))).unwrap();
        let node_id = self
            .node_id
            .or(Some(
                NodeId::from_key(&public_key).map_err(TestFactoryError::build_failed())?,
            ))
            .unwrap();
        let control_service_address = self
            .control_service_address
            .or(Some(super::net_address::create().build()?))
            .unwrap();

        Ok(NodeIdentity {
            identity: PeerNodeIdentity::new(node_id, public_key),
            secret_key,
            control_service_address,
        })
    }
}

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
use crate::{
    peer_manager::{NodeIdentity, PeerFeatures},
    types::CommsSecretKey,
};
use multiaddr::Multiaddr;
use rand::rngs::OsRng;
use tari_crypto::keys::SecretKey;

pub fn create() -> NodeIdentityFactory {
    NodeIdentityFactory::default()
}

#[derive(Default, Clone)]
pub struct NodeIdentityFactory {
    control_service_address: Option<Multiaddr>,
    secret_key: Option<CommsSecretKey>,
    //    public_key: Option<CommsPublicKey>,
    peer_features: PeerFeatures,
}

impl NodeIdentityFactory {
    factory_setter!(with_control_service_address, control_service_address, Option<Multiaddr>);

    factory_setter!(with_secret_key, secret_key, Option<CommsSecretKey>);

    factory_setter!(with_peer_features, peer_features, PeerFeatures);
}

impl TestFactory for NodeIdentityFactory {
    type Object = NodeIdentity;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        // Generate a test identity, set it and return it
        let secret_key = self
            .secret_key
            .or_else(|| Some(CommsSecretKey::random(&mut OsRng)))
            .unwrap();

        let control_service_address = self
            .control_service_address
            .or(Some(super::net_address::create().build()?))
            .unwrap();

        Ok(NodeIdentity::new(
            secret_key,
            control_service_address,
            self.peer_features,
        ))
    }
}

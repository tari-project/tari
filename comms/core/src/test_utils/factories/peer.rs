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

use super::{net_address::NetAddressesFactory, TestFactory, TestFactoryError};
use crate::{
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use multiaddr::Multiaddr;
use rand::rngs::OsRng;
use std::iter::repeat_with;
use tari_crypto::keys::PublicKey;

pub fn create_many(n: usize) -> PeersFactory {
    PeersFactory::default().with_count(n)
}

pub fn create() -> PeerFactory {
    PeerFactory::default()
}

#[derive(Default, Clone)]
pub struct PeerFactory {
    node_id: Option<NodeId>,
    flags: Option<PeerFlags>,
    public_key: Option<CommsPublicKey>,
    net_addresses_factory: NetAddressesFactory,
    net_addresses: Option<Vec<Multiaddr>>,
    peer_features: PeerFeatures,
}

impl PeerFactory {
    factory_setter!(with_node_id, node_id, Option<NodeId>);

    factory_setter!(with_flags, flags, Option<PeerFlags>);

    factory_setter!(with_public_key, public_key, Option<CommsPublicKey>);

    factory_setter!(with_peer_features, peer_features, PeerFeatures);

    factory_setter!(with_net_addresses_factory, net_addresses_factory, NetAddressesFactory);

    factory_setter!(with_net_addresses, net_addresses, Option<Vec<Multiaddr>>);
}

impl TestFactory for PeerFactory {
    type Object = Peer;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        let flags = self.flags.or_else(|| Some(PeerFlags::empty())).unwrap();
        let public_key = self
            .public_key
            .clone()
            .or_else(|| {
                let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
                Some(pk)
            })
            .unwrap();

        let node_id = self
            .node_id
            .clone()
            .or_else(|| Some(NodeId::from_key(&public_key)))
            .unwrap();

        let default = self.net_addresses_factory.build().ok();
        let addresses = self
            .net_addresses
            .or(default)
            .ok_or_else(|| TestFactoryError::BuildFailed("Failed to build net addresses for peer".to_string()))?;

        Ok(Peer::new(
            public_key,
            node_id,
            addresses.into(),
            flags,
            self.peer_features,
            Default::default(),
            Default::default(),
        ))
    }
}

//---------------------------------- PeersFactory --------------------------------------------//

#[derive(Default)]
pub struct PeersFactory {
    count: Option<usize>,
    peer_factory: PeerFactory,
}

impl PeersFactory {
    factory_setter!(with_count, count, Option<usize>);

    factory_setter!(with_factory, peer_factory, PeerFactory);

    fn create_peer(&self) -> Peer {
        self.peer_factory.clone().build().unwrap()
    }
}

impl TestFactory for PeersFactory {
    type Object = Vec<Peer>;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        Ok(repeat_with(|| self.create_peer())
            .take(self.count.or(Some(1)).unwrap())
            .collect::<Vec<Peer>>())
    }
}

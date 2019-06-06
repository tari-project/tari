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

use super::{net_address::NetAddressesFactory, Factory, FactoryError};

use crate::{
    connection::NetAddress,
    peer_manager::{NodeId, Peer, PeerFlags},
    types::{CommsDataStore, CommsPublicKey},
};

use crate::test_support::makers::{comms_keys as ristretto_maker, node_id as node_id_maker};

use std::iter::repeat_with;

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
    net_addresses: Option<Vec<NetAddress>>,
}

impl PeerFactory {
    factory_setter!(with_node_id, node_id, Option<NodeId>);

    factory_setter!(with_flags, flags, Option<PeerFlags>);

    factory_setter!(with_public_key, public_key, Option<CommsPublicKey>);

    factory_setter!(with_net_addresses_factory, net_addresses_factory, NetAddressesFactory);

    factory_setter!(with_net_addresses, net_addresses, Option<Vec<NetAddress>>);
}

impl Factory for PeerFactory {
    type Object = Peer<CommsPublicKey>;

    fn build(self) -> Result<Self::Object, FactoryError> {
        let node_id = self.node_id.clone().or(Some(node_id_maker::make_node_id())).unwrap();
        let flags = self.flags.clone().or(Some(PeerFlags::empty())).unwrap().clone();
        let public_key = self
            .public_key
            .clone()
            .or_else(|| {
                let (_, pk) = ristretto_maker::make_random_keypair();
                Some(pk)
            })
            .unwrap();

        let addresses =
            self.net_addresses
                .or(self.net_addresses_factory.build().ok())
                .ok_or(FactoryError::BuildFailed(format!(
                    "Failed to build net addresses for peer"
                )))?;

        Ok(Peer {
            node_id,
            flags,
            public_key,
            addresses: addresses.into(),
        })
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

    fn create_peer(&self) -> Peer<CommsPublicKey> {
        self.peer_factory.clone().build().unwrap()
    }
}

impl Factory for PeersFactory {
    type Object = Vec<Peer<CommsPublicKey>>;

    fn build(self) -> Result<Self::Object, FactoryError> {
        Ok(repeat_with(|| self.create_peer())
            .take(self.count.or(Some(1)).unwrap())
            .collect::<Vec<Peer<CommsPublicKey>>>())
    }
}

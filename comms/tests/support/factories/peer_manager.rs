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

use super::{peer::PeersFactory, TestFactory, TestFactoryError};

use tari_comms::{
    peer_manager::{Peer, PeerManager},
    types::CommsDataStore,
};

pub fn create() -> PeerManagerFactory {
    PeerManagerFactory::default()
}

#[derive(Default)]
pub struct PeerManagerFactory {
    peers_factory: PeersFactory,
    peers: Option<Vec<Peer>>,
}

impl PeerManagerFactory {
    factory_setter!(with_peers_factory, peers_factory, PeersFactory);

    factory_setter!(with_peers, peers, Option<Vec<Peer>>);
}

impl TestFactory for PeerManagerFactory {
    type Object = PeerManager<CommsDataStore>;

    fn build(self) -> Result<Self::Object, TestFactoryError> {
        let pm = PeerManager::<CommsDataStore>::new(None)
            .map_err(|err| TestFactoryError::BuildFailed(format!("Failed to build peer manager: {:?}", err)))?;

        let peers = self
            .peers
            .or(self.peers_factory.build().ok())
            .ok_or(TestFactoryError::BuildFailed("Failed to build peers".into()))?;
        for peer in peers {
            pm.add_peer(peer)
                .map_err(|err| TestFactoryError::BuildFailed(format!("Failed to build peer manager: {:?}", err)))?;
        }
        Ok(pm)
    }
}

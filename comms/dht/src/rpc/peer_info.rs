//  Copyright 2022. The Tari Project
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

use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures, PeerIdentityClaim},
    protocol::ProtocolId,
    types::CommsPublicKey,
};

pub struct PeerInfo {
    pub public_key: CommsPublicKey,
    pub addresses: Vec<PeerInfoAddress>,
    pub peer_features: PeerFeatures,
    pub user_agent: String,
    pub supported_protocols: Vec<ProtocolId>,
}

pub struct PeerInfoAddress {
    pub address: Multiaddr,
    pub peer_identity_claim: PeerIdentityClaim,
}

impl From<Peer> for PeerInfo {
    fn from(peer: Peer) -> Self {
        PeerInfo {
            public_key: peer.public_key,
            addresses: peer
                .addresses
                .addresses()
                .iter()
                .filter_map(|addr| {
                    // TODO: find the source of the empty addresses
                    if addr.address().is_empty() {
                        return None;
                    }
                    addr.source.peer_identity_claim().map(|claim| PeerInfoAddress {
                        address: addr.address().clone(),
                        peer_identity_claim: claim.clone(),
                    })
                })
                .collect(),
            peer_features: peer.features,
            user_agent: peer.user_agent,
            supported_protocols: peer.supported_protocols,
        }
    }
}

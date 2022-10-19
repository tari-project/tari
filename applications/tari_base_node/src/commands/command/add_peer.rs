//  Copyright 2022, The Tari Project
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

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniPublicKey;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
};

use super::{CommandContext, HandleCommand};

/// Adds a peer
#[derive(Debug, Parser)]
pub struct ArgsAddPeer {
    /// Peer public key
    public_key: UniPublicKey,
    /// Peer address
    address: Multiaddr,
}

#[async_trait]
impl HandleCommand<ArgsAddPeer> for CommandContext {
    async fn handle_command(&mut self, args: ArgsAddPeer) -> Result<(), Error> {
        let public_key = args.public_key.into();
        let peer_manager = self.comms.peer_manager();
        if peer_manager.exists(&public_key).await {
            return Err(anyhow!("Peer with public key '{}' already exists", public_key));
        }
        let node_id = NodeId::from_public_key(&public_key);
        let peer = Peer::new(
            public_key,
            node_id.clone(),
            vec![args.address].into(),
            PeerFlags::empty(),
            PeerFeatures::empty(),
            vec![],
            String::new(),
        );
        peer_manager.add_peer(peer).await?;
        println!("Peer with node id '{}'was added to the base node.", node_id);
        Ok(())
    }
}

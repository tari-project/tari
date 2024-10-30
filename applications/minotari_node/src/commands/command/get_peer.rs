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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use minotari_app_utilities::utilities::UniPeerId;
use tari_common_types::emoji::EmojiId;
use tari_network::{identity::PeerId, ToPeerId};
use thiserror::Error;

use super::{CommandContext, HandleCommand, TypeOrHex};

/// Get all available info about peer
#[derive(Debug, Parser)]
pub struct Args {
    value: UniPeerId,
}

impl From<TypeOrHex<UniPeerId>> for Vec<u8> {
    fn from(value: TypeOrHex<UniPeerId>) -> Self {
        match value {
            TypeOrHex::Type(value) => value.to_peer_id().to_bytes(),
            TypeOrHex::Hex(vec) => vec.0,
        }
    }
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.get_peer(args.value).await
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("Peer {peer_id} not found")]
    PeerNotFound { peer_id: PeerId },
}

impl CommandContext {
    pub async fn get_peer(&self, peer_id: UniPeerId) -> Result<(), Error> {
        let peer_id = peer_id.to_peer_id();
        let peer = self
            .network
            .get_connection(peer_id)
            .await?
            .ok_or(ArgsError::PeerNotFound { peer_id })?;

        match peer.public_key {
            Some(pk) => {
                let pk = pk.try_into_sr25519()?;
                let eid = EmojiId::from(pk.inner_key()).to_string();
                println!("Emoji ID: {}", eid);
                println!("Public Key: {}", pk.inner_key());
            },
            None => {
                println!("Public Key: Unknown");
            },
        };
        println!("NodeId: {}", peer.peer_id);
        println!("Addresses:");
        println!(
            "- {} Latency: {:?}",
            peer.endpoint.get_remote_address(),
            peer.ping_latency,
        );
        match peer.user_agent {
            Some(ua) => {
                println!("User agent: {ua}");
            },
            None => {
                println!("User agent: Unknown");
            },
        }
        print!("Supported protocols:");
        if peer.supported_protocols.is_empty() {
            println!(" Unknown");
        } else {
            println!();
            for p in &peer.supported_protocols {
                println!("- {}", p);
            }
        }
        if let Some(banned_peer) = self.network.get_banned_peer(peer.peer_id).await? {
            print!("Banned ");
            match banned_peer.remaining_ban() {
                Some(until) => {
                    print!("until {}", humantime::format_duration(until));
                },
                None => {
                    print!("indefinitely")
                },
            }
            println!(", reason: {}", banned_peer.ban_reason);
        }

        Ok(())
    }
}

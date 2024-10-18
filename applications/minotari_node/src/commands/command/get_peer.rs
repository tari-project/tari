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
use minotari_app_utilities::utilities::{parse_emoji_id_or_public_key, UniPeerId};
use tari_common_types::emoji::EmojiId;
use tari_network::ToPeerId;
use thiserror::Error;

use super::{CommandContext, HandleCommand, TypeOrHex};

/// Get all available info about peer
#[derive(Debug, Parser)]
pub struct Args {
    /// Partial NodeId | PublicKey | EmojiId
    value: String,
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
    #[error("No peer matching: {original_str}")]
    NoPeerMatching { original_str: String },
}

impl CommandContext {
    pub async fn get_peer(&self, original_str: String) -> Result<(), Error> {
        let pk = parse_emoji_id_or_public_key(&original_str).ok_or_else(|| ArgsError::NoPeerMatching {
            original_str: original_str.clone(),
        })?;
        let peer_id = pk.to_peer_id();
        let peer = self
            .network
            .get_connection(peer_id)
            .await?
            .ok_or(ArgsError::NoPeerMatching { original_str })?;

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
        // TODO: we could also provide this
        // println!("Supported protocols:");
        // peer.supported_protocols.iter().for_each(|p| {
        //     println!("- {}", String::from_utf8_lossy(p));
        // });
        // if let Some(dt) = peer.banned_until() {
        //     println!("Banned until {}, reason: {}", dt, peer.banned_reason);
        // }
        // if let Some(dt) = peer.last_seen() {
        //     println!("Last seen: {}", dt);
        // }
        Ok(())
    }
}

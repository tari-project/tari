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
use minotari_app_utilities::utilities::{parse_emoji_id_or_public_key, UniNodeId};
use tari_common_types::emoji::EmojiId;
use tari_comms::peer_manager::NodeId;
use tari_utilities::ByteArray;
use thiserror::Error;

use super::{CommandContext, HandleCommand, TypeOrHex};

/// Get all available info about peer
#[derive(Debug, Parser)]
pub struct Args {
    /// Partial NodeId | PublicKey | EmojiId
    value: String,
}

impl From<TypeOrHex<UniNodeId>> for Vec<u8> {
    fn from(value: TypeOrHex<UniNodeId>) -> Self {
        match value {
            TypeOrHex::Type(value) => NodeId::from(value).to_vec(),
            TypeOrHex::Hex(vec) => vec.0,
        }
    }
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let value: TypeOrHex<UniNodeId> = args.value.parse()?;
        self.get_peer(value.into(), args.value).await
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("No peer matching: {original_str}")]
    NoPeerMatching { original_str: String },
}

impl CommandContext {
    pub async fn get_peer(&self, partial: Vec<u8>, original_str: String) -> Result<(), Error> {
        let peer_manager = self.comms.peer_manager();
        let peers = peer_manager.find_all_starts_with(&partial).await?;
        let peers = {
            if peers.is_empty() {
                let pk = parse_emoji_id_or_public_key(&original_str).ok_or_else(|| ArgsError::NoPeerMatching {
                    original_str: original_str.clone(),
                })?;
                let peer = peer_manager
                    .find_by_public_key(&pk)
                    .await?
                    .ok_or(ArgsError::NoPeerMatching { original_str })?;
                vec![peer]
            } else {
                peers
            }
        };

        for peer in peers {
            let eid = EmojiId::from_public_key(&peer.public_key).to_emoji_string();
            println!("Emoji ID: {}", eid);
            println!("Public Key: {}", peer.public_key);
            println!("NodeId: {}", peer.node_id);
            println!("Addresses:");
            peer.addresses.addresses().iter().for_each(|a| {
                println!(
                    "- {} Score: {:?}  - Source: {} Latency: {:?} - Last Seen: {} - Last Failure:{}",
                    a.address(),
                    a.quality_score(),
                    a.source(),
                    a.avg_latency(),
                    a.last_seen()
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "Never".to_string()),
                    a.last_failed_reason().unwrap_or("None")
                );
            });
            println!("User agent: {}", peer.user_agent);
            println!("Features: {:?}", peer.features);
            println!("Flags: {:?}", peer.flags);
            println!("Supported protocols:");
            peer.supported_protocols.iter().for_each(|p| {
                println!("- {}", String::from_utf8_lossy(p));
            });
            if let Some(dt) = peer.banned_until() {
                println!("Banned until {}, reason: {}", dt, peer.banned_reason);
            }
            if let Some(dt) = peer.last_seen() {
                println!("Last seen: {}", dt);
            }
        }
        Ok(())
    }
}

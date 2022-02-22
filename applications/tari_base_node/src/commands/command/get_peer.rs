use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::{parse_emoji_id_or_public_key, UniNodeId};
use tari_common_types::emoji::EmojiId;
use tari_comms::peer_manager::NodeId;
use tari_utilities::ByteArray;

use super::{CommandContext, HandleCommand, TypeOrHex};

/// Get all available info about peer
#[derive(Debug, Parser)]
pub struct Args {
    /// Partial NodeId | PublicKey | EmojiId
    value: TypeOrHex<UniNodeId>,
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
        let original_string = todo!();
        self.get_peer(args.value.into(), original_string).await
    }
}

impl CommandContext {
    pub async fn get_peer(&self, partial: Vec<u8>, original_str: String) -> Result<(), Error> {
        let peer = match self.peer_manager.find_all_starts_with(&partial).await {
            Ok(peers) if peers.is_empty() => {
                if let Some(pk) = parse_emoji_id_or_public_key(&original_str) {
                    if let Ok(Some(peer)) = self.peer_manager.find_by_public_key(&pk).await {
                        peer
                    } else {
                        println!("No peer matching '{}'", original_str);
                        // TODO: Return error
                        return Ok(());
                    }
                } else {
                    println!("No peer matching '{}'", original_str);
                    // TODO: Return error
                    return Ok(());
                }
            },
            Ok(mut peers) => peers.remove(0),
            Err(err) => {
                println!("{}", err);
                // TODO: Return error
                return Ok(());
            },
        };

        let eid = EmojiId::from_pubkey(&peer.public_key);
        println!("Emoji ID: {}", eid);
        println!("Public Key: {}", peer.public_key);
        println!("NodeId: {}", peer.node_id);
        println!("Addresses:");
        peer.addresses.iter().for_each(|a| {
            println!("- {}", a);
        });
        println!("User agent: {}", peer.user_agent);
        println!("Features: {:?}", peer.features);
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
        if let Some(updated_at) = peer.identity_signature.map(|i| i.updated_at()) {
            println!("Last updated: {} (UTC)", updated_at);
        }
        Ok(())
    }
}

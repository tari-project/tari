use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_comms::peer_manager::PeerQuery;

use super::{CommandContext, HandleCommand};

/// Unbans all peers
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.unban_all_peers().await
    }
}

impl CommandContext {
    pub async fn unban_all_peers(&self) -> Result<(), Error> {
        let query = PeerQuery::new().select_where(|p| p.is_banned());
        let peers = self.peer_manager.perform_query(query).await?;
        let num_peers = peers.len();
        for peer in peers {
            if let Err(err) = self.peer_manager.unban_peer(&peer.node_id).await {
                println!("Failed to unban peer: {}", err);
            }
        }
        println!("Unbanned {} peer(s) from node", num_peers);
        Ok(())
    }
}

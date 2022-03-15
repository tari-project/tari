use std::time::Instant;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::utilities::UniNodeId;
use tari_comms::peer_manager::NodeId;

use super::{CommandContext, HandleCommand};

/// Attempt to connect to a known peer
#[derive(Debug, Parser)]
pub struct Args {
    /// hex public key or emoji id
    node_id: UniNodeId,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.dial_peer(args.node_id.into()).await
    }
}

impl CommandContext {
    /// Function to process the dial-peer command
    pub async fn dial_peer(&self, dest_node_id: NodeId) -> Result<(), Error> {
        let start = Instant::now();
        println!("☎️  Dialing peer...");

        let connection = self.connectivity.dial_peer(dest_node_id).await?;
        println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
        println!("Connection: {}", connection);
        Ok(())
    }
}

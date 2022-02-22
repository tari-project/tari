use std::time::{Duration, Instant};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Parser;
use tari_app_utilities::utilities::UniNodeId;

use super::{CommandContext, HandleCommand};
use crate::{commands::status_line::StatusLine, StatusOutput};

#[derive(Debug, Parser)]
pub struct Args {
    node_id: UniNodeId,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let dest_node_id = args.node_id;
        let start = Instant::now();
        println!("☎️  Dialing peer...");
        let connection = self.connectivity.dial_peer(dest_node_id).await?;
        println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
        println!("Connection: {}", connection);
        Ok(())
    }
}

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
    #[clap(default_value_t = std::u64::MAX)]
    lenght: u64,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let node_id = args.node_id.into();
        let lenght = args.lenght;
        let ban = false;
        let duration = Duration::from_secs(lenght);
        if self.base_node_identity.node_id() == &node_id {
            Err(Error::msg("Cannot ban our own node"))
        } else {
            if ban {
                self.connectivity
                    .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
                    .await?;
                println!("Peer was banned in base node.");
            } else {
                self.peer_manager.unban_peer(&node_id).await?;
                println!("Peer ban was removed from base node.");
            }
            Ok(())
        }
    }
}

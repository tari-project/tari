use anyhow::Error;
use async_trait::async_trait;
use chrono::Utc;
use clap::Parser;
use tari_comms::peer_manager::{PeerFeatures, PeerQuery};
use tari_core::base_node::state_machine_service::states::PeerMetadata;

use super::{CommandContext, HandleCommand};
use crate::{table::Table, utils::format_duration_basic};

/// Lists peers that have been banned by the node or wallet
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.list_banned_peers().await
    }
}

impl CommandContext {
    pub async fn list_banned_peers(&self) -> Result<(), Error> {
        let banned = self.fetch_banned_peers().await?;
        if banned.is_empty() {
            println!("No peers banned from node.")
        } else {
            println!("Peers banned from node ({}):", banned.len());
            for peer in banned {
                println!("{}", peer);
            }
        }
        Ok(())
    }
}

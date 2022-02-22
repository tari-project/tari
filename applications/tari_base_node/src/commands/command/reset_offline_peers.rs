use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

/// Clear offline flag from all peers
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.reset_offline_peers().await
    }
}

impl CommandContext {
    pub async fn reset_offline_peers(&self) -> Result<(), Error> {
        let num_updated = self
            .peer_manager
            .update_each(|mut peer| {
                if peer.is_offline() {
                    peer.set_offline(false);
                    Some(peer)
                } else {
                    None
                }
            })
            .await?;

        println!("{} peer(s) were unmarked as offline.", num_updated);
        Ok(())
    }
}

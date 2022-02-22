use anyhow::Error;
use async_trait::async_trait;
use chrono::Utc;
use clap::Parser;
use tari_comms::peer_manager::{PeerFeatures, PeerQuery};
use tari_core::{
    base_node::state_machine_service::states::PeerMetadata,
    blocks::{BlockHeader, ChainHeader},
};
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::{commands::args::ArgsError, table::Table, utils::format_duration_basic, LOG_TARGET};

/// Retrieves your mempools stats
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.get_mempool_stats().await
    }
}

impl CommandContext {
    /// Function to process the get-mempool-stats command
    pub async fn get_mempool_stats(&mut self) -> Result<(), Error> {
        let stats = self.mempool_service.get_mempool_stats().await?;
        println!("{}", stats);
        Ok(())
    }
}

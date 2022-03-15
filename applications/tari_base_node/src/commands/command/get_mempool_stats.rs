use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

/// Retrieves your mempools stats
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
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

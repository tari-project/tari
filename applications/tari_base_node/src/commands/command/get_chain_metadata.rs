use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

/// Gets your base node chain meta data
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.get_chain_meta().await
    }
}

impl CommandContext {
    pub async fn get_chain_meta(&mut self) -> Result<(), Error> {
        let data = self.node_service.get_metadata().await?;
        println!("{}", data);
        Ok(())
    }
}

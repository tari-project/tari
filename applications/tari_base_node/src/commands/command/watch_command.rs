use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

#[derive(Debug, Parser)]
pub struct Args {
    pub command: String,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        Ok(())
    }
}

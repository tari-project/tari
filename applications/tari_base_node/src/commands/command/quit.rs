use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};
use crate::LOG_TARGET;

#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _args: Args) -> Result<(), Error> {
        println!("Shutting down...");
        log::info!(
            target: LOG_TARGET,
            "Termination signal received from user. Shutting node down."
        );
        self.shutdown.trigger();
        Ok(())
    }
}

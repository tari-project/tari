use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

/// Prints out the status of the base node state machine
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.state_info()
    }
}

impl CommandContext {
    /// Function to process the get-state-info command
    pub fn state_info(&self) -> Result<(), Error> {
        println!("Current state machine state:\n{}", *self.state_machine_info.borrow());
        Ok(())
    }
}

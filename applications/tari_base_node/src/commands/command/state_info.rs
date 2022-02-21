use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

#[derive(Debug, Parser)]
pub struct StateInfoArgs {}

#[async_trait]
impl HandleCommand<StateInfoArgs> for CommandContext {
    async fn handle_command(&mut self, _: StateInfoArgs) -> Result<(), Error> {
        println!("Current state machine state:\n{}", *self.state_machine_info.borrow());
        Ok(())
    }
}

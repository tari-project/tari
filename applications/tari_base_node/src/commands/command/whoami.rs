use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;

use super::{CommandContext, HandleCommand};

/// Display identity information about this node,
/// including: public key, node ID and the public address
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.whoami()
    }
}

impl CommandContext {
    /// Function to process the whoami command
    pub fn whoami(&self) -> Result<(), Error> {
        println!("{}", self.base_node_identity);
        Ok(())
    }
}

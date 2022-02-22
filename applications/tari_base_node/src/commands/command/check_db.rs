use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_app_utilities::consts;
use tokio::io::{self, AsyncWriteExt};

use super::{CommandContext, HandleCommand};
use crate::LOG_TARGET;

/// Checks the blockchain database for missing blocks and headers
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.check_db().await
    }
}

impl CommandContext {
    /// Function to process the check-db command
    pub async fn check_db(&mut self) -> Result<(), Error> {
        let meta = self.node_service.get_metadata().await?;
        let mut height = meta.height_of_longest_chain();
        let mut missing_blocks = Vec::new();
        let mut missing_headers = Vec::new();
        print!("Searching for height: ");
        // We need to check every header, but not every block.
        let horizon_height = meta.horizon_block(height);
        while height > 0 {
            print!("{}", height);
            io::stdout().flush().await?;
            // we can only check till the pruning horizon, 0 is archive node so it needs to check every block.
            if height > horizon_height {
                match self.node_service.get_block(height).await {
                    Err(err) => {
                        // We need to check the data itself, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        log::error!(target: LOG_TARGET, "{}", err);
                        missing_blocks.push(height);
                    },
                    Ok(Some(_)) => {},
                    Ok(None) => missing_blocks.push(height),
                };
            }
            height -= 1;
            let next_header = self.node_service.get_header(height).await.ok().flatten();
            if next_header.is_none() {
                // this header is missing, so we stop here and need to ask for this header
                missing_headers.push(height);
            };
            print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
        }
        println!("Complete");
        for missing_block in missing_blocks {
            println!("Missing block at height: {}", missing_block);
        }
        for missing_header_height in missing_headers {
            println!("Missing header at height: {}", missing_header_height)
        }
        Ok(())
    }
}

//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
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
        let horizon_height = meta.horizon_block_height(height);
        while height > 0 {
            print!("{}", height);
            io::stdout().flush().await?;
            // we can only check till the pruning horizon, 0 is archive node so it needs to check every block.
            if height > horizon_height {
                match self.node_service.get_block(height, false).await {
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

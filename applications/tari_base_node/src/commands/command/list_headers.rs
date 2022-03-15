use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_utilities::hex::Hex;
use thiserror::Error;

use super::{CommandContext, HandleCommand};

/// List the amount of headers, can be called in the following two ways:
#[derive(Debug, Parser)]
pub struct Args {
    /// number of headers starting from the chain tip back or the first header height (if the last set too)
    start: u64,
    /// last header height
    end: Option<u64>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.list_headers(args.start, args.end).await
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("No headers found")]
    NoHeaders,
}

impl CommandContext {
    /// Function to process the list-headers command
    pub async fn list_headers(&self, start: u64, end: Option<u64>) -> Result<(), Error> {
        let headers = self.get_chain_headers(start, end).await?;
        if !headers.is_empty() {
            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
            Ok(())
        } else {
            Err(ArgsError::NoHeaders.into())
        }
    }
}

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_core::blocks::BlockHeader;
use thiserror::Error;

use super::{CommandContext, HandleCommand};

/// Calculates the maximum, minimum, and average time taken to mine a given range of blocks
#[derive(Debug, Parser)]
pub struct Args {
    /// number of blocks from chain tip or start height
    /// (it should be at least 2 if end parameter is not set)
    start: u64,
    /// end height
    end: Option<u64>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        if args.end.is_none() && args.start < 2 {
            Err(ArgsError::AtLeastTwo.into())
        } else {
            self.block_timing(args.start, args.end).await
        }
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("Number of headers must be at least 2")]
    AtLeastTwo,
    #[error("No headers found")]
    NoHeaders,
    #[error("No first or last header")]
    HeaderLost,
}

impl CommandContext {
    pub async fn block_timing(&self, start: u64, end: Option<u64>) -> Result<(), Error> {
        let headers = self.get_chain_headers(start, end).await?;
        if !headers.is_empty() {
            let headers = headers.into_iter().map(|ch| ch.into_header()).rev().collect::<Vec<_>>();
            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            let first = headers.first().ok_or(ArgsError::HeaderLost)?.height;
            let last = headers.last().ok_or(ArgsError::HeaderLost)?.height;
            println!("Timing for blocks #{} - #{}", first, last);
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
            Ok(())
        } else {
            Err(ArgsError::NoHeaders.into())
        }
    }
}

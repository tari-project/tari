use anyhow::Error;
use async_trait::async_trait;
use chrono::Utc;
use clap::Parser;
use tari_comms::peer_manager::{PeerFeatures, PeerQuery};
use tari_core::{
    base_node::state_machine_service::states::PeerMetadata,
    blocks::{BlockHeader, ChainHeader},
};
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::{commands::args::ArgsError, table::Table, utils::format_duration_basic, LOG_TARGET};

/// Calculates the maximum, minimum, and average time taken to mine a given range of blocks
#[derive(Debug, Parser)]
pub struct Args {
    /// number of blocks from chain tip or start height
    start: u64,
    /// end height
    end: Option<u64>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        // TODO: is that possible to validate it with clap?
        if args.end.is_none() && args.start < 2 {
            Err(ArgsError::new("start", "Number of headers must be at least 2.").into())
        } else {
            self.block_timing(args.start, args.end).await
        }
    }
}

impl CommandContext {
    pub async fn block_timing(&self, start: u64, end: Option<u64>) -> Result<(), Error> {
        let headers = self.get_chain_headers(start, end).await?;
        if !headers.is_empty() {
            let headers = headers.into_iter().map(|ch| ch.into_header()).rev().collect::<Vec<_>>();
            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            println!(
                "Timing for blocks #{} - #{}",
                headers.first().unwrap().height,
                headers.last().unwrap().height
            );
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        } else {
            println!("No headers found");
        }
        Ok(())
    }
}

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
use crate::{table::Table, utils::format_duration_basic, LOG_TARGET};

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

impl CommandContext {
    /// Function to process the list-headers command
    pub async fn list_headers(&self, start: u64, end: Option<u64>) -> Result<(), Error> {
        let res = self.get_chain_headers(start, end).await;
        match res {
            Ok(h) if h.is_empty() => {
                println!("No headers found");
            },
            Ok(headers) => {
                for header in headers {
                    println!("\n\nHeader hash: {}", header.hash().to_hex());
                    println!("{}", header);
                }
            },
            // TODO: Handle results properly
            Err(err) => {
                println!("Failed to retrieve headers: {:?}", err);
                log::warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
            },
        }
        Ok(())
    }
}

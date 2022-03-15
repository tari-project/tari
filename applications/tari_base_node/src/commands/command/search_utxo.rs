use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use tari_common_types::types::Commitment;
use tari_utilities::hex::Hex;

use super::{CommandContext, HandleCommand};
use crate::commands::parser::FromHex;

/// This will search the main chain for the utxo.
/// If the utxo is found, it will print out
/// the block it was found in.
#[derive(Debug, Parser)]
pub struct Args {
    /// hex of commitment of the utxo
    commitment: FromHex<Commitment>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.search_utxo(args.commitment.0).await
    }
}

impl CommandContext {
    /// Function to process the search utxo command
    pub async fn search_utxo(&mut self, commitment: Commitment) -> Result<(), Error> {
        let v = self
            .node_service
            .fetch_blocks_with_utxos(vec![commitment.clone()])
            .await?
            .pop()
            .ok_or_else(|| anyhow!("Block not found for utxo commitment {}", commitment.to_hex()))?;
        println!("{}", v.block());
        Ok(())
    }
}

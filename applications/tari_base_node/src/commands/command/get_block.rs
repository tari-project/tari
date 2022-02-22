use std::str::FromStr;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use strum::EnumString;
use tari_common_types::types::HashOutput;
use tari_utilities::message_format::MessageFormat;

use super::{CommandContext, HandleCommand, TypeOrHex};
use crate::commands::{args::FromHex, command_handler::Format};

/// Display a block by height or hash
#[derive(Debug, Parser)]
pub struct Args {
    /// The height or hash of the block to fetch
    /// from the main chain. The genesis block
    /// has height zero.
    value: TypeOrHex<u64>,
    /// Supported options are 'json' and 'text'. 'text' is the default if omitted.
    #[clap(default_value_t)]
    format: Format,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let format = args.format;
        match args.value {
            TypeOrHex::Type(value) => self.get_block(value, format).await,
            TypeOrHex::Hex(hex) => self.get_block_by_hash(hex.0, format).await,
        }
    }
}

impl CommandContext {
    pub async fn get_block(&self, height: u64, format: Format) -> Result<(), Error> {
        let mut data = self.blockchain_db.fetch_blocks(height..=height).await?;
        match (data.pop(), format) {
            (Some(block), Format::Text) => {
                let block_data = self
                    .blockchain_db
                    .fetch_block_accumulated_data(block.hash().clone())
                    .await?;

                println!("{}", block);
                println!("-- Accumulated data --");
                println!("{}", block_data);
            },
            (Some(block), Format::Json) => println!("{}", block.to_json()?),
            (None, _) => println!("Block not found at height {}", height),
        }
        Ok(())
    }

    pub async fn get_block_by_hash(&self, hash: HashOutput, format: Format) -> Result<(), Error> {
        let data = self.blockchain_db.fetch_block_by_hash(hash).await?;
        match (data, format) {
            (Some(block), Format::Text) => println!("{}", block),
            (Some(block), Format::Json) => println!("{}", block.to_json()?),
            (None, _) => println!("Block not found"),
        }
        Ok(())
    }
}

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_common_types::types::HashOutput;
use tari_utilities::message_format::MessageFormat;
use thiserror::Error;

use super::{CommandContext, HandleCommand, TypeOrHex};
use crate::commands::parser::Format;

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

#[derive(Error, Debug)]
enum ArgsError {
    #[error("Block not found at height {height}")]
    NotFoundAt { height: u64 },
    #[error("Block not found")]
    NotFound,
}

impl CommandContext {
    pub async fn get_block(&self, height: u64, format: Format) -> Result<(), Error> {
        let block = self
            .blockchain_db
            .fetch_blocks(height..=height)
            .await?
            .pop()
            .ok_or(ArgsError::NotFoundAt { height })?;
        match format {
            Format::Text => {
                let block_data = self
                    .blockchain_db
                    .fetch_block_accumulated_data(block.hash().clone())
                    .await?;

                println!("{}", block);
                println!("-- Accumulated data --");
                println!("{}", block_data);
            },
            Format::Json => println!("{}", block.to_json()?),
        }
        Ok(())
    }

    pub async fn get_block_by_hash(&self, hash: HashOutput, format: Format) -> Result<(), Error> {
        let block = self
            .blockchain_db
            .fetch_block_by_hash(hash)
            .await?
            .ok_or(ArgsError::NotFound)?;
        match format {
            Format::Text => println!("{}", block),
            Format::Json => println!("{}", block.to_json()?),
        }
        Ok(())
    }
}

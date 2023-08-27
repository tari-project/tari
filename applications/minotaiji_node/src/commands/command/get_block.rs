//  Copyright 2022, The Taiji Project
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

use std::convert::TryInto;

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use taiji_common_types::types::HashOutput;
use tari_utilities::message_format::{MessageFormat, MessageFormatError};
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
            TypeOrHex::Hex(hex) => {
                let hash = hex.0.try_into()?;
                self.get_block_by_hash(hash, format).await
            },
        }
    }
}

#[derive(Error, Debug)]
enum ArgsError {
    #[error("Block not found at height {height}")]
    NotFoundAt { height: u64 },
    #[error("Block not found")]
    NotFound,
    #[error("Serializing/Deserializing error: `{0}`")]
    MessageFormatError(String),
}

impl From<MessageFormatError> for ArgsError {
    fn from(e: MessageFormatError) -> Self {
        ArgsError::MessageFormatError(e.to_string())
    }
}

impl CommandContext {
    pub async fn get_block(&self, height: u64, format: Format) -> Result<(), Error> {
        let block = self
            .blockchain_db
            .fetch_blocks(height..=height, false)
            .await?
            .pop()
            .ok_or(ArgsError::NotFoundAt { height })?;
        match format {
            Format::Text => {
                let block_data = self.blockchain_db.fetch_block_accumulated_data(*block.hash()).await?;

                println!("{}", block);
                println!("-- Accumulated data --");
                println!("{}", block_data);
            },
            Format::Json => println!(
                "{}",
                block
                    .to_json()
                    .map_err(|e| ArgsError::MessageFormatError(format!("{}", e)))?
            ),
        }
        Ok(())
    }

    pub async fn get_block_by_hash(&self, hash: HashOutput, format: Format) -> Result<(), Error> {
        let block = self
            .blockchain_db
            .fetch_block_by_hash(hash, false)
            .await?
            .ok_or(ArgsError::NotFound)?;
        match format {
            Format::Text => println!("{}", block),
            Format::Json => println!(
                "{}",
                block
                    .to_json()
                    .map_err(|e| ArgsError::MessageFormatError(format!("{}", e)))?
            ),
        }
        Ok(())
    }
}

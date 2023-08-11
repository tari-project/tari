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
        if headers.is_empty() {
            Err(ArgsError::NoHeaders.into())
        } else {
            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
            Ok(())
        }
    }
}

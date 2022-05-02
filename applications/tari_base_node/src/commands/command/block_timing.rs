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
        if headers.is_empty() {
            Err(ArgsError::NoHeaders.into())
        } else {
            let headers = headers.into_iter().map(|ch| ch.into_header()).rev().collect::<Vec<_>>();
            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            let first = headers.first().ok_or(ArgsError::HeaderLost)?.height;
            let last = headers.last().ok_or(ArgsError::HeaderLost)?.height;
            println!("Timing for blocks #{} - #{}", first, last);
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
            Ok(())
        }
    }
}

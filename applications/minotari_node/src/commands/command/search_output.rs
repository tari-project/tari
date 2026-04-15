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
use tari_common_types::{payment_reference::generate_payment_reference, types::FixedHash};

use super::{CommandContext, HandleCommand};
use crate::commands::parser::FromHex;

/// This will search the main chain for the utxo.
/// If the utxo is found, it will print out
/// the block it was found in.
#[derive(Debug, Parser)]
pub struct Args {
    /// hex of commitment of the utxo
    hash: FromHex<FixedHash>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.search_output(args.hash.0).await
    }
}

impl CommandContext {
    /// Function to process the search utxo command
    pub async fn search_output(&mut self, hash: FixedHash) -> Result<(), Error> {
        let mined_info = self.node_service.fetch_mined_info_by_output_hash(&hash).await?;
        println!("---- Mined info ----");
        println!("{mined_info}");
        if let Some(spent_info) = mined_info.input {
            let payref = generate_payment_reference(&spent_info.header_hash, &hash);
            println!("Payref for output: {payref}");
        }
        Ok(())
    }
}

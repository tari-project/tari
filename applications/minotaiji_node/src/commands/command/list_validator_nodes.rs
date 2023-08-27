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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use taiji_common_types::{epoch::VnEpoch, types::PublicKey};
use tari_utilities::hex::to_hex;

use super::{CommandContext, HandleCommand};
use crate::table::Table;

/// Lists the peer connections currently held by this node
#[derive(Debug, Parser)]
pub struct Args {
    epoch: Option<VnEpoch>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.list_validator_nodes(args).await
    }
}

impl CommandContext {
    async fn print_validator_nodes_list(&mut self, vns: &[(PublicKey, [u8; 32])]) {
        let num_vns = vns.len();
        let mut table = Table::new();
        table.set_titles(vec!["Public Key", "Shard ID"]);
        for (public_key, shard_key) in vns {
            table.add_row(row![public_key, to_hex(shard_key),]);
        }

        table.print_stdout();

        println!();
        println!("{} active validator(s)", num_vns);
    }
}

impl CommandContext {
    /// Function to process the list-connections command
    pub async fn list_validator_nodes(&mut self, args: Args) -> Result<(), Error> {
        let metadata = self.blockchain_db.get_chain_metadata().await?;
        let constants = self
            .consensus_rules
            .consensus_constants(metadata.height_of_longest_chain());
        let height = args
            .epoch
            .map(|epoch| constants.epoch_to_block_height(epoch))
            .unwrap_or_else(|| metadata.height_of_longest_chain());
        let vns = self.blockchain_db.fetch_active_validator_nodes(height).await?;

        println!();
        println!(
            "Registered validator nodes for epoch {}",
            constants.block_height_to_epoch(height).as_u64()
        );
        println!("----------------------------------");
        if vns.is_empty() {
            println!("No active validator nodes.");
        } else {
            println!();
            self.print_validator_nodes_list(&vns).await;
        }
        Ok(())
    }
}

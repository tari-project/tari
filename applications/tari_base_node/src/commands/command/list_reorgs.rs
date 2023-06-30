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

use super::{CommandContext, HandleCommand};
use crate::table::Table;

/// List tracked reorgs
/// This feature must be enabled by
/// setting `track_reorgs = true` in
/// the [base_node] section of your config."
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.list_reorgs()
    }
}

impl CommandContext {
    pub fn list_reorgs(&self) -> Result<(), Error> {
        if self.config.base_node.storage.track_reorgs {
            let reorgs = self.blockchain_db.inner().fetch_all_reorgs()?;
            let mut table = Table::new();
            table.set_titles(vec!["#", "New Tip", "Prev Tip", "Depth", "Timestamp"]);

            for (i, reorg) in reorgs.iter().enumerate() {
                table.add_row(row![
                    i + 1,
                    format!("#{} ({})", reorg.new_height, reorg.new_hash.to_hex()),
                    format!("#{} ({})", reorg.prev_height, reorg.prev_hash.to_hex()),
                    format!("{} added, {} removed", reorg.num_blocks_added, reorg.num_blocks_removed),
                    reorg.local_time
                ]);
            }
            table.enable_row_count().print_stdout();
        } else {
            println!(
                "Reorg tracking is turned off. Add `track_reorgs = true` to the [base_node] section of your config to \
                 turn it on."
            );
        }
        Ok(())
    }
}

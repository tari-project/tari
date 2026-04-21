//  Copyright 2026, The Tari Project
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

use anyhow::{Error, anyhow};
use async_trait::async_trait;
use clap::Parser;
use tari_core::chain_storage::JmtPruningMode;

use super::{CommandContext, HandleCommand};

/// Manually prune stale JMT nodes when `jmt_pruning_mode = "manual"`
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        match self.config.base_node.storage.jmt_pruning_mode {
            JmtPruningMode::Manual => {},
            JmtPruningMode::Off => {
                return Err(anyhow!(
                    "JMT pruning mode is 'off'. Set [base_node.storage].jmt_pruning_mode = \"manual\" to use \
                     prune-jmt."
                ));
            },
            JmtPruningMode::Background => {
                return Err(anyhow!(
                    "JMT pruning mode is 'background'. Automatic pruning is already enabled; switch to 'manual' to \
                     use prune-jmt explicitly."
                ));
            },
        }

        let (nodes_deleted, index_entries_removed) = self.blockchain_db.prune_jmt_stale_nodes().await?;
        println!(
            "JMT prune complete: deleted {nodes_deleted} node(s), removed {index_entries_removed} stale index entries."
        );
        Ok(())
    }
}

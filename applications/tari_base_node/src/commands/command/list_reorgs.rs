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
        if !self.config.blockchain_track_reorgs {
            // TODO: Return error/report
            println!(
                "Reorg tracking is turned off. Add `track_reorgs = true` to the [base_node] section of your config to \
                 turn it on."
            );
        } else {
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
        }
        Ok(())
    }
}

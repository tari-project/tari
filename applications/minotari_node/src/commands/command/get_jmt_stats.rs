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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use tari_core::chain_storage::DatabaseStats;

use super::{CommandContext, HandleCommand};

/// Show Jellyfish Merkle Tree pruning statistics (stale nodes, prune backlog, last-run deletions)
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        let database_stats = self.blockchain_db.get_database_stats().await?;
        let stats = &database_stats.jmt_pruning_stats;
        let mode = self.config.base_node.storage.jmt_pruning_mode;

        println!("JMT pruning stats");
        println!("-----------------");
        println!("Configured pruning mode:          {mode}");
        println!("Stale nodes from last block:      {}", stats.stale_nodes_last_block);
        println!("Total awaiting prune (backlog):   {}", stats.total_pending_stale_nodes);
        println!("Nodes deleted in last prune run:  {}", stats.last_prune_deleted_nodes);
        println!("Index entries removed last run:   {}", stats.last_prune_deleted_index);
        println!();
        println!("Compact: {}", format_jmt_status(&database_stats));
        Ok(())
    }
}

pub(crate) fn format_jmt_status(stats: &DatabaseStats) -> String {
    let stats = &stats.jmt_pruning_stats;
    format!(
        "new_stale={} awaiting_prune={} pruned={} nodes/{} idx",
        format_compact_count(stats.stale_nodes_last_block),
        format_compact_count(stats.total_pending_stale_nodes),
        format_compact_count(stats.last_prune_deleted_nodes),
        format_compact_count(stats.last_prune_deleted_index),
    )
}

fn format_compact_count(count: u64) -> String {
    const THOUSAND: f64 = 1_000.0;
    const MILLION: f64 = 1_000_000.0;

    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format_compact_scaled(count as f64 / THOUSAND, "k"),
        _ => format_compact_scaled(count as f64 / MILLION, "M"),
    }
}

fn format_compact_scaled(value: f64, suffix: &str) -> String {
    let precision = if value < 10.0 { 1 } else { 0 };
    let formatted = format!("{value:.precision$}");
    let formatted = if precision > 0 {
        formatted.trim_end_matches('0').trim_end_matches('.')
    } else {
        &formatted
    };
    format!("{formatted}{suffix}")
}

#[cfg(test)]
mod tests {
    use tari_core::chain_storage::{DatabaseStats, JmtPruningStats};

    use super::format_jmt_status;

    #[test]
    fn format_jmt_status_displays_zero_values() {
        let stats = DatabaseStats::default();
        assert_eq!(
            format_jmt_status(&stats),
            "new_stale=0 awaiting_prune=0 pruned=0 nodes/0 idx"
        );
    }

    #[test]
    fn format_jmt_status_compacts_large_pending_values() {
        let stats = DatabaseStats {
            jmt_pruning_stats: JmtPruningStats {
                stale_nodes_last_block: 23,
                total_pending_stale_nodes: 1_400,
                last_prune_deleted_nodes: 120,
                last_prune_deleted_index: 500,
            },
            ..Default::default()
        };

        assert_eq!(
            format_jmt_status(&stats),
            "new_stale=23 awaiting_prune=1.4k pruned=120 nodes/500 idx"
        );
    }

    #[test]
    fn format_jmt_status_compacts_million_scale_values() {
        let stats = DatabaseStats {
            jmt_pruning_stats: JmtPruningStats {
                stale_nodes_last_block: 12_400,
                total_pending_stale_nodes: 1_200_000,
                last_prune_deleted_nodes: 0,
                last_prune_deleted_index: 0,
            },
            ..Default::default()
        };

        assert_eq!(
            format_jmt_status(&stats),
            "new_stale=12k awaiting_prune=1.2M pruned=0 nodes/0 idx"
        );
    }
}

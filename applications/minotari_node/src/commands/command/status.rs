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

use std::time::{Duration, Instant};

use anyhow::{Error, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Parser;
use minotari_app_utilities::consts;
use tari_comms::connection_manager::SelfLivenessStatus;
use tari_core::chain_storage::DatabaseStats;
use tokio::time;

use super::{CommandContext, HandleCommand};
use crate::commands::status_line::{StatusLine, StatusLineOutput};

/// Prints out the status of this node
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(short, long, default_value_t = StatusLineOutput::StdOutAndLog)]
    output: StatusLineOutput,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.status(args.output).await
    }
}

impl CommandContext {
    // converting u64 to i64 is okay as the this is only for viewing timestamps.
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::too_many_lines)]
    pub async fn status(&mut self, output: StatusLineOutput) -> Result<(), Error> {
        let mut full_log = false;
        if self.last_time_full.elapsed() > Duration::from_secs(120) {
            self.last_time_full = Instant::now();
            full_log = true;
        }

        let mut status_line = StatusLine::new();
        status_line.add_field("", format!("v{}", consts::APP_VERSION_NUMBER));
        status_line.add_field("", self.config.network());
        status_line.add_field("State", self.state_machine_info.borrow().state_info.short_desc());

        let metadata = self.node_service.get_metadata().await?;
        let height = metadata.best_block_height();
        let last_header = self
            .node_service
            .get_header(height)
            .await?
            .ok_or_else(|| anyhow!("No last header"))?;
        let last_block_time =
            DateTime::<Utc>::from_timestamp(last_header.header().timestamp.as_u64() as i64, 0).unwrap_or_default();
        status_line.add_field(
            "Tip",
            format!("{} ({})", metadata.best_block_height(), last_block_time.to_rfc2822()),
        );

        let database_stats = self.blockchain_db.get_database_stats().await?;
        status_line.add_field("JMT", format_jmt_status(&database_stats));

        let constants = self.consensus_rules.consensus_constants(metadata.best_block_height());
        let fut = self.mempool_service.get_mempool_stats();
        if let Ok(mempool_stats) = time::timeout(Duration::from_secs(5), fut).await? {
            status_line.add_field(
                "Mempool",
                format!(
                    "{}tx ({}g, +/- {}blks)",
                    mempool_stats.unconfirmed_txs,
                    mempool_stats.unconfirmed_weight,
                    if mempool_stats.unconfirmed_weight == 0 {
                        0
                    } else {
                        1 + mempool_stats.unconfirmed_weight / constants.max_block_transaction_weight()
                    },
                ),
            );
        } else {
            status_line.add_field("Mempool", "query timed out");
        };

        let conns = self.comms.connectivity().get_active_connections().await?;
        let (num_nodes, num_clients) = conns.iter().fold((0usize, 0usize), |(nodes, clients), conn| {
            if conn.peer_features().is_node() {
                (nodes + 1, clients)
            } else {
                (nodes, clients + 1)
            }
        });
        status_line.add_field("Connections", format!("{num_nodes}|{num_clients}"));
        let banned_peers = self.fetch_banned_peers().await?;
        status_line.add_field("Banned", banned_peers.len());

        let num_messages = self
            .dht_metrics_collector
            .get_total_message_count_in_timespan(Duration::from_secs(60))
            .await?;
        status_line.add_field("Messages (last 60s)", num_messages);

        let num_active_rpc_sessions = self.rpc_server.get_num_active_sessions().await?;
        status_line.add_field(
            "Rpc",
            format!(
                "{}/{}",
                num_active_rpc_sessions, self.config.base_node.p2p.rpc_max_simultaneous_sessions
            ),
        );

        match self.comms.liveness_status() {
            SelfLivenessStatus::Disabled => {},
            SelfLivenessStatus::Checking => {
                status_line.add("⏳️️");
            },
            SelfLivenessStatus::Unreachable => {
                status_line.add("️🔌");
            },
            SelfLivenessStatus::Live(latency) => {
                status_line.add(format!("⚡️ {latency:.2?}"));
            },
        }

        if full_log {
            status_line.add_field(
                "RandomX",
                format!(
                    "#{} with flags {:?}",
                    self.state_machine_info.borrow().randomx_vm_cnt,
                    self.state_machine_info.borrow().randomx_vm_flags
                ),
            );
        }

        let target = "base_node::app::status";
        match output {
            StatusLineOutput::StdOutAndLog => {
                println!("{status_line}");
                log::info!(target: target, "{status_line}");
            },
            StatusLineOutput::Log => log::info!(target: target, "{status_line}"),
        };
        Ok(())
    }
}

fn format_jmt_status(stats: &DatabaseStats) -> String {
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
    let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
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

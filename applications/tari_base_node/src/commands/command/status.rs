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

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Parser;
use tari_app_utilities::consts;

use super::{CommandContext, HandleCommand};
use crate::commands::status_line::{StatusLine, StatusLineOutput};

/// Prints out the status of this node
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(default_value_t = StatusLineOutput::StdOutAndLog)]
    output: StatusLineOutput,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.status(args.output).await
    }
}

impl CommandContext {
    pub async fn status(&mut self, output: StatusLineOutput) -> Result<(), Error> {
        let mut full_log = false;
        if self.last_time_full.elapsed() > Duration::from_secs(120) {
            self.last_time_full = Instant::now();
            full_log = true;
        }

        let mut status_line = StatusLine::new();
        status_line.add_field("", format!("v{}", consts::APP_VERSION_NUMBER));
        status_line.add_field("", self.config.network);
        status_line.add_field("State", self.state_machine_info.borrow().state_info.short_desc());

        let metadata = self.node_service.get_metadata().await?;
        let height = metadata.height_of_longest_chain();
        let last_header = self
            .node_service
            .get_header(height)
            .await?
            .ok_or_else(|| anyhow!("No last header"))?;
        let last_block_time = DateTime::<Utc>::from(last_header.header().timestamp);
        status_line.add_field(
            "Tip",
            format!(
                "{} ({})",
                metadata.height_of_longest_chain(),
                last_block_time.to_rfc2822()
            ),
        );

        let constants = self
            .consensus_rules
            .consensus_constants(metadata.height_of_longest_chain());
        let mempool_stats = self.mempool_service.get_mempool_stats().await?;
        status_line.add_field(
            "Mempool",
            format!(
                "{}tx ({}g, +/- {}blks)",
                mempool_stats.unconfirmed_txs,
                mempool_stats.total_weight,
                if mempool_stats.total_weight == 0 {
                    0
                } else {
                    1 + mempool_stats.total_weight / constants.get_max_block_transaction_weight()
                },
            ),
        );

        let conns = self.connectivity.get_active_connections().await?;
        status_line.add_field("Connections", conns.len());
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
                num_active_rpc_sessions,
                self.config
                    .comms_rpc_max_simultaneous_sessions
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "∞".to_string()),
            ),
        );
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
                println!("{}", status_line);
                log::info!(target: target, "{}", status_line);
            },
            StatusLineOutput::Log => log::info!(target: target, "{}", status_line),
        };
        Ok(())
    }
}

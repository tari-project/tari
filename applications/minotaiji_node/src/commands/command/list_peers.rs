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
use chrono::Utc;
use clap::Parser;
use taiji_comms::peer_manager::PeerQuery;
use taiji_core::base_node::state_machine_service::states::PeerMetadata;

use super::{CommandContext, HandleCommand};
use crate::{table::Table, utils::format_duration_basic};

/// Lists the peers that this node knows about
#[derive(Debug, Parser)]
pub struct Args {
    filter: Option<String>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.list_peers(args.filter).await
    }
}

impl CommandContext {
    pub async fn list_peers(&self, filter: Option<String>) -> Result<(), Error> {
        let mut query = PeerQuery::new();
        if let Some(f) = filter {
            let filter = f.to_lowercase();
            query = query.select_where(move |p| match filter.as_str() {
                "basenode" | "basenodes" | "base_node" | "base-node" | "bn" => p.features.is_node(),
                "wallet" | "wallets" | "w" => p.features.is_client(),
                _ => false,
            })
        }
        let mut peers = self.comms.peer_manager().perform_query(query).await?;
        let num_peers = peers.len();
        println!();
        let mut table = Table::new();
        table.set_titles(vec!["NodeId", "Public Key", "Role", "User Agent", "Info"]);

        peers.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        for peer in peers {
            let info_str = {
                let mut s = vec![];
                if peer.is_seed() {
                    s.push("SEED".to_string());
                }
                if peer.is_offline() && !peer.is_banned() {
                    s.push("OFFLINE".to_string());
                }

                if let Some(dt) = peer.banned_until() {
                    s.push(format!(
                        "BANNED({}, {})",
                        dt.signed_duration_since(Utc::now().naive_utc())
                            .to_std()
                            .map(format_duration_basic)
                            .unwrap_or_else(|_| "∞".to_string()),
                        peer.banned_reason
                    ));
                }

                if let Some(metadata) = peer
                    .get_metadata(1)
                    .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                {
                    s.push(format!("chain height: {}", metadata.metadata.height_of_longest_chain()));
                }

                if let Some(last_seen) = peer.addresses.last_seen() {
                    let duration = Utc::now()
                        .naive_utc()
                        .signed_duration_since(last_seen)
                        .to_std()
                        .map(format_duration_basic)
                        .unwrap_or_else(|_| "?".into());
                    s.push(format!("last seen: {}", duration));
                }

                if s.is_empty() {
                    "--".to_string()
                } else {
                    s.join(", ")
                }
            };
            let ua = peer.user_agent;
            table.add_row(row![
                peer.node_id,
                peer.public_key,
                {
                    if peer.features.is_client() {
                        "Wallet"
                    } else {
                        "Base node"
                    }
                },
                {
                    if ua.is_empty() {
                        "<unknown>"
                    } else {
                        ua.as_ref()
                    }
                },
                info_str,
            ]);
        }
        table.print_stdout();

        println!("{} peer(s) known by this node", num_peers);
        Ok(())
    }
}

use std::time::{Duration, Instant};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clap::Parser;
use tari_app_utilities::consts;
use tari_comms::peer_manager::{PeerFeatures, PeerQuery};
use tari_core::base_node::state_machine_service::states::PeerMetadata;

use super::{CommandContext, HandleCommand};
use crate::{commands::status_line::StatusLine, table::Table, utils::format_duration_basic, StatusOutput};

#[derive(Debug, Parser)]
pub struct Args {
    filter: Option<String>,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        let filter = args.filter;
        let mut query = PeerQuery::new();
        if let Some(f) = filter {
            let filter = f.to_lowercase();
            query = query.select_where(move |p| match filter.as_str() {
                "basenode" | "basenodes" | "base_node" | "base-node" | "bn" => {
                    p.features == PeerFeatures::COMMUNICATION_NODE
                },
                "wallet" | "wallets" | "w" => p.features == PeerFeatures::COMMUNICATION_CLIENT,
                _ => false,
            })
        }
        let peers = self.peer_manager.perform_query(query).await?;
        let num_peers = peers.len();
        println!();
        let mut table = Table::new();
        table.set_titles(vec!["NodeId", "Public Key", "Role", "User Agent", "Info"]);

        for peer in peers {
            let info_str = {
                let mut s = Vec::new();

                if peer.is_offline() {
                    if !peer.is_banned() {
                        s.push("OFFLINE".to_string());
                    }
                } else if let Some(dt) = peer.last_seen() {
                    s.push(format!(
                        "LAST_SEEN: {}",
                        Utc::now()
                            .naive_utc()
                            .signed_duration_since(dt)
                            .to_std()
                            .map(format_duration_basic)
                            .unwrap_or_else(|_| "?".into())
                    ));
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

                if let Some(updated_at) = peer.identity_signature.map(|i| i.updated_at()) {
                    s.push(format!("updated_at: {} (UTC)", updated_at));
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
                    if peer.features == PeerFeatures::COMMUNICATION_CLIENT {
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

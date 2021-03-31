// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::LOG_TARGET;
use crate::{builder::BaseNodeContext, table::Table, utils::format_duration_basic};
use chrono::{DateTime, Utc};
use log::*;
use regex::Regex;
use std::{
    io::{self, Write},
    string::ToString,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerManager, PeerManagerError, PeerQuery},
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester, MetricsCollectorHandle};
use tari_core::{
    base_node::{
        state_machine_service::states::{PeerMetadata, StatusInfo},
        LocalNodeCommsInterface,
    },
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, ChainHeader, LMDBDatabase},
    mempool::service::LocalMempoolService,
    proof_of_work::PowAlgorithm,
    tari_utilities::{hex::Hex, message_format::MessageFormat},
    transactions::types::{Commitment, HashOutput, Signature},
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_wallet::util::emoji::EmojiId;
use tokio::{runtime, sync::watch};
// Import the auto-generated const values from the Manifest and Git

include!(concat!(env!("OUT_DIR"), "/consts.rs"));
pub struct CommandHandler {
    executor: runtime::Handle,
    blockchain_db: AsyncBlockchainDb<LMDBDatabase>,
    discovery_service: DhtDiscoveryRequester,
    dht_metrics_collector: MetricsCollectorHandle,
    base_node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connectivity: ConnectivityRequester,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    state_machine_info: watch::Receiver<StatusInfo>,
}

impl CommandHandler {
    pub fn new(executor: runtime::Handle, ctx: &BaseNodeContext) -> Self {
        CommandHandler {
            executor,
            blockchain_db: ctx.blockchain_db().into(),
            discovery_service: ctx.base_node_dht().discovery_service_requester(),
            dht_metrics_collector: ctx.base_node_dht().metrics_collector(),
            base_node_identity: ctx.base_node_identity(),
            peer_manager: ctx.base_node_comms().peer_manager(),
            connectivity: ctx.base_node_comms().connectivity(),
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            state_machine_info: ctx.get_state_machine_info_channel(),
        }
    }

    pub fn status(&self) {
        let mut state_info = self.state_machine_info.clone();
        let mut node = self.node_service.clone();
        let mut mempool = self.mempool_service.clone();
        let peer_manager = self.peer_manager.clone();
        let mut connectivity = self.connectivity.clone();
        let mut metrics = self.dht_metrics_collector.clone();

        self.executor.spawn(async move {
            let state = state_info.recv().await.unwrap();
            let metadata = node.get_metadata().await.unwrap();
            let last_header = node
                .get_headers(vec![metadata.height_of_longest_chain()])
                .await
                .unwrap()
                .pop()
                .unwrap();
            let last_block_time: DateTime<Utc> = last_header.timestamp.into();
            let mempool_stats = mempool.get_mempool_stats().await.unwrap();
            let banned_peers = banned_peers(&peer_manager).await.unwrap();
            let conns = connectivity.get_active_connections().await.unwrap();
            let messages = metrics
                .get_total_message_count_in_timespan(Duration::from_secs(60))
                .await
                .unwrap();
            let status = format!(
                "{}: State: {}, Tip: {} ({}), Mempool: {}tx ({}g, +/- {}blks), Connections: {}, Messages (last \
                 60s):{}, Banned: {}",
                Utc::now().format("%H:%M"),
                state.state_info.short_desc(),
                metadata.height_of_longest_chain(),
                last_block_time.to_rfc2822(),
                mempool_stats.total_txs,
                mempool_stats.total_weight,
                if mempool_stats.total_weight == 0 {
                    0
                } else {
                    1 + mempool_stats.total_weight / 19500
                },
                conns.len(),
                messages,
                banned_peers.len()
            );
            info!(target: "base_node::app::status", "{}", status);
            println!("{}", status);
        });
    }

    /// Function to process the get-state-info command
    pub fn state_info(&self) {
        let mut channel = self.state_machine_info.clone();
        self.executor.spawn(async move {
            match channel.recv().await {
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Error communicating with state machine, channel could have been closed"
                    );
                    return;
                },
                Some(data) => println!("Current state machine state:\n{}", data),
            };
        });
    }

    /// Function process the version command
    pub fn print_version(&self) {
        println!("Version: {}", VERSION);
        println!("Author: {}", AUTHOR);
    }

    pub fn get_chain_meta(&self) {
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_metadata().await {
                Err(err) => {
                    println!("Failed to retrieve chain metadata: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {:?}", err);
                    return;
                },
                Ok(data) => println!("{}", data),
            };
        });
    }

    pub fn get_block(&self, height: u64, format: Format) {
        let blockchain = self.blockchain_db.clone();
        self.executor.spawn(async move {
            match blockchain.fetch_blocks(height..=height).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {}", err);
                    warn!(target: LOG_TARGET, "{}", err);
                    return;
                },
                Ok(mut data) => match (data.pop(), format) {
                    (Some(block), Format::Text) => println!("{}", block),
                    (Some(block), Format::Json) => println!(
                        "{}",
                        block.to_json().unwrap_or_else(|_| "Error deserializing block".into())
                    ),
                    (None, _) => println!("Block not found at height {}", height),
                },
            };
        });
    }

    pub fn get_block_by_hash(&self, hash: HashOutput, format: Format) {
        let blockchain = self.blockchain_db.clone();
        self.executor.spawn(async move {
            match blockchain.fetch_block_by_hash(hash).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {}", err);
                    warn!(target: LOG_TARGET, "{}", err);
                    return;
                },
                Ok(data) => match (data, format) {
                    (Some(block), Format::Text) => println!("{}", block),
                    (Some(block), Format::Json) => println!(
                        "{}",
                        block.to_json().unwrap_or_else(|_| "Error deserializing block".into())
                    ),
                    (None, _) => println!("Block not found"),
                },
            };
        });
    }

    pub fn search_utxo(&self, commitment: Commitment) {
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.fetch_blocks_with_utxos(vec![commitment.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: utxo found, but lock not found for utxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
    }

    pub fn search_stxo(&self, commitment: Commitment) {
        let mut handler = self.node_service.clone();
        self.executor.spawn(async move {
            match handler.get_blocks_with_stxos(vec![commitment.clone()]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: stxo found, but block not found for stxo commitment {}",
                        commitment.to_hex()
                    ),
                },
            };
        });
    }

    pub fn search_kernel(&self, excess_sig: Signature) {
        let mut handler = self.node_service.clone();
        let hex_sig = excess_sig.get_signature().to_hex();
        self.executor.spawn(async move {
            match handler.get_blocks_with_kernels(vec![excess_sig]).await {
                Err(err) => {
                    println!("Failed to retrieve blocks: {:?}", err);
                    warn!(
                        target: LOG_TARGET,
                        "Error communicating with local base node: {:?}", err,
                    );
                    return;
                },
                Ok(mut data) => match data.pop() {
                    Some(v) => println!("{}", v.block()),
                    _ => println!(
                        "Pruned node: kernel found, but block not found for kernel signature {}",
                        hex_sig
                    ),
                },
            };
        });
    }

    /// Function to process the get-mempool-stats command
    pub fn get_mempool_stats(&self) {
        let mut handler = self.mempool_service.clone();
        self.executor.spawn(async move {
            match handler.get_mempool_stats().await {
                Ok(stats) => println!("{}", stats),
                Err(err) => {
                    println!("Failed to retrieve mempool stats: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with local mempool: {:?}", err,);
                    return;
                },
            };
        });
    }

    /// Function to process the get-mempool-state command
    pub fn get_mempool_state(&self) {
        let mut handler = self.mempool_service.clone();
        self.executor.spawn(async move {
            match handler.get_mempool_state().await {
                Ok(state) => println!("{}", state),
                Err(err) => {
                    println!("Failed to retrieve mempool state: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with local mempool: {:?}", err,);
                    return;
                },
            };
        });
    }

    pub fn discover_peer(&self, dest_pubkey: Box<RistrettoPublicKey>) {
        let mut dht = self.discovery_service.clone();

        self.executor.spawn(async move {
            let start = Instant::now();
            println!("🌎 Peer discovery started.");

            match dht
                .discover_peer(dest_pubkey.clone(), NodeDestination::PublicKey(dest_pubkey))
                .await
            {
                Ok(p) => {
                    println!("⚡️ Discovery succeeded in {}ms!", start.elapsed().as_millis());
                    println!("This peer was found:");
                    println!("{}", p);
                },
                Err(err) => {
                    println!("💀 Discovery failed: '{:?}'", err);
                },
            }
        });
    }

    pub fn get_peer(&self, node_id: NodeId) {
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            match peer_manager.find_by_node_id(&node_id).await {
                Ok(peer) => {
                    let eid = EmojiId::from_pubkey(&peer.public_key);
                    println!("Emoji ID: {}", eid);
                    println!("Public Key: {}", peer.public_key);
                    println!("NodeId: {}", peer.node_id);
                    println!("Addresses:");
                    peer.addresses.iter().for_each(|a| {
                        println!("- {}", a);
                    });
                    println!("User agent: {}", peer.user_agent);
                    println!("Features: {:?}", peer.features);
                    println!("Supported protocols:");
                    peer.supported_protocols.iter().for_each(|p| {
                        println!("- {}", String::from_utf8_lossy(p));
                    });
                    if let Some(dt) = peer.banned_until() {
                        println!("Banned until {}, reason: {}", dt, peer.banned_reason);
                    }
                    if let Some(dt) = peer.last_seen() {
                        println!("Last seen: {}", dt);
                    }
                },
                Err(err) => {
                    println!("{}", err);
                },
            }
        });
    }

    pub fn list_peers(&self, filter: Option<String>) {
        let peer_manager = self.peer_manager.clone();
        self.executor.spawn(async move {
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
            match peer_manager.perform_query(query).await {
                Ok(peers) => {
                    let num_peers = peers.len();
                    println!();
                    let mut table = Table::new();
                    table.set_titles(vec!["NodeId", "Public Key", "Flags", "Role", "User Agent", "Info"]);

                    for peer in peers {
                        let info_str = {
                            let mut s = vec![];

                            if peer.is_offline() {
                                if !peer.is_banned() {
                                    s.push("OFFLINE".to_string());
                                }
                            } else if let Some(dt) = peer.last_seen() {
                                s.push(format!(
                                    "LAST_SEEN = {}",
                                    Utc::now()
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
                                s.push(format!(
                                    "chain height = {}",
                                    metadata.metadata.height_of_longest_chain()
                                ));
                            }

                            if s.is_empty() {
                                "--".to_string()
                            } else {
                                s.join(", ")
                            }
                        };
                        table.add_row(row![
                            peer.node_id,
                            peer.public_key,
                            format!("{:?}", peer.flags),
                            {
                                if peer.features == PeerFeatures::COMMUNICATION_CLIENT {
                                    "Wallet"
                                } else {
                                    "Base node"
                                }
                            },
                            Some(peer.user_agent)
                                .map(|ua| if ua.is_empty() { "<unknown>".to_string() } else { ua })
                                .unwrap(),
                            info_str,
                        ]);
                    }
                    table.print_std();

                    println!("{} peer(s) known by this node", num_peers);
                },
                Err(err) => {
                    println!("Failed to list peers: {:?}", err);
                    error!(target: LOG_TARGET, "Could not list peers: {:?}", err);
                    return;
                },
            }
        });
    }

    pub fn dial_peer(&self, dest_node_id: NodeId) {
        let mut connectivity = self.connectivity.clone();

        self.executor.spawn(async move {
            let start = Instant::now();
            println!("☎️  Dialing peer...");

            match connectivity.dial_peer(dest_node_id).await {
                Ok(p) => {
                    println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
                    println!("Connection: {}", p);
                },
                Err(err) => {
                    println!("📞  Dial failed: {}", err);
                },
            }
        });
    }

    pub fn ban_peer(&self, node_id: NodeId, duration: Duration, must_ban: bool) {
        if self.base_node_identity.node_id() == &node_id {
            println!("Cannot ban our own node");
            return;
        }

        let mut connectivity = self.connectivity.clone();
        let peer_manager = self.peer_manager.clone();

        self.executor.spawn(async move {
            if must_ban {
                match connectivity
                    .ban_peer_until(node_id.clone(), duration, "UI manual ban".to_string())
                    .await
                {
                    Ok(_) => println!("Peer was banned in base node."),
                    Err(err) => {
                        println!("Failed to ban peer: {:?}", err);
                        error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                    },
                }
            } else {
                match peer_manager.unban_peer(&node_id).await {
                    Ok(_) => {
                        println!("Peer ban was removed from base node.");
                    },
                    Err(err) if err.is_peer_not_found() => {
                        println!("Peer not found in base node");
                    },
                    Err(err) => {
                        println!("Failed to ban peer: {:?}", err);
                        error!(target: LOG_TARGET, "Could not ban peer: {:?}", err);
                    },
                }
            }
        });
    }

    pub fn unban_all_peers(&self) {
        let peer_manager = self.peer_manager.clone();
        self.executor.spawn(async move {
            async fn unban_all(pm: &PeerManager) -> usize {
                let query = PeerQuery::new().select_where(|p| p.is_banned());
                match pm.perform_query(query).await {
                    Ok(peers) => {
                        let num_peers = peers.len();
                        for peer in peers {
                            if let Err(err) = pm.unban_peer(&peer.node_id).await {
                                println!("Failed to unban peer: {}", err);
                            }
                        }
                        num_peers
                    },
                    Err(err) => {
                        println!("Failed to unban peers: {}", err);
                        0
                    },
                }
            }

            let n = unban_all(&peer_manager).await;
            println!("Unbanned {} peer(s) from node", n);
        });
    }

    pub fn list_banned_peers(&self) {
        let peer_manager = self.peer_manager.clone();
        self.executor.spawn(async move {
            match banned_peers(&peer_manager).await {
                Ok(banned) => {
                    if banned.is_empty() {
                        println!("No peers banned from node.")
                    } else {
                        println!("Peers banned from node ({}):", banned.len());
                        for peer in banned {
                            println!("{}", peer);
                        }
                    }
                },
                Err(e) => println!("Error listing peers: {}", e),
            }
        });
    }

    /// Function to process the list-connections command
    pub fn list_connections(&self) {
        let mut connectivity = self.connectivity.clone();
        let peer_manager = self.peer_manager.clone();
        let node_id = self.base_node_identity.node_id().clone();

        self.executor.spawn(async move {
            match connectivity.get_active_connections().await {
                Ok(conns) if conns.is_empty() => {
                    println!("No active peer connections.");
                },
                Ok(conns) => {
                    println!();
                    let num_connections = conns.len();
                    let mut table = Table::new();
                    table.set_titles(vec![
                        "NodeId",
                        "Public Key",
                        "Address",
                        "Direction",
                        "Dist Bucket",
                        "Age",
                        "Role",
                        "User Agent",
                        "Chain Height",
                    ]);
                    for conn in conns {
                        let peer = peer_manager
                            .find_by_node_id(conn.peer_node_id())
                            .await
                            .expect("Unexpected peer database error or peer not found");

                        let chain_height = if let Some(metadata) = peer
                            .get_metadata(1)
                            .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                        {
                            Some(format!("Height = #{}", metadata.metadata.height_of_longest_chain()))
                        } else {
                            None
                        };

                        table.add_row(row![
                            peer.node_id,
                            peer.public_key,
                            conn.address(),
                            conn.direction(),
                            peer.node_id.distance(&node_id).get_bucket(4).2,
                            format_duration_basic(conn.age()),
                            {
                                if peer.features == PeerFeatures::COMMUNICATION_CLIENT {
                                    "Wallet"
                                } else {
                                    "Base node"
                                }
                            },
                            Some(peer.user_agent)
                                .map(|ua| if ua.is_empty() { "<unknown>".to_string() } else { ua })
                                .unwrap(),
                            chain_height.unwrap_or_default(),
                        ]);
                    }

                    table.print_std();

                    println!("{} active connection(s)", num_connections);
                },
                Err(err) => {
                    println!("Failed to list connections: {:?}", err);
                    error!(target: LOG_TARGET, "Could not list connections: {:?}", err);
                    return;
                },
            }
        });
    }

    pub fn reset_offline_peers(&self) {
        let peer_manager = self.peer_manager.clone();
        self.executor.spawn(async move {
            let result = peer_manager
                .update_each(|mut peer| {
                    if peer.is_offline() {
                        peer.set_offline(false);
                        Some(peer)
                    } else {
                        None
                    }
                })
                .await;

            match result {
                Ok(num_updated) => {
                    println!("{} peer(s) were unmarked as offline.", num_updated);
                },
                Err(err) => {
                    println!("Failed to clear offline peer states: {:?}", err);
                    error!(target: LOG_TARGET, "{:?}", err);
                    return;
                },
            }
        });
    }

    pub fn list_headers(&self, start: u64, end: Option<u64>) {
        let blockchain_db = self.blockchain_db.clone();
        self.executor.spawn(async move {
            let headers = match Self::get_chain_headers(&blockchain_db, start, end).await {
                Ok(h) if h.is_empty() => {
                    println!("No headers found");
                    return;
                },
                Ok(h) => h,
                Err(err) => {
                    println!("Failed to retrieve headers: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
            };

            for header in headers {
                println!("\n\nHeader hash: {}", header.hash().to_hex());
                println!("{}", header);
            }
        });
    }

    /// Function to process the get-headers command
    async fn get_chain_headers(
        blockchain_db: &AsyncBlockchainDb<LMDBDatabase>,
        start: u64,
        end: Option<u64>,
    ) -> Result<Vec<ChainHeader>, anyhow::Error>
    {
        match end {
            Some(end) => blockchain_db.fetch_chain_headers(start..=end).await.map_err(Into::into),
            None => {
                let from_tip = start;
                if from_tip == 0 {
                    return Ok(Vec::new());
                }
                let tip = blockchain_db.fetch_tip_header().await?.height();
                blockchain_db
                    .fetch_chain_headers(tip.saturating_sub(from_tip - 1)..=tip)
                    .await
                    .map_err(Into::into)
            },
        }
    }

    pub fn calc_timing(&self, start: u64, end: Option<u64>) {
        let blockchain_db = self.blockchain_db.clone();
        self.executor.spawn(async move {
            let headers = match Self::get_chain_headers(&blockchain_db, start, end).await {
                Ok(h) if h.is_empty() => {
                    println!("No headers found");
                    return;
                },
                Ok(h) => h.into_iter().map(|ch| ch.header).rev().collect::<Vec<_>>(),
                Err(err) => {
                    println!("Failed to retrieve headers: {:?}", err);
                    warn!(target: LOG_TARGET, "Error communicating with base node: {}", err,);
                    return;
                },
            };

            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            println!(
                "Timing for blocks #{} - #{}",
                headers.first().unwrap().height,
                headers.last().unwrap().height
            );
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        });
    }

    /// Function to process the check-db command
    pub fn check_db(&self) {
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain();
            let mut missing_blocks = Vec::new();
            let mut missing_headers = Vec::new();
            print!("Searching for height: ");
            // We need to check every header, but not every block.
            let horizon_height = meta.horizon_block(height);
            while height > 0 {
                print!("{}", height);
                io::stdout().flush().unwrap();
                // we can only check till the pruning horizon, 0 is archive node so it needs to check every block.
                if height > horizon_height {
                    match node.get_blocks(vec![height]).await {
                        Err(_err) => {
                            missing_blocks.push(height);
                        },
                        Ok(mut data) => match data.pop() {
                            // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                            // logging it.
                            Some(_historical_block) => {},
                            None => missing_blocks.push(height),
                        },
                    };
                }
                height -= 1;
                let next_header = node.get_headers(vec![height]).await;
                if next_header.is_err() {
                    // this header is missing, so we stop here and need to ask for this header
                    missing_headers.push(height);
                };
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            }
            println!("Complete");
            for missing_block in missing_blocks {
                println!("Missing block at height: {}", missing_block);
            }
            for missing_header_height in missing_headers {
                println!("Missing header at height: {}", missing_header_height)
            }
        });
    }

    #[allow(deprecated)]
    pub fn period_stats(&self, period_end: u64, mut period_ticker_end: u64, period: u64) {
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let meta = node.get_metadata().await.expect("Could not retrieve chain meta");

            let mut height = meta.height_of_longest_chain();
            // Currently gets the stats for: tx count, hash rate estimation, target difficulty, solvetime.
            let mut results: Vec<(usize, f64, u64, u64, usize)> = Vec::new();

            let mut period_ticker_start = period_ticker_end - period;
            let mut period_tx_count = 0;
            let mut period_block_count = 0;
            let mut period_hash = 0.0;
            let mut period_difficulty = 0;
            let mut period_solvetime = 0;
            print!("Searching for height: ");
            while height > 0 {
                print!("{}", height);
                io::stdout().flush().unwrap();

                let block = match node.get_blocks(vec![height]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                let prev_block = match node.get_blocks(vec![height - 1]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                height -= 1;
                if block.block().header.timestamp.as_u64() > period_ticker_end {
                    print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
                    continue;
                };
                while block.block().header.timestamp.as_u64() < period_ticker_start {
                    results.push((
                        period_tx_count,
                        period_hash,
                        period_difficulty,
                        period_solvetime,
                        period_block_count,
                    ));
                    period_tx_count = 0;
                    period_block_count = 0;
                    period_hash = 0.0;
                    period_difficulty = 0;
                    period_solvetime = 0;
                    period_ticker_end -= period;
                    period_ticker_start -= period;
                }
                period_tx_count += block.block().body.kernels().len() - 1;
                period_block_count += 1;
                let st = if prev_block.block().header.timestamp.as_u64() >= block.block().header.timestamp.as_u64() {
                    1.0
                } else {
                    (block.block().header.timestamp.as_u64() - prev_block.block().header.timestamp.as_u64()) as f64
                };
                let diff = block.accumulated_data.target_difficulty.as_u64();
                period_difficulty += diff;
                period_solvetime += st as u64;
                period_hash += diff as f64 / st / 1_000_000.0;
                if period_ticker_end <= period_end {
                    break;
                }
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            }
            println!("Complete");
            println!("Results of tx count, hash rate estimation, target difficulty, solvetime, block count");
            for data in results {
                println!("{},{},{},{},{}", data.0, data.1, data.2, data.3, data.4);
            }
        });
    }

    pub fn raw_stats(&self, start_height: u64, end_height: u64) {
        let mut node = self.node_service.clone();
        self.executor.spawn(async move {
            let mut results: Vec<(u64, usize, u64, u64, u64, PowAlgorithm)> = Vec::new();

            print!("Searching for height: ");
            for height in start_height..=end_height {
                print!("{}", height);
                io::stdout().flush().unwrap();

                let block = match node.get_blocks(vec![height]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                let prev_block = match node.get_blocks(vec![height - 1]).await {
                    Err(_err) => {
                        println!("Error in db, could not get block");
                        break;
                    },
                    Ok(mut data) => match data.pop() {
                        // We need to check the data it self, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        Some(historical_block) => historical_block,
                        None => {
                            println!("Error in db, could not get block");
                            break;
                        },
                    },
                };
                let st = if prev_block.block().header.timestamp.as_u64() >= block.block().header.timestamp.as_u64() {
                    1.0
                } else {
                    (block.block().header.timestamp.as_u64() - prev_block.block().header.timestamp.as_u64()) as f64
                };
                results.push((
                    height,
                    block.block().body.kernels().len() - 1,
                    block.accumulated_data.target_difficulty.as_u64(),
                    st as u64,
                    block.block().header.timestamp.as_u64(),
                    block.block().header.pow.pow_algo,
                ));
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            }
            println!("Complete");
            println!("Result of height, tx count, target difficulty, solvetime, timestamp, algo");
            for data in results {
                println!("{},{},{},{},{},{}", data.0, data.1, data.2, data.3, data.4, data.5);
            }
        });
    }

    /// Function to process the whoami command
    pub fn whoami(&self) {
        println!("{}", self.base_node_identity);
    }
}

async fn banned_peers(pm: &PeerManager) -> Result<Vec<Peer>, PeerManagerError> {
    let query = PeerQuery::new().select_where(|p| p.is_banned());
    pm.perform_query(query).await
}

pub enum Format {
    Json,
    Text,
}

// Function to delimit arguments using spaces and pairs of quotation marks, which may include spaces
pub fn delimit_command_string(command_str: &str) -> Vec<String> {
    // Delimit arguments using spaces and pairs of quotation marks, which may include spaces
    let arg_temp = command_str.trim().to_string();
    let re = Regex::new(r#"[^\s"]+|"(?:\\"|[^"])+""#).unwrap();
    let arg_temp_vec: Vec<&str> = re.find_iter(&arg_temp).map(|mat| mat.as_str()).collect();
    // Remove quotation marks left behind by `Regex` - it does not support look ahead and look behind
    let mut del_arg_vec = Vec::new();
    for arg in arg_temp_vec.iter().skip(1) {
        del_arg_vec.push(str::replace(arg, "\"", ""));
    }
    del_arg_vec
}

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

use std::{
    cmp,
    io::{self, Write},
    ops::Deref,
    str::FromStr,
    string::ToString,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Error};
use chrono::{DateTime, Utc};
use log::*;
use tari_app_utilities::{consts, utilities::parse_emoji_id_or_public_key};
use tari_common::GlobalConfig;
use tari_common_types::{
    emoji::EmojiId,
    types::{Commitment, HashOutput, Signature},
};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerManager, PeerManagerError, PeerQuery},
    protocol::rpc::RpcServerHandle,
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester, MetricsCollectorHandle};
use tari_core::{
    base_node::{
        comms_interface::BlockEvent,
        state_machine_service::states::{PeerMetadata, StatusInfo},
        LocalNodeCommsInterface,
    },
    blocks::{BlockHeader, ChainHeader},
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    consensus::ConsensusManager,
    mempool::service::LocalMempoolService,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_p2p::{
    auto_update::SoftwareUpdaterHandle,
    services::liveness::{LivenessEvent, LivenessHandle},
};
use tari_utilities::{hex::Hex, message_format::MessageFormat, Hashable};
use thiserror::Error;
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{broadcast, watch},
};

use super::status_line::StatusLine;
use crate::{builder::BaseNodeContext, table::Table, utils::format_duration_basic, LOG_TARGET};

pub enum StatusLineOutput {
    Log,
    StdOutAndLog,
}

pub struct CommandHandler {
    config: Arc<GlobalConfig>,
    consensus_rules: ConsensusManager,
    blockchain_db: AsyncBlockchainDb<LMDBDatabase>,
    discovery_service: DhtDiscoveryRequester,
    dht_metrics_collector: MetricsCollectorHandle,
    rpc_server: RpcServerHandle,
    base_node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    connectivity: ConnectivityRequester,
    liveness: LivenessHandle,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    state_machine_info: watch::Receiver<StatusInfo>,
    software_updater: SoftwareUpdaterHandle,
    last_time_full: Instant,
}

impl CommandHandler {
    pub fn new(ctx: &BaseNodeContext) -> Self {
        Self {
            config: ctx.config(),
            consensus_rules: ctx.consensus_rules().clone(),
            blockchain_db: ctx.blockchain_db().into(),
            discovery_service: ctx.base_node_dht().discovery_service_requester(),
            dht_metrics_collector: ctx.base_node_dht().metrics_collector(),
            rpc_server: ctx.rpc_server(),
            base_node_identity: ctx.base_node_identity(),
            peer_manager: ctx.base_node_comms().peer_manager(),
            connectivity: ctx.base_node_comms().connectivity(),
            liveness: ctx.liveness(),
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            state_machine_info: ctx.get_state_machine_info_channel(),
            software_updater: ctx.software_updater(),
            last_time_full: Instant::now(),
        }
    }

    pub fn global_config(&self) -> Arc<GlobalConfig> {
        self.config.clone()
    }

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
        let banned_peers = fetch_banned_peers(&self.peer_manager).await?;
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
                info!(target: target, "{}", status_line);
            },
            StatusLineOutput::Log => info!(target: target, "{}", status_line),
        };
        Ok(())
    }

    /// Function to process the get-state-info command
    pub fn state_info(&self) -> Result<(), Error> {
        println!("Current state machine state:\n{}", *self.state_machine_info.borrow());
        Ok(())
    }

    /// Check for updates
    pub async fn check_for_updates(&mut self) -> Result<(), Error> {
        println!("Checking for updates (current version: {})...", consts::APP_VERSION);
        match self.software_updater.check_for_updates().await {
            Some(update) => {
                println!(
                    "Version {} of the {} is available: {} (sha: {})",
                    update.version(),
                    update.app(),
                    update.download_url(),
                    update.to_hash_hex()
                );
            },
            None => {
                println!("No updates found.",);
            },
        }
        Ok(())
    }

    /// Function process the version command
    pub fn print_version(&self) -> Result<(), Error> {
        println!("Version: {}", consts::APP_VERSION);
        println!("Author: {}", consts::APP_AUTHOR);
        println!("Avx2: {}", match cfg!(feature = "avx2") {
            true => "enabled",
            false => "disabled",
        });

        if let Some(ref update) = *self.software_updater.new_update_notifier().borrow() {
            println!(
                "Version {} of the {} is available: {} (sha: {})",
                update.version(),
                update.app(),
                update.download_url(),
                update.to_hash_hex()
            );
        }
        Ok(())
    }

    pub async fn get_chain_meta(&mut self) -> Result<(), Error> {
        let data = self.node_service.get_metadata().await?;
        println!("{}", data);
        Ok(())
    }

    pub async fn get_block(&self, height: u64, format: Format) -> Result<(), Error> {
        let mut data = self.blockchain_db.fetch_blocks(height..=height).await?;
        match (data.pop(), format) {
            (Some(block), Format::Text) => {
                let block_data = self
                    .blockchain_db
                    .fetch_block_accumulated_data(block.hash().clone())
                    .await?;

                println!("{}", block);
                println!("-- Accumulated data --");
                println!("{}", block_data);
            },
            (Some(block), Format::Json) => println!("{}", block.to_json()?),
            (None, _) => println!("Block not found at height {}", height),
        }
        Ok(())
    }

    pub async fn get_block_by_hash(&self, hash: HashOutput, format: Format) -> Result<(), Error> {
        let data = self.blockchain_db.fetch_block_by_hash(hash).await?;
        match (data, format) {
            (Some(block), Format::Text) => println!("{}", block),
            (Some(block), Format::Json) => println!("{}", block.to_json()?),
            (None, _) => println!("Block not found"),
        }
        Ok(())
    }

    pub async fn search_utxo(&mut self, commitment: Commitment) -> Result<(), Error> {
        let v = self
            .node_service
            .fetch_blocks_with_utxos(vec![commitment.clone()])
            .await?
            .pop()
            .ok_or_else(|| anyhow!("Block not found for utxo commitment {}", commitment.to_hex()))?;
        println!("{}", v.block());
        Ok(())
    }

    pub async fn search_kernel(&mut self, excess_sig: Signature) -> Result<(), Error> {
        let hex_sig = excess_sig.get_signature().to_hex();
        let v = self
            .node_service
            .get_blocks_with_kernels(vec![excess_sig])
            .await?
            .pop()
            .ok_or_else(|| anyhow!("No kernel with signature {} found", hex_sig))?;
        println!("{}", v);
        Ok(())
    }

    /// Function to process the get-mempool-stats command
    pub async fn get_mempool_stats(&mut self) -> Result<(), Error> {
        let stats = self.mempool_service.get_mempool_stats().await?;
        println!("{}", stats);
        Ok(())
    }

    /// Function to process the get-mempool-state command
    pub async fn get_mempool_state(&mut self, filter: Option<String>) -> Result<(), Error> {
        let state = self.mempool_service.get_mempool_state().await?;
        println!("----------------- Mempool -----------------");
        println!("--- Unconfirmed Pool ---");
        for tx in &state.unconfirmed_pool {
            let tx_sig = tx
                .first_kernel_excess_sig()
                .map(|sig| sig.get_signature().to_hex())
                .unwrap_or_else(|| "N/A".to_string());
            if let Some(ref filter) = filter {
                if !tx_sig.contains(filter) {
                    println!("--- TX: {} ---", tx_sig);
                    println!("{}", tx.body);
                    continue;
                }
            } else {
                println!(
                    "    {} Fee: {}, Outputs: {}, Kernels: {}, Inputs: {}, metadata: {} bytes",
                    tx_sig,
                    tx.body.get_total_fee(),
                    tx.body.outputs().len(),
                    tx.body.kernels().len(),
                    tx.body.inputs().len(),
                    tx.body.sum_metadata_size(),
                );
            }
        }
        if filter.is_none() {
            println!("--- Reorg Pool ---");
            for excess_sig in &state.reorg_pool {
                println!("    {}", excess_sig.get_signature().to_hex());
            }
        }
        Ok(())
    }

    pub async fn discover_peer(&mut self, dest_pubkey: Box<RistrettoPublicKey>) -> Result<(), Error> {
        let start = Instant::now();
        println!("🌎 Peer discovery started.");
        let peer = self
            .discovery_service
            .discover_peer(dest_pubkey.deref().clone(), NodeDestination::PublicKey(dest_pubkey))
            .await?;
        println!("⚡️ Discovery succeeded in {}ms!", start.elapsed().as_millis());
        println!("This peer was found:");
        println!("{}", peer);
        Ok(())
    }

    pub async fn get_peer(&self, partial: Vec<u8>, original_str: String) {
        let peer = match self.peer_manager.find_all_starts_with(&partial).await {
            Ok(peers) if peers.is_empty() => {
                if let Some(pk) = parse_emoji_id_or_public_key(&original_str) {
                    if let Ok(Some(peer)) = self.peer_manager.find_by_public_key(&pk).await {
                        peer
                    } else {
                        println!("No peer matching '{}'", original_str);
                        return;
                    }
                } else {
                    println!("No peer matching '{}'", original_str);
                    return;
                }
            },
            Ok(mut peers) => peers.remove(0),
            Err(err) => {
                println!("{}", err);
                return;
            },
        };

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
        if let Some(updated_at) = peer.identity_signature.map(|i| i.updated_at()) {
            println!("Last updated: {} (UTC)", updated_at);
        }
    }

    pub async fn list_peers(&self, filter: Option<String>) -> Result<(), Error> {
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
                let mut s = vec![];

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

    pub async fn dial_peer(&self, dest_node_id: NodeId) -> Result<(), Error> {
        let start = Instant::now();
        println!("☎️  Dialing peer...");

        let connection = self.connectivity.dial_peer(dest_node_id).await?;
        println!("⚡️ Peer connected in {}ms!", start.elapsed().as_millis());
        println!("Connection: {}", connection);
        Ok(())
    }

    pub async fn ping_peer(&mut self, dest_node_id: NodeId) -> Result<(), Error> {
        println!("🏓 Pinging peer...");
        let mut liveness_events = self.liveness.get_event_stream();

        self.liveness.send_ping(dest_node_id.clone()).await?;
        loop {
            match liveness_events.recv().await {
                Ok(event) => {
                    if let LivenessEvent::ReceivedPong(pong) = &*event {
                        if pong.node_id == dest_node_id {
                            println!(
                                "🏓️ Pong received, latency in is {:.2?}!",
                                pong.latency.unwrap_or_default()
                            );
                            break;
                        }
                    }
                },
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                },
                _ => {},
            }
        }
        Ok(())
    }

    pub async fn ban_peer(&mut self, node_id: NodeId, duration: Duration, must_ban: bool) {
        if self.base_node_identity.node_id() == &node_id {
            println!("Cannot ban our own node");
            return;
        }

        if must_ban {
            match self
                .connectivity
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
            match self.peer_manager.unban_peer(&node_id).await {
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
    }

    pub async fn unban_all_peers(&self) -> Result<(), Error> {
        let query = PeerQuery::new().select_where(|p| p.is_banned());
        let peers = self.peer_manager.perform_query(query).await?;
        let num_peers = peers.len();
        for peer in peers {
            if let Err(err) = self.peer_manager.unban_peer(&peer.node_id).await {
                println!("Failed to unban peer: {}", err);
            }
        }
        println!("Unbanned {} peer(s) from node", num_peers);
        Ok(())
    }

    pub async fn list_banned_peers(&self) -> Result<(), Error> {
        let banned = fetch_banned_peers(&self.peer_manager).await?;
        if banned.is_empty() {
            println!("No peers banned from node.")
        } else {
            println!("Peers banned from node ({}):", banned.len());
            for peer in banned {
                println!("{}", peer);
            }
        }
        Ok(())
    }

    /// Function to process the list-connections command
    pub async fn list_connections(&mut self) -> Result<(), Error> {
        let conns = self.connectivity.get_active_connections().await?;
        if conns.is_empty() {
            println!("No active peer connections.");
        } else {
            println!();
            let num_connections = conns.len();
            let mut table = Table::new();
            table.set_titles(vec![
                "NodeId",
                "Public Key",
                "Address",
                "Direction",
                "Age",
                "Role",
                "User Agent",
                "Info",
            ]);
            for conn in conns {
                let peer = self
                    .peer_manager
                    .find_by_node_id(conn.peer_node_id())
                    .await
                    .expect("Unexpected peer database error")
                    .expect("Peer not found");

                let chain_height = peer
                    .get_metadata(1)
                    .and_then(|v| bincode::deserialize::<PeerMetadata>(v).ok())
                    .map(|metadata| format!("height: {}", metadata.metadata.height_of_longest_chain()));

                let ua = peer.user_agent;
                table.add_row(row![
                    peer.node_id,
                    peer.public_key,
                    conn.address(),
                    conn.direction(),
                    format_duration_basic(conn.age()),
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
                    format!(
                        "substreams: {}{}",
                        conn.substream_count(),
                        chain_height.map(|s| format!(", {}", s)).unwrap_or_default()
                    ),
                ]);
            }

            table.print_stdout();

            println!("{} active connection(s)", num_connections);
        }
        Ok(())
    }

    pub async fn reset_offline_peers(&self) -> Result<(), Error> {
        let num_updated = self
            .peer_manager
            .update_each(|mut peer| {
                if peer.is_offline() {
                    peer.set_offline(false);
                    Some(peer)
                } else {
                    None
                }
            })
            .await?;

        println!("{} peer(s) were unmarked as offline.", num_updated);
        Ok(())
    }

    pub async fn list_headers(&self, start: u64, end: Option<u64>) {
        let headers = match Self::get_chain_headers(&self.blockchain_db, start, end).await {
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
    }

    /// Function to process the get-headers command
    async fn get_chain_headers(
        blockchain_db: &AsyncBlockchainDb<LMDBDatabase>,
        start: u64,
        end: Option<u64>,
    ) -> Result<Vec<ChainHeader>, anyhow::Error> {
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

    pub async fn block_timing(&self, start: u64, end: Option<u64>) -> Result<(), Error> {
        let headers = Self::get_chain_headers(&self.blockchain_db, start, end).await?;
        if !headers.is_empty() {
            let headers = headers.into_iter().map(|ch| ch.into_header()).rev().collect::<Vec<_>>();
            let (max, min, avg) = BlockHeader::timing_stats(&headers);
            println!(
                "Timing for blocks #{} - #{}",
                headers.first().unwrap().height,
                headers.last().unwrap().height
            );
            println!("Max block time: {}", max);
            println!("Min block time: {}", min);
            println!("Avg block time: {}", avg);
        } else {
            println!("No headers found");
        }
        Ok(())
    }

    /// Function to process the check-db command
    pub async fn check_db(&mut self) -> Result<(), Error> {
        let meta = self.node_service.get_metadata().await?;
        let mut height = meta.height_of_longest_chain();
        let mut missing_blocks = Vec::new();
        let mut missing_headers = Vec::new();
        print!("Searching for height: ");
        // We need to check every header, but not every block.
        let horizon_height = meta.horizon_block(height);
        while height > 0 {
            print!("{}", height);
            io::stdout().flush()?;
            // we can only check till the pruning horizon, 0 is archive node so it needs to check every block.
            if height > horizon_height {
                match self.node_service.get_block(height).await {
                    Err(err) => {
                        // We need to check the data itself, as FetchMatchingBlocks will suppress any error, only
                        // logging it.
                        error!(target: LOG_TARGET, "{}", err);
                        missing_blocks.push(height);
                    },
                    Ok(Some(_)) => {},
                    Ok(None) => missing_blocks.push(height),
                };
            }
            height -= 1;
            let next_header = self.node_service.get_header(height).await.ok().flatten();
            if next_header.is_none() {
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
        Ok(())
    }

    #[allow(deprecated)]
    pub async fn period_stats(
        &mut self,
        period_end: u64,
        mut period_ticker_end: u64,
        period: u64,
    ) -> Result<(), Error> {
        let meta = self.node_service.get_metadata().await?;

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
            io::stdout().flush()?;

            let block = self
                .node_service
                .get_block(height)
                .await?
                .ok_or_else(|| anyhow!("Error in db, block not found at height {}", height))?;

            let prev_block = self
                .node_service
                .get_block(height - 1)
                .await?
                .ok_or_else(|| anyhow!("Error in db, block not found at height {}", height))?;

            height -= 1;
            if block.header().timestamp.as_u64() > period_ticker_end {
                print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
                continue;
            };
            while block.header().timestamp.as_u64() < period_ticker_start {
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
            let st = if prev_block.header().timestamp.as_u64() >= block.header().timestamp.as_u64() {
                1.0
            } else {
                (block.header().timestamp.as_u64() - prev_block.header().timestamp.as_u64()) as f64
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
        Ok(())
    }

    pub async fn save_header_stats(
        &self,
        start_height: u64,
        end_height: u64,
        filename: String,
        pow_algo: Option<PowAlgorithm>,
    ) -> Result<(), Error> {
        let mut output = File::create(&filename).await?;

        println!(
            "Loading header from height {} to {} and dumping to file [working-dir]/{}.{}",
            start_height,
            end_height,
            filename,
            pow_algo.map(|a| format!(" PoW algo = {}", a)).unwrap_or_default()
        );

        let start_height = cmp::max(start_height, 1);
        let mut prev_header = self.blockchain_db.fetch_chain_header(start_height - 1).await?;

        let mut buff = Vec::new();
        writeln!(
            buff,
            "Height,Achieved,TargetDifficulty,CalculatedDifficulty,SolveTime,NormalizedSolveTime,Algo,Timestamp,\
             Window,Acc.Monero,Acc.Sha3"
        )?;
        output.write_all(&buff).await?;

        for height in start_height..=end_height {
            let header = self.blockchain_db.fetch_chain_header(height).await?;

            // Optionally, filter out pow algos
            if pow_algo.map(|algo| header.header().pow_algo() != algo).unwrap_or(false) {
                continue;
            }

            let target_diff = self
                .blockchain_db
                .fetch_target_difficulties_for_next_block(prev_header.hash().clone())
                .await?;
            let pow_algo = header.header().pow_algo();

            let min = self
                .consensus_rules
                .consensus_constants(height)
                .min_pow_difficulty(pow_algo);
            let max = self
                .consensus_rules
                .consensus_constants(height)
                .max_pow_difficulty(pow_algo);

            let calculated_target_difficulty = target_diff.get(pow_algo).calculate(min, max);
            let existing_target_difficulty = header.accumulated_data().target_difficulty;
            let achieved = header.accumulated_data().achieved_difficulty;
            let solve_time = header.header().timestamp.as_u64() as i64 - prev_header.header().timestamp.as_u64() as i64;
            let normalized_solve_time = cmp::min(
                cmp::max(solve_time, 1) as u64,
                self.consensus_rules
                    .consensus_constants(height)
                    .get_difficulty_max_block_interval(pow_algo),
            );
            let acc_sha3 = header.accumulated_data().accumulated_sha_difficulty;
            let acc_monero = header.accumulated_data().accumulated_monero_difficulty;

            buff.clear();
            writeln!(
                buff,
                "{},{},{},{},{},{},{},{},{},{},{}",
                height,
                achieved.as_u64(),
                existing_target_difficulty.as_u64(),
                calculated_target_difficulty.as_u64(),
                solve_time,
                normalized_solve_time,
                pow_algo,
                chrono::DateTime::from(header.header().timestamp),
                target_diff.get(pow_algo).len(),
                acc_monero.as_u64(),
                acc_sha3.as_u64(),
            )?;
            output.write_all(&buff).await?;

            if header.header().hash() != header.accumulated_data().hash {
                eprintln!(
                    "Difference in hash at {}! header = {} and accum hash = {}",
                    height,
                    header.header().hash().to_hex(),
                    header.accumulated_data().hash.to_hex()
                );
            }

            if existing_target_difficulty != calculated_target_difficulty {
                eprintln!(
                    "Difference at {}! existing = {} and calculated = {}",
                    height, existing_target_difficulty, calculated_target_difficulty
                );
            }

            print!("{}", height);
            io::stdout().flush()?;
            print!("\x1B[{}D\x1B[K", (height + 1).to_string().chars().count());
            prev_header = header;
        }
        println!("Complete");
        Ok(())
    }

    pub async fn rewind_blockchain(&self, new_height: u64) -> Result<(), Error> {
        let blocks = self.blockchain_db.rewind_to_height(new_height).await?;
        if !blocks.is_empty() {
            self.node_service
                .publish_block_event(BlockEvent::BlockSyncRewind(blocks));
        }
        Ok(())
    }

    /// Function to process the whoami command
    pub fn whoami(&self) -> Result<(), Error> {
        println!("{}", self.base_node_identity);
        Ok(())
    }

    pub(crate) fn get_software_updater(&self) -> SoftwareUpdaterHandle {
        self.software_updater.clone()
    }

    pub async fn get_blockchain_db_stats(&self) -> Result<(), Error> {
        const BYTES_PER_MB: usize = 1024 * 1024;

        let stats = self.blockchain_db.get_stats().await?;
        let mut table = Table::new();
        table.set_titles(vec![
            "Name",
            "Entries",
            "Depth",
            "Branch Pages",
            "Leaf Pages",
            "Overflow Pages",
            "Est. Size (MiB)",
            "% of total",
        ]);
        let total_db_size = stats.db_stats().iter().map(|s| s.total_page_size()).sum::<usize>();
        stats.db_stats().iter().for_each(|stat| {
            table.add_row(row![
                stat.name,
                stat.entries,
                stat.depth,
                stat.branch_pages,
                stat.leaf_pages,
                stat.overflow_pages,
                format!("{:.2}", stat.total_page_size() as f32 / BYTES_PER_MB as f32),
                format!("{:.2}%", (stat.total_page_size() as f32 / total_db_size as f32) * 100.0)
            ]);
        });

        table.print_stdout();
        println!();
        println!(
            "{} databases, {:.2} MiB used ({:.2}%), page size: {} bytes, env_info = ({})",
            stats.root().entries,
            total_db_size as f32 / BYTES_PER_MB as f32,
            (total_db_size as f32 / stats.env_info().mapsize as f32) * 100.0,
            stats.root().psize as usize,
            stats.env_info()
        );

        println!();
        println!("Totalling DB entry sizes. This may take a few seconds...");
        println!();
        let stats = self.blockchain_db.fetch_total_size_stats().await?;
        println!();
        let mut table = Table::new();
        table.set_titles(vec![
            "Name",
            "Entries",
            "Total Size (MiB)",
            "Avg. Size/Entry (bytes)",
            "% of total",
        ]);
        let total_data_size = stats.sizes().iter().map(|s| s.total()).sum::<u64>();
        stats.sizes().iter().for_each(|size| {
            let total = size.total() as f32 / BYTES_PER_MB as f32;
            table.add_row(row![
                size.name,
                size.num_entries,
                format!("{:.2}", total),
                format!("{}", size.avg_bytes_per_entry()),
                format!("{:.2}%", (size.total() as f32 / total_data_size as f32) * 100.0)
            ])
        });
        table.print_stdout();
        println!();
        println!(
            "Total blockchain data size: {:.2} MiB ({:.2} % of LMDB map size)",
            total_data_size as f32 / BYTES_PER_MB as f32,
            (total_data_size as f32 / total_db_size as f32) * 100.0
        );
        Ok(())
    }

    #[cfg(not(feature = "metrics"))]
    pub fn get_network_stats(&self) -> Result<(), Error> {
        println!(
            "Metrics are not enabled in this binary. Recompile Tari base node with `--features metrics` to enable \
             them."
        );
        Ok(())
    }

    #[cfg(feature = "metrics")]
    pub fn get_network_stats(&self) -> Result<(), Error> {
        use tari_metrics::proto::MetricType;
        let metric_families = tari_metrics::get_default_registry().gather();
        let metric_family_iter = metric_families
            .into_iter()
            .filter(|family| family.get_name().starts_with("tari_comms"));

        // TODO: Make this useful
        let mut table = Table::new();
        table.set_titles(vec!["name", "type", "value"]);
        for family in metric_family_iter {
            let field_type = family.get_field_type();
            let name = family.get_name();
            for metric in family.get_metric() {
                let value = match field_type {
                    MetricType::COUNTER => metric.get_counter().get_value(),
                    MetricType::GAUGE => metric.get_gauge().get_value(),
                    MetricType::SUMMARY => {
                        let summary = metric.get_summary();
                        summary.get_sample_sum() / summary.get_sample_count() as f64
                    },
                    MetricType::UNTYPED => metric.get_untyped().get_value(),
                    MetricType::HISTOGRAM => {
                        let histogram = metric.get_histogram();
                        histogram.get_sample_sum() / histogram.get_sample_count() as f64
                    },
                };

                let field_type = match field_type {
                    MetricType::COUNTER => "COUNTER",
                    MetricType::GAUGE => "GAUGE",
                    MetricType::SUMMARY => "SUMMARY",
                    MetricType::UNTYPED => "UNTYPED",
                    MetricType::HISTOGRAM => "HISTOGRAM",
                };

                table.add_row(row![name, field_type, value]);
            }
        }
        table.print_stdout();
        Ok(())
    }

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

async fn fetch_banned_peers(pm: &PeerManager) -> Result<Vec<Peer>, PeerManagerError> {
    let query = PeerQuery::new().select_where(|p| p.is_banned());
    pm.perform_query(query).await
}

#[derive(Debug, Error)]
#[error("invalid format '{0}'")]
pub struct FormatParseError(String);

pub enum Format {
    Json,
    Text,
}

impl Default for Format {
    fn default() -> Self {
        Self::Text
    }
}

impl FromStr for Format {
    type Err = FormatParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_ref() {
            "json" => Ok(Self::Json),
            "text" => Ok(Self::Text),
            _ => Err(FormatParseError(s.into())),
        }
    }
}

// TODO: This is not currently used, but could be pretty useful (maybe as an iterator)
// Function to delimit arguments using spaces and pairs of quotation marks, which may include spaces
// pub fn delimit_command_string(command_str: &str) -> Vec<String> {
//     // Delimit arguments using spaces and pairs of quotation marks, which may include spaces
//     let arg_temp = command_str.trim().to_string();
//     let re = Regex::new(r#"[^\s"]+|"(?:\\"|[^"])+""#).unwrap();
//     let arg_temp_vec: Vec<&str> = re.find_iter(&arg_temp).map(|mat| mat.as_str()).collect();
//     // Remove quotation marks left behind by `Regex` - it does not support look ahead and look behind
//     let mut del_arg_vec = Vec::new();
//     for arg in arg_temp_vec.iter().skip(1) {
//         del_arg_vec.push(str::replace(arg, "\"", ""));
//     }
//     del_arg_vec
// }

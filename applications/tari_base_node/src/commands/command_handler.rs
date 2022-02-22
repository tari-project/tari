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
use strum::{Display, EnumString};
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
    base_node::{comms_interface::BlockEvent, state_machine_service::states::StatusInfo, LocalNodeCommsInterface},
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
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

use crate::{builder::BaseNodeContext, table::Table};

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
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

    pub(crate) fn get_software_updater(&self) -> SoftwareUpdaterHandle {
        self.software_updater.clone()
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

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
    time::Instant,
};

use anyhow::{anyhow, Error};
use log::*;
use strum::{Display, EnumString};
use tari_app_utilities::utilities::parse_emoji_id_or_public_key;
use tari_common::GlobalConfig;
use tari_common_types::{emoji::EmojiId, types::HashOutput};
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::PeerManager,
    protocol::rpc::RpcServerHandle,
    NodeIdentity,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester, MetricsCollectorHandle};
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface},
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    consensus::ConsensusManager,
    mempool::service::LocalMempoolService,
    proof_of_work::PowAlgorithm,
};
use tari_crypto::ristretto::RistrettoPublicKey;
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_utilities::{hex::Hex, message_format::MessageFormat, Hashable};
use thiserror::Error;
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

use crate::builder::BaseNodeContext;

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

    pub(crate) fn get_software_updater(&self) -> SoftwareUpdaterHandle {
        self.software_updater.clone()
    }
}

#[derive(Debug, Error)]
#[error("invalid format '{0}'")]
pub struct FormatParseError(String);

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum Format {
    Json,
    Text,
}

impl Default for Format {
    fn default() -> Self {
        Self::Text
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

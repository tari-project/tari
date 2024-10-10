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

mod add_peer;
mod ban_peer;
mod block_timing;
mod check_db;
mod check_for_updates;
mod create_tls_certs;
mod dial_peer;
mod discover_peer;
mod get_block;
mod get_chain_metadata;
mod get_db_stats;
mod get_mempool_state;
mod get_mempool_stats;
mod get_network_stats;
mod get_peer;
mod get_state_info;
mod header_stats;
mod list_banned_peers;
mod list_connections;
mod list_headers;
mod list_peers;
mod list_reorgs;
mod list_validator_nodes;
mod period_stats;
mod ping_peer;
mod quit;
mod rewind_blockchain;
mod search_kernel;
mod search_utxo;
mod status;
mod unban_all_peers;
mod version;
mod watch_command;
mod whoami;

use std::{
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use strum::{EnumVariantNames, VariantNames};
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface},
    blocks::ChainHeader,
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    consensus::ConsensusManager,
    mempool::service::LocalMempoolService,
};
use tari_network::{ NetworkHandle};
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_rpc_framework::RpcServerHandle;
use tari_shutdown::Shutdown;
use tokio::{sync::watch, time};
pub use watch_command::WatchCommand;

use crate::{
    builder::BaseNodeContext,
    commands::{nom_parser::ParsedCommand, parser::FromHex},
    ApplicationConfig,
};

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, EnumVariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    Version(version::Args),
    CheckForUpdates(check_for_updates::Args),
    Status(status::Args),
    GetChainMetadata(get_chain_metadata::Args),
    GetDbStats(get_db_stats::Args),
    GetPeer(get_peer::Args),
    ListPeers(list_peers::Args),
    DialPeer(dial_peer::Args),
    PingPeer(ping_peer::Args),
    RewindBlockchain(rewind_blockchain::Args),
    AddPeer(add_peer::ArgsAddPeer),
    BanPeer(ban_peer::ArgsBan),
    UnbanPeer(ban_peer::ArgsUnban),
    UnbanAllPeers(unban_all_peers::Args),
    ListBannedPeers(list_banned_peers::Args),
    ListConnections(list_connections::Args),
    ListHeaders(list_headers::Args),
    CheckDb(check_db::Args),
    PeriodStats(period_stats::Args),
    HeaderStats(header_stats::Args),
    BlockTiming(block_timing::Args),
    ListReorgs(list_reorgs::Args),
    DiscoverPeer(discover_peer::Args),
    GetBlock(get_block::Args),
    SearchUtxo(search_utxo::Args),
    SearchKernel(search_kernel::Args),
    GetMempoolStats(get_mempool_stats::Args),
    GetMempoolState(get_mempool_state::Args),
    GetMempoolTx(get_mempool_state::ArgsTx),
    Whoami(whoami::Args),
    GetStateInfo(get_state_info::Args),
    GetNetworkStats(get_network_stats::Args),
    ListValidatorNodes(list_validator_nodes::Args),
    CreateTlsCerts(create_tls_certs::Args),
    Quit(quit::Args),
    Exit(quit::Args),
    Watch(watch_command::Args),
}

impl Command {
    pub fn variants() -> Vec<String> {
        Command::VARIANTS.iter().map(|s| s.to_string()).collect()
    }
}

#[async_trait]
pub trait HandleCommand<T> {
    async fn handle_command(&mut self, args: T) -> Result<(), Error>;
}

pub struct CommandContext {
    pub config: Arc<ApplicationConfig>,
    consensus_rules: ConsensusManager,
    blockchain_db: AsyncBlockchainDb<LMDBDatabase>,
    rpc_server: RpcServerHandle,
    network: NetworkHandle,
    liveness: LivenessHandle,
    node_service: LocalNodeCommsInterface,
    mempool_service: LocalMempoolService,
    state_machine_info: watch::Receiver<StatusInfo>,
    pub software_updater: SoftwareUpdaterHandle,
    last_time_full: Instant,
    pub shutdown: Shutdown,
}

impl CommandContext {
    pub fn new(ctx: &BaseNodeContext, shutdown: Shutdown) -> Self {
        Self {
            config: ctx.config(),
            consensus_rules: ctx.consensus_rules().clone(),
            blockchain_db: ctx.blockchain_db().into(),
            rpc_server: ctx.rpc_server(),
            network: ctx.network_handle().clone(),
            liveness: ctx.liveness(),
            node_service: ctx.local_node(),
            mempool_service: ctx.local_mempool(),
            state_machine_info: ctx.get_state_machine_info_channel(),
            software_updater: ctx.software_updater(),
            last_time_full: Instant::now(),
            shutdown,
        }
    }

    pub async fn handle_command_str(&mut self, line: &str) -> Result<Option<WatchCommand>, Error> {
        let args: Args = line.parse()?;
        if let Command::Watch(command) = args.command {
            Ok(Some(command))
        } else {
            let time_out = match args.command {
                // These commands should complete quickly, some of them like 'discover-peer' returns immediately
                // although the requested action can take a long time
                Command::Version(_) |
                Command::Whoami(_) |
                Command::CheckForUpdates(_) |
                Command::AddPeer(_) |
                Command::BanPeer(_) |
                Command::UnbanAllPeers(_) |
                Command::UnbanPeer(_) |
                Command::GetPeer(_) |
                Command::DialPeer(_) |
                Command::PingPeer(_) |
                Command::DiscoverPeer(_) |
                Command::ListPeers(_) |
                Command::ListBannedPeers(_) |
                Command::ListConnections(_) |
                Command::GetNetworkStats(_) |
                Command::BlockTiming(_) |
                Command::GetChainMetadata(_) |
                Command::GetDbStats(_) |
                Command::GetStateInfo(_) |
                Command::ListReorgs(_) |
                Command::GetBlock(_) |
                Command::ListHeaders(_) |
                Command::HeaderStats(_) |
                Command::SearchUtxo(_) |
                Command::SearchKernel(_) |
                Command::GetMempoolStats(_) |
                Command::GetMempoolState(_) |
                Command::GetMempoolTx(_) |
                Command::Status(_) |
                Command::Watch(_) |
                Command::ListValidatorNodes(_) |
                Command::CreateTlsCerts(_) |
                Command::Quit(_) |
                Command::Exit(_) => 30,
                // These commands involve intense blockchain db operations and needs a lot of time to complete
                Command::CheckDb(_) | Command::PeriodStats(_) | Command::RewindBlockchain(_) => 600,
            };
            let fut = self.handle_command(args.command);
            if let Err(e) = time::timeout(Duration::from_secs(time_out), fut).await? {
                return Err(Error::msg(format!("{} ({} s)", e, time_out)));
            };
            Ok(None)
        }
    }
}

impl FromStr for Args {
    type Err = Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let args = ParsedCommand::parse(line)?;
        let matches = Args::command().no_binary_name(true).try_get_matches_from(args)?;
        let command = Args::from_arg_matches(&matches)?;
        Ok(command)
    }
}

#[async_trait]
impl HandleCommand<Command> for CommandContext {
    async fn handle_command(&mut self, command: Command) -> Result<(), Error> {
        match command {
            Command::Version(args) => self.handle_command(args).await,
            Command::CheckForUpdates(args) => self.handle_command(args).await,
            Command::Status(args) => self.handle_command(args).await,
            Command::GetChainMetadata(args) => self.handle_command(args).await,
            Command::GetDbStats(args) => self.handle_command(args).await,
            Command::GetPeer(args) => self.handle_command(args).await,
            Command::GetStateInfo(args) => self.handle_command(args).await,
            Command::GetNetworkStats(args) => self.handle_command(args).await,
            Command::ListPeers(args) => self.handle_command(args).await,
            Command::DialPeer(args) => self.handle_command(args).await,
            Command::PingPeer(args) => self.handle_command(args).await,
            Command::AddPeer(args) => self.handle_command(args).await,
            Command::BanPeer(args) => self.handle_command(args).await,
            Command::UnbanPeer(args) => self.handle_command(args).await,
            Command::RewindBlockchain(args) => self.handle_command(args).await,
            Command::UnbanAllPeers(args) => self.handle_command(args).await,
            Command::ListHeaders(args) => self.handle_command(args).await,
            Command::CheckDb(args) => self.handle_command(args).await,
            Command::PeriodStats(args) => self.handle_command(args).await,
            Command::HeaderStats(args) => self.handle_command(args).await,
            Command::BlockTiming(args) => self.handle_command(args).await,
            Command::ListReorgs(args) => self.handle_command(args).await,
            Command::DiscoverPeer(args) => self.handle_command(args).await,
            Command::GetBlock(args) => self.handle_command(args).await,
            Command::SearchUtxo(args) => self.handle_command(args).await,
            Command::SearchKernel(args) => self.handle_command(args).await,
            Command::ListConnections(args) => self.handle_command(args).await,
            Command::GetMempoolStats(args) => self.handle_command(args).await,
            Command::GetMempoolState(args) => self.handle_command(args).await,
            Command::GetMempoolTx(args) => self.handle_command(args).await,
            Command::Whoami(args) => self.handle_command(args).await,
            Command::ListBannedPeers(args) => self.handle_command(args).await,
            Command::Quit(args) | Command::Exit(args) => self.handle_command(args).await,
            Command::Watch(args) => self.handle_command(args).await,
            Command::ListValidatorNodes(args) => self.handle_command(args).await,
            Command::CreateTlsCerts(args) => self.handle_command(args).await,
        }
    }
}

impl CommandContext {
    async fn fetch_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let pm = self.network.peer_manager();
        let query = PeerQuery::new().select_where(|p| p.is_banned());
        pm.perform_query(query).await
    }

    /// Function to process the get-headers command
    async fn get_chain_headers(&self, start: u64, end: Option<u64>) -> Result<Vec<ChainHeader>, Error> {
        let blockchain_db = &self.blockchain_db;
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
}

#[derive(Debug)]
pub enum TypeOrHex<T> {
    Type(T),
    Hex(FromHex<Vec<u8>>),
}

impl<T> FromStr for TypeOrHex<T>
where T: FromStr
{
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(t) = T::from_str(s) {
            Ok(Self::Type(t))
        } else {
            FromHex::from_str(s).map(Self::Hex).map_err(|_| {
                anyhow!(
                    "Argument was not a valid string for {} or hex value",
                    std::any::type_name::<T>()
                )
            })
        }
    }
}

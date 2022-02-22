mod ban_peer;
mod block_timing;
mod check_db;
mod check_for_updates;
mod dial_peer;
mod discover_peer;
mod get_block;
mod get_chain_metadata;
mod get_db_stats;
mod get_mempool_state;
mod get_mempool_stats;
mod get_network_stats;
mod get_state_info;
mod list_banned_peers;
mod list_connections;
mod list_headers;
mod list_peers;
mod list_reorgs;
mod ping_peer;
mod reset_offline_peers;
mod rewind_blockchain;
mod search_kernel;
mod search_utxo;
mod status;
mod unban_all_peers;
mod version;
mod whoami;

use std::{sync::Arc, time::Instant};

use anyhow::Error;
use async_trait::async_trait;
use clap::{AppSettings, Parser, Subcommand};
use tari_common::GlobalConfig;
use tari_comms::{
    connectivity::ConnectivityRequester,
    peer_manager::{Peer, PeerManager, PeerManagerError, PeerQuery},
    protocol::rpc::RpcServerHandle,
    NodeIdentity,
};
use tari_comms_dht::{DhtDiscoveryRequester, MetricsCollectorHandle};
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface},
    blocks::ChainHeader,
    chain_storage::{async_db::AsyncBlockchainDb, LMDBDatabase},
    consensus::ConsensusManager,
    mempool::service::LocalMempoolService,
};
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tokio::sync::watch;

use crate::builder::BaseNodeContext;

#[derive(Debug, Parser)]
#[clap(setting = AppSettings::NoBinaryName)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Version(version::Args),
    CheckForUpdates(check_for_updates::Args),
    Status(status::Args),
    GetChainMetadata(get_chain_metadata::Args),
    GetDbStats(get_db_stats::Args),
    // GetPeer,
    ListPeers(list_peers::Args),
    DialPeer(dial_peer::Args),
    PingPeer(ping_peer::Args),
    ResetOfflinePeers(reset_offline_peers::Args),
    RewindBlockchain(rewind_blockchain::Args),
    BanPeer(ban_peer::ArgsBan),
    UnbanPeer(ban_peer::ArgsUnban),
    UnbanAllPeers(unban_all_peers::Args),
    ListBannedPeers(list_banned_peers::Args),
    ListConnections(list_connections::Args),
    ListHeaders(list_headers::Args),
    CheckDb(check_db::Args),
    // PeriodStats,
    // HeaderStats,
    BlockTiming(block_timing::Args),
    CalcTiming(block_timing::Args),
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
    // GetStateInfo,
    GetNetworkStats(get_network_stats::Args),
    /* Quit,
     * Exit, */
}

#[async_trait]
pub trait HandleCommand<T> {
    async fn handle_command(&mut self, args: T) -> Result<(), Error>;
}

pub struct CommandContext {
    pub config: Arc<GlobalConfig>,
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

impl CommandContext {
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
            Command::GetStateInfo(args) => self.handle_command(args).await,
            Command::GetNetworkStats(args) => self.handle_command(args).await,
            Command::ListPeers(args) => self.handle_command(args).await,
            Command::DialPeer(args) => self.handle_command(args).await,
            Command::PingPeer(args) => self.handle_command(args).await,
            Command::BanPeer(args) => self.handle_command(args).await,
            Command::UnbanPeer(args) => self.handle_command(args).await,
            Command::ResetOfflinePeers(args) => self.handle_command(args).await,
            Command::RewindBlockchain(args) => self.handle_command(args).await,
            Command::UnbanAllPeers(args) => self.handle_command(args).await,
            Command::ListHeaders(args) => self.handle_command(args).await,
            Command::CheckDb(args) => self.handle_command(args).await,
            Command::BlockTiming(args) | Command::CalcTiming(args) => self.handle_command(args).await,
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
        }
    }
}

impl CommandContext {
    async fn fetch_banned_peers(&self) -> Result<Vec<Peer>, PeerManagerError> {
        let pm = &self.peer_manager;
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

mod ban_peer;
mod check_for_updates;
mod dial_peer;
mod get_chain_metadata;
mod get_db_stats;
mod get_state_info;
mod list_banned_peers;
mod list_connections;
mod list_peers;
mod ping_peer;
mod status;
mod unban_all_peers;
mod version;

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
    // ResetOfflinePeers,
    // RewindBlockchain,
    BanPeer(ban_peer::ArgsBan),
    UnbanPeer(ban_peer::ArgsUnban),
    UnbanAllPeers(unban_all_peers::Args),
    ListBannedPeers(list_banned_peers::Args),
    ListConnections(list_connections::Args),
    // ListHeaders,
    // CheckDb,
    // PeriodStats,
    // HeaderStats,
    // BlockTiming,
    // CalcTiming,
    // ListReorgs,
    // DiscoverPeer,
    // GetBlock,
    // SearchUtxo,
    // SearchKernel,
    // GetMempoolStats,
    // GetMempoolState,
    // GetMempoolTx,
    // Whoami,
    GetStateInfo(get_state_info::Args),
    /* GetStateInfo,
     * GetNetworkStats,
     * Quit,
     * Exit,
     */
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
            Command::ListPeers(args) => self.handle_command(args).await,
            Command::DialPeer(args) => self.handle_command(args).await,
            Command::PingPeer(args) => self.handle_command(args).await,
            Command::BanPeer(args) => self.handle_command(args).await,
            Command::UnbanPeer(args) => self.handle_command(args).await,
            Command::UnbanAllPeers(args) => self.handle_command(args).await,
            Command::ListConnections(args) => self.handle_command(args).await,
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
}

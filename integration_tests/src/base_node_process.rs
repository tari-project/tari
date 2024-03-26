//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    default::Default,
    fmt::{Debug, Formatter},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use minotari_app_utilities::identity_management::save_as_json;
use minotari_node::{config::GrpcMethod, run_base_node, BaseNodeConfig, MetricsConfig};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use rand::rngs::OsRng;
use tari_common::{
    configuration::{CommonConfig, MultiaddrList},
    network_check::set_network_if_choice_valid,
};
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_p2p::{auto_update::AutoUpdateConfig, Network, PeerSeedsConfig, TransportType};
use tari_shutdown::Shutdown;
use tokio::task;
use tonic::transport::Channel;

use crate::{get_peer_addresses, get_port, wait_for_service, TariWorld};

#[derive(Clone)]
pub struct BaseNodeProcess {
    pub name: String,
    pub port: u64,
    pub grpc_port: u64,
    pub identity: NodeIdentity,
    pub temp_dir_path: PathBuf,
    pub is_seed_node: bool,
    pub seed_nodes: Vec<String>,
    pub config: BaseNodeConfig,
    pub kill_signal: Shutdown,
}

impl Drop for BaseNodeProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

// NOTE: implemented to skip `cx`, because BaseNodeContext doesn't implement Debug
impl Debug for BaseNodeProcess {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BaseNodeProcess")
            .field("name", &self.name)
            .field("port", &self.port)
            .field("grpc_port", &self.grpc_port)
            .field("identity", &self.identity)
            .field("temp_dir_path", &self.temp_dir_path)
            .field("is_seed_node", &self.is_seed_node)
            .finish()
    }
}

pub async fn spawn_base_node(world: &mut TariWorld, is_seed_node: bool, bn_name: String, peers: Vec<String>) {
    spawn_base_node_with_config(world, is_seed_node, bn_name, peers, BaseNodeConfig::default()).await;
}

#[allow(clippy::too_many_lines)]
pub async fn spawn_base_node_with_config(
    world: &mut TariWorld,
    is_seed_node: bool,
    bn_name: String,
    peers: Vec<String>,
    mut base_node_config: BaseNodeConfig,
) {
    std::env::set_var("TARI_NETWORK", "localnet");
    set_network_if_choice_valid(Network::LocalNet).unwrap();

    let port: u64;
    let grpc_port: u64;
    let temp_dir_path: PathBuf;
    let base_node_identity: NodeIdentity;

    if let Some(node_ps) = world.base_nodes.get(&bn_name) {
        port = node_ps.port;
        grpc_port = node_ps.grpc_port;
        temp_dir_path = node_ps.temp_dir_path.clone();
        base_node_config = node_ps.config.clone();

        base_node_identity = node_ps.identity.clone();
    } else {
        // each spawned wallet will use different ports
        port = get_port(18000..18499).unwrap();
        grpc_port = get_port(18500..18999).unwrap();
        // create a new temporary directory
        // temp_dir_path = tempdir().unwrap().path().to_path_buf();
        temp_dir_path = world
            .current_base_dir
            .as_ref()
            .expect("Base dir on world")
            .join("base_nodes")
            .join(format!("{}_grpc_port_{}", bn_name.clone(), grpc_port));

        let base_node_address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
        base_node_identity = NodeIdentity::random(&mut OsRng, base_node_address, PeerFeatures::COMMUNICATION_NODE);
        save_as_json(temp_dir_path.join("base_node.json"), &base_node_identity).unwrap();
    };

    println!("Base node identity: {}", base_node_identity);
    let identity = base_node_identity.clone();

    let shutdown = Shutdown::new();
    let process = BaseNodeProcess {
        name: bn_name.clone(),
        port,
        grpc_port,
        identity,
        temp_dir_path: temp_dir_path.clone(),
        is_seed_node,
        seed_nodes: peers.clone(),
        config: base_node_config.clone(),
        kill_signal: shutdown.clone(),
    };

    let name_cloned = bn_name.clone();

    let peer_addresses = get_peer_addresses(world, &peers).await;

    let mut common_config = CommonConfig::default();
    common_config.base_path = temp_dir_path.clone();
    task::spawn(async move {
        let mut base_node_config = minotari_node::ApplicationConfig {
            common: common_config,
            auto_update: AutoUpdateConfig::default(),
            base_node: base_node_config,
            metrics: MetricsConfig::default(),
            peer_seeds: PeerSeedsConfig {
                peer_seeds: peer_addresses.into(),
                dns_seeds_use_dnssec: false,
                ..Default::default()
            },
        };

        println!("Using base_node temp_dir: {}", temp_dir_path.clone().display());
        base_node_config.base_node.network = Network::LocalNet;
        base_node_config.base_node.grpc_enabled = true;
        base_node_config.base_node.grpc_address = Some(format!("/ip4/127.0.0.1/tcp/{}", grpc_port).parse().unwrap());
        base_node_config.base_node.report_grpc_error = true;
        base_node_config.base_node.metadata_auto_ping_interval = Duration::from_secs(15);

        base_node_config.base_node.data_dir = temp_dir_path.to_path_buf();
        base_node_config.base_node.identity_file = PathBuf::from("base_node_id.json");
        base_node_config.base_node.tor_identity_file = PathBuf::from("base_node_tor_id.json");
        base_node_config.base_node.max_randomx_vms = 1;

        base_node_config.base_node.lmdb_path = temp_dir_path.to_path_buf();
        base_node_config.base_node.p2p.transport.transport_type = TransportType::Tcp;
        base_node_config.base_node.p2p.transport.tcp.listener_address =
            format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap();
        base_node_config.base_node.p2p.public_addresses = MultiaddrList::from(vec![base_node_config
            .base_node
            .p2p
            .transport
            .tcp
            .listener_address
            .clone()]);
        base_node_config.base_node.p2p.dht = DhtConfig::default_local_test();
        base_node_config.base_node.p2p.dht.database_url = DbConnectionUrl::file(format!("{}-dht.sqlite", port));
        base_node_config.base_node.p2p.dht.network_discovery.enabled = true;
        base_node_config.base_node.p2p.allow_test_addresses = true;
        base_node_config.base_node.storage.orphan_storage_capacity = 10;
        if base_node_config.base_node.storage.pruning_horizon != 0 {
            base_node_config.base_node.storage.pruning_interval = 1;
        };
        base_node_config.base_node.grpc_server_allow_methods = vec![
            GrpcMethod::ListHeaders,
            GrpcMethod::GetHeaderByHash,
            GrpcMethod::GetBlocks,
            GrpcMethod::GetBlockTiming,
            GrpcMethod::GetConstants,
            GrpcMethod::GetBlockSize,
            GrpcMethod::GetBlockFees,
            GrpcMethod::GetVersion,
            GrpcMethod::CheckForUpdates,
            GrpcMethod::GetTokensInCirculation,
            GrpcMethod::GetNetworkDifficulty,
            GrpcMethod::GetNewBlockTemplate,
            GrpcMethod::GetNewBlock,
            GrpcMethod::GetNewBlockBlob,
            GrpcMethod::SubmitBlock,
            GrpcMethod::SubmitBlockBlob,
            GrpcMethod::SubmitTransaction,
            GrpcMethod::GetSyncInfo,
            GrpcMethod::GetSyncProgress,
            GrpcMethod::GetTipInfo,
            GrpcMethod::SearchKernels,
            GrpcMethod::SearchUtxos,
            GrpcMethod::FetchMatchingUtxos,
            GrpcMethod::GetPeers,
            GrpcMethod::GetMempoolTransactions,
            GrpcMethod::TransactionState,
            GrpcMethod::Identify,
            GrpcMethod::GetNetworkStatus,
            GrpcMethod::ListConnectedPeers,
            GrpcMethod::GetMempoolStats,
            GrpcMethod::GetActiveValidatorNodes,
            GrpcMethod::GetShardKey,
            GrpcMethod::GetTemplateRegistrations,
            GrpcMethod::GetSideChainUtxos,
        ];

        // Heirachically set the base path for all configs
        base_node_config.base_node.set_base_path(temp_dir_path.clone());

        println!(
            "Initializing base node: name={}; port={}; grpc_port={}; is_seed_node={}",
            name_cloned, port, grpc_port, is_seed_node
        );

        let result = run_base_node(shutdown, Arc::new(base_node_identity), Arc::new(base_node_config)).await;
        if let Err(e) = result {
            panic!("{:?}", e);
        }
    });

    // make the new base node able to be referenced by other processes
    world.base_nodes.insert(bn_name.clone(), process);
    if is_seed_node {
        world.seed_nodes.push(bn_name);
    }

    wait_for_service(port).await;
    wait_for_service(grpc_port).await;
}

impl BaseNodeProcess {
    pub async fn get_grpc_client(&self) -> anyhow::Result<BaseNodeGrpcClient<Channel>> {
        Ok(BaseNodeGrpcClient::connect(format!("http://127.0.0.1:{}", self.grpc_port)).await?)
    }

    pub fn kill(&mut self) {
        self.kill_signal.trigger();
    }
}

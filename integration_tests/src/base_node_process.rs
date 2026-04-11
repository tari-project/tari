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
    fmt::{Debug, Formatter},
    net::TcpListener,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use minotari_app_utilities::identity_management::save_as_json;
use minotari_node::{BaseNodeConfig, GrpcMethod, MetricsConfig, run_base_node};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use rand::rngs::OsRng;
use tari_common::{
    MAX_GRPC_MESSAGE_SIZE,
    configuration::{CommonConfig, MultiaddrList},
    network_check::set_network_if_choice_valid,
};
use tari_common_sqlite::connection::DbConnectionUrl;
use tari_comms::{NodeIdentity, multiaddr::Multiaddr, peer_manager::PeerFeatures};
use tari_comms_dht::DhtConfig;
use tari_p2p::{Network, PeerSeedsConfig, TransportType, auto_update::AutoUpdateConfig};
use tari_shutdown::Shutdown;
use tokio::task;
use tonic::transport::Channel;

use crate::{TariWorld, get_peer_addresses, wait_for_service};

#[derive(Clone)]
pub struct BaseNodeProcess {
    pub name: String,
    pub port: u16,
    pub grpc_port: u16,
    pub http_port: u16,
    pub xmrig_proxy_port: u16,
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
    set_network_if_choice_valid(Network::LocalNet).unwrap();

    let port: u16;
    let grpc_port: u16;
    let http_port: u16;
    let xmrig_proxy_port: u16;
    let temp_dir_path: PathBuf;
    let base_node_identity: NodeIdentity;

    if let Some(node_ps) = world.base_nodes.get(&bn_name) {
        port = node_ps.port;
        grpc_port = node_ps.grpc_port;
        http_port = node_ps.http_port;
        xmrig_proxy_port = node_ps.xmrig_proxy_port;
        temp_dir_path = node_ps.temp_dir_path.clone();
        base_node_config = node_ps.config.clone();

        base_node_identity = node_ps.identity.clone();
    } else {
        // Allocate ports from the global pool (pre-scanned at startup, much faster than
        // random scanning per node)
        let ports = crate::port_pool::global_port_pool()
            .allocate_base_node_ports()
            .expect("Port pool exhausted — too many concurrent base nodes");
        port = ports.p2p;
        grpc_port = ports.grpc;
        http_port = ports.http;
        xmrig_proxy_port = ports.xmrig_proxy;
        // Track in world for backwards compatibility
        world.assigned_ports.insert(port, port);
        world.assigned_ports.insert(grpc_port, grpc_port);
        world.assigned_ports.insert(http_port, http_port);
        // create a new temporary directory
        temp_dir_path = world
            .current_base_dir
            .as_ref()
            .expect("Base dir on world")
            .join("base_nodes")
            .join(format!("{}_grpc_port_{}", bn_name.clone(), grpc_port));

        let base_node_address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{port}")).unwrap();
        base_node_identity = NodeIdentity::random(&mut OsRng, base_node_address, PeerFeatures::COMMUNICATION_NODE);
        save_as_json(temp_dir_path.join("base_node.json"), &base_node_identity).unwrap();
    };

    println!("Base node identity: {base_node_identity}");
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
        http_port,
        xmrig_proxy_port,
    };

    let name_cloned = bn_name.clone();
    let world_default_payment_address = world.default_payment_address.to_base58();

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
        base_node_config.base_node.grpc_address = Some(format!("/ip4/127.0.0.1/tcp/{grpc_port}").parse().unwrap());
        base_node_config.base_node.report_grpc_error = true;
        base_node_config.base_node.metadata_auto_ping_interval = Duration::from_secs(3);
        base_node_config.base_node.http_wallet_query_service.port = http_port;
        base_node_config.base_node.http_wallet_query_service.listen_ip = Some("127.0.0.1".to_string().parse().unwrap());
        base_node_config.base_node.http_wallet_query_service.external_address =
            Some(format!("http://127.0.0.1:{http_port}").parse().unwrap());

        // Enable the built-in XMRig proxy for RandomXT mining (used by merge mining tests)
        base_node_config.base_node.xmrig_proxy_enabled = true;
        base_node_config.base_node.xmrig_proxy_address =
            format!("/ip4/127.0.0.1/tcp/{xmrig_proxy_port}").parse().unwrap();
        base_node_config.base_node.xmrig_proxy_wallet_payment_address =
            world_default_payment_address.clone();

        base_node_config.base_node.data_dir = temp_dir_path.to_path_buf();
        base_node_config.base_node.identity_file = PathBuf::from("base_node_id.json");
        base_node_config.base_node.tor_identity_file = PathBuf::from("base_node_tor_id.json");
        base_node_config.base_node.max_randomx_vms = 1;

        base_node_config.base_node.lmdb_path = temp_dir_path.to_path_buf();
        base_node_config.base_node.p2p.transport.transport_type = TransportType::Tcp;
        base_node_config.base_node.p2p.transport.tcp.listener_address =
            format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap();
        base_node_config.base_node.p2p.public_addresses = MultiaddrList::from(vec![
            base_node_config.base_node.p2p.transport.tcp.listener_address.clone(),
        ]);
        base_node_config.base_node.p2p.allow_test_addresses = true;
        base_node_config.base_node.p2p.dht = DhtConfig::default_local_test();
        base_node_config.base_node.p2p.dht.database_url = DbConnectionUrl::file(format!("{port}-dht.sqlite"));
        base_node_config.base_node.p2p.dht.network_discovery.enabled = true;
        base_node_config
            .base_node
            .p2p
            .dht
            .network_discovery
            .max_seed_peer_sync_count = 1;
        base_node_config
            .base_node
            .p2p
            .dht
            .network_discovery
            .seed_peer_min_initial_sync_peers_needed = 1;
        base_node_config
            .base_node
            .p2p
            .dht
            .network_discovery
            .min_successful_seed_contacts_for_early_exit = 1;
        base_node_config.base_node.p2p.dht.network_discovery.bootstrap_timeout = Duration::from_secs(5);
        base_node_config.base_node.p2p.dht.connectivity.update_interval = Duration::from_secs(2);
        base_node_config
            .base_node
            .p2p
            .dht
            .connectivity
            .random_pool_refresh_interval = Duration::from_secs(2);
        base_node_config.base_node.storage.orphan_storage_capacity = 10;
        if base_node_config.base_node.storage.pruning_horizon != 0 {
            base_node_config.base_node.storage.pruning_interval = 1;
        };
        base_node_config.base_node.grpc_server_allow_methods = GrpcMethod::ALL_VARIANTS.to_vec().into();

        // Hierarchically set the base path for all configs
        base_node_config.base_node.set_base_path(temp_dir_path.clone());

        base_node_config
            .base_node
            .state_machine
            .blocks_behind_before_considered_lagging = 1;
        base_node_config.base_node.state_machine.time_before_considered_lagging = Duration::from_secs(3);
        base_node_config.base_node.state_machine.initial_sync_peer_count = 1;

        // Tune blockchain sync for faster test execution
        let sync_cfg = &mut base_node_config.base_node.state_machine.blockchain_sync_config;
        sync_cfg.num_initial_sync_rounds_seed_bootstrap = 1;
        sync_cfg.initial_max_sync_latency = Duration::from_secs(60); // prod: 240s — tests use local TCP, not tor
        sync_cfg.rpc_deadline = Duration::from_secs(60); // prod: 240s
        sync_cfg.short_ban_period = Duration::from_secs(30); // prod: 240s — faster retry after transient failures

        println!(
            "Initializing base node: name={name_cloned}; port={port}; grpc_port={grpc_port}; \
             is_seed_node={is_seed_node}, http_port={http_port}, xmrig_proxy_port={xmrig_proxy_port}"
        );

        let result = run_base_node(shutdown, Arc::new(base_node_identity), Arc::new(base_node_config)).await;
        if let Err(e) = result {
            panic!("{e:?}");
        }
    });

    // make the new base node able to be referenced by other processes
    world.base_nodes.insert(bn_name.clone(), process);
    if is_seed_node {
        world.seed_nodes.push(bn_name);
    }

    wait_for_service(http_port).await;
    wait_for_service(port).await;
    wait_for_service(grpc_port).await;
    wait_for_service(xmrig_proxy_port).await;
}

impl BaseNodeProcess {
    pub async fn get_grpc_client(&self) -> anyhow::Result<BaseNodeGrpcClient<Channel>> {
        let endpoint = tonic::transport::Endpoint::from_shared(format!("http://127.0.0.1:{}", self.grpc_port))?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30));
        Ok(BaseNodeGrpcClient::new(endpoint.connect().await?)
            .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE))
    }

    pub fn kill(&mut self) {
        self.kill_signal.trigger();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        for (label, port) in [
            ("p2p", self.port),
            ("grpc", self.grpc_port),
            ("http", self.http_port),
            ("xmrig_proxy", self.xmrig_proxy_port),
        ] {
            while std::time::Instant::now() < deadline {
                if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if TcpListener::bind(("127.0.0.1", port)).is_err() {
                eprintln!(
                    "WARNING: base node '{}' {} port {} was not released within 30s timeout",
                    self.name, label, port
                );
            }
        }
    }
}

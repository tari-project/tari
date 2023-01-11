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
};

use rand::rngs::OsRng;
use tari_base_node::{run_base_node, BaseNodeConfig, MetricsConfig};
use tari_base_node_grpc_client::BaseNodeGrpcClient;
use tari_common::configuration::CommonConfig;
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_p2p::{auto_update::AutoUpdateConfig, Network, PeerSeedsConfig, TransportType};
use tari_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::task;
use tonic::transport::Channel;

use crate::{
    utils::{get_port, wait_for_service},
    TariWorld,
};

pub struct BaseNodeProcess {
    pub name: String,
    pub port: u64,
    pub grpc_port: u64,
    pub identity: NodeIdentity,
    pub temp_dir_path: PathBuf,
    pub is_seed_node: bool,
    pub seed_nodes: Vec<String>,
    pub pruning_horizon: u64,
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

pub async fn spawn_base_node(
    world: &mut TariWorld,
    is_seed_node: bool,
    bn_name: String,
    peers: Vec<String>,
    pruning_horizon: Option<u64>,
) {
    let mut config = BaseNodeConfig::default();
    config.storage.pruning_horizon = pruning_horizon.unwrap_or_default();

    spawn_base_node_with_config(world, is_seed_node, bn_name, peers, config).await;
}

pub async fn spawn_base_node_with_config(
    world: &mut TariWorld,
    is_seed_node: bool,
    bn_name: String,
    peers: Vec<String>,
    base_node_config: BaseNodeConfig,
) {
    let port: u64;
    let grpc_port: u64;
    let temp_dir_path: PathBuf;

    if let Some(node_ps) = world.base_nodes.get(&bn_name) {
        port = node_ps.port;
        grpc_port = node_ps.grpc_port;
        temp_dir_path = node_ps.temp_dir_path.clone()
    } else {
        // each spawned wallet will use different ports
        port = get_port(18000..18499).unwrap();
        grpc_port = get_port(18500..18999).unwrap();
        // create a new temporary directory
        temp_dir_path = tempdir().unwrap().path().to_path_buf()
    };

    let base_node_address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
    let base_node_identity = NodeIdentity::random(&mut OsRng, base_node_address, PeerFeatures::COMMUNICATION_NODE);
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
        pruning_horizon: base_node_config.storage.pruning_horizon,
        kill_signal: shutdown.clone(),
    };

    let name_cloned = bn_name.clone();

    let mut peer_addresses = vec![];
    for peer in &peers {
        let peer = world.base_nodes.get(peer.as_str()).unwrap();
        peer_addresses.push(format!(
            "{}::{}",
            peer.identity.public_key(),
            peer.identity.public_address()
        ));
    }

    let mut common_config = CommonConfig::default();
    common_config.base_path = temp_dir_path.clone();
    task::spawn(async move {
        let mut base_node_config = tari_base_node::ApplicationConfig {
            common: common_config,
            auto_update: AutoUpdateConfig::default(),
            base_node: base_node_config,
            metrics: MetricsConfig::default(),
            peer_seeds: PeerSeedsConfig {
                peer_seeds: peer_addresses.into(),
                ..Default::default()
            },
        };

        println!("Using base_node temp_dir: {}", temp_dir_path.clone().display());
        base_node_config.base_node.network = Network::LocalNet;
        base_node_config.base_node.grpc_enabled = true;
        base_node_config.base_node.grpc_address = Some(format!("/ip4/127.0.0.1/tcp/{}", grpc_port).parse().unwrap());
        base_node_config.base_node.report_grpc_error = true;

        base_node_config.base_node.data_dir = temp_dir_path.to_path_buf();
        base_node_config.base_node.identity_file = temp_dir_path.clone().join("base_node_id.json");
        base_node_config.base_node.tor_identity_file = temp_dir_path.clone().join("base_node_tor_id.json");

        base_node_config.base_node.lmdb_path = temp_dir_path.to_path_buf();
        base_node_config.base_node.p2p.transport.transport_type = TransportType::Tcp;
        base_node_config.base_node.p2p.transport.tcp.listener_address =
            format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap();
        base_node_config.base_node.p2p.public_address =
            Some(base_node_config.base_node.p2p.transport.tcp.listener_address.clone());
        base_node_config.base_node.p2p.datastore_path = temp_dir_path.to_path_buf();
        base_node_config.base_node.p2p.dht = DhtConfig::default_local_test();
        base_node_config.base_node.p2p.dht.database_url =
            DbConnectionUrl::File(temp_dir_path.clone().join("dht.sqlit"));
        base_node_config.base_node.p2p.allow_test_addresses = true;
        if base_node_config.base_node.storage.pruning_horizon > 0 {
            base_node_config.base_node.storage.pruning_interval = 1;
        };

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

// pub async fn get_base_node_client(port: u64) -> GrpcBaseNodeClient {
//     let endpoint: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
//     GrpcBaseNodeClient::new(endpoint)
// todo!()
// }

impl BaseNodeProcess {
    pub async fn get_grpc_client(&self) -> anyhow::Result<BaseNodeGrpcClient<Channel>> {
        Ok(BaseNodeGrpcClient::connect(format!("http://127.0.0.1:{}", self.grpc_port)).await?)
    }

    pub fn kill(&mut self) {
        self.kill_signal.trigger();
    }
}

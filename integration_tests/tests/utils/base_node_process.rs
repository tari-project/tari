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
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::Local;
use rand::rngs::OsRng;
use tari_base_node::{run_base_node, BaseNodeConfig, MetricsConfig};
use tari_base_node_grpc_client::BaseNodeGrpcClient;
use tari_common::configuration::CommonConfig;
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::DhtConfig;
use tari_p2p::{auto_update::AutoUpdateConfig, Network, PeerSeedsConfig, TransportType};
use tempfile::tempdir;
use tokio::task;
use tonic::transport::Channel;

use crate::TariWorld;

pub struct BaseNodeProcess {
    pub name: String,
    pub port: u64,
    pub grpc_port: u64,
    pub identity: NodeIdentity,
    pub temp_dir_path: String,
    pub is_seed_node: bool,
    pub kill_signal: Option<tokio::sync::oneshot::Sender<()>>,
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
    // each spawned base node will use different ports
    let (port, grpc_port) = match world.base_nodes.values().last() {
        Some(v) => (v.port + 1, v.grpc_port + 1),
        None => (19000, 19500), // default ports if it's the first base node to be spawned
    };
    let base_node_address = Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
    let base_node_identity = NodeIdentity::random(&mut OsRng, base_node_address, PeerFeatures::COMMUNICATION_NODE);
    println!("Base node identity: {}", base_node_identity);
    let identity = base_node_identity.clone();
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().display().to_string();

    let (kill_signal_sender, kill_signal_receiver) = tokio::sync::oneshot::channel::<()>();
    let process = BaseNodeProcess {
        name: bn_name.clone(),
        port,
        grpc_port,
        identity,
        temp_dir_path,
        is_seed_node,
        kill_signal: Some(kill_signal_sender),
    };

    let name_cloned = bn_name.clone();

    let mut peer_addresses = vec![];
    for peer in peers {
        let peer = world.base_nodes.get(peer.as_str()).unwrap();
        peer_addresses.push(format!(
            "{}::{}",
            peer.identity.public_key(),
            peer.identity.public_address()
        ));
    }

    let mut common_config = CommonConfig::default();
    common_config.base_path = temp_dir.clone();
    task::spawn(futures::future::select(
        kill_signal_receiver,
        Box::pin(async move {
            let mut base_node_config = tari_base_node::ApplicationConfig {
                common: common_config,
                auto_update: AutoUpdateConfig::default(),
                base_node: BaseNodeConfig::default(),
                metrics: MetricsConfig::default(),
                peer_seeds: PeerSeedsConfig {
                    peer_seeds: peer_addresses.into(),
                    ..Default::default()
                },
            };

            println!("Using base_node temp_dir: {}", temp_dir.as_path().display());
            base_node_config.base_node.network = Network::LocalNet;
            base_node_config.base_node.grpc_enabled = true;
            base_node_config.base_node.grpc_address =
                Some(format!("/ip4/127.0.0.1/tcp/{}", grpc_port).parse().unwrap());
            base_node_config.base_node.report_grpc_error = true;

            base_node_config.base_node.data_dir = temp_dir.clone();
            base_node_config.base_node.identity_file = temp_dir.join("base_node_id.json");
            base_node_config.base_node.tor_identity_file = temp_dir.join("base_node_tor_id.json");

            base_node_config.base_node.lmdb_path = temp_dir.clone();
            base_node_config.base_node.p2p.transport.transport_type = TransportType::Tcp;
            base_node_config.base_node.p2p.transport.tcp.listener_address =
                format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();
            base_node_config.base_node.p2p.public_address =
                Some(format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap());
            base_node_config.base_node.p2p.datastore_path = temp_dir.join("p2p");
            base_node_config.base_node.p2p.dht = DhtConfig::default_testnet();
            base_node_config.base_node.p2p.allow_test_addresses = true;

            println!(
                "Initializing base node: name={}; port={}; grpc_port={}; is_seed_node={}",
                name_cloned, port, grpc_port, is_seed_node
            );

        let result = run_base_node(Arc::new(base_node_identity), Arc::new(base_node_config)).await;
        if let Err(e) = result {
            panic!("{:?}", e);
        }
    });

    // make the new base node able to be referenced by other processes
    world.base_nodes.insert(bn_name.clone(), process);
    if is_seed_node {
        world.seed_nodes.push(bn_name);
    }
    // We need to give it time for the base node to startup
    // TODO: it would be better to scan the base node to detect when it has started
    tokio::time::sleep(Duration::from_secs(5)).await;
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

    pub async fn kill(&mut self) {
        self.kill_signal.take().unwrap().send(());
        // This value is arbitrary. If there is no sleep the file might still be locked.
        sleep_until(Instant::now() + Duration::from_secs(5)).await;
    }
}

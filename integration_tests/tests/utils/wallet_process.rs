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

use std::{str::FromStr, thread, time::Duration};

use tari_app_grpc::tari_rpc::SetBaseNodeRequest;
use tari_common::configuration::CommonConfig;
use tari_comms::multiaddr::Multiaddr;
use tari_comms_dht::DhtConfig;
use tari_console_wallet::run_wallet;
use tari_p2p::{auto_update::AutoUpdateConfig, Network, PeerSeedsConfig, TransportType};
use tari_wallet::WalletConfig;
use tari_wallet_grpc_client::WalletGrpcClient;
use tempfile::tempdir;
use tokio::{runtime, task};
use tonic::transport::Channel;

use crate::TariWorld;

#[derive(Debug)]
pub struct WalletProcess {
    pub name: String,
    pub port: u64,
    pub grpc_port: u64,
    pub temp_dir_path: String,
    pub kill_signal: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for WalletProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

pub async fn spawn_wallet(
    world: &mut TariWorld,
    wallet_name: String,
    base_node_name: Option<String>,
    peer_seeds: Vec<String>,
) {
    // each spawned wallet will use different ports
    let (port, grpc_port) = match world.base_nodes.values().last() {
        Some(v) => (v.port + 1, v.grpc_port + 1),
        None => (48000, 48500), // default ports if it's the first wallet to be spawned
    };

    let base_node = base_node_name.map(|name| {
        let pubkey = world.base_nodes.get(&name).unwrap().identity.public_key().clone();
        let port = world.base_nodes.get(&name).unwrap().port;
        let set_base_node_request = SetBaseNodeRequest {
            net_address: format! {"/ip4/127.0.0.1/tcp/{}", port},
            public_key_hex: pubkey.to_string(),
        };

        (pubkey, port, set_base_node_request)
    });

    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().display().to_string();

    let mut peer_addresses = vec![];
    for peer in peer_seeds {
        let peer = world.base_nodes.get(peer.as_str()).unwrap();
        peer_addresses.push(format!(
            "{}::{}",
            peer.identity.public_key(),
            peer.identity.public_address()
        ));
    }

    let base_node_cloned = base_node.clone();
    let (kill_signal_sender, kill_signal_receiver) = tokio::sync::oneshot::channel::<()>();
    task::spawn(futures::future::select(
        kill_signal_receiver,
        Box::pin(async move {
            let mut wallet_config = tari_console_wallet::ApplicationConfig {
                common: CommonConfig::default(),
                auto_update: AutoUpdateConfig::default(),
                wallet: WalletConfig::default(),
                peer_seeds: PeerSeedsConfig {
                    peer_seeds: peer_addresses.into(),
                    ..Default::default()
                },
            };

            eprintln!("Using wallet temp_dir: {}", temp_dir.path().display());

            wallet_config.wallet.network = Network::LocalNet;
            wallet_config.wallet.password = Some("test".into());
            wallet_config.wallet.grpc_enabled = true;
            wallet_config.wallet.grpc_address =
                Some(Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", grpc_port)).unwrap());
            wallet_config.wallet.data_dir = temp_dir.path().join("data/wallet");
            wallet_config.wallet.db_file = temp_dir.path().join("db/console_wallet.db");
            wallet_config.wallet.p2p.transport.transport_type = TransportType::Tcp;
            wallet_config.wallet.p2p.transport.tcp.listener_address =
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/tcp/{}", port)).unwrap();
            wallet_config.wallet.p2p.public_address =
                Some(wallet_config.wallet.p2p.transport.tcp.listener_address.clone());
            wallet_config.wallet.p2p.datastore_path = temp_dir.path().join("peer_db/wallet");
            wallet_config.wallet.p2p.dht = DhtConfig::default_local_test();

            // FIXME: wallet doesn't pick up the custom base node for some reason atm
            wallet_config.wallet.custom_base_node =
                base_node_cloned.map(|(pubkey, port, _)| format!("{}::/ip4/127.0.0.1/tcp/{}", pubkey, port));

            let rt = runtime::Builder::new_multi_thread().enable_all().build().unwrap();

            if let Err(e) = run_wallet(rt, &mut wallet_config) {
                panic!("{:?}", e);
            }
        }),
    ));

    // make the new wallet able to be referenced by other processes
    world.wallets.insert(wallet_name.clone(), WalletProcess {
        name: wallet_name.clone(),
        port,
        grpc_port,
        temp_dir_path,
        kill_signal: Some(kill_signal_sender),
    });

    // We need to give it time for the wallet to startup
    // TODO: it would be better to scan the wallet to detect when it has started
    tokio::time::sleep(Duration::from_secs(10)).await;

    // TODO: fix the wallet configuration so the base node is correctly setted on startup insted of afterwards
    if let Some((_, _, hacky_request)) = base_node {
        let mut wallet_client = create_wallet_client(world, wallet_name)
            .await
            .expect("wallet grpc client");

        let _resp = wallet_client.set_base_node(hacky_request).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(10)).await;
}

pub async fn create_wallet_client(world: &TariWorld, wallet_name: String) -> anyhow::Result<WalletGrpcClient<Channel>> {
    let wallet_grpc_port = world.wallets.get(&wallet_name).unwrap().grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{}", wallet_grpc_port);

    eprintln!("Wallet GRPC at {}", wallet_addr);

    Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
}

impl WalletProcess {
    #[allow(dead_code)]
    pub async fn get_grpc_client(&self) -> anyhow::Result<WalletGrpcClient<Channel>> {
        let wallet_addr = format!("http://127.0.0.1:{}", self.grpc_port);
        Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
    }

    pub async fn kill(&mut self) {
        self.kill_signal.take().unwrap().send(());
    }
}

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

use std::{convert::TryInto, thread};

use minotari_app_grpc::tari_rpc::GetIdentityRequest;
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use minotari_merge_mining_proxy::{merge_miner, Cli};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{json, Value};
use tari_common::{configuration::Network, network_check::set_network_if_choice_valid};
use tari_common_types::{tari_address::TariAddress, types::PublicKey};
use tari_utilities::ByteArray;
use tempfile::tempdir;
use tokio::runtime;
use tonic::transport::Channel;

use super::get_port;
use crate::TariWorld;

#[derive(Clone, Debug)]
pub struct MergeMiningProxyProcess {
    pub name: String,
    pub base_node_name: String,
    pub wallet_name: String,
    pub port: u64,
    pub origin_submission: bool,
    id: u64,
    pub stealth: bool,
}

pub async fn register_merge_mining_proxy_process(
    world: &mut TariWorld,
    merge_mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
    origin_submission: bool,
    stealth: bool,
) {
    let merge_mining_proxy = MergeMiningProxyProcess {
        name: merge_mining_proxy_name.clone(),
        base_node_name,
        wallet_name,
        port: get_port(18000..18499).unwrap(),
        origin_submission,
        id: 0,
        stealth,
    };

    merge_mining_proxy.start(world).await;
    world
        .merge_mining_proxies
        .insert(merge_mining_proxy_name, merge_mining_proxy);
}

impl MergeMiningProxyProcess {
    pub async fn start(&self, world: &mut TariWorld) {
        std::env::set_var("TARI_NETWORK", "localnet");
        set_network_if_choice_valid(Network::LocalNet).unwrap();

        let temp_dir = tempdir().unwrap();
        let data_dir = temp_dir.path().join("data/miner");
        let data_dir_str = data_dir.clone().into_os_string().into_string().unwrap();
        let mut config_path = data_dir;
        config_path.push("config.toml");
        let base_node_grpc_port = world.get_node(&self.base_node_name).unwrap().grpc_port;
        let proxy_full_address = format! {"/ip4/127.0.0.1/tcp/{}", self.port};
        let origin_submission = self.origin_submission;
        let mut wallet_client = create_wallet_client(world, self.wallet_name.clone())
            .await
            .expect("wallet grpc client");
        let wallet_public_key = PublicKey::from_vec(
            &wallet_client
                .identify(GetIdentityRequest {})
                .await
                .unwrap()
                .into_inner()
                .public_key,
        )
        .unwrap();
        let wallet_payment_address = TariAddress::new(wallet_public_key, Network::LocalNet);
        let stealth = self.stealth;
        thread::spawn(move || {
            let cli = Cli {
                common: CommonCliArgs {
                    base_path: data_dir_str,
                    config: config_path.into_os_string().into_string().unwrap(),
                    log_config: None,
                    log_level: None,
                    network: Some("localnet".to_string().try_into().unwrap()),
                    config_property_overrides: vec![
                        ("merge_mining_proxy.listener_address".to_string(), proxy_full_address),
                        (
                            "merge_mining_proxy.base_node_grpc_address".to_string(),
                            format!("/ip4/127.0.0.1/tcp/{}", base_node_grpc_port),
                        ),
                        (
                            "merge_mining_proxy.monerod_url".to_string(),
                            [
                                "http://stagenet.xmr-tw.org:38081",
                                "http://stagenet.community.xmr.to:38081",
                                "http://monero-stagenet.exan.tech:38081",
                                "http://xmr-lux.boldsuck.org:38081",
                                "http://singapore.node.xmr.pm:38081",
                            ]
                            .join(","),
                        ),
                        ("merge_mining_proxy.monerod_use_auth".to_string(), "false".to_string()),
                        ("merge_mining_proxy.monerod_username".to_string(), "".to_string()),
                        ("merge_mining_proxy.monerod_password".to_string(), "".to_string()),
                        (
                            "merge_mining_proxy.wait_for_initial_sync_at_startup".to_string(),
                            "false".to_string(),
                        ),
                        (
                            "merge_mining_proxy.submit_to_origin".to_string(),
                            origin_submission.to_string(),
                        ),
                        (
                            "merge_mining_proxy.wallet_payment_address".to_string(),
                            wallet_payment_address.to_hex(),
                        ),
                        ("merge_mining_proxy.stealth_payment".to_string(), stealth.to_string()),
                    ],
                },
                non_interactive_mode: false,
            };
            let rt = runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            if let Err(e) = rt.block_on(merge_miner(cli)) {
                println!("Error running merge mining proxy : {:?}", e);
            }
        });
    }

    async fn get_response(&self, path: &str) -> Value {
        let full_address = format!("http://127.0.0.1:{}", self.port);
        reqwest::get(format!("{}/{}", full_address, path))
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    }

    async fn json_rpc_call(&mut self, method_name: &str, params: &Value) -> Value {
        let client = reqwest::Client::new();
        let json = json!({
            "jsonrpc": "2.0",
            "method": method_name,
            "params": params,
            "id":self.id}
        );
        println!("json_rpc_call {}", method_name);
        println!("json payload {}", json);
        self.id += 1;
        let full_address = format!("http://127.0.0.1:{}/json_rpc", self.port);
        client
            .post(full_address)
            .json(&json)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    pub async fn get_height(&self) -> Value {
        self.get_response("get_height").await
    }

    pub async fn get_block_template(&mut self) -> Value {
        let params = json!({
            "wallet_address":"5AUoj81i63cBUbiKY5jybsZXRDYb9CppmSjiZXC8ZYT6HZH6ebsQvBecYfRKDYoyzKF2uML9FKkTAc7nJvHKdoDYQEeteRW",
            "reserve_size":60
        });
        self.json_rpc_call("getblocktemplate", &params).await
    }

    pub async fn submit_block(&mut self, block_template_blob: &Value) -> Value {
        self.json_rpc_call("submit_block", &json!(vec![block_template_blob]))
            .await
    }

    pub async fn get_last_block_header(&mut self) -> Value {
        self.json_rpc_call("get_last_block_header", &json!({})).await
    }

    pub async fn get_block_header_by_hash(&mut self, hash: Value) -> Value {
        self.json_rpc_call("get_block_header_by_hash", &json!({ "hash": hash }))
            .await
    }

    pub async fn mine(&mut self) -> Value {
        let template = self.get_block_template().await;
        let template = template.get("result").unwrap();
        // XMRig always calls this, so duplicated here
        self.get_height().await;
        let block = template.get("blocktemplate_blob").unwrap();
        self.submit_block(block).await
    }
}

pub async fn create_wallet_client(world: &TariWorld, wallet_name: String) -> anyhow::Result<WalletGrpcClient<Channel>> {
    let wallet_grpc_port = world.wallets.get(&wallet_name).unwrap().grpc_port;
    let wallet_addr = format!("http://127.0.0.1:{}", wallet_grpc_port);

    eprintln!("Wallet GRPC at {}", wallet_addr);

    Ok(WalletGrpcClient::connect(wallet_addr.as_str()).await?)
}

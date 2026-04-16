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
//   SERVICES; LOSS OF USE, DATA, OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF
//   THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use minotari_wallet_grpc_client::{grpc, WalletGrpcClient};
use serde_json::{json, Value};
use tari_common_types::tari_address::TariAddress;

use crate::{wait_for_service, TariWorld};

#[derive(Clone, Debug)]
pub struct MergeMiningProxyProcess {
    pub name: String,
    pub base_node_name: String,
    pub wallet_name: String,
    pub port: u16,
    id: u64,
}

pub async fn register_merge_mining_proxy_process(
    world: &mut TariWorld,
    merge_mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
) {
    let proxy_port = crate::port_pool::global_port_pool()
        .allocate_merge_mining_proxy_port()
        .expect("Port pool exhausted — too many concurrent merge mining proxies");

    let merge_mining_proxy = MergeMiningProxyProcess {
        name: merge_mining_proxy_name.clone(),
        base_node_name,
        wallet_name,
        port: proxy_port,
        id: 0,
    };

    merge_mining_proxy.start(world).await;
    world
        .merge_mining_proxies
        .insert(merge_mining_proxy_name, merge_mining_proxy);
}

impl MergeMiningProxyProcess {
    /// Start the XMRig-compatible RxT mining proxy.
    ///
    /// This enables the built-in xmrig proxy on the base node by modifying its config,
    /// then restarts the base node with xmrig proxy enabled. The proxy uses RandomXT
    /// (Tari-native RandomX) which requires no external MoneroD dependency.
    pub async fn start(&self, world: &mut TariWorld) {
        // Collect all needed data from the base node before mutating world
        let (is_seed_node, seed_nodes, mut config) = {
            let base_node = world.get_node(&self.base_node_name).unwrap();
            (
                base_node.is_seed_node,
                base_node.seed_nodes.clone(),
                base_node.config.clone(),
            )
        };

        // Get wallet payment address
        let wallet_grpc_port = world.wallets.get(&self.wallet_name).unwrap().grpc_port;
        let wallet_addr = format!("http://127.0.0.1:{wallet_grpc_port}");
        let mut wallet_client =
            WalletGrpcClient::connect(&wallet_addr).await.expect("wallet grpc client");
        let wallet_address_bytes = wallet_client
            .get_address(grpc::Empty {})
            .await
            .unwrap()
            .into_inner()
            .interactive_address;
        let wallet_payment_address = TariAddress::from_bytes(&wallet_address_bytes).unwrap();

        // Kill the existing base node and remove it from world
        if let Some(mut node) = world.base_nodes.remove(&self.base_node_name) {
            node.kill();
        }

        // Configure xmrig proxy on the base node
        config.xmrig_proxy_enabled = true;
        config.xmrig_proxy_address = format!("/ip4/127.0.0.1/tcp/{}", self.port)
            .parse()
            .expect("valid xmrig proxy address");
        config.xmrig_proxy_wallet_payment_address = wallet_payment_address.to_base58();

        // Restart the base node with xmrig proxy enabled
        crate::base_node_process::spawn_base_node_with_config(
            world,
            is_seed_node,
            self.base_node_name.clone(),
            seed_nodes,
            config,
        )
        .await;

        // Wait for the xmrig proxy port to become available
        wait_for_service(self.port).await;
    }

    /// Get the current height via the XMRig proxy (GET /get_height).
    ///
    /// Returns `{ "count": <height>, "status": "OK" }`.
    pub async fn get_height(&self) -> Value {
        let full_address = format!("http://127.0.0.1:{}", self.port);
        reqwest::get(format!("{full_address}/get_height"))
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    }

    /// Request a block template via the XMRig proxy JSON-RPC.
    pub async fn get_block_template(&mut self) -> Value {
        let client = reqwest::Client::new();
        let json = json!({
            "jsonrpc": "2.0",
            "method": "getblocktemplate",
            "params": {},
            "id": self.id
        });
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

    /// Submit a mined block via the XMRig proxy JSON-RPC.
    ///
    /// Returns `{ "status": "OK", "untrusted": false }` on success.
    pub async fn submit_block(&mut self, block_template_blob: &Value) -> Value {
        let client = reqwest::Client::new();
        let json = json!({
            "jsonrpc": "2.0",
            "method": "submitblock",
            "params": json!([block_template_blob]),
            "id": self.id
        });
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

    /// Mine a single block: get template, then submit via the proxy.
    pub async fn mine(&mut self) -> Value {
        const MAX_RETRIES: u32 = 5;
        let mut template_result = None;
        for attempt in 1..=MAX_RETRIES {
            let template = self.get_block_template().await;
            if let Some(result) = template.get("result") {
                template_result = Some(result.clone());
                break;
            }
            if attempt < MAX_RETRIES {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
        let template_result = template_result.expect("Failed to get block template after retries");
        // XMRig always calls this, so duplicated here
        self.get_height().await;
        let block = template_result.get("blocktemplate_blob").unwrap();
        self.submit_block(block).await
    }
}

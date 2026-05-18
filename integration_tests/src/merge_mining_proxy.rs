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

use serde_json::{Value, json};

use crate::TariWorld;

#[derive(Clone, Debug)]
pub struct MergeMiningProxyProcess {
    pub name: String,
    pub base_node_name: String,
    pub port: u16,
    id: u64,
}

pub async fn register_merge_mining_proxy_process(
    world: &mut TariWorld,
    merge_mining_proxy_name: String,
    base_node_name: String,
) {
    let base_node = world
        .get_node(&base_node_name)
        .expect("Base node not found for merge mining proxy");
    let proxy_port = base_node.xmrig_proxy_port;

    let merge_mining_proxy = MergeMiningProxyProcess {
        name: merge_mining_proxy_name.clone(),
        base_node_name,
        port: proxy_port,
        id: 0,
    };

    world
        .merge_mining_proxies
        .insert(merge_mining_proxy_name, merge_mining_proxy);
}

impl MergeMiningProxyProcess {
    async fn get_response(&self, path: &str) -> Value {
        let full_address = format!("http://127.0.0.1:{}", self.port);
        reqwest::get(format!("{full_address}/{path}"))
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
            "id": self.id}
        );
        println!("json_rpc_call {method_name}");
        println!("json payload {json}");
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
        self.json_rpc_call("getblocktemplate", &json!({})).await
    }

    pub async fn submit_block(&mut self, block_template_blob: &Value) -> Value {
        self.json_rpc_call("submitblock", &json!(vec![block_template_blob]))
            .await
    }

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

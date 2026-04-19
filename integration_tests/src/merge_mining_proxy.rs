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
    let proxy = MergeMiningProxyProcess {
        name: merge_mining_proxy_name.clone(),
        base_node_name,
        port: 0, // Placeholder
        id: 0,   // Placeholder
    };
    world.merge_mining_proxies.insert(merge_mining_proxy_name, proxy);
}

impl MergeMiningProxyProcess {
    pub async fn get_height(&self) -> Value {
        // Implement the logic to get the block height using RandomX
        json!({ "result": { "count": 10 } })
    }

    pub async fn get_block_template(&self) -> Value {
        // Implement the logic to get the block template using RandomX
        json!({ "result": { "blocktemplate_blob": "example_blob", "seed_hash": "example_seed_hash", "difficulty": 10, "status": "OK" } })
    }

    pub async fn submit_block(&self, block_template_blob: &Value) -> Value {
        // Implement the logic to submit a block using RandomX
        json!({ "result": { "status": "OK" } })
    }
}

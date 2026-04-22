//   Copyright 2023. The Tari Project
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

use cucumber::{then, when};
use tari_integration_tests::{TariWorld, merge_mining_proxy::register_merge_mining_proxy_process};

const TARI_MINING_BLOB_SIZE: usize = 76;
const POW_ALGO_OFFSET: usize = 43;
const POW_ALGO_RANDOMXT: u8 = 2;

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("hex string must have an even length".to_string());
    }

    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

// Merge mining proxy steps

#[when(expr = "I have a merge mining proxy {word} connected to {word} with default config")]
async fn merge_mining_proxy_with_default_config(
    world: &mut TariWorld,
    mining_proxy_name: String,
    base_node_name: String,
) {
    register_merge_mining_proxy_process(world, mining_proxy_name, base_node_name).await;
}

#[when(expr = "I ask for a block height from proxy {word}")]
async fn merge_mining_ask_for_block_height(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_height().await;
}

#[then(expr = "Proxy response height is valid")]
async fn merge_mining_response_height(world: &mut TariWorld) {
    let count = world.last_merge_miner_response.get("result");
    assert!(
        count.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = count.unwrap();
    assert!(result.get("count").is_some(), "Result has no `count` {result}");
    assert!(
        result.get("count").unwrap().as_u64().is_some(),
        "Count is invalid {result}"
    );
}

#[when(expr = "I ask for a block template from proxy {word}")]
async fn merge_mining_ask_for_block_template(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_block_template().await;
}

#[then(expr = "Proxy response block template is valid")]
async fn merge_mining_response_block_template_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    assert!(
        result.get("blocktemplate_blob").is_some(),
        "Result has no `blocktemplate_blob` {result}"
    );
    assert!(result.get("seed_hash").is_some(), "Result has no `seed_hash` {result}");
    assert!(
        result.get("difficulty").is_some(),
        "Result has no `difficulty` {result}"
    );
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {result}"
    );

    let blob_hex = result
        .get("blocktemplate_blob")
        .and_then(|v| v.as_str())
        .expect("`blocktemplate_blob` must be a hex string");
    let blob = decode_hex(blob_hex).expect("`blocktemplate_blob` must decode as hex");
    assert_eq!(
        blob.len(),
        TARI_MINING_BLOB_SIZE,
        "Unexpected mining blob size: expected {TARI_MINING_BLOB_SIZE}, got {}",
        blob.len()
    );
    assert_eq!(
        blob[POW_ALGO_OFFSET],
        POW_ALGO_RANDOMXT,
        "Expected RandomXT pow algo byte ({POW_ALGO_RANDOMXT}) at offset {POW_ALGO_OFFSET}, got {}",
        blob[POW_ALGO_OFFSET]
    );
}

#[when(expr = "I submit a block through proxy {word}")]
async fn merge_mining_submit_block(world: &mut TariWorld, mining_proxy_name: String) {
    let block_template_blob = world
        .last_merge_miner_response
        .get("result")
        .unwrap()
        .get("blocktemplate_blob");
    assert!(
        block_template_blob.is_some(),
        "The last response doesn't have `blocktemplate_blob` {}",
        world.last_merge_miner_response
    );
    let block_template_blob = block_template_blob.unwrap().clone();
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    println!("block_template {block_template_blob:?}");
    world.last_merge_miner_response = merge_miner.submit_block(&block_template_blob).await;
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
}

#[then(expr = "Proxy response block submission is valid")]
async fn merge_mining_submission_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    assert!(
        result.get("status").is_some(),
        "Response has no `status` {}",
        world.last_merge_miner_response
    );
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Status is not OK {}",
        world.last_merge_miner_response
    );
}

#[when(expr = "I merge mine {int} blocks via {word}")]
async fn merge_mining_mine(world: &mut TariWorld, count: u64, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    for _ in 0..count {
        merge_miner.mine().await;
    }
}

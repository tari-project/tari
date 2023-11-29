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
use tari_integration_tests::{merge_mining_proxy::register_merge_mining_proxy_process, TariWorld};

// Merge mining proxy steps

#[when(expr = "I have a merge mining proxy {word} connected to {word} and {word} with origin submission {word}")]
async fn merge_mining_proxy_with_submission(
    world: &mut TariWorld,
    mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
    enabled: String,
) {
    let enabled = match enabled.as_str() {
        "enabled" => true,
        "disabled" => false,
        _ => panic!("This should be a boolean"),
    };
    register_merge_mining_proxy_process(world, mining_proxy_name, base_node_name, wallet_name, enabled, true).await;
}

#[when(expr = "I have a merge mining proxy {word} connected to {word} and {word} with default config")]
async fn merge_mining_proxy_with_default_config(
    world: &mut TariWorld,
    mining_proxy_name: String,
    base_node_name: String,
    wallet_name: String,
) {
    register_merge_mining_proxy_process(world, mining_proxy_name, base_node_name, wallet_name, true, false).await;
}

#[when(expr = "I ask for a block height from proxy {word}")]
async fn merge_mining_ask_for_block_height(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_height().await;
}

#[then(expr = "Proxy response height is valid")]
async fn merge_mining_response_height(world: &mut TariWorld) {
    let height = world.last_merge_miner_response.get("height");
    assert!(
        height.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let height = height.unwrap();
    assert!(height.as_u64().is_some(), "Height is invalid {}", height);
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
    assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
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
    println!("block_template {:?}", block_template_blob);
    world.last_merge_miner_response = merge_miner.submit_block(&block_template_blob).await;
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
    println!("last_merge_miner_response {:?}", world.last_merge_miner_response);
}

#[then(expr = "Proxy response block submission is valid {word} submitting to origin")]
async fn merge_mining_submission_is_valid(world: &mut TariWorld, how: String) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    if how == *"with" {
        assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
        let status = result.get("status");
        assert!(status.is_some(), "Result has no status {}", result);
    } else {
        assert!(
            world.last_merge_miner_response.get("status").is_some(),
            "Response has no `status` {}",
            world.last_merge_miner_response
        );
    }
}

#[when(expr = "I merge mine {int} blocks via {word}")]
async fn merge_mining_mine(world: &mut TariWorld, count: u64, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    for _ in 0..count {
        merge_miner.mine().await;
    }
}

#[when(expr = "I ask for the last block header from proxy {word}")]
async fn merge_mining_ask_for_last_block_header(world: &mut TariWorld, mining_proxy_name: String) {
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_last_block_header().await;
}

#[then(expr = "Proxy response for block header by hash is valid")]
async fn merge_mining_bloch_header_by_hash_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    let status = result.get("status");
    assert!(status.is_some(), "Result has no status {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
    );
}

#[then(expr = "Proxy response for last block header is valid")]
async fn merge_mining_response_last_block_header_is_valid(world: &mut TariWorld) {
    let result = world.last_merge_miner_response.get("result");
    assert!(
        result.is_some(),
        "Response is invalid {}",
        world.last_merge_miner_response
    );
    let result = result.unwrap();
    assert!(result.get("_aux").is_some(), "Result has no `_aux` {}", result);
    let status = result.get("status");
    assert!(status.is_some(), "Result has no status {}", result);
    assert_eq!(
        result.get("status").unwrap().as_str().unwrap(),
        "OK",
        "Result has no `status` {}",
        result
    );
    let block_header = result.get("block_header");
    assert!(block_header.is_some(), "Result has no `block_header` {}", result);
    let block_header = block_header.unwrap();
    assert!(
        block_header.get("hash").is_some(),
        "Block_header has no `hash` {}",
        block_header
    );
}

#[when(expr = "I ask for a block header by hash using last block header from proxy {word}")]
async fn merge_mining_ask_for_block_header_by_hash(world: &mut TariWorld, mining_proxy_name: String) {
    let hash = world
        .last_merge_miner_response
        .get("result")
        .unwrap()
        .get("block_header")
        .unwrap()
        .get("hash")
        .unwrap()
        .clone();
    let merge_miner = world.get_mut_merge_miner(&mining_proxy_name).unwrap();
    world.last_merge_miner_response = merge_miner.get_block_header_by_hash(hash).await;
}

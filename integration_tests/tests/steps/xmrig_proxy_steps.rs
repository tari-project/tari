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

use serde_json::{Value, json};

use tari_integration_tests::TariWorld;
use cucumber::{then, when};

// Helper to resolve the XMRig proxy port for a given base node
fn get_xmrig_proxy_port(world: &TariWorld, base_node_name: &String) -> u16 {
    world
        .get_node(base_node_name)
        .expect("Base node not found for XMRig proxy")
        .xmrig_proxy_port
}

// ---------------------------------------------------------------------------
// JSON-RPC steps — POST /json_rpc
// ---------------------------------------------------------------------------

#[when(expr = "I call JSON-RPC getheight on proxy of node {word}")]
async fn xmrig_proxy_jsonrpc_getheight(world: &mut TariWorld, base_node_name: String) {
    let port = get_xmrig_proxy_port(world, &base_node_name);
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "method": "getheight",
        "params": {},
        "id": 1
    });
    world.last_merge_miner_response = client
        .post(format!("http://127.0.0.1:{port}/json_rpc"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
}

#[when(expr = "I call JSON-RPC getinfo on proxy of node {word}")]
async fn xmrig_proxy_jsonrpc_getinfo(world: &mut TariWorld, base_node_name: String) {
    let port = get_xmrig_proxy_port(world, &base_node_name);
    let client = reqwest::Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "method": "getinfo",
        "params": {},
        "id": 1
    });
    world.last_merge_miner_response = client
        .post(format!("http://127.0.0.1:{port}/json_rpc"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// GET steps — GET /getheight, GET /getinfo
// ---------------------------------------------------------------------------

#[when(expr = r"I call GET \/getheight on proxy of node {word}")]
async fn xmrig_proxy_get_getheight(world: &mut TariWorld, base_node_name: String) {
    let port = get_xmrig_proxy_port(world, &base_node_name);
    world.last_merge_miner_response = reqwest::get(format!("http://127.0.0.1:{port}/getheight"))
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
}

#[when(expr = r"I call GET \/getinfo on proxy of node {word}")]
async fn xmrig_proxy_get_getinfo(world: &mut TariWorld, base_node_name: String) {
    let port = get_xmrig_proxy_port(world, &base_node_name);
    world.last_merge_miner_response = reqwest::get(format!("http://127.0.0.1:{port}/getinfo"))
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Validation steps
// ---------------------------------------------------------------------------

#[then(expr = "XMRig getheight response is valid")]
async fn xmrig_getheight_response_is_valid(world: &mut TariWorld) {
    let resp = &world.last_merge_miner_response;

    // JSON-RPC wrapped: { "result": { "height": ..., "hash": ..., "status": "OK" } }
    if let Some(result) = resp.get("result") {
        assert!(result.get("height").is_some(), "Result has no `height` {result}");
        assert!(
            result.get("height").unwrap().as_u64().is_some(),
            "Height is not u64 {result}"
        );
        assert!(result.get("hash").is_some(), "Result has no `hash` {result}");
        assert!(
            result.get("hash").unwrap().as_str().is_some(),
            "Hash is not a string {result}"
        );
        assert_eq!(
            result.get("status").unwrap().as_str().unwrap(),
            "OK",
            "Status is not OK {result}"
        );
    } else {
        // Flat GET response: { "height": ..., "hash": ..., "status": "OK" }
        assert!(resp.get("height").is_some(), "Response has no `height` {resp}");
        assert!(
            resp.get("height").unwrap().as_u64().is_some(),
            "Height is not u64 {resp}"
        );
        assert!(resp.get("hash").is_some(), "Response has no `hash` {resp}");
        assert_eq!(
            resp.get("status").unwrap().as_str().unwrap(),
            "OK",
            "Status is not OK {resp}"
        );
    }
}

#[then(expr = "XMRig getinfo response is valid")]
async fn xmrig_getinfo_response_is_valid(world: &mut TariWorld) {
    let resp = &world.last_merge_miner_response;

    // JSON-RPC wrapped: { "result": { "response": { ... }, "id": ... } }
    if let Some(result) = resp.get("result") {
        let inner = result
            .get("response")
            .expect("getinfo result missing `response` object");
        assert!(inner.get("top_hash").is_some(), "Missing `top_hash` {inner}");
        assert!(inner.get("height").is_some(), "Missing `height` {inner}");
        assert!(
            inner.get("height").unwrap().as_u64().is_some(),
            "Height is not u64 {inner}"
        );
        assert!(
            inner.get("height_no_orphans").is_some(),
            "Missing `height_no_orphans` {inner}"
        );
        assert!(inner.get("target").is_some(), "Missing `target` {inner}");
        assert!(inner.get("target_height").is_some(), "Missing `target_height` {inner}");
        assert!(
            inner.get("unlocked").unwrap().as_bool() == Some(true),
            "Unlocked is not true {inner}"
        );
        assert_eq!(
            inner.get("status").unwrap().as_str().unwrap(),
            "OK",
            "Status is not OK {inner}"
        );
    } else {
        // Flat GET response: { "response": { ... } }
        assert!(resp.get("response").is_some(), "Response has no `response` {resp}");
        let inner = resp.get("response").unwrap();
        assert!(inner.get("top_hash").is_some(), "Missing `top_hash` {inner}");
        assert!(inner.get("height").is_some(), "Missing `height` {inner}");
        assert_eq!(
            inner.get("status").unwrap().as_str().unwrap(),
            "OK",
            "Status is not OK {inner}"
        );
    }
}

// ---------------------------------------------------------------------------
// Height-aware validation — verify getheight reflects mined blocks
// ---------------------------------------------------------------------------

#[then(expr = "XMRig getheight response height matches node height")]
async fn xmrig_getheight_matches_node_height(world: &mut TariWorld) {
    let resp = &world.last_merge_miner_response;

    // Extract height from either JSON-RPC or flat response
    let height = if let Some(result) = resp.get("result") {
        result.get("height").unwrap().as_u64().unwrap()
    } else {
        resp.get("height").unwrap().as_u64().unwrap()
    };

    // Compare against the first base node's height
    let node_name = world
        .base_nodes
        .keys()
        .next()
        .expect("No base node found to compare height against");
    let mut client = world
        .get_node_client(node_name)
        .await
        .expect("Failed to get gRPC client");
    let tip_info = client
        .get_tip_info(minotari_node_grpc_client::grpc::Empty {})
        .await
        .expect("Failed to get tip info")
        .into_inner();
    let best_height = tip_info.metadata.unwrap().best_block_height;
    assert_eq!(
        height,
        best_height,
        "XMRig getheight height {height} does not match node height {best_height}"
    );
}

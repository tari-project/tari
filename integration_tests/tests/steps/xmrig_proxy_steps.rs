//   Copyright 2026. The Tari Project
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

use cucumber::when;
use serde_json::Value;
use tari_integration_tests::TariWorld;

// Helper to resolve the XMRig proxy port for a given base node
fn get_xmrig_proxy_port(world: &TariWorld, base_node_name: &String) -> u16 {
    world
        .get_node(base_node_name)
        .expect("Base node not found for XMRig proxy")
        .xmrig_proxy_port
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

    // ---------------------------------------------------------------------------
    // Compare heights to validate
    // ---------------------------------------------------------------------------

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
    println!("Height: {} node height: {}", height, best_height);
    assert_eq!(
        height, best_height,
        "XMRig getheight height {height} does not match node height {best_height}"
    );
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

    // ---------------------------------------------------------------------------
    // Compare heights to validate
    // ---------------------------------------------------------------------------

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
        height, best_height,
        "XMRig getinfo height {height} does not match node height {best_height}"
    );
}

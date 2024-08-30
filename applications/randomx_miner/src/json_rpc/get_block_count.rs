//  Copyright 2024. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use log::{debug, error};
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{
    error::{Error, RequestError},
    Request,
};

pub const LOG_TARGET: &str = "minotari::randomx_miner::json_rpc::get_block_count";

#[allow(dead_code)] // jsonrpc and id fields
#[derive(Deserialize, Debug)]
pub struct GetBlockCountResponse {
    jsonrpc: String,
    id: String,
    pub result: BlockCount,
}

#[derive(Deserialize, Debug)]
pub struct BlockCount {
    pub count: u64,
    pub status: String,
}

pub async fn get_block_count(client: &Client, node_address: &String, tip: Arc<Mutex<u64>>) -> Result<(), Error> {
    let response = client
        .post(format!("{}/json_rpc", &node_address.to_string()))
        .json(&Request::new("get_block_count", serde_json::Value::Null))
        .send()
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Reqwest error: {:?}", e);
            Error::from(RequestError::GetBlockCount(e.to_string()))
        })?
        .json::<GetBlockCountResponse>()
        .await?;
    debug!(target: LOG_TARGET, "`get_block_count` Response: {:?}", response);

    if response.result.status == "OK" {
        debug!(target: LOG_TARGET, "`get_block_count` Blockchain tip (block height): {}", response.result.count);
        *tip.lock().await = response.result.count;
    } else {
        debug!(target: LOG_TARGET, "Failed to get the block count. Status: {}", response.result.status);
        return Err(RequestError::GetBlockCount(format!(
            "Failed to get the block count. Status: {}",
            response.result.status
        ))
        .into());
    }

    Ok(())
}

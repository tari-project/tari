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

use log::{debug, error};
use reqwest::Client;
use serde_json::json;

use crate::{error::RequestError, json_rpc::Request};

pub const LOG_TARGET: &str = "minotari::randomx_miner::json_rpc::submit_block";

pub async fn submit_block<'a>(client: &Client, node_address: &'a str, block_hash: String) -> Result<(), RequestError> {
    let response = client
        .post(format!("{}/json_rpc", &node_address.to_string()))
        .json(&Request::new("submitblock", json!([block_hash])))
        .send()
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Reqwest error: {:?}", e);
            RequestError::SubmitBlock(e.to_string())
        })?;
    debug!(target: LOG_TARGET, "`submit_block` Response: {:?}", response);

    if response.status().is_success() {
        Ok(())
    } else {
        debug!(target: LOG_TARGET, "Failed to get the block template. {:?}", response);
        Err(RequestError::SubmitBlock(format!(
            "Failed to get the block template. Status: {:?}",
            response
        )))
    }
}

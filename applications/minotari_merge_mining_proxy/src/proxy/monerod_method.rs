//  Copyright 2020, The Tari Project
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

use std::{fmt::Display, str::FromStr};

use bytes::Bytes;
use hyper::{Method, Uri};
use log::warn;

use crate::error::MmProxyError;

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::monerod_method";

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum MonerodMethod {
    GetHeight,
    GetVersion,
    GetBlockTemplate,
    SubmitBlock,
    GetBlockHeaderByHash,
    GetLastBlockHeader,
    #[allow(dead_code)]
    GetBlock,
    RpcMethodNotDefined,
}

impl FromStr for MonerodMethod {
    type Err = MmProxyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "/get_height" | "/getheight" => Ok(MonerodMethod::GetHeight),
            "get_height" | "getheight" => Ok(MonerodMethod::GetHeight),
            "get_version" | "getversion" => Ok(MonerodMethod::GetVersion),
            "get_block_template" | "getblocktemplate" => Ok(MonerodMethod::GetBlockTemplate),
            "submit_block" | "submitblock" => Ok(MonerodMethod::SubmitBlock),
            "get_block_header_by_hash" | "getblockheaderbyhash" => Ok(MonerodMethod::GetBlockHeaderByHash),
            "get_last_block_header" | "getlastblockheader" => Ok(MonerodMethod::GetLastBlockHeader),
            "get_block" | "getblovk" => Ok(MonerodMethod::GetLastBlockHeader),
            _ => {
                let msg = format!("Unknown monerod rpc method: '{}'", s);
                warn!(target: LOG_TARGET, "{}", msg);
                Err(MmProxyError::ConversionError(msg))
            },
        }
    }
}

impl Display for MonerodMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            MonerodMethod::GetHeight => "get_height".to_string(),
            MonerodMethod::GetVersion => "get_version".to_string(),
            MonerodMethod::GetBlockTemplate => "get_block_template".to_string(),
            MonerodMethod::SubmitBlock => "submit_block".to_string(),
            MonerodMethod::GetBlockHeaderByHash => "get_block_header_by_hash".to_string(),
            MonerodMethod::GetLastBlockHeader => "get_last_block_header".to_string(),
            MonerodMethod::GetBlock => "get_block".to_string(),
            MonerodMethod::RpcMethodNotDefined => "rpc_method_not_defined".to_string(),
        };
        write!(f, "{}", str)
    }
}

/// Parse the monerod RPC method from the request components
pub fn parse_monerod_rpc_method(request_method: &Method, request_uri: &Uri, request_body: &Bytes) -> MonerodMethod {
    match *request_method {
        // All get requests go to /request_name, methods do not have a body, optionally could have query params
        // if applicable.
        Method::GET => MonerodMethod::from_str(request_uri.path()).unwrap_or(MonerodMethod::RpcMethodNotDefined),
        // All post requests go to /json_rpc, body of request contains a field `method` to indicate which call
        // takes place.
        Method::POST => {
            let json = serde_json::from_slice::<serde_json::Value>(&request_body[..]).unwrap_or_default();
            if let Some(method) = json["method"].as_str() {
                MonerodMethod::from_str(method).unwrap_or(MonerodMethod::RpcMethodNotDefined)
            } else {
                MonerodMethod::RpcMethodNotDefined
            }
        },
        _ => MonerodMethod::RpcMethodNotDefined,
    }
}

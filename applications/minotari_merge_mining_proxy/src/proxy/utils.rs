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

use bytes::Bytes;
use hyper::{Method, Request, Response};
use minotari_app_grpc::tari_rpc;
use reqwest::ResponseBuilderExt;
use serde_json as json;
use serde_json::json;
use tari_utilities::hex::Hex;

use crate::error::MmProxyError;

/// The JSON object key name used for merge mining proxy response extensions
pub(crate) const MMPROXY_AUX_KEY_NAME: &str = "_aux";

pub async fn convert_reqwest_response_to_hyper_json_response(
    resp: reqwest::Response,
) -> Result<Response<json::Value>, MmProxyError> {
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.extend(resp.headers().iter().map(|(name, value)| (name.clone(), value.clone())));

    builder = builder
        .version(resp.version())
        .status(resp.status())
        .url(resp.url().clone());

    let body = resp.json().await.map_err(MmProxyError::MonerodRequestFailed)?;
    let resp = builder.body(body)?;
    Ok(resp)
}

/// Add mmproxy extensions object to JSON RPC success response
pub fn add_aux_data(mut response: json::Value, mut ext: json::Value) -> json::Value {
    if response["result"].is_null() {
        return response;
    }
    match response["result"][MMPROXY_AUX_KEY_NAME].as_object_mut() {
        Some(obj_mut) => {
            let ext_mut = ext
                .as_object_mut()
                .expect("invalid parameter: expected `ext: json::Value` to be an object but it was not");
            obj_mut.append(ext_mut);
        },
        None => {
            response["result"][MMPROXY_AUX_KEY_NAME] = ext;
        },
    }
    response
}

/// Append chain data to the result object. If the result object is null, a JSON object is created.
///
/// ## Panics
///
/// If response["result"] is not a JSON object type or null.
pub fn append_aux_chain_data(mut response: json::Value, chain_data: json::Value) -> json::Value {
    let result = &mut response["result"];
    if result.is_null() {
        *result = json!({});
    }
    let chains = match result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut() {
        Some(arr_mut) => arr_mut,
        None => {
            result[MMPROXY_AUX_KEY_NAME]["chains"] = json!([]);
            result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut().unwrap()
        },
    };

    chains.push(chain_data);
    response
}

pub fn try_into_json_block_header(header: tari_rpc::BlockHeaderResponse) -> Result<json::Value, MmProxyError> {
    let tari_rpc::BlockHeaderResponse {
        header,
        reward,
        confirmations,
        difficulty,
        num_transactions,
    } = header;
    let header = header.ok_or_else(|| {
        MmProxyError::UnexpectedTariBaseNodeResponse(
            "Base node GRPC returned an empty header field when calling get_header_by_hash".into(),
        )
    })?;

    Ok(json!({
        "block_size": 0,
        "depth": confirmations,
        "difficulty": difficulty,
        "hash": header.hash.to_hex(),
        "height": header.height,
        "major_version": header.version,
        "minor_version": 0,
        "nonce": header.nonce,
        "num_txes": num_transactions,
        // Cannot be an orphan
        "orphan_status": false,
        "prev_hash": header.prev_hash.to_hex(),
        "reward": reward,
        "timestamp": header.timestamp
    }))
}

/// Parse the method name from the request
pub fn parse_method_name(request: &Request<Bytes>) -> String {
    match *request.method() {
        Method::GET => {
            let mut chars = request.uri().path().chars();
            chars.next();
            chars.as_str().to_string()
        },
        Method::POST => {
            let json = json::from_slice::<json::Value>(request.body()).unwrap_or_default();
            str::replace(json["method"].as_str().unwrap_or_default(), "\"", "")
        },
        _ => "unsupported".to_string(),
    }
}

/// Convert a request with a Bytes body to a request with a json Value body
pub fn request_bytes_to_value(request: Request<Bytes>) -> Result<Request<json::Value>, MmProxyError> {
    let json = json::from_slice::<json::Value>(request.body())?;
    Ok(request.map(move |_| json))
}

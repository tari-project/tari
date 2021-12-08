//  Copyright 2021, The Tari Project
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

use json::json;
use serde_json as json;
use tari_app_grpc::tari_rpc as grpc;
use tari_utilities::hex::Hex;

use crate::error::StratumTranscoderProxyError;

/// Create a standard JSON RPC error response
/// More info: https://www.jsonrpc.org/specification#error_object
pub fn standard_error_response(
    req_id: Option<i64>,
    err: jsonrpc::error::StandardError,
    data: Option<json::Value>,
) -> json::Value {
    let data = data.and_then(|value| json::value::to_raw_value(&value).ok());
    let err = jsonrpc::error::standard_error(err, data);
    json!({
        "id":  req_id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "error": err,
    })
}

/// Create a JSON RPC success response
/// More info: https://www.jsonrpc.org/specification#response_object
pub fn success_response(req_id: Option<i64>, result: json::Value) -> json::Value {
    json!({
       "id": req_id.unwrap_or(-1),
       "jsonrpc": "2.0",
       "result": result,
    })
}

/// Create a JSON RPC error response
/// More info: https://www.jsonrpc.org/specification#error_object
pub fn error_response(
    req_id: Option<i64>,
    err_code: i32,
    err_message: &str,
    err_data: Option<json::Value>,
) -> json::Value {
    let mut err = json!({
        "code": err_code,
        "message": err_message,
    });

    if let Some(d) = err_data {
        err["data"] = d;
    }

    json!({
        "id":  req_id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "error": err
    })
}

/// Convert a BlockHeaderResponse into a JSON response
pub(crate) fn try_into_json_block_header_response(
    header: grpc::BlockHeaderResponse,
    request_id: Option<i64>,
) -> Result<json::Value, StratumTranscoderProxyError> {
    let grpc::BlockHeaderResponse {
        header,
        reward,
        confirmations,
        difficulty,
        num_transactions,
    } = header;
    let header = header.ok_or_else(|| {
        StratumTranscoderProxyError::UnexpectedTariBaseNodeResponse(
            "Base node GRPC returned an empty header field when calling get_header_by_hash".into(),
        )
    })?;

    let blockheader = json!({
        "block_size": 0, // TODO
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
            "timestamp": header.timestamp.map(|ts| ts.seconds.into()).unwrap_or_else(|| json!(null)),
    });

    Ok(json!({
        "id": request_id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "result": {
            "blockheader": blockheader.as_object().unwrap(),
        },
        "status": "OK",
    }))
}

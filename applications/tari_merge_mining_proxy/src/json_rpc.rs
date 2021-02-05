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

/// Create a JSON RPC success response
/// More info: https://www.jsonrpc.org/specification#response_object
pub fn success_response(req_id: Option<i64>, result: json::Value) -> json::Value {
    json!({
       "id": req_id.unwrap_or(-1),
       "jsonrpc": "2.0",
       "result": result,
       "status": "OK",
       "untrusted": false
    })
}

/// Create a standard JSON RPC error response
/// More info: https://www.jsonrpc.org/specification#error_object
pub fn standard_error_response(
    req_id: Option<i64>,
    err: jsonrpc::error::StandardError,
    data: Option<json::Value>,
) -> json::Value
{
    let err = jsonrpc::error::standard_error(err, data);
    json!({
        "id":  req_id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "error": err,
    })
}

/// Create a JSON RPC error response
/// More info: https://www.jsonrpc.org/specification#error_object
pub fn error_response(
    req_id: Option<i64>,
    err_code: i32,
    err_message: &str,
    err_data: Option<json::Value>,
) -> json::Value
{
    let mut err = json!({
        "code": err_code,
        "message": err_message,
    });

    if let Some(d) = err_data {
        err["error"]["data"] = d;
    }

    json!({
        "id":  req_id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "error": err
    })
}

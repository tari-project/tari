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

//! Methods for converting responses to json.

use json::json;
use serde_json as json;

/// Default accept response for submit block that goes back to XMRig
/// Refer to XMRig. Do not change existing json values unless it is changed in XMRig.
pub fn default_block_accept_response(req_id: Option<i64>) -> json::Value {
    json!({
       "id": req_id.unwrap_or(-1),
       "jsonrpc": "2.0",
       "result": "{}",
       "status": "OK",
       "untrusted": false,
    })
}

/// Create a JSON RPC success response
/// More info: <https://www.jsonrpc.org/specification#response_object>
pub fn success_response(req_id: Option<i64>, result: json::Value) -> json::Value {
    json!({
       "id": req_id.unwrap_or(-1),
       "jsonrpc": "2.0",
       "result": result,
    })
}

/// Create a standard JSON RPC error response
/// More info: <https://www.jsonrpc.org/specification#error_object>
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

/// Create a JSON RPC error response
/// More info: <https://www.jsonrpc.org/specification#error_object>
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

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    pub fn test_default_block_accept_response() {
        let resp = default_block_accept_response(Some(12));
        assert_eq!(resp["id"], 12);
        assert_eq!(resp["result"], "{}");
        let resp = default_block_accept_response(None);
        assert_eq!(resp["id"], -1);
        assert_eq!(resp["result"], "{}");
    }

    #[test]
    pub fn test_success_response() {
        let result = json::json!({"test key": "test value"});
        let resp = success_response(Some(12), result.clone());
        assert_eq!(resp["id"], 12);
        assert_eq!(resp["result"], result);
        let resp = success_response(None, result.clone());
        assert_eq!(resp["id"], -1);
        assert_eq!(resp["result"], result);
    }

    #[test]
    pub fn test_standard_error_response() {
        let result = json::json!({"test key": "test value"});
        let resp = standard_error_response(
            Some(12),
            jsonrpc::error::StandardError::ParseError,
            Some(result.clone()),
        );
        assert!(!resp["error"].is_null());
        assert_eq!(resp["id"], json::json!(12i64));
        assert_eq!(resp["error"]["data"], result);
        assert_eq!(resp["error"]["message"], "Parse error");
    }

    #[test]
    pub fn test_error_response() {
        let req_id = 12;
        let err_code = 200;
        let err_message = "error message";
        let err_data = json::json!({"test key":"test value"});
        let response = error_response(Some(req_id), err_code, err_message, Some(err_data.clone()));
        assert_eq!(response["id"], req_id);
        assert_eq!(response["error"]["data"], err_data);
        assert_eq!(response["error"]["code"], err_code);
        assert_eq!(response["error"]["message"], err_message);
        let response = error_response(None, err_code, err_message, None);
        assert_eq!(response["id"], -1);
        assert!(response["error"]["data"].is_null());
        assert_eq!(response["error"]["code"], err_code);
        assert_eq!(response["error"]["message"], err_message);
    }
}

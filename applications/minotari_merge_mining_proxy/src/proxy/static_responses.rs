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

use log::trace;
use tracing::debug;
use url::Url;

use crate::{common::json_rpc, error::MmProxyError, proxy::monerod_method::MonerodMethod};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::static_responses";

struct StaticResponse {
    headers: hyper::HeaderMap,
    version: hyper::Version,
    status: hyper::StatusCode,
    body: serde_json::Value,
}

#[allow(clippy::too_many_lines)]
fn get_static_monerod_response(
    method: MonerodMethod,
    req_id: Option<i64>,
    height: Option<i64>,
    hash: Option<String>,
) -> StaticResponse {
    match method {
        MonerodMethod::GetHeight => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "hash": hash.unwrap_or("98f83f921a006ccb8ab14ec7e7245e4a4350471027b4d490c41e8d84e4b8a196".to_string()),
                "height": height.unwrap_or(3331664),
                "status": "OK",
                "untrusted": false
            }),
        },
        MonerodMethod::GetVersion => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "id": req_id.unwrap_or(-1),
                "jsonrpc": "2.0",
                "result": {
                    "current_height": height.unwrap_or(3331664),
                    "hard_forks": [
                        {"height": 1, "hf_version": 1},
                        {"height": 1009827, "hf_version": 2},
                        {"height": 1141317, "hf_version": 3},
                        {"height": 1220516, "hf_version": 4},
                        {"height": 1288616, "hf_version": 5},
                        {"height": 1400000, "hf_version": 6},
                        {"height": 1546000, "hf_version": 7},
                        {"height": 1685555, "hf_version": 8},
                        {"height": 1686275, "hf_version": 9},
                        {"height": 1788000, "hf_version": 10},
                        {"height": 1788720, "hf_version": 11},
                        {"height": 1978433, "hf_version": 12},
                        {"height": 2210000, "hf_version": 13},
                        {"height": 2210720, "hf_version": 14},
                        {"height": 2688888, "hf_version": 15},
                        {"height": 2689608, "hf_version": 16}
                    ],
                    "release": true,
                    "status": "OK",
                    "untrusted": false,
                    "version": 196622
                }
            }),
        },
        MonerodMethod::GetBlockTemplate => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "id": req_id.unwrap_or(-1),
                "jsonrpc": "2.0",
                "result": {
                    "blockhashing_blob": "101089c3dcbc060bf216adb952642f1062e0edce81ec3694d7e8c70d823da2cf9aa1b326c64faa000000005f16ead697b2787a034270bfc29708590a68690087342a468a372fa9dbc108e811",
                    "blocktemplate_blob": "101089c3dcbc060bf216adb952642f1062e0edce81ec3694d7e8c70d823da2cf9aa1b326c64faa0000000002c8c1cb0101ff8cc1cb0101a09cd5c0d51103d663ad8819203aa8f64b3106dc6b66eef3cb8d151673456e95f57834261cf960d82b01aca79d68243304d560a499fba4c93df92e9a6535d88cdb9d9e745cf79d34fe8b0208ecf0efe9283a3ad50010b705639fc17f12e0ae7e6cb900509465919b5986d77800abcb756c8f61626570429c39fe4a3b325facbef13aebbc85ece3ad757fa3f6dc289ee083a5157f4dbb887d1437875bd6020c15ec61542e636a75e49c995364a01867ab64bfd09fdb50aff6b30abed4b34cb394e3819839934a01c7c1fa215fcffc3421e433ae1fef35c89243cba33ae5b3dd87f71c0485282a9e4b57f541227f40e2382ea60cf898038f47deaded65b0b49ff46c8de7abbf95a76379343b95d062b70b3ece5fa15573f2ac7bad1c3b601f62025e3c7da113f3a2f4a984cfe919f1d62105e229ee93f3ba218ad054f03d5a563f72c8a10874f5fe0b867dcaa4f9eca117652c8620ccfd8d7b0a74cda3b2a0e801192d175b97c7bf65ac41ebff453c6c44d6047a878992dc6f23d7eaac223eb4f11506f6364df254f7918ae72b37eabef884e0331de9d343c3ad6b97cc40ffbc2b007bff614090fc71206d2ea2fbcfd0702bef85302d9d5d3fca67470aef2db5ac9fc79c3ede72183b1ce48f741e204a0140579c88b6d48c39f926ebc143805c062309a878afb4a006c0a7fa11d236770f54f4d3e14a92f7aa2ee2e5e16bb25c952ea14336e2f597a6530ae98c1bf5dd4421cd3ed8e7e8572e8a1cc2f16e92f0cca38a384e43d9e2fb41730bcf619aa797f5ed0da482cc7e4b2641ebec1479e563645f7dad2d3fc6b2f8218cb6653bb6923acd7cd62f32",
                    "difficulty": 460784537709_i64,
                    "difficulty_top64": 0,
                    "expected_reward": 607068180000_i64,
                    "height": height.unwrap_or(3334284),
                    "next_seed_hash": "",
                    "prev_hash": "0bf216adb952642f1062e0edce81ec3694d7e8c70d823da2cf9aa1b326c64faa",
                    "reserved_offset": 131,
                    "seed_hash": "1074eda86e47631a44c38cfbdc4ff81b452ba036a217025393fc202ad9911552",
                    "seed_height": (height.unwrap_or(3334284) - 140),
                    "status": "OK",
                    "untrusted": false,
                    "wide_difficulty": "0x6b48e6106d"
                }
            }),
        },
        MonerodMethod::SubmitBlock => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "error": {
                    "code": -7,
                    "message": "Block not accepted"
                },
                "id": req_id.unwrap_or(-1),
                "jsonrpc": "2.0"
            }),
        },
        MonerodMethod::GetBlockHeaderByHash => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "error": {
                    "code": -5,
                    "message": "Internal error: can't get block by hash."
                },
                "id": req_id.unwrap_or(-1),
                "jsonrpc": "2.0"
            }),
        },
        MonerodMethod::GetLastBlockHeader => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "error": {
                    "code": -5,
                    "message": "Internal error: can't get last block header."
                },
                "id": req_id.unwrap_or(-1),
                "jsonrpc": "2.0"
            }),
        },
        MonerodMethod::RpcMethodNotDefined => StaticResponse {
            headers: hyper::HeaderMap::new(),
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::BAD_REQUEST,
            body: serde_json::json!({"error": "Unknown method"}),
        },
    }
}

pub(crate) fn convert_static_monerod_response_to_hyper_response(
    method: MonerodMethod,
    req_id: Option<i64>,
    height: Option<i64>,
    hash: Option<String>,
) -> Result<hyper::Response<serde_json::Value>, MmProxyError> {
    trace!(
        target: LOG_TARGET,
        "[monerod] use static response for {}, req_id, {:?}, height, {:?}, hash{:?}",
        method, req_id, height, hash
    );
    let static_response = get_static_monerod_response(method, req_id, height, hash);

    let mut builder = hyper::Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.extend(
        static_response
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );

    builder = builder.version(static_response.version).status(static_response.status);

    let resp = builder.body(static_response.body)?;
    Ok(resp)
}

/// This is required for self-select configuration if the request is a block submission and we are not submitting blocks
/// to the origin (self-select mode)
pub(crate) fn self_select_submit_block_monerod_response(request_id: Option<i64>) -> serde_json::Value {
    debug!(
        target: LOG_TARGET,
        "[monerod] skip: Proxy configured for self-select mode. Pool will submit to MoneroD, submitting to \
         Minotari.",
    );

    // We are not submitting the block to Monero here (the pool does this),
    // we are only interested in intercepting the request for the purposes of
    // submitting the block to Tari which will only happen if the accept response
    // (which normally would occur for normal mining) is provided here.
    // There is no point in trying to submit the block to Monero here since the
    // share submitted by XMRig is only guaranteed to meet the difficulty of
    // min(Tari,Monero) since that is what was returned with the original template.
    // So it would otherwise be a duplicate submission of what the pool will do
    // itself (whether the miner submits directly to monerod or the pool does,
    // the pool is the only one being paid out here due to the nature
    // of self-select). Furthermore, discussions with devs from Monero and XMRig are
    // very much against spamming the nodes unnecessarily.
    // NB!: This is by design, do not change this without understanding
    // it's implications.
    json_rpc::default_block_accept_response(request_id)
}

pub(crate) fn static_json_rpc_url() -> Url {
    Url::parse("http://82.64.166.200:18081/json_rpc").expect("Invalid URL")
}

#[cfg(test)]
mod test {
    use hyper::HeaderMap;
    use serde_json::json;
    use url::Url;

    use crate::proxy::{
        monerod_method::MonerodMethod,
        static_responses::{
            convert_static_monerod_response_to_hyper_response,
            get_static_monerod_response,
            self_select_submit_block_monerod_response,
            static_json_rpc_url,
        },
        test,
    };

    // To execute this test a merge mining proxy must be running (just verify the port, default used), together with a
    // base node.
    #[tokio::test]
    #[ignore]
    async fn get_new_monerod_static_responses() {
        let json_rpc_port = 18181;
        let mut responses = Vec::with_capacity(50);
        println!();
        for method in [
            MonerodMethod::GetHeight,
            MonerodMethod::GetVersion,
            MonerodMethod::GetBlockTemplate,
            MonerodMethod::SubmitBlock,
            MonerodMethod::GetBlockHeaderByHash,
            MonerodMethod::GetLastBlockHeader,
        ] {
            test::inner_json_rpc(method, json_rpc_port, &mut responses, 1, true).await;
        }
        println!();
        for response in responses {
            println!("{}", response);
        }
    }

    fn headers_to_json(headers: &HeaderMap) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (key, value) in headers {
            map.insert(
                key.to_string(),
                serde_json::Value::String(value.to_str().unwrap().to_string()),
            );
        }
        serde_json::Value::Object(map)
    }

    #[test]
    fn test_monerod_static_responses() {
        for method in [
            MonerodMethod::GetHeight,
            MonerodMethod::GetVersion,
            MonerodMethod::GetBlockTemplate,
            MonerodMethod::SubmitBlock,
            MonerodMethod::GetBlockHeaderByHash,
            MonerodMethod::GetLastBlockHeader,
            MonerodMethod::RpcMethodNotDefined,
        ] {
            let static_hyper_response = convert_static_monerod_response_to_hyper_response(
                method,
                Some(123),
                Some(3331664),
                Some("98f83f921a006ccb8ab14ec7e7245e4a4350471027b4d490c41e8d84e4b8a196".to_string()),
            )
            .unwrap();
            let get_static = get_static_monerod_response(
                method,
                Some(123),
                Some(3331664),
                Some("98f83f921a006ccb8ab14ec7e7245e4a4350471027b4d490c41e8d84e4b8a196".to_string()),
            );

            // Version
            assert_eq!(static_hyper_response.version(), get_static.version);

            // Status
            assert_eq!(static_hyper_response.status(), get_static.status);

            // Headers
            assert_eq!(
                headers_to_json(static_hyper_response.headers()),
                headers_to_json(&get_static.headers)
            );

            // Body
            assert_eq!(static_hyper_response.body(), &get_static.body);
        }

        let monerod_response = self_select_submit_block_monerod_response(Some(123));
        assert_eq!(
            monerod_response,
            json!({
               "id": 123,
               "jsonrpc": "2.0",
               "result": "{}",
               "status": "OK",
               "untrusted": false,
            })
        );

        assert_eq!(
            static_json_rpc_url(),
            Url::parse("http://82.64.166.200:18081/json_rpc").unwrap()
        );
    }
}

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

use std::str::FromStr;

use log::trace;
use monero::{
    blockdata::{
        transaction,
        transaction::{
            ExtraField,
            RawExtraField,
            SubField::{Nonce, TxPublicKey},
        },
    },
    util::ringct::Key,
    Block,
    BlockHeader,
    Hash,
    PublicKey,
    Transaction,
    TransactionPrefix,
    VarInt,
};
use tari_core::proof_of_work::monero_rx;
use tracing::debug;
use url::Url;

use crate::{
    common::json_rpc,
    error::MmProxyError,
    proxy::{inner::MonerodCacheValues, monerod_method::MonerodMethod},
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::static_responses";

struct StaticResponse {
    headers: hyper::HeaderMap,
    version: hyper::Version,
    status: hyper::StatusCode,
    body: serde_json::Value,
}

// Default metadata for Monero block at height 3336491
pub(crate) const BLOCK_HEIGHT_3336491: u64 = 3336491;
pub(crate) const BLOCK_HASH_AT_3336491: &str = "a1b9f62c45d67d5e2acb21efcaf8804d3674005190152d11b6f04d80acf8013c";
pub(crate) const TIMESTAMP_AT_3336491: u64 = 1738223139;
const SEED_HEIGHT_AT_3336491: u64 = 3336192;
const SEED_HASH_AT_3336491: &str = "91ef83186cefaa646dc4c6e950e68e4debab52b4f4a9b7f465891e91fe5f6ce4";
const DIFFICULTY_AT_3336491: u64 = 490520097899;
const REWARD_AT_3336491: u64 = 600741780000;
const WIDE_DIFFICULTY_AT_3336491: &str = "0x6df86346d3";
const MAX_HF_VERSION: u64 = 16;

// These static monerod responses can be captured using the merge mining proxy connected to XMRig and monerod as the
// last log entry in `applications/minotari_merge_mining_proxy/src/proxy/inner.rs` method
// `async fn proxy_request_to_monerod(`. Also see `get_new_monerod_static_responses()` test function in this file.
#[allow(clippy::too_many_lines)]
fn get_static_monerod_response(
    method: MonerodMethod,
    req_id: Option<i64>,
    monerod_cache_values: Option<MonerodCacheValues>,
) -> Result<StaticResponse, MmProxyError> {
    let req_id = req_id.unwrap_or(-1);
    let (height, hash, timestamp, seed_height, seed_hash) = if let Some(cache_values) = monerod_cache_values {
        (
            cache_values.height,
            cache_values.prev_hash,
            cache_values.timestamp.unwrap_or(TIMESTAMP_AT_3336491),
            cache_values.seed_height.unwrap_or(SEED_HEIGHT_AT_3336491),
            if let Some(hash) = cache_values.seed_hash {
                &hex::encode(hash)
            } else {
                SEED_HASH_AT_3336491
            },
        )
    } else {
        (
            BLOCK_HEIGHT_3336491,
            Hash::from_str(BLOCK_HASH_AT_3336491).expect("will not fail"),
            TIMESTAMP_AT_3336491,
            SEED_HEIGHT_AT_3336491,
            SEED_HASH_AT_3336491,
        )
    };

    let response = match method {
        // If hash and height are not provided, this will return the Monero hash for block 3336491
        MonerodMethod::GetHeight => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                // The monero hash for blockchain at height 3331664
                "hash": hex::encode(hash),
                "height": height,
                "status": "OK",
                "untrusted": false
            }),
        },
        // This was the 'get_version' response for monero blockchain height 3336491. A custom 'height' can be provided,
        // but hard fork and version information will go out of date at the next hard fork.
        MonerodMethod::GetVersion => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "id": req_id,
                "jsonrpc": "2.0",
                "result": {
                    "current_height": height,
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
        // This will return an empty 'get_block_template' response for monero blockchain height 'height + 1'. Dynamic
        // values are used as far as possible, for example 'hash' and 'timestamp' must be supplied.
        MonerodMethod::GetBlockTemplate => {
            let monero_block = get_empty_monero_block(hash, timestamp, height + 1, None);
            let blockhashing_blob = monero_rx::create_blockhashing_blob_from_block(&monero_block)?;
            let blocktemplate_blob = monero_rx::serialize_monero_block_to_hex(&monero_block)?;

            StaticResponse {
                headers: {
                    let mut headers = hyper::HeaderMap::new();
                    headers.insert("content-type", "application/json".parse().unwrap());
                    headers
                },
                version: hyper::Version::HTTP_11,
                status: hyper::StatusCode::OK,
                body: serde_json::json!({
                    "id": req_id,
                    "jsonrpc": "2.0",
                    // The 'get_block_template' response for monero blockchain height 3334284
                    "result": {
                        "blockhashing_blob": blockhashing_blob,
                        "blocktemplate_blob": blocktemplate_blob,
                        "difficulty": DIFFICULTY_AT_3336491,
                        "difficulty_top64": 0,
                        "expected_reward": REWARD_AT_3336491,
                        "height": height,
                        "next_seed_hash": "",
                        "prev_hash": hex::encode(monero_block.header.prev_id),
                        "reserved_offset": 0,
                        "seed_hash": seed_hash,
                        "seed_height": seed_height,
                        "status": "OK",
                        "untrusted": false,
                        "wide_difficulty": WIDE_DIFFICULTY_AT_3336491
                    }
                }),
            }
        },
        // This return an error response by design, as we can never construct a static block that will be accepted, and
        // most if not all cases a block solved while merge mining will not be accepted by the Monero network.
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
                "id": req_id,
                "jsonrpc": "2.0"
            }),
        },
        // This return an error response by design, as it is impossible to return the correct header or block
        // corresponding to 1 of millions of correct answers while offline.
        MonerodMethod::GetBlockHeaderByHash | MonerodMethod::GetBlock => StaticResponse {
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
                    "message": &format!("Internal error: can't get block by hash '{}'.", method)
                },
                "id": req_id,
                "jsonrpc": "2.0"
            }),
        },
        // This return a fixed header by design, as it is impossible to return the correct header corresponding to
        // the last block while offline.
        MonerodMethod::GetLastBlockHeader => StaticResponse {
            headers: {
                let mut headers = hyper::HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers
            },
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::OK,
            body: serde_json::json!({
                "id": req_id,
                "jsonrpc": "2.0",
                "result": {
                    "block_header": {
                        "block_size": 3865,
                        "block_weight": 3865,
                        "cumulative_difficulty": 418129042015270429u64,
                        "cumulative_difficulty_top64": 0,
                        "depth": 0,
                        "difficulty": DIFFICULTY_AT_3336491,
                        "difficulty_top64": 0,
                        "hash": BLOCK_HASH_AT_3336491,
                        "height": height,
                        "long_term_weight": 176470,
                        "major_version": MAX_HF_VERSION,
                        "miner_tx_hash": "8112cdbbd21a99a347386d03e0798d095a356ddde84ebb574011cb8cc33c200f",
                        "minor_version": MAX_HF_VERSION,
                        "nonce": 67153,
                        "num_txes": 38,
                        "orphan_status": false,
                        "pow_hash": "",
                        "prev_hash": &hex::encode(hash),
                        "reward": REWARD_AT_3336491,
                        "timestamp": timestamp,
                        "wide_cumulative_difficulty": "0x5cd7e25fb98361d",
                        "wide_difficulty": WIDE_DIFFICULTY_AT_3336491
                    },
                    "credits": 0,
                    "status": "OK",
                    "top_hash": "",
                    "untrusted": false
                }
            }),
        },
        MonerodMethod::RpcMethodNotDefined => StaticResponse {
            headers: hyper::HeaderMap::new(),
            version: hyper::Version::HTTP_11,
            status: hyper::StatusCode::BAD_REQUEST,
            body: serde_json::json!({"error": "Unknown method"}),
        },
    };

    Ok(response)
}

// Monero block with only the miner transaction and no other transactions - miner data correspond to block 3336491
pub(crate) fn get_empty_monero_block(
    prev_id: Hash,
    timestamp: u64,
    height: u64,
    merge_mining_tag: Option<transaction::SubField>,
) -> Block {
    // Miner transaction data for block 3336491
    const TX_KEY: &str = "67399a3d8caf949713cf3aae1f4027b29a8df626a167ed84aef2c011e3a9ff5f";
    const PUBLIC_KEY: &str = "9785629f62f7688cd7fc7025d1c6837a818fc5d09c0a2adb4f77545cfe57fb6b";
    const NONCE: &str = "115fe80c4c8a36c100000000000000000000";

    let key = Key::from(Hash::from_str(TX_KEY).unwrap().to_bytes()).key;

    let mut sub_fields = vec![
        TxPublicKey(PublicKey::from_str(PUBLIC_KEY).unwrap()),
        Nonce(hex::decode(NONCE).unwrap()),
    ];
    if let Some(tag) = merge_mining_tag {
        sub_fields.insert(0, tag);
    }
    let extra = RawExtraField::from(ExtraField(sub_fields));

    Block {
        header: BlockHeader {
            major_version: VarInt(MAX_HF_VERSION),
            minor_version: VarInt(MAX_HF_VERSION),
            timestamp: VarInt(timestamp),
            prev_id,
            nonce: 0,
        },
        // This is an arbitrary miner transaction
        miner_tx: Transaction {
            prefix: TransactionPrefix {
                version: VarInt(2),
                unlock_time: VarInt(height + 60),
                inputs: vec![transaction::TxIn::Gen { height: VarInt(height) }],
                outputs: vec![transaction::TxOut {
                    amount: VarInt(600741780000),
                    target: transaction::TxOutTarget::ToTaggedKey { key, view_tag: 223 },
                }],
                extra,
            },
            signatures: vec![],
            rct_signatures: monero::util::ringct::RctSig {
                sig: Some(monero::util::ringct::RctSigBase {
                    rct_type: monero::util::ringct::RctType::Null,
                    txn_fee: monero::util::amount::Amount::ZERO,
                    pseudo_outs: vec![],
                    ecdh_info: vec![],
                    out_pk: vec![],
                }),
                p: None,
            },
        },
        tx_hashes: vec![
            Hash::from_str("6893e92efa26b95975f96c493de78600e2aac40b833552421ebe579d67b7b6ec").expect("will not fail"),
            Hash::from_str("ddbeb9bc923255a3117c1483c14449bf459fea824ab13917516d3863d89e5d6a").expect("will not fail"),
        ],
    }
}

pub(crate) fn convert_static_monerod_response_to_hyper_response(
    method: MonerodMethod,
    req_id: Option<i64>,
    monerod_cache_values: Option<MonerodCacheValues>,
) -> Result<hyper::Response<serde_json::Value>, MmProxyError> {
    if let Some(cache_values) = monerod_cache_values.clone() {
        trace!(
            target: LOG_TARGET,
            "[monerod] use static response for {}, req_id: {:?}, height: {:?}, prev_hash: {:?}, timestamp: {:?}, \
            seed_height: {:?}, seed_hash: {:?}",
            method, req_id,
            cache_values.height,
            cache_values.prev_hash,
            cache_values.timestamp,
            cache_values.seed_height,
            cache_values.seed_hash.map(hex::encode),
        );
    } else {
        trace!(target: LOG_TARGET, "[monerod] use static response for {}, req_id: {:?}", method, req_id);
    }
    let static_response = get_static_monerod_response(method, req_id, monerod_cache_values)?;

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
    use std::str::FromStr;

    use hyper::HeaderMap;
    use monero::{blockdata::transaction::SubField, Hash, VarInt};
    use regex::Regex;
    use serde_json::json;
    use tari_core::proof_of_work::monero_rx;
    use url::Url;

    use crate::proxy::{
        inner::MonerodCacheValues,
        monerod_method::MonerodMethod,
        static_responses::{
            convert_static_monerod_response_to_hyper_response,
            get_empty_monero_block,
            get_static_monerod_response,
            self_select_submit_block_monerod_response,
            static_json_rpc_url,
            BLOCK_HASH_AT_3336491,
            MAX_HF_VERSION,
        },
        test,
    };

    fn extract_error_json(response: &str) -> Option<serde_json::Value> {
        let re = Regex::new(r"Error: '.*?' failed \((\{.*\})\)").unwrap();
        if let Some(captures) = re.captures(response) {
            if let Some(json_str) = captures.get(1) {
                let json_value: serde_json::Value = serde_json::from_str(json_str.as_str()).unwrap();
                return Some(json_value);
            }
        }
        None
    }

    // To execute this test a merge mining proxy must be running (just verify the port, default used), together with a
    // base node. This function can also be used to capture monerod responses.
    // Note: `config.monerod_fallback` must be `MonerodOnly` or `StaticWhenMonerodFails`
    #[tokio::test]
    #[ignore]
    async fn get_monerod_dynamic_responses() {
        let json_rpc_port = 18081;
        let mut responses = Vec::with_capacity(50);

        println!();
        for method in [
            MonerodMethod::GetHeight,
            MonerodMethod::GetVersion,
            MonerodMethod::GetBlockTemplate,
            MonerodMethod::SubmitBlock,
            MonerodMethod::GetBlockHeaderByHash,
            MonerodMethod::GetLastBlockHeader,
            MonerodMethod::GetBlock,
        ] {
            let block_hash = if method == MonerodMethod::GetBlockHeaderByHash || method == MonerodMethod::GetBlock {
                Some(BLOCK_HASH_AT_3336491.to_string())
            } else {
                None
            };
            test::inner_json_rpc(method, json_rpc_port, &mut responses, 1, true, block_hash.clone(), None).await;
            // Investigate the responses
            let json: serde_json::Value = if responses[responses.len() - 1].contains(" response body: ") {
                let response = responses[responses.len() - 1]
                    .split(" response body: ")
                    .collect::<Vec<&str>>()[1];
                serde_json::from_str(response).unwrap_or(serde_json::Value::Null)
            } else {
                extract_error_json(&responses[responses.len() - 1]).unwrap_or(serde_json::Value::Null)
            };
            match method {
                MonerodMethod::GetVersion => {
                    // Assert no error
                    assert_eq!(json["error"], json!(null));
                    // Verify the hard fork version information is still up to date
                    let max_hf_version = get_max_hf_version(json);
                    assert_eq!(max_hf_version, MAX_HF_VERSION);
                },
                MonerodMethod::GetHeight |
                MonerodMethod::GetBlockTemplate |
                MonerodMethod::GetBlockHeaderByHash |
                MonerodMethod::GetLastBlockHeader |
                MonerodMethod::GetBlock => {
                    // Assert no error
                    assert_eq!(json["error"], json!(null));
                },
                MonerodMethod::SubmitBlock => {
                    // Assert error
                    assert_ne!(json["error"], json!(null));
                },
                MonerodMethod::RpcMethodNotDefined => {},
            }
        }

        // Manipulate the first numeric character of the hash that is less than 9 to get a different hash (we want
        // "get_block_header_by_hash" to fail)
        let mut hash = BLOCK_HASH_AT_3336491.to_string();
        if let Some(pos) = hash
            .chars()
            .position(|c| c.is_ascii_digit() && c.to_digit(10).unwrap() < 9)
        {
            let mut chars: Vec<char> = hash.chars().collect();
            chars[pos] = std::char::from_digit(chars[pos].to_digit(10).unwrap() + 1, 10).unwrap();
            hash = chars.into_iter().collect();
        }
        let method = MonerodMethod::GetBlockHeaderByHash;
        test::inner_json_rpc(method, json_rpc_port, &mut responses, 1, true, Some(hash), None).await;
        if let Some(json) = extract_error_json(&responses[responses.len() - 1]) {
            // Assert error
            assert_ne!(json["error"], json!(null));
        } else {
            panic!("Expected error response");
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
            MonerodMethod::GetBlock,
            MonerodMethod::RpcMethodNotDefined,
        ] {
            let static_hyper_response = convert_static_monerod_response_to_hyper_response(
                method,
                Some(123),
                Some(MonerodCacheValues {
                    height: 3331664,
                    prev_hash: Hash::from_str("98f83f921a006ccb8ab14ec7e7245e4a4350471027b4d490c41e8d84e4b8a196")
                        .unwrap(),
                    timestamp: Some(12345678),
                    seed_height: None,
                    seed_hash: None,
                }),
            )
            .unwrap();
            let static_response = get_static_monerod_response(
                method,
                Some(123),
                Some(MonerodCacheValues {
                    height: 3331664,
                    prev_hash: Hash::from_str("98f83f921a006ccb8ab14ec7e7245e4a4350471027b4d490c41e8d84e4b8a196")
                        .unwrap(),
                    timestamp: Some(12345678),
                    seed_height: None,
                    seed_hash: None,
                }),
            )
            .unwrap();

            // Version
            assert_eq!(static_hyper_response.version(), static_response.version);

            // Status
            assert_eq!(static_hyper_response.status(), static_response.status);

            // Headers
            assert_eq!(
                headers_to_json(static_hyper_response.headers()),
                headers_to_json(&static_response.headers)
            );

            // Body
            assert_eq!(static_hyper_response.body(), &static_response.body);

            if method == MonerodMethod::GetBlockTemplate {
                let (_parts, monerod_resp) = static_hyper_response.into_parts();
                let blocktemplate_blob = monerod_resp["result"]["blocktemplate_blob"]
                    .to_string()
                    .replace('\"', "");
                assert!(monero_rx::deserialize_monero_block_from_hex(&blocktemplate_blob).is_ok());
            }
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

    #[test]
    fn test_get_empty_block() {
        let prev_id = Hash::from_str("840915066009f63da3bf1160ce0ac3b2a57865d0b9329dcbf9ae1627200987d7").unwrap();
        let timestamp = 1234567890;
        let height = 123456;
        let block = get_empty_monero_block(prev_id, timestamp, height, None);
        let extra = block.miner_tx.prefix.extra.try_parse();
        for field in &extra.0 {
            if let SubField::MergeMining(..) = field {
                panic!("Merge mining tag should not be present");
            }
        }

        assert_eq!(block.header.prev_id, prev_id);
        assert_eq!(block.header.timestamp, VarInt(timestamp));
        assert_eq!(block.header.major_version, VarInt(MAX_HF_VERSION));
        assert_eq!(block.header.minor_version, VarInt(MAX_HF_VERSION));

        let blocktemplate_blob = monero_rx::serialize_monero_block_to_hex(&block).unwrap();
        assert!(monero_rx::deserialize_monero_block_from_hex(&blocktemplate_blob).is_ok());

        const ARBITRARY_MERGE_MINING_HASH: &str = "8e6dab82d22909b40bda27ec0e96aa0c6c0012023d353f3be941eb6ef1793cad";
        let merge_mining_tag = Some(SubField::MergeMining(
            VarInt(0),
            Hash::from_str(ARBITRARY_MERGE_MINING_HASH).expect("will not fail"),
        ));
        let block = get_empty_monero_block(prev_id, timestamp, height, merge_mining_tag);
        let extra = block.miner_tx.prefix.extra.try_parse();
        let mut found_merge_mining_tag = false;
        for field in &extra.0 {
            if let SubField::MergeMining(..) = field {
                found_merge_mining_tag = true;
            }
        }
        assert!(found_merge_mining_tag);
    }

    fn get_max_hf_version(json: serde_json::Value) -> u64 {
        json["result"]["hard_forks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hf| hf["hf_version"].as_u64().unwrap())
            .max()
            .unwrap()
    }

    #[test]
    fn test_hf_version() {
        let get_static = get_static_monerod_response(MonerodMethod::GetVersion, Some(123), None).unwrap();
        let max_hf_version = get_max_hf_version(get_static.body);
        assert_eq!(max_hf_version, MAX_HF_VERSION);
    }
}

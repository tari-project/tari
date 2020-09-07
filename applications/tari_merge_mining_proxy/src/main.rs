// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod error;

extern crate arrayref;
extern crate chrono;
extern crate jsonrpc;
use chrono::Local;
use config;
use curl::easy::{Auth, Easy, List};
use structopt::StructOpt;
use tari_common::{ConfigBootstrap, ConfigError, GlobalConfig};

// TODO: Log to file
use crate::error::MmProxyError;
use json::JsonValue;
use log::*;
use monero::{
    blockdata::Block,
    consensus::deserialize,
    cryptonote::hash::{Hash, Hashable},
};
use rand::rngs::OsRng;
use regex::Regex;
use std::{
    convert::TryFrom,
    io::{prelude::*, Read},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};
use tari_app_grpc::tari_rpc as grpc;
use tari_common::configuration::bootstrap::ApplicationType;
use tari_core::{
    blocks::NewBlockTemplate,
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    proof_of_work::monero_rx::{
        append_merge_mining_tag,
        create_input_blob,
        from_hashes,
        from_slice,
        tree_hash,
        MoneroData,
    },
    transactions::{types::CryptoFactories, CoinbaseBuilder},
};
use tari_crypto::{keys::SecretKey, ristretto::RistrettoSecretKey};
use tokio::runtime::Runtime;

pub type PrivateKey = RistrettoSecretKey;

pub const LOG_TARGET: &str = "tari_mm_proxy::app";

#[derive(Clone, Debug)]
pub struct TransientData {
    tari_block: Option<grpc::GetNewBlockResult>,
    monero_seed: Option<String>,
    tari_height: Option<u64>,
    tari_prev_submit_height: Option<u64>,
}

// setup curl authentication
fn base_curl_auth(curl: &mut Easy, config: &GlobalConfig) -> Result<(), MmProxyError> {
    curl.username(config.curl_username.as_str())
        .map_err(|_| MmProxyError::CurlError("Could not set username".to_string()))?;
    curl.password(config.curl_password.as_str())
        .map_err(|_| MmProxyError::CurlError("Could no set password".to_string()))?;
    let mut auth = Auth::new();
    auth.basic(true);
    curl.http_auth(&auth)
        .map_err(|_| MmProxyError::CurlError("Could not set auth".to_string()))?;
    Ok(())
}

// Set up curl
fn base_curl(len: u64, url: &str, post: bool, config: &GlobalConfig) -> Result<Easy, MmProxyError> {
    let mut easy = Easy::new();
    easy.url(url)
        .map_err(|_| MmProxyError::CurlError("Could not set URL".to_string()))?;
    let mut list = List::new();
    list.append("Content-Type: application/json")
        .map_err(|_| MmProxyError::OtherError("Could not append to list".to_string()))?;
    easy.http_headers(list)
        .map_err(|_| MmProxyError::CurlError("Could not set http header".to_string()))?;
    if config.curl_use_auth {
        base_curl_auth(&mut easy, config)?;
    }
    if post {
        easy.post(true)
            .map_err(|_| MmProxyError::CurlError("Curl missing".to_string()))?;
        easy.post_field_size(len)
            .map_err(|_| MmProxyError::CurlError("Curl missing".to_string()))?;
    }
    Ok(easy)
}

// Perform curl request to monerod api
fn do_curl(curl: &mut Easy, request: &[u8]) -> Result<Vec<u8>, MmProxyError> {
    let mut transfer_data = request;
    let mut data = Vec::new();
    {
        let mut transfer = curl.transfer();

        transfer
            .read_function(|buf| Ok(transfer_data.read(buf).unwrap_or(0)))
            .map_err(|e| MmProxyError::CurlError(format!("Failure to assign read function, {}", e)))?;

        transfer
            .write_function(|new_data| {
                data.extend_from_slice(new_data);
                Ok(new_data.len())
            })
            .map_err(|e| MmProxyError::CurlError(format!("Failure to assign write function, {}", e)))?;

        transfer
            .perform()
            .map_err(|e| MmProxyError::CurlError(format!("Request failure, {}", e)))?;
    }
    Ok(data)
}

// Structure http header and body
// TODO: Error headers
fn structure_response(response_data: &[u8], code: u32) -> String {
    let header = format!(
        "HTTP/1.1 {} \
         OK\r\nAccept-Ranges:bytes\r\nContent-Length:{}\r\nContent-Type:application/json\r\nServer:Epee-based\r\n\r\n",
        code,
        String::from_utf8_lossy(response_data).len()
    );
    format!("{}{}", header, String::from_utf8_lossy(response_data))
}

// Get the url bits from http header
fn get_url_part(request: &[u8]) -> String {
    let string = String::from_utf8_lossy(&request[..]).to_string();
    let mut split_request = string.lines();
    let first_line = split_request.next().unwrap_or_default().to_string();
    let mut iter = first_line.split_whitespace();
    iter.next().unwrap_or_default();
    iter.next().unwrap_or_default().to_string()
}

// Get the request type from http header
fn get_request_type(request: &[u8]) -> String {
    let string = String::from_utf8_lossy(&request[..]).to_string();
    let mut split_request = string.lines();
    let first_line = split_request.next().unwrap_or_default().to_string();
    let mut iter = first_line.split_whitespace();
    iter.next().unwrap_or_default().to_string()
}

// Extract json from response
fn get_json(request: &[u8]) -> Option<Vec<u8>> {
    match Regex::new(r"\{(.*)\}") {
        Ok(re) => {
            let string = stringify_request(request);
            let caps = re.captures(&string);
            match caps {
                Some(caps) => {
                    match caps.get(0) {
                        Some(json) => {
                            let result = json.as_str().as_bytes().to_vec();
                            Some(result)
                        },
                        None => {
                            // Request was malformed.
                            debug!(target: LOG_TARGET, "Malformed JSON Request");
                            None
                        },
                    }
                },
                None => {
                    // Request didn't contain any json.
                    debug!(target: LOG_TARGET, "No JSON Request: {}", string);
                    None
                },
            }
        },
        Err(_) => None,
    }
}

// Convert json bytes to json string
fn stringify_request(buffer: &[u8]) -> String {
    String::from_utf8_lossy(&buffer).to_string()
}

// Get method name from post request
fn get_post_json_method(json: &[u8]) -> String {
    let parsed = json::parse(&stringify_request(json)).unwrap_or_else(|_| JsonValue::Null);
    trace!(target: LOG_TARGET, "{}", parsed);
    if parsed["method"].is_null() {
        return "".to_string();
    }
    parsed["method"].to_string()
}

// Get id from request
fn get_request_id(json: &[u8]) -> i64 {
    let parsed = json::parse(&stringify_request(json)).unwrap_or_else(|_| JsonValue::Null);
    trace!(target: LOG_TARGET, "{}", parsed);
    if parsed["id"].is_null() {
        return -1;
    }
    parsed["id"].as_i64().unwrap_or_else(|| -1)
}

fn get_monero_data(data: &[u8], seed: String) -> Option<MoneroData> {
    let parsed = json::parse(&stringify_request(data));
    match parsed {
        Ok(parsed) => {
            if parsed["params"].is_null() {
                return None;
            }
            // TODO: Params possibly can be an array, for a single miner it seems to only have one entry per block
            // submission Levenshtein Comparison to Percentage Formula: 100 - ( ((2*Lev_distance(Q, Mi)) /
            // (Q.length + Mi.length)) * 100 )
            let s = format!("{}", parsed["params"][0].clone());
            let hex = hex::decode(s);
            match hex {
                Ok(hex) => {
                    let block = deserialize::<Block>(&hex);
                    match block {
                        Ok(block) => {
                            let count = 1 + (block.tx_hashes.len() as u16);
                            let mut hashes = Vec::with_capacity(count as usize);
                            let mut proof = Vec::with_capacity(count as usize);
                            hashes.push(block.miner_tx.hash());
                            proof.push(block.miner_tx.hash());
                            for item in block.clone().tx_hashes {
                                hashes.push(item);
                                proof.push(item);
                            }
                            match tree_hash(hashes) {
                                Ok(root) => Some(MoneroData {
                                    header: block.header.clone(),
                                    key: seed,
                                    count,
                                    transaction_root: from_slice(root.as_slice()),
                                    transaction_hashes: from_hashes(&proof),
                                    coinbase_tx: block.miner_tx,
                                }),
                                Err(e) => {
                                    error!(
                                        target: LOG_TARGET,
                                        "{:#}",
                                        MmProxyError::ParseError(format!(
                                            "Failure to calculate Monero root to get monero data, {:?}",
                                            e
                                        ))
                                    );
                                    None
                                },
                            }
                        },
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "{:#}",
                                MmProxyError::ParseError(format!(
                                    "Failure to deserialize block to get monero data, {:?}",
                                    e
                                ))
                            );
                            None
                        },
                    }
                },
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "{:#}",
                        MmProxyError::ParseError(format!("Failure to decode hex to get monero data, {:?}", e))
                    );
                    None
                },
            }
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "{:#}",
                MmProxyError::ParseError(format!("Failure to parse json to get monero data, {:?}", e))
            );
            None
        },
    }
}

fn get_seed_hash(data: &[u8]) -> Option<String> {
    let parsed = json::parse(&stringify_request(data));
    match parsed {
        Ok(parsed) => {
            if parsed["result"]["seed_hash"].is_null() {
                return None;
            }
            let seed = &parsed["result"]["seed_hash"].to_string();
            Some(seed.to_owned())
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "{:#}",
                MmProxyError::ParseError(format!("Failure to parse json to get seed hash, {:?}", e))
            );
            None
        },
    }
}

// TODO: Temporary till RPC call is in place
fn add_coinbase(consensus: ConsensusManager, grpc_block: grpc::NewBlockTemplate) -> Option<grpc::NewBlockTemplate> {
    let block = NewBlockTemplate::try_from(grpc_block);
    match block {
        Ok(mut block) => {
            let fees = block.body.get_total_fee();
            let (key, r) = get_spending_key();
            let factories = CryptoFactories::default();
            let builder = CoinbaseBuilder::new(factories);
            let builder = builder
                .with_block_height(block.header.height)
                .with_fees(fees)
                .with_nonce(r)
                .with_spend_key(key);
            let coinbase_builder = builder.build(consensus.consensus_constants(), consensus.emission_schedule());
            match coinbase_builder {
                Ok((tx, _unblinded_output)) => {
                    block.body.add_output(tx.body.outputs()[0].clone());
                    block.body.add_kernel(tx.body.kernels()[0].clone());
                    let template = grpc::NewBlockTemplate::try_from(block);
                    match template {
                        Ok(template) => Some(template),
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "{:#}",
                                MmProxyError::ParseError(format!("Failure to convert grpc template, {:?}", e))
                            );
                            None
                        },
                    }
                },
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "{:#}",
                        MmProxyError::OtherError(format!("Failure to add coinbase, {:?}", e))
                    );
                    None
                },
            }
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "{:#}",
                MmProxyError::ParseError(format!("Failure to convert grpc block, {:?}", e))
            );
            None
        },
    }
}

// TODO: Temporary till RPC call is in place
fn get_spending_key() -> (PrivateKey, PrivateKey) {
    let r = PrivateKey::random(&mut OsRng);
    let key = PrivateKey::random(&mut OsRng);
    (key, r)
}

// Add merge mining tag to response
fn add_merge_mining_tag(data: &[u8], hash: &[u8]) -> Vec<u8> {
    // Parse the JSON
    let parsed = json::parse(&stringify_request(data));
    match parsed {
        Ok(mut parsed) => {
            if parsed["result"]["blocktemplate_blob"].is_null() {
                error!(
                    target: LOG_TARGET,
                    "{:#}",
                    MmProxyError::ParseError("Monero response invalid, cannot add merge mining tag".to_string())
                );
                return data.to_vec();
            }
            // Decode and dserialize the blocktemplate_blob
            let block_template_blob = &parsed["result"]["blocktemplate_blob"];
            let s = format!("{}", block_template_blob);
            let hex = hex::decode(s);
            match hex {
                Ok(hex) => {
                    let block = deserialize::<Block>(&hex[..]);
                    match block {
                        Ok(mut block) => {
                            let mm_tag = append_merge_mining_tag(&mut block, Hash(from_slice(hash)));
                            match mm_tag {
                                Ok(mm_tagged_template) => {
                                    let count = 1 + block.tx_hashes.len() as u16;
                                    let mut hashes = block.clone().tx_hashes;
                                    hashes.push(block.miner_tx.hash());
                                    let input_blob = create_input_blob(&block.header, &count, &from_hashes(&hashes));
                                    match input_blob {
                                        Ok(input_blob) => {
                                            parsed["result"]["blocktemplate_blob"] = mm_tagged_template.into();
                                            parsed["result"]["blockhashing_blob"] = input_blob.into();
                                            parsed.dump().into()
                                        },
                                        Err(e) => {
                                            error!(
                                                target: LOG_TARGET,
                                                "{:#}",
                                                MmProxyError::MissingDataError(format!(
                                                    "Failed to create input blob, {:?}",
                                                    e
                                                ))
                                            );
                                            data.to_vec()
                                        },
                                    }
                                },
                                Err(e) => {
                                    error!(
                                        target: LOG_TARGET,
                                        "{:#}",
                                        MmProxyError::MissingDataError(format!(
                                            "Failed to append merge mining tag, {:?}",
                                            e
                                        ))
                                    );
                                    data.to_vec()
                                },
                            }
                        },
                        Err(e) => {
                            error!(
                                target: LOG_TARGET,
                                "{:#}",
                                MmProxyError::ParseError(format!(
                                    "Failure to deserialize block to add merge mining tag, {:?}",
                                    e
                                ))
                            );
                            data.to_vec()
                        },
                    }
                },
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "{:#}",
                        MmProxyError::ParseError(format!("Failure to decode hex to add merge mining tag {}", e))
                    );
                    data.to_vec()
                },
            }
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "{:#}",
                MmProxyError::ParseError(format!("Failure to parse json {}", e))
            );
            data.to_vec()
        },
    }
}

// Add height to response
fn adjust_height(data: &[u8], height: u64) -> Vec<u8> {
    // Parse the JSON
    debug!(target: LOG_TARGET, "Tari tip changed");
    let parsed = json::parse(&stringify_request(data));
    match parsed {
        Ok(mut parsed) => {
            if parsed["height"].is_null() {
                error!(
                    target: LOG_TARGET,
                    "{:#}",
                    MmProxyError::ParseError("Monero response invalid, cannot adjust height".to_string())
                );
                return data.to_vec();
            }
            parsed["height"] = height.into();
            // perhaps change parsed["hash"] here too.
            parsed.dump().into()
        },
        Err(e) => {
            error!(
                target: LOG_TARGET,
                "{:#}",
                MmProxyError::ParseError(format!("Failure to parse json {}", e))
            );
            data.to_vec()
        },
    }
}

fn handle_get(
    url_part: &str,
    data: &[u8],
    transient: &mut TransientData,
    rt: &mut Runtime,
    config: &GlobalConfig,
) -> Result<Vec<u8>, MmProxyError>
{
    let mut grpcclient = rt
        .block_on(grpc::base_node_client::BaseNodeClient::connect(format!(
            "{}{}",
            "http://",
            config.grpc_address.to_string()
        )))
        .map_err(|e| MmProxyError::OtherError(format!("GRPC Error: {:#}", e)))?;
    // GET requests
    match url_part {
        "/getheight" => {
            let result = rt
                .block_on(grpcclient.get_tip_info(grpc::Empty {}))
                .map_err(|e| MmProxyError::OtherError(format!("GRPC Error: {:#}", e)))?;
            let meta = result
                .into_inner()
                .metadata
                .ok_or_else(|| MmProxyError::OtherError("GRPC data is malformed".to_string()))?;
            let height = meta.height_of_longest_chain;
            // Short Circuit XMRig to request a new block template
            // TODO: needs additional testing
            if let Some(current_height) = transient.tari_height {
                if height != current_height {
                    adjust_height(&data, current_height);
                } else if let Some(submit_height) = transient.tari_prev_submit_height {
                    if submit_height >= current_height {
                        adjust_height(&data, current_height);
                        debug!(target: LOG_TARGET, "Already submitted for current Tari height");
                    }
                }
            }
            transient.tari_height = Some(height);
        },
        _ => {
            debug!(
                target: LOG_TARGET,
                "{:#}",
                format!("Unhandled GET request {}", url_part)
            );
        },
    }
    Ok(data.to_vec())
}

fn handle_post(
    method: &str,
    json: &[u8],
    data: Vec<u8>,
    transient: &mut TransientData,
    consensus: ConsensusManager,
    rt: &mut Runtime,
    config: &GlobalConfig,
) -> Result<Vec<u8>, MmProxyError>
{
    let mut grpcclient = rt
        .block_on(grpc::base_node_client::BaseNodeClient::connect(format!(
            "{}{}",
            "http://",
            config.grpc_address.to_string()
        )))
        .map_err(|e| MmProxyError::OtherError(format!("GRPC Error: {:#}", e)))?;
    let mut current_data = data.clone();
    match method {
        "submitblock" => {
            let response = json::parse(&String::from_utf8_lossy(&current_data).to_string());
            match response {
                Ok(response) => {
                    if response["result"]["status"] == "OK" {
                        let tari_block = transient
                            .tari_block
                            .clone()
                            .ok_or_else(|| MmProxyError::OtherError("Invalid transient block".to_string()))?;
                        let monero_seed = transient
                            .monero_seed
                            .clone()
                            .ok_or_else(|| MmProxyError::OtherError("Invalid transient seed".to_string()))?;
                        let mut block: grpc::Block = tari_block
                            .block
                            .ok_or_else(|| MmProxyError::MissingDataError("Invalid transient block".to_string()))?;
                        let pow_data = get_monero_data(&json, monero_seed)
                            .ok_or_else(|| MmProxyError::MissingDataError("No merge mining data".to_string()))?;
                        let mut tariheader = block
                            .header
                            .ok_or_else(|| MmProxyError::MissingDataError("Invalid transient header".to_string()))?;
                        let mut pow = tariheader.pow.clone().ok_or_else(|| {
                            MmProxyError::MissingDataError("Invalid transient proof of work".to_string())
                        })?;
                        let serialized = bincode::serialize(&pow_data)
                            .map_err(|e| MmProxyError::OtherError(format!("Failed to serialize pow data: {}", e)))?;
                        pow.pow_data = serialized;
                        tariheader.pow = Some(pow);
                        block.header = Some(tariheader);
                        trace!(target: LOG_TARGET, "Tari Block {:?}", block);
                        rt.block_on(grpcclient.submit_block(block))
                            .map_err(|e| MmProxyError::OtherError(format!("GRPC Error: {}", e)))?;
                        transient.tari_prev_submit_height = transient.tari_height;
                        // Clear data on submission
                        transient.tari_block = None;
                        transient.monero_seed = None;
                    } else {
                        // Failure here means XMRig wont submit since it already succeeded to monero
                        transient.tari_block = None;
                        transient.monero_seed = None;
                        return Err(MmProxyError::OtherError(format!(
                            "Response status failed: {:#}",
                            response
                        )));
                    }
                },
                Err(e) => {
                    return Err(MmProxyError::ParseError(format!("Failure to parse json {}", e)));
                },
            }
        },
        "getblocktemplate" => {
            // Add merge mining tag on blocktemplate request
            // PowAlgo = 0 is Monero
            let new_block_template_response = rt
                .block_on(grpcclient.get_new_block_template(grpc::PowAlgo { pow_algo: 0 }))
                .map_err(|e| {
                    MmProxyError::MissingDataError(format!("GRPC Error, failed to get newblocktemplate {:#}", e))
                })?;
            let new_block_template = new_block_template_response
                .into_inner()
                .new_block_template
                .ok_or_else(|| MmProxyError::MissingDataError("GRPC data is malformed".to_string()))?;
            let coinbased_block = add_coinbase(consensus, new_block_template)
                .ok_or_else(|| MmProxyError::MissingDataError("Failed to add coinbase".to_string()))?;
            let new_block = rt
                .block_on(grpcclient.get_new_block(coinbased_block))
                .map_err(|e| MmProxyError::MissingDataError(format!("GRPC Error, failed to get newblock {:#}", e)))?;
            let block = new_block.into_inner();
            let mining_data = block
                .clone()
                .mining_data
                .ok_or_else(|| MmProxyError::MissingDataError("Invalid mining data".to_string()))?;
            let hash = mining_data.mergemining_hash;
            current_data = add_merge_mining_tag(&current_data[..], &hash);
            let seed_hash = get_seed_hash(&data);
            let parsed = json::parse(&String::from_utf8_lossy(&current_data))
                .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
            transient.tari_block = Some(block);
            transient.monero_seed = seed_hash;
            debug!(
                target: LOG_TARGET,
                "BlockTempBlob: {:#}", &parsed["result"]["blocktemplate_blob"]
            );
            debug!(
                target: LOG_TARGET,
                "BlockHashBlob: {:#}", &parsed["result"]["blockhashing_blob"]
            );
        },
        _ => {},
    }
    Ok(current_data.to_vec())
}

// Handles connection from xmrig, passing it through to monerod and back
#[allow(clippy::unused_io_amount)]
fn handle_connection(
    mut stream: TcpStream,
    transient: &mut TransientData,
    consensus: ConsensusManager,
    rt: &mut Runtime,
    config: &GlobalConfig,
)
{
    info!(target: LOG_TARGET, "Handling Connection");
    let mut buffer = [0; 4096];
    stream.read(&mut buffer).unwrap_or_default();
    let date = Local::now();
    let request_string = stringify_request(&buffer[..]);
    debug!(target: LOG_TARGET, "Request: {}", request_string);
    let request_type = get_request_type(&buffer[..]);
    let url_part = get_url_part(&buffer[..]);
    let url = format!("{}{}", config.monerod_url, url_part);
    let mut data = Vec::new();
    // Result has to go back to XMRig if possible, thread cannot fail prematurely.
    let mut default_response = format!(
        "{} \"error\": {} \"code\": \"{}\", \"message\": \"{}\"{}, \"id\": {}, \"jsonrpc\": \"2.0\"{}",
        "{", "{", 418, "Curl failed to brew the request", "}", -1, "}"
    )
    .as_bytes()
    .to_vec();
    if request_type.starts_with("GET") {
        match base_curl(0, &url, false, config) {
            Ok(mut curl) => match do_curl(&mut curl, b"") {
                Ok(response) => {
                    data = response;
                    debug!(target: LOG_TARGET, "Handling GET Method: {}", url_part);

                    match handle_get(&url_part, &data, transient, rt, config) {
                        Ok(result) => {
                            data = result;
                        },
                        Err(e) => {
                            error!(target: LOG_TARGET, "{}", e);
                        },
                    }
                },
                Err(e) => {
                    data = default_response;
                    error!(target: LOG_TARGET, "Failed to perform curl request, {}", e);
                },
            },
            Err(e) => {
                data = default_response;
                error!(target: LOG_TARGET, "Failed to setup curl, {}", e);
            },
        }
    } else if request_type.starts_with("POST") {
        let json_bytes = get_json(&buffer[..]);
        if let Some(json) = json_bytes {
            let method = get_post_json_method(&json);
            let request_id = get_request_id(&json);
            default_response = format!(
                "{} \"error\": {} \"code\": \"{}\", \"message\": \"{}\"{}, \"id\": {}, \"jsonrpc\": \"2.0\"{}",
                "{", "{", 418, "Curl failed to brew the request", "}", request_id, "}"
            )
            .as_bytes()
            .to_vec();
            match base_curl(json.len() as u64, &url, true, config) {
                Ok(mut curl) => {
                    match do_curl(&mut curl, &json) {
                        // request_id
                        Ok(response) => {
                            data = response;
                            debug!(target: LOG_TARGET, "Handling POST Method: {}", method);
                            debug!(
                                target: LOG_TARGET,
                                "Response: {:#}",
                                String::from_utf8_lossy(&data).to_string()
                            );
                            match handle_post(&method, &json, data.clone(), transient, consensus, rt, config) {
                                Ok(result) => {
                                    data = result;
                                },
                                Err(e) => {
                                    error!(target: LOG_TARGET, "{}", e);
                                },
                            }
                        },
                        Err(e) => {
                            data = default_response;
                            error!(target: LOG_TARGET, "Failed to perform curl request, {}", e);
                        },
                    }
                },
                Err(e) => {
                    data = default_response;
                    error!(target: LOG_TARGET, "Failed to setup curl, {}", e);
                },
            }
        } else {
            data = default_response;
            error!(target: LOG_TARGET, "XMRig is submitting invalid requests");
        }
    } else {
        // Not implemented
        debug!(target: LOG_TARGET, "Request neither GET or POST");
        debug!(target: LOG_TARGET, "Request: {}", request_string);
    }
    // Always return response to XMRig
    let response = structure_response(&data[..], 200);
    debug!(
        target: LOG_TARGET,
        "Response: {:#}",
        String::from_utf8_lossy(&data).to_string()
    );
    stream.write(response.as_bytes()).unwrap_or_default();
    stream.flush().unwrap_or_default();

    debug!(target: LOG_TARGET, "{}", date.format("%Y-%m-%d %H:%M:%S"));
}

fn main() {
    let cfg = setup_logging()
        .map_err(|e| {
            error!(target: LOG_TARGET, "{:?}", e);
            e
        })
        .unwrap_or_default();
    let network = Network::Rincewind;
    let rules = ConsensusManagerBuilder::new(network).build();
    let transient = TransientData {
        tari_block: None,
        monero_seed: None,
        tari_height: None,
        tari_prev_submit_height: None,
    };
    match GlobalConfig::convert_from(cfg) {
        Ok(config) => match setup(config.clone()) {
            Ok((rt, listener)) => {
                if let Err(e) = stream_handler(listener, transient, rules, rt, config) {
                    error!(target: LOG_TARGET, "{}", e);
                }
            },
            Err(e) => {
                error!(target: LOG_TARGET, "{}", e);
                println!("Exiting. Check Configuration, {:?}", e);
            },
        },
        Err(e) => {
            println!("Exiting. Check Configuration, {:?}", e);
        },
    }
}

/// Sets up the base node and runs the cli_loop
fn setup_logging() -> Result<config::Config, ConfigError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::MergeMiningProxy)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    Ok(cfg)
}

fn setup(config: GlobalConfig) -> Result<(Runtime, TcpListener), MmProxyError> {
    let listener = TcpListener::bind(config.proxy_host_address)
        .map_err(|e| MmProxyError::OtherError(format!("Failure to create tcp listener: {}", e)))?;
    let rt = Runtime::new().map_err(|e| MmProxyError::OtherError(format!("Failure to create runtime: {}", e)))?;
    Ok((rt, listener))
}

fn stream_handler(
    listener: TcpListener,
    transient: TransientData,
    rules: ConsensusManager,
    rt: Runtime,
    config: GlobalConfig,
) -> Result<(), MmProxyError>
{
    let transient_mutex = Mutex::new(transient);
    let transient_arc = Arc::new(transient_mutex);
    let runtime_mutex = Mutex::new(rt);
    let runtime_arc = Arc::new(runtime_mutex);
    let config_mutex = Mutex::new(config);
    let config_arc = Arc::new(config_mutex);
    println!("Merged Mining Proxy started.");
    for stream in listener.incoming() {
        let t = transient_arc.clone();
        let r = rules.clone();
        let k = runtime_arc.clone();
        let c = config_arc.clone();
        // TODO: Refactor into a ThreadPool
        thread::spawn(move || match t.lock() {
            Ok(mut tg) => match k.lock() {
                Ok(mut kg) => match c.lock() {
                    Ok(cg) => match stream {
                        Ok(stream) => {
                            handle_connection(stream, &mut tg, r, &mut kg, &cg);
                            Ok(())
                        },
                        Err(e) => {
                            return Err(MmProxyError::OtherError(format!("Stream error, {:?}", e)));
                        },
                    },
                    Err(e) => {
                        return Err(MmProxyError::OtherError(format!("Failed to acquire lock, {:?}", e)));
                    },
                },
                Err(e) => {
                    return Err(MmProxyError::OtherError(format!("Failed to acquire lock, {:?}", e)));
                },
            },
            Err(e) => {
                return Err(MmProxyError::OtherError(format!("Failed to acquire lock, {:?}", e)));
            },
        });
    }
    Ok(())
}

// TODO: Write integration tests

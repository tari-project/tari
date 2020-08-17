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
use curl::easy::{Auth, Easy, List};
use structopt::StructOpt;
use tari_common::{ConfigBootstrap, ConfigError};

// TODO: Log to file
use crate::error::MmProxyError;
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
use tari_app_grpc::base_node_grpc as grpc;
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

// TODO: Refactor into a configuration file
const MONEROD_URL: &str = "monero-stagenet.exan.tech:38081";
const MONEROD_USER: &str = "user";
const MONEROD_PASS: &str = "pass";
const TARI_GRPC_URL: &str = "http://127.0.0.1:18142";
const LOCALHOST: &str = "127.0.0.1:7878";
const USE_AUTH: bool = false;

#[derive(Clone, Debug)]
pub struct TransientData {
    tari_block: Option<grpc::GetNewBlockResult>,
    monero_seed: Option<String>,
    tari_height: Option<u64>,
    tari_prev_submit_height: Option<u64>,
}

// setup curl authentication
fn base_curl_auth(curl: &mut Easy) -> Result<(), MmProxyError> {
    curl.username(MONEROD_USER)
        .map_err(|_| MmProxyError::CurlError("Invalid username".to_string()))?;
    curl.password(MONEROD_PASS)
        .map_err(|_| MmProxyError::CurlError("Invalid password".to_string()))?;
    let mut auth = Auth::new();
    auth.basic(true);
    curl.http_auth(&auth)
        .map_err(|_| MmProxyError::CurlError("Authentication failure".to_string()))
}

// Set up curl
fn base_curl(len: u64, url: &str, post: bool) -> Result<Easy, MmProxyError> {
    let mut easy = Easy::new();
    easy.url(url)
        .map_err(|_| MmProxyError::CurlError("Invalid url".to_string()))?;
    let mut list = List::new();
    list.append("'Content-Type: application/json")
        .map_err(|_| MmProxyError::OtherError("Invalid header".to_string()))?;
    easy.http_headers(list)
        .map_err(|_| MmProxyError::OtherError("Could not add header".to_string()))?;
    if USE_AUTH {
        base_curl_auth(&mut easy).map_err(|e| MmProxyError::CurlError(format!("Authenticate failure: {}", e)))?;
    }
    if post {
        easy.post(true)
            .map_err(|_| MmProxyError::OtherError("Post error".to_string()))?;
        easy.post_field_size(len)
            .map_err(|_| MmProxyError::OtherError("Post error".to_string()))?;
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
            .map_err(|e| MmProxyError::CurlError(format!("Failure to read function: {}", e)))?;

        transfer
            .write_function(|new_data| {
                data.extend_from_slice(new_data);
                Ok(new_data.len())
            })
            .map_err(|e| MmProxyError::CurlError(format!("Failure to write function {}", e)))?;

        // prevent panic if server unavailable
        transfer.perform().map_err(|e| {
            MmProxyError::CurlError(format!(
                "Server unavailable, code: {}, description: {}",
                e.code(),
                e.description()
            ))
        })?;
    }
    Ok(data)
}

// Structure http header and body
// TODO: Error headers
fn structure_response(response_data: &[u8], code: u32) -> Result<String, MmProxyError> {
    let header = format!(
        "HTTP/1.1 {} \
         OK\r\nAccept-Ranges:bytes\r\nContent-Length:{}\r\nContent-Type:application/json\r\nServer:Epee-based\r\n\r\n",
        code,
        String::from_utf8_lossy(response_data).len()
    );
    Ok(format!("{}{}", header, String::from_utf8_lossy(response_data)))
}

// Get the url bits from http header
fn get_url_part(request: &[u8]) -> Result<String, MmProxyError> {
    let string = String::from_utf8_lossy(&request[..]).to_string();
    let mut split_request = string.lines();
    let first_line = split_request
        .next()
        .ok_or_else(|| MmProxyError::OtherError("Failure to get next iter".to_string()))?
        .to_string();
    let mut iter = first_line.split_whitespace();
    iter.next()
        .ok_or_else(|| MmProxyError::OtherError("Failure to get next iter".to_string()))?;
    Ok(iter
        .next()
        .ok_or_else(|| MmProxyError::OtherError("Failure to get next iter".to_string()))?
        .to_string())
}

// Get the request type from http header
fn get_request_type(request: &[u8]) -> Result<String, MmProxyError> {
    let string = String::from_utf8_lossy(&request[..]).to_string();
    let mut split_request = string.lines();
    let first_line = split_request
        .next()
        .ok_or_else(|| MmProxyError::OtherError("Failure to get next iter".to_string()))?
        .to_string();
    let mut iter = first_line.split_whitespace();
    Ok(iter
        .next()
        .ok_or_else(|| MmProxyError::OtherError("Failure to get next iter".to_string()))?
        .to_string())
}

// Extract json from response
fn get_json(request: &[u8]) -> Option<Vec<u8>> {
    let re = if let Ok(v) = Regex::new(r"\{(.*)\}") {
        v
    }
    // Match text from first '{' to last '}'
    else {
        return None;
    };
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
                    debug!(target: LOG_TARGET, "Malformed Request");
                    None
                },
            }
        },
        None => {
            // Request didn't contain any json.
            debug!(target: LOG_TARGET, "No Request");
            debug!(target: LOG_TARGET, "Request: {}", string);
            None
        },
    }
}

// Convert json bytes to json string
fn stringify_request(buffer: &[u8]) -> String {
    String::from_utf8_lossy(&buffer).to_string()
}

// Get method name from post request
fn get_post_json_method(json: &[u8]) -> Result<String, MmProxyError> {
    let parsed = json::parse(&stringify_request(json))
        .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
    trace!(target: LOG_TARGET, "{}", parsed);
    if !parsed["method"].is_null() {
        return Ok(parsed["method"].to_string());
    }
    Ok("".to_string())
}

fn get_monero_data(data: &[u8], seed: String) -> Result<MoneroData, MmProxyError> {
    // TODO: Params possibly can be an array, for a single miner it seems to only have one entry per block submission
    // Levenshtein Comparison to Percentage Formula: 100 - ( ((2*Lev_distance(Q, Mi)) / (Q.length + Mi.length)) * 100 )
    let parsed = json::parse(&stringify_request(data))
        .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
    let s = format!("{}", parsed["params"][0].clone());
    let hex = hex::decode(s).map_err(|e| MmProxyError::ParseError(format!("Failure to decode hex {}", e)))?;
    let block = deserialize::<Block>(&hex)
        .map_err(|_| MmProxyError::ParseError("Failure to deserialize block ".to_string()))?;
    let mut hashes = block.clone().tx_hashes;
    hashes.push(block.miner_tx.hash());
    let root = tree_hash(hashes);
    let mut proof = block.clone().tx_hashes;
    proof.push(block.miner_tx.hash());
    Ok(MoneroData {
        header: block.header.clone(),
        key: seed,
        count: (block.tx_hashes.len() as u16) + 1,
        transaction_root: from_slice(root.as_slice()),
        transaction_hashes: from_hashes(&proof),
        coinbase_tx: block.miner_tx,
    })
}

fn get_seed_hash(data: &[u8]) -> Result<String, MmProxyError> {
    let parsed = json::parse(&stringify_request(data))
        .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
    let seed = &parsed["result"]["seed_hash"].to_string();
    Ok(seed.to_owned())
}

// TODO: Temporary till RPC call is in place
fn add_coinbase(
    consensus: ConsensusManager,
    grpc_block: grpc::NewBlockTemplate,
) -> Result<grpc::NewBlockTemplate, MmProxyError>
{
    let mut block: NewBlockTemplate = NewBlockTemplate::try_from(grpc_block)
        .map_err(|e| MmProxyError::ParseError(format!("Failure to convert grpc {}", e)))?;
    let fees = block.body.get_total_fee();
    let (key, r) = get_spending_key()?;
    let factories = CryptoFactories::default();
    let builder = CoinbaseBuilder::new(factories);
    let builder = builder
        .with_block_height(block.header.height)
        .with_fees(fees)
        .with_nonce(r)
        .with_spend_key(key);
    let (tx, _unblinded_output) = builder
        .build(consensus.consensus_constants(), consensus.emission_schedule())
        .expect("invalid constructed coinbase");
    block.body.add_output(tx.body.outputs()[0].clone());
    block.body.add_kernel(tx.body.kernels()[0].clone());
    Ok(grpc::NewBlockTemplate::try_from(block)
        .map_err(|e| MmProxyError::ParseError(format!("Failure to convert grpc {}", e)))?)
}

// TODO: Temporary till RPC call is in place
fn get_spending_key() -> Result<(PrivateKey, PrivateKey), MmProxyError> {
    let r = PrivateKey::random(&mut OsRng);
    let key = PrivateKey::random(&mut OsRng);
    Ok((key, r))
}

// Add merge mining tag to response
fn add_merge_mining_tag(data: &[u8], hash: &[u8]) -> Result<Vec<u8>, MmProxyError> {
    // Parse the JSON
    let mut parsed = json::parse(&stringify_request(data))
        .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
    if parsed["result"]["blocktemplate_blob"].is_null() {
        return Err(MmProxyError::ParseError(
            "Monero response invalid, cannot add merge mining tag".to_string(),
        ));
    }
    // Decode and dserialize the blocktemplate_blob
    let block_template_blob = &parsed["result"]["blocktemplate_blob"];
    let s = format!("{}", block_template_blob);
    let hex = hex::decode(s).map_err(|e| MmProxyError::ParseError(format!("Failure to decode hex {}", e)))?;
    let block = deserialize::<Block>(&hex[..])
        .map_err(|_| MmProxyError::ParseError("Failure to deserialize block ".to_string()))?;
    parsed["result"]["blocktemplate_blob"] = append_merge_mining_tag(&block, Hash(from_slice(hash)))?.into();

    let count = 1 + block.tx_hashes.len() as u16;
    let mut hashes = block.clone().tx_hashes;
    hashes.push(block.miner_tx.hash());
    parsed["result"]["blockhashing_blob"] = create_input_blob(&block.header, &count, &from_hashes(&hashes))?.into();
    Ok(parsed.dump().into())
}

// Add height to response
fn adjust_height(data: &[u8], height: u64) -> Result<Vec<u8>, MmProxyError> {
    // Parse the JSON
    debug!(target: LOG_TARGET, "Tari tip changed");
    let mut parsed = json::parse(&stringify_request(data))
        .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
    if parsed["height"].is_null() {
        trace!(target: LOG_TARGET, "Data: {:?}", data);
        return Err(MmProxyError::ParseError(
            "Monero response invalid, cannot adjust height".to_string(),
        ));
    }
    parsed["height"] = height.into();
    Ok(parsed.dump().into())
}

// Handles connection from xmrig, passing it through to monerod and back
#[allow(clippy::unused_io_amount)]
fn handle_connection(
    mut stream: TcpStream,
    mut transient: &mut TransientData,
    consensus: ConsensusManager,
    rt: &mut Runtime,
) -> Result<(), MmProxyError>
{
    info!(target: LOG_TARGET, "Handling Connection");
    let grpcclientresult = rt.block_on(grpc::base_node_client::BaseNodeClient::connect(TARI_GRPC_URL));
    let mut buffer = [0; 4096];
    stream
        .read(&mut buffer)
        .map_err(|e| MmProxyError::OtherError(format!("Error reading from stream {}", e)))?;
    let date = Local::now();
    let request_string = stringify_request(&buffer[..]);
    debug!(target: LOG_TARGET, "Request: {}", request_string);
    let request_type = get_request_type(&buffer[..])?;
    let url_part = get_url_part(&buffer[..])?;
    if request_type.starts_with("GET") {
        debug!(target: LOG_TARGET, "Handling GET Method: {}", url_part);
        // GET requests
        let url = format!("{}{}", MONEROD_URL, url_part);
        let mut curl = base_curl(0, &url, false)?;
        let data = do_curl(&mut curl, b"")?;
        match url_part.as_str() {
            "/getheight" => match grpcclientresult {
                Ok(mut grpcclient) => {
                    match rt.block_on(grpcclient.get_tip_info(grpc::Empty {})) {
                        Ok(result) => {
                            let height = result.into_inner().metadata.unwrap().height_of_longest_chain;
                            // Short Circuit XMRig to request a new block template
                            // TODO: needs additional testing
                            if let Some(current_height) = transient.tari_height {
                                if height != current_height {
                                    adjust_height(&data, current_height)?;
                                } else if let Some(submit_height) = transient.tari_prev_submit_height {
                                    if submit_height >= current_height {
                                        adjust_height(&data, current_height)?;
                                        debug!(target: LOG_TARGET, "Already submitted for current Tari height");
                                    }
                                }
                            }
                            transient.tari_height = Some(height);
                        },
                        Err(e) => {
                            error!(target: LOG_TARGET, "{:#}", e);
                        },
                    }
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "{:#}", e);
                },
            },
            _ => {},
        }
        let response = structure_response(&data[..], 200)?;
        stream
            .write(response.as_bytes())
            .map_err(|e| MmProxyError::OtherError(format!("Error writing to stream {}", e)))?;
        stream
            .flush()
            .map_err(|e| MmProxyError::OtherError(format!("Error flushing stream {}", e)))?;
    } else if request_type.starts_with("POST") {
        // POST requests
        let json_bytes = get_json(&buffer[..]);
        if let Some(json) = json_bytes {
            let url = format!("{}{}", MONEROD_URL, url_part);
            let mut curl = base_curl(json.len() as u64, &url, true)?;
            let method = get_post_json_method(&json)?;
            let mut data = do_curl(&mut curl, &json)?;
            debug!(target: LOG_TARGET, "Handling POST Method: {}", method.as_str());
            match method.as_str() {
                "submitblock" => match grpcclientresult {
                    Ok(mut grpcclient) => {
                        let response = json::parse(&String::from_utf8_lossy(&data).to_string())
                            .map_err(|e| MmProxyError::ParseError(format!("Failure to parse json {}", e)))?;
                        if response["result"]["status"] == "OK" {
                            if transient.tari_block.is_some() && transient.monero_seed.is_some() {
                                let blockresult: grpc::GetNewBlockResult = transient
                                    .clone()
                                    .tari_block
                                    .ok_or_else(|| MmProxyError::MissingDataError("New Block result".to_string()))?;
                                let mut block: grpc::Block = blockresult
                                    .block
                                    .ok_or_else(|| MmProxyError::MissingDataError("Block".to_string()))?;
                                match get_monero_data(&json, transient.clone().monero_seed.unwrap()) {
                                    Ok(pow_data) => {
                                        let mut tariheader: grpc::BlockHeader = block
                                            .header
                                            .ok_or_else(|| MmProxyError::MissingDataError("Header".to_string()))?;
                                        let mut powdata = tariheader
                                            .pow
                                            .clone()
                                            .ok_or_else(|| MmProxyError::MissingDataError("Pow data".to_string()))?;
                                        powdata.pow_data = bincode::serialize(&pow_data).map_err(|_| {
                                            MmProxyError::ParseError("Failure to serialize block".to_string())
                                        })?;
                                        tariheader.pow = Some(powdata);
                                        block.header = Some(tariheader);
                                        trace!(target: LOG_TARGET, "Tari Block {:?}", block);
                                        match rt.block_on(grpcclient.submit_block(block)) {
                                            Ok(result) => {
                                                result.into_inner();
                                                transient.tari_prev_submit_height = transient.tari_height;
                                            },
                                            Err(e) => {
                                                error!(target: LOG_TARGET, "{:#}", e);
                                            },
                                        }
                                    },
                                    Err(e) => {
                                        error!(target: LOG_TARGET, "{:#}", e);
                                    },
                                }
                            }
                        } else {
                            error!(target: LOG_TARGET, "{:#}", response);
                        }
                    },
                    Err(e) => {
                        error!(target: LOG_TARGET, "{:#}", e);
                    },
                },
                "getblocktemplate" => {
                    // Add merge mining tag on blocktemplate request
                    // PowAlgo = 0 is Monero
                    match grpcclientresult {
                        Ok(mut grpcclient) => {
                            match rt.block_on(grpcclient.get_new_block_template(grpc::PowAlgo { pow_algo: 0 })) {
                                Ok(new_block_template_response) => {
                                    let new_template =
                                        new_block_template_response.into_inner().new_block_template.ok_or_else(
                                            || MmProxyError::MissingDataError("New block template".to_string()),
                                        )?;
                                    let coinbased_block = add_coinbase(consensus, new_template)?;
                                    match rt.block_on(grpcclient.get_new_block(coinbased_block)) {
                                        Ok(newblock) => {
                                            let block = newblock.into_inner();
                                            let hash = block
                                                .clone()
                                                .mining_data
                                                .ok_or_else(|| {
                                                    MmProxyError::MissingDataError("Mining data".to_string())
                                                })?
                                                .mergemining_hash;
                                            data = add_merge_mining_tag(&data[..], &hash)?;
                                            let seed_hash = get_seed_hash(&data)?;
                                            let parsed = json::parse(&String::from_utf8_lossy(&data)).map_err(|e| {
                                                MmProxyError::ParseError(format!("Failure to parse json {}", e))
                                            })?;
                                            transient.tari_block = Some(block);
                                            transient.monero_seed = Some(seed_hash);
                                            debug!(
                                                target: LOG_TARGET,
                                                "BlockTempBlob: {:#}", &parsed["result"]["blocktemplate_blob"]
                                            );
                                            debug!(
                                                target: LOG_TARGET,
                                                "BlockHashBlob: {:#}", &parsed["result"]["blockhashing_blob"]
                                            );
                                        },
                                        Err(e) => {
                                            error!(target: LOG_TARGET, "{:#}", e);
                                        },
                                    }
                                },
                                Err(e) => {
                                    error!(target: LOG_TARGET, "{:#}", e);
                                },
                            }
                        },
                        Err(e) => {
                            error!(target: LOG_TARGET, "Tari base node unavailable, monero mining only");
                            error!(target: LOG_TARGET, "{:#}", e);
                            transient.tari_block = None;
                            transient.monero_seed = None;
                        },
                    }
                },
                _ => {},
            }
            let response = structure_response(&data[..], 200)?;
            trace!(target: LOG_TARGET, "Response: {}", response);
            stream
                .write(response.as_bytes())
                .map_err(|e| MmProxyError::OtherError(format!("Error writing to stream {}", e)))?;
            stream
                .flush()
                .map_err(|e| MmProxyError::OtherError(format!("Error flushing stream {}", e)))?;
        }
    } else {
        // Not implemented
        debug!(target: LOG_TARGET, "Request neither GET or POST");
        debug!(target: LOG_TARGET, "Request: {}", request_string);
    }
    debug!(target: LOG_TARGET, "{}", date.format("%Y-%m-%d %H:%M:%S"));
    Ok(())
}

fn main() {
    let _ = setup_logging()
        .map_err(|e| {
            error!(target: LOG_TARGET, "{:?}", e);
            e
        })
        .unwrap();
    let network = Network::Rincewind;
    let rules = ConsensusManagerBuilder::new(network).build();
    let transient = TransientData {
        tari_block: None,
        monero_seed: None,
        tari_height: None,
        tari_prev_submit_height: None,
    };
    let (rt, listener) = setup()
        .map_err(|e| {
            error!(target: LOG_TARGET, "{}", e);
            e
        })
        .unwrap();
    if let Err(e) = stream_handler(listener, transient, rules, rt) {
        error!(target: LOG_TARGET, "{}", e);
    }
}

/// Sets up the base node and runs the cli_loop
fn setup_logging() -> Result<(), ConfigError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs()?;

    // Load and apply configuration file
    let _cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()
}

fn setup() -> Result<(Runtime, TcpListener), MmProxyError> {
    let listener = TcpListener::bind(LOCALHOST)
        .map_err(|e| MmProxyError::OtherError(format!("Failure to create tcp listener: {}", e)))?;
    let rt = Runtime::new().map_err(|e| MmProxyError::OtherError(format!("Failure to create runtime: {}", e)))?;
    Ok((rt, listener))
}

fn stream_handler(
    listener: TcpListener,
    transient: TransientData,
    rules: ConsensusManager,
    rt: Runtime,
) -> Result<(), MmProxyError>
{
    let transient_mutex = Mutex::new(transient);
    let transient_arc = Arc::new(transient_mutex);
    // let rules_mutex = Mutex::new(rules);
    // let rules_arc = Arc::new(rules_mutex);
    let runtime_mutex = Mutex::new(rt);
    let runtime_arc = Arc::new(runtime_mutex);
    for stream in listener.incoming() {
        let t = transient_arc.clone();
        let r = rules.clone();
        let k = runtime_arc.clone();
        thread::spawn(move || {
            let mut tg = t
                .lock()
                .map_err(|e| {
                    error!(target: LOG_TARGET, "{}", e);
                    e
                })
                .unwrap();
            let mut kg = k
                .lock()
                .map_err(|e| {
                    error!(target: LOG_TARGET, "{}", e);
                    e
                })
                .unwrap();
            let stream = stream
                .map_err(|e| {
                    error!(target: LOG_TARGET, "{}", e);
                    e
                })
                .unwrap();
            if let Err(e) = handle_connection(stream, &mut tg, r, &mut kg) {
                error!(target: LOG_TARGET, "One of the threads crashed: {}", e);
            };
        });
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::tree_hash;
    use monero::{
        blockdata::{
            block::BlockHeader,
            transaction::{ExtraField, SubField, TxOutTarget},
            Block,
            TransactionPrefix,
            TxIn,
        },
        consensus::{deserialize, encode::VarInt, serialize},
        cryptonote::hash::Hashable,
        util::ringct::{RctSig, RctSigBase, RctType},
        PublicKey,
        Transaction,
        TxOut,
    };
    use tari_utilities::ByteArray;

    // TODO: Write integration tests

    // This tests checks the hash of monero-rs
    #[test]
    fn test_miner_tx_hash() {
        let tx = "f8ad7c58e6fce1792dd78d764ce88a11db0e3c3bb484d868ae05a7321fb6c6b0";

        let pk_extra = vec![
            179, 155, 220, 223, 213, 23, 81, 160, 95, 232, 87, 102, 151, 63, 70, 249, 139, 40, 110, 16, 51, 193, 175,
            208, 38, 120, 65, 191, 155, 139, 1, 4,
        ];
        let transaction = Transaction {
            prefix: TransactionPrefix {
                version: VarInt(2),
                unlock_time: VarInt(2143845),
                inputs: vec![TxIn::Gen {
                    height: VarInt(2143785),
                }],
                outputs: vec![TxOut {
                    amount: VarInt(1550800739964),
                    target: TxOutTarget::ToKey {
                        key: PublicKey::from_slice(
                            hex::decode("e2e19d8badb15e77c8e1f441cf6acd9bcde34a07cae82bbe5ff9629bf88e6e81")
                                .unwrap()
                                .as_slice(),
                        )
                        .unwrap(),
                    },
                }],
                extra: ExtraField {
                    0: vec![
                        SubField::TxPublicKey(PublicKey::from_slice(pk_extra.as_slice()).unwrap()),
                        SubField::Nonce(vec![196, 37, 4, 0, 27, 37, 187, 163, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                    ],
                },
            },
            signatures: vec![],
            rct_signatures: RctSig {
                sig: Option::from(RctSigBase {
                    rct_type: RctType::Null,
                    txn_fee: Default::default(),
                    pseudo_outs: vec![],
                    ecdh_info: vec![],
                    out_pk: vec![],
                }),
                p: None,
            },
        };
        assert_eq!(
            tx.as_bytes().to_vec(),
            hex::encode(transaction.hash().0.to_vec()).as_bytes().to_vec()
        );
        println!("{:?}", tx.as_bytes().to_vec());
        println!("{:?}", hex::encode(transaction.hash().0.to_vec()));
        let hex = hex::encode(serialize::<Transaction>(&transaction));
        deserialize::<Transaction>(&hex::decode(&hex).unwrap()).unwrap();
    }

    // This tests checks the blockhashing blob of monero-rs
    #[test]
    fn test_block_ser() {
        // block with only the miner tx and no other transactions
        let hex = "0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000029b892201ffdf882201b699d4c8b1ec020223df524af2a2ef5f870adb6e1ceb03a475c39f8b9ef76aa50b46ddd2a18349402b012839bfa19b7524ec7488917714c216ca254b38ed0424ca65ae828a7c006aeaf10208f5316a7f6b99cca60000";
        // blockhashing blob for above block as accepted by monero
        let hex_blockhash_blob="0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000602d0d4710e2c2d38da0cce097accdf5dc18b1d34323880c1aae90ab8f6be6e201";
        let bytes = hex::decode(hex).unwrap();
        let block = deserialize::<Block>(&bytes[..]).unwrap();
        let header = serialize::<BlockHeader>(&block.header);
        let mut count = serialize::<VarInt>(&VarInt(1 + block.tx_hashes.len() as u64));
        let mut hashes = block.clone().tx_hashes;
        hashes.push(block.miner_tx.hash());
        let mut root = tree_hash(hashes); // tree_hash.c used by monero
        let mut encode2 = header;
        encode2.append(&mut root);
        encode2.append(&mut count);
        assert_eq!(hex::encode(encode2), hex_blockhash_blob);
        let bytes2 = serialize::<Block>(&block);
        assert_eq!(bytes, bytes2);
        let hex2 = hex::encode(bytes2);
        assert_eq!(hex, hex2);
    }
}

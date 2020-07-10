extern crate arrayref;
extern crate chrono;
extern crate jsonrpc;
use chrono::Local;
use curl::{
    easy::{Auth, Easy, List},
    Error,
};

// TODO: Log to file
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
use tari_core::{
    blocks::NewBlockTemplate,
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    mining::CoinbaseBuilder,
    proof_of_work::monero_rx::{
        append_merge_mining_tag,
        create_input_blob,
        from_hashes,
        from_slice,
        tree_hash,
        MoneroData,
    },
    transactions::types::CryptoFactories,
};
use tari_crypto::{keys::SecretKey, ristretto::RistrettoSecretKey};
use tokio::runtime::Runtime;

mod grpc_conversions;

pub type PrivateKey = RistrettoSecretKey;

pub mod grpc {
    tonic::include_proto!("tari.base_node"); // The string specified here must match the proto package name
}

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
}

// setup curl authentication
fn base_curl_auth(curl: &mut Easy) -> Result<(), Error> {
    curl.username(MONEROD_USER).unwrap();
    curl.password(MONEROD_PASS).unwrap();
    let mut auth = Auth::new();
    auth.basic(true);
    curl.http_auth(&auth)
}

// Set up curl
fn base_curl(len: u64, url: &str, post: bool) -> Easy {
    let mut easy = Easy::new();
    easy.url(url).unwrap();
    let mut list = List::new();
    list.append("'Content-Type: application/json").unwrap();
    easy.http_headers(list).unwrap();
    if USE_AUTH {
        match base_curl_auth(&mut easy) {
            Ok(()) => {},
            Err(e) => {
                debug!("{:#}#", e);
            },
        }
    }
    if post {
        easy.post(true).unwrap();
        easy.post_field_size(len).unwrap();
    }
    easy
}

// Perform curl request to monerod api
fn do_curl(curl: &mut Easy, request: &[u8]) -> Vec<u8> {
    let mut transfer_data = request;
    let mut data = Vec::new();
    {
        let mut transfer = curl.transfer();
        transfer
            .read_function(|buf| Ok(transfer_data.read(buf).unwrap_or(0)))
            .unwrap();

        transfer
            .write_function(|new_data| {
                data.extend_from_slice(new_data);
                Ok(new_data.len())
            })
            .unwrap();

        // prevent panic if server unavailable
        match transfer.perform() {
            Ok(()) => {},
            Err(e) => {
                return format!(
                    "{}\"error\": {} \"code\": \"{}\", \"message\": \"{}\"{}, \"id\": -1, \"jsonrpc\": \"2.0\"{}",
                    "{",
                    "{",
                    e.code(),
                    e.description(),
                    "}",
                    "}"
                )
                .as_bytes()
                .to_vec();
            },
        }
    }
    data
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
    let first_line = split_request.next().unwrap().to_string();
    let mut iter = first_line.split_whitespace();
    iter.next();
    iter.next().unwrap().to_string()
}

// Get the request type from http header
fn get_request_type(request: &[u8]) -> String {
    let string = String::from_utf8_lossy(&request[..]).to_string();
    let mut split_request = string.lines();
    let first_line = split_request.next().unwrap().to_string();
    let mut iter = first_line.split_whitespace();
    iter.next().unwrap().to_string()
}

// Extract json from response
fn get_json(request: &[u8]) -> Option<Vec<u8>> {
    let re = Regex::new(r"\{(.*)\}").unwrap(); // Match text from first '{' to last '}'
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
                    debug!("Malformed Request");
                    None
                },
            }
        },
        None => {
            // Request didn't contain any json.
            debug!("No Request");
            debug!("Request: {}", string);
            None
        },
    }
}

// Convert json bytes to json string
fn stringify_request(buffer: &[u8]) -> String {
    String::from_utf8_lossy(&buffer).to_string()
}

// Get method name from post request
fn get_post_json_method(json: &[u8]) -> String {
    let parsed = json::parse(&stringify_request(json)).unwrap();
    debug!("{}", parsed);
    if !parsed["method"].is_null() {
        return parsed["method"].to_string();
    }
    "".to_string()
}

fn get_monero_data(data: &[u8], seed: String) -> Result<MoneroData, Error> {
    // TODO: Params possibly can be an array, for a single miner it seems to only have one entry per block submission
    let parsed = json::parse(&stringify_request(data)).unwrap();
    let s = format!("{}", parsed["params"][0].clone());
    let hex = hex::decode(s).unwrap();
    let block = deserialize::<Block>(&hex).unwrap();
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

fn get_seed_hash(data: &[u8]) -> String {
    let parsed = json::parse(&stringify_request(data)).unwrap();
    let seed = &parsed["result"]["seed_hash"].to_string();
    seed.to_owned()
}

// TODO: Temporary till RPC call is in place
fn add_coinbase(
    consensus: ConsensusManager,
    grpc_block: grpc::NewBlockTemplate,
) -> Result<grpc::NewBlockTemplate, Error>
{
    let mut block: NewBlockTemplate = NewBlockTemplate::try_from(grpc_block).unwrap();
    let fees = block.body.get_total_fee();
    let (key, r) = get_spending_key()?;
    let factories = CryptoFactories::default();
    let builder = CoinbaseBuilder::new(factories);
    let builder = builder
        .with_block_height(block.header.height)
        .with_fees(fees)
        .with_nonce(r)
        .with_spend_key(key);
    let (tx, _unblinded_output) = builder.build(consensus).expect("invalid constructed coinbase");
    block.body.add_output(tx.body.outputs()[0].clone());
    block.body.add_kernel(tx.body.kernels()[0].clone());
    Ok(grpc::NewBlockTemplate::try_from(block).unwrap())
}

// TODO: Temporary till RPC call is in place
fn get_spending_key() -> Result<(PrivateKey, PrivateKey), Error> {
    let r = PrivateKey::random(&mut OsRng);
    let key = PrivateKey::random(&mut OsRng);
    Ok((key, r))
}

// Add merge mining tag to response
fn add_merge_mining_tag(data: &[u8], hash: &[u8]) -> Vec<u8> {
    // Parse the JSON
    let mut parsed = json::parse(&stringify_request(data)).unwrap();

    // Decode and dserialize the blocktemplate_blob
    let block_template_blob = &parsed["result"]["blocktemplate_blob"];
    let s = format!("{}", block_template_blob);
    let hex = hex::decode(s).unwrap();
    let block = deserialize::<Block>(&hex[..]).unwrap();
    parsed["result"]["blocktemplate_blob"] = append_merge_mining_tag(&block, Hash(from_slice(hash))).unwrap().into();

    let count = 1 + block.tx_hashes.len() as u16;
    let mut hashes = block.clone().tx_hashes;
    hashes.push(block.miner_tx.hash());
    parsed["result"]["blockhashing_blob"] = create_input_blob(&block.header, &count, &from_hashes(&hashes))
        .unwrap()
        .into();
    parsed.dump().into()
}

// Handles connection from xmrig, passing it through to monerod and back
#[allow(clippy::unused_io_amount)]
fn handle_connection(
    mut stream: TcpStream,
    mut transient: &mut TransientData,
    consensus: ConsensusManager,
    rt: &mut Runtime,
)
{
    println!("Handling Connection");
    let grpcclientresult = rt.block_on(grpc::base_node_client::BaseNodeClient::connect(TARI_GRPC_URL));
    let mut buffer = [0; 4096];
    stream.read(&mut buffer).unwrap();
    let date = Local::now();
    let request_string = stringify_request(&buffer[..]);
    let request_type = get_request_type(&buffer[..]);
    let url_part = get_url_part(&buffer[..]);
    if request_type.starts_with("GET") {
        println!("Handling Method: {}", url_part);
        // GET requests
        let url = format!("{}{}", MONEROD_URL, url_part);
        let mut curl = base_curl(0, &url, false);
        let data = do_curl(&mut curl, b"");
        let response = structure_response(&data[..], 200);
        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    } else if request_type.starts_with("POST") {
        // POST requests
        let json_bytes = get_json(&buffer[..]);
        if let Some(json) = json_bytes {
            let url = format!("{}{}", MONEROD_URL, url_part);
            let mut curl = base_curl(json.len() as u64, &url, true);
            debug!("Request: {}", request_string);
            let method = get_post_json_method(&json);
            let mut data = do_curl(&mut curl, &json);

            // TODO: Check tari height, monero check height request is type get (/getheight)

            println!("Handling Method: {}", method.as_str());
            match method.as_str() {
                "submitblock" => {
                    match grpcclientresult {
                        Ok(mut grpcclient) => {
                            // let mut grpcclient = grpcclientresult.unwrap();
                            let response = json::parse(&String::from_utf8_lossy(&data).to_string()).unwrap();
                            if response["result"]["status"] == "OK" {
                                if transient.tari_block.is_some() && transient.monero_seed.is_some() {
                                    let blockresult: grpc::GetNewBlockResult = transient.clone().tari_block.unwrap();
                                    let mut block: grpc::Block = blockresult.block.unwrap();
                                    match get_monero_data(&json, transient.clone().monero_seed.unwrap()) {
                                        Ok(pow_data) => {
                                            let mut tariheader: grpc::BlockHeader = block.header.unwrap();
                                            let mut powdata = tariheader.pow.clone().unwrap();
                                            powdata.pow_data = bincode::serialize(&pow_data).unwrap();
                                            tariheader.pow = Some(powdata);
                                            block.header = Some(tariheader);
                                            trace!("Tari Block {:?}", block);
                                            match rt.block_on(grpcclient.submit_block(block)) {
                                                Ok(result) => {
                                                    result.into_inner();
                                                },
                                                Err(e) => {
                                                    debug!("{:#}", e);
                                                },
                                            }
                                        },
                                        Err(e) => {
                                            debug!("{:#}", e);
                                        },
                                    }
                                }
                            } else {
                                debug!("{:#}", response);
                            }
                        },
                        Err(e) => {
                            debug!("{:#}", e);
                        },
                    }
                },
                "getblocktemplate" => {
                    // Add merge mining tag on blocktemplate request
                    // PowAlgo = 0 is Monero
                    match grpcclientresult {
                        Ok(mut grpcclient) => {
                            match rt.block_on(grpcclient.get_new_block_template(grpc::PowAlgo { pow_algo: 0 })) {
                                Ok(newblocktemplate) => {
                                    let coinbased_block =
                                        add_coinbase(consensus, newblocktemplate.into_inner()).unwrap();
                                    match rt.block_on(grpcclient.get_new_block(coinbased_block)) {
                                        Ok(newblock) => {
                                            let block = newblock.into_inner();
                                            let hash = block.clone().mining_data.unwrap().mergemining_hash;
                                            data = add_merge_mining_tag(&data[..], &hash);
                                            let seed_hash = get_seed_hash(&data);
                                            let parsed = json::parse(&String::from_utf8_lossy(&data)).unwrap();
                                            transient.tari_block = Some(block);
                                            transient.monero_seed = Some(seed_hash);
                                            debug!("BlockTempBlob: {:#}", &parsed["result"]["blocktemplate_blob"]);
                                            debug!("BlockHashBlob: {:#}", &parsed["result"]["blockhashing_blob"]);
                                            debug!("Transient: {:?}", transient);
                                        },
                                        Err(e) => {
                                            debug!("{:#}", e);
                                        },
                                    }
                                },
                                Err(e) => {
                                    debug!("{:#}", e);
                                },
                            }
                        },
                        Err(e) => {
                            println!("Tari base node unavailable, monero mining only");
                            debug!("{:#}", e);
                            transient.tari_block = None;
                            transient.monero_seed = None;
                        },
                    }
                },
                _ => {},
            }
            let response = structure_response(&data[..], 200);
            debug!("Response: {}", response);
            stream.write(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    } else {
        // Not implemented
        println!("Request neither GET or POST");
        debug!("Request: {}", request_string);
    }
    println!("{}", date.format("%Y-%m-%d %H:%M:%S"));
}

fn main() {
    let network = Network::Rincewind;
    let rules = ConsensusManagerBuilder::new(network).build();
    let listener = TcpListener::bind(LOCALHOST).unwrap();
    let rt = Runtime::new().unwrap();
    let transient = TransientData {
        tari_block: None,
        monero_seed: None,
    };
    let transient_mutex = Mutex::new(transient);
    let transient_arc = Arc::new(transient_mutex);
    let rules_mutex = Mutex::new(rules);
    let rules_arc = Arc::new(rules_mutex);
    let runtime_mutex = Mutex::new(rt);
    let runtime_arc = Arc::new(runtime_mutex);
    // TODO: Needs a better way to shutdown rather than killall
    for stream in listener.incoming() {
        let t = transient_arc.clone();
        let r = rules_arc.clone();
        let k = runtime_arc.clone();
        thread::spawn(move || {
            let mut tg = t.lock().unwrap();
            let rg = r.lock().unwrap();
            let mut kg = k.lock().unwrap();
            let stream = stream.unwrap();
            handle_connection(stream, &mut tg, rg.clone(), &mut kg);
        });
    }
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

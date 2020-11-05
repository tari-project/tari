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

use crate::error::MmProxyError;
use json::Value;
use monero::{
    blockdata::{transaction::SubField, Block},
    consensus::{deserialize, serialize},
    cryptonote::hash::Hash,
};
use serde_json as json;
use std::convert::TryFrom;
use tari_app_grpc::tari_rpc as grpc;
use tari_core::{
    blocks::NewBlockTemplate,
    proof_of_work::{monero_rx, monero_rx::MoneroData},
    transactions::{transaction::TransactionKernel, TransactionOutput},
};

pub fn deserialize_monero_block_from_hex<T>(data: T) -> Result<Block, MmProxyError>
where T: AsRef<[u8]> {
    let bytes = hex::decode(data)?;
    let obj = deserialize::<Block>(&bytes);
    match obj {
        Ok(obj) => Ok(obj),
        Err(_e) => Err(MmProxyError::MissingDataError("blocktemplate blob invalid".to_string())),
    }
}

pub fn serialize_monero_block_to_hex(obj: &Block) -> Result<String, MmProxyError> {
    let data = serialize::<Block>(obj);
    let bytes = hex::encode(data);
    Ok(bytes)
}

pub fn construct_monero_data(block: Block, seed: String) -> Result<MoneroData, MmProxyError> {
    let hashes = monero_rx::create_ordered_transaction_hashes_from_block(&block);
    let root = monero_rx::tree_hash(&hashes)?;
    Ok(MoneroData {
        header: block.header,
        key: seed,
        count: hashes.len() as u16,
        transaction_root: root.to_fixed_bytes(),
        transaction_hashes: hashes.into_iter().map(|h| h.to_fixed_bytes()).collect(),
        coinbase_tx: block.miner_tx,
    })
}

pub fn add_coinbase(
    coinbase: Option<grpc::Transaction>,
    mut block: NewBlockTemplate,
) -> Result<grpc::NewBlockTemplate, MmProxyError>
{
    if let Some(tx) = coinbase {
        let output = TransactionOutput::try_from(tx.clone().body.unwrap().outputs[0].clone())
            .map_err(MmProxyError::MissingDataError)?;
        let kernel =
            TransactionKernel::try_from(tx.body.unwrap().kernels[0].clone()).map_err(MmProxyError::MissingDataError)?;
        block.body.add_output(output);
        block.body.add_kernel(kernel);
        let template = grpc::NewBlockTemplate::try_from(block);
        match template {
            Ok(template) => Ok(template),
            Err(_e) => Err(MmProxyError::MissingDataError("Template Invalid".to_string())),
        }
    } else {
        Err(MmProxyError::MissingDataError("Coinbase Invalid".to_string()))
    }
}

pub fn default_accept(json: &Value) -> Value {
    let id = json["id"].as_i64().unwrap_or_else(|| -1);
    let accept_response = format!(
        "{} \"id\": {}, \"jsonrpc\": \"2.0\", \"result\": {} \"status\": \"OK\",\"untrusted\": false {}{}",
        "{", id, "{", "}", "}",
    );
    json::from_str(&accept_response).unwrap_or_default()
}

pub fn extract_tari_hash(monero: &Block) -> Option<&Hash> {
    for item in monero.miner_tx.prefix.extra.0.iter() {
        if let SubField::MergeMining(_depth, merge_mining_hash) = item {
            return Some(merge_mining_hash);
        }
    }
    None
}

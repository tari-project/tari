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
use monero::{
    blockdata::{Block as MoneroBlock, Block},
    consensus::{deserialize, serialize},
};
use rand::rngs::OsRng;
use tari_core::{
    blocks::NewBlockTemplate,
    consensus::ConsensusManager,
    proof_of_work::{monero_rx, monero_rx::MoneroData},
    transactions::{
        transaction::UnblindedOutput,
        types::{CryptoFactories, PrivateKey},
        CoinbaseBuilder,
    },
};
use tari_crypto::keys::SecretKey;

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

pub fn construct_monero_data(block: MoneroBlock, seed: String) -> Result<MoneroData, MmProxyError> {
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

// TODO: Temporary until RPC call is in place
pub fn add_coinbase(
    consensus: &ConsensusManager,
    block: &mut NewBlockTemplate,
) -> Result<UnblindedOutput, MmProxyError>
{
    let fees = block.body.get_total_fee();
    let (key, r) = new_spending_key();
    let factories = CryptoFactories::default();
    let builder = CoinbaseBuilder::new(factories);
    let builder = builder
        .with_block_height(block.header.height)
        .with_fees(fees)
        .with_nonce(r)
        .with_spend_key(key);
    let (tx, unblinded_output) = builder.build(consensus.consensus_constants(), consensus.emission_schedule())?;
    block.body.add_output(tx.body.outputs()[0].clone());
    block.body.add_kernel(tx.body.kernels()[0].clone());
    Ok(unblinded_output)
}

fn new_spending_key() -> (PrivateKey, PrivateKey) {
    let r = PrivateKey::random(&mut OsRng);
    let key = PrivateKey::random(&mut OsRng);
    (key, r)
}

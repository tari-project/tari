// Copyright 2019. The Tari Project
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

use crate::{blocks::BlockHeader, proof_of_work::Difficulty, tari_utilities::ByteArray, U256};
use monero::{
    blockdata::{
        block::{Block as MoneroBlock, BlockHeader as MoneroBlockHeader},
        transaction::SubField,
        Transaction as MoneroTransaction,
    },
    consensus::{encode::VarInt, serialize},
    cryptonote::hash::Hash,
};
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_TARGET: U256 = U256::MAX;

#[derive(Debug, Error, Clone)]
pub enum MergeMineError {
    #[error("Serialization error: {0}")]
    SerializeError(String),
    #[error("Error deserializing Monero data")]
    DeserializeError,
    #[error("Hashing of Monero data failed")]
    HashingError,
    #[error("RandomX error: {0}")]
    RandomXError(#[from] RandomXError),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MoneroData {
    // Monero header fields
    // #[serde(with = "HashMoneroHeader")]
    pub header: MoneroBlockHeader,
    // randomX vm key
    pub key: String,
    // transaction count
    pub count: u16,
    // transaction root
    pub transaction_root: [u8; 32],
    // Transaction proof of work.
    pub transaction_hashes: Vec<[u8; 32]>,
    // Coinbase tx from Monero
    pub coinbase_tx: MoneroTransaction,
}

// Hash algorithm in monero
pub fn cn_fast_hash(data: &[u8]) -> Vec<u8> {
    Hash::hash(data).0.as_bytes().to_vec()
}

// Tree hash count in monero
fn tree_hash_cnt(count: usize) -> usize {
    assert!(count >= 3);
    assert!(count <= 0x10000000);

    let mut pow: usize = 2;
    while pow < count {
        pow <<= 1;
    }
    pow >> 1
}

/// Tree hash algorithm in monero
#[allow(clippy::needless_range_loop)]
pub fn tree_hash(hashes: Vec<Hash>) -> Vec<u8> {
    assert!(!hashes.is_empty());
    match hashes.len() {
        1 => hashes[0].0.to_vec(),
        2 => {
            let mut buf: [u8; 64] = [0; 64];
            buf[..32].copy_from_slice(&hashes[0].0.to_vec());
            buf[32..].copy_from_slice(&hashes[1].0.to_vec());
            cn_fast_hash(&buf)
        },
        _ => {
            let mut cnt = tree_hash_cnt(hashes.len());
            let mut buf: Vec<u8> = Vec::with_capacity(cnt * 32);

            for i in 0..(2 * cnt - hashes.len()) {
                for val in &hashes[i].0.to_vec() {
                    buf.push(val.to_owned());
                }
            }

            for _i in (2 * cnt - hashes.len())..(cnt * 32) {
                buf.push(0);
            }

            let mut i: usize = 2 * cnt - hashes.len();
            for j in (2 * cnt - hashes.len())..cnt {
                let mut tmp: [u8; 64] = [0; 64];
                tmp[..32].copy_from_slice(&hashes[i].0.to_vec());
                tmp[32..].copy_from_slice(&hashes[i + 1].0.to_vec());
                let tmp = cn_fast_hash(&tmp);
                buf[(j * 32)..((j + 1) * 32)].copy_from_slice(tmp.as_slice());
                i += 2;
            }
            assert_eq!(i, hashes.len());

            while cnt > 2 {
                cnt >>= 1;
                let mut i = 0;
                for j in (0..(cnt * 32)).step_by(32) {
                    let tmp = cn_fast_hash(&buf[i..(i + 64)]);
                    buf[j..(j + 32)].copy_from_slice(tmp.as_slice());
                    i += 64;
                }
            }

            cn_fast_hash(&buf[..64])
        },
    }
}

impl MoneroData {
    pub fn new(tari_header: &BlockHeader) -> Result<MoneroData, MergeMineError> {
        bincode::deserialize(&tari_header.pow.pow_data).map_err(|_| MergeMineError::DeserializeError)
    }
}

/// Calculate the difficulty attained for the given block deserialized the Monero header from the provided header
pub fn monero_difficulty(header: &BlockHeader) -> Difficulty {
    match monero_difficulty_calculation(header) {
        Ok(v) => v,
        Err(_) => 0.into(),
    }
}

/// Internal function to calculate the difficulty attained for the given block Deserialized the Monero header from the
/// provided header
fn monero_difficulty_calculation(header: &BlockHeader) -> Result<Difficulty, MergeMineError> {
    let monero = MoneroData::new(header)?;
    verify_header(&header, &monero)?;
    let flags = RandomXFlag::get_recommended_flags();
    let key = monero.key.clone();
    let input = create_input_blob(&monero.header, &monero.count, &monero.transaction_hashes)?;
    let cache = RandomXCache::new(flags, (&key).as_ref())?;
    let dataset = RandomXDataset::new(flags, &cache, 0)?;
    let vm = RandomXVM::new(flags, Some(&cache), Some(&dataset))?;
    let hash = vm.calculate_hash((&input).as_ref())?;
    let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
    let result = MAX_TARGET / scalar;
    let difficulty = Difficulty::from(result.low_u64());
    Ok(difficulty)
}

/// Appends merge mining hash to a Monero block and returns the Monero blocktemplate_blob
pub fn append_merge_mining_tag(block: &MoneroBlock, hash: Hash) -> Result<String, MergeMineError> {
    let mut monero_block = block.clone();
    let mm_tag = SubField::MergeMining(VarInt(0), hash);
    monero_block.miner_tx.prefix.extra.0.push(mm_tag);
    let serialized = serialize::<MoneroBlock>(&block);
    Ok(hex::encode(&serialized))
}

/// Calculates the Monero blockhashing_blob
pub fn create_input_blob(
    header: &MoneroBlockHeader,
    tx_count: &u16,
    tx_hashes: &Vec<[u8; 32]>,
) -> Result<String, MergeMineError>
{
    let header = serialize::<MoneroBlockHeader>(header);
    // Note count assumes the miner tx is included already
    let mut count = serialize::<VarInt>(&VarInt(tx_count.clone() as u64));
    let mut hashes = Vec::new();
    for item in tx_hashes {
        hashes.push(Hash(from_slice(item.clone().as_bytes())));
    }
    let mut root = tree_hash(hashes);
    let mut encode2 = header;
    encode2.append(&mut root);
    encode2.append(&mut count);
    Ok(hex::encode(encode2))
}

/// Utility function to transform array to fixed array
pub fn from_slice(bytes: &[u8]) -> [u8; 32] {
    let mut array = [0; 32];
    let bytes = &bytes[..array.len()]; // panics if not enough data
    array.copy_from_slice(bytes);
    array
}

/// Utility function to transform array of hash to fixed array of [u8; 32]
pub fn from_hashes(hashes: &[Hash]) -> Vec<[u8; 32]> {
    let mut result = Vec::new();
    for item in hashes {
        result.push(item.0);
    }
    result
}

fn verify_root(monero_data: &MoneroData) -> Result<(), MergeMineError> {
    let mut hashes = Vec::new();
    for item in &monero_data.transaction_hashes {
        hashes.push(Hash(from_slice(item.to_vec().as_slice())));
    }
    let root = tree_hash(hashes);

    if monero_data.transaction_root.to_vec() != root {
        return Err(MergeMineError::ValidationError(
            "Transaction root did not match".to_string(),
        ));
    }
    Ok(())
}

fn merged_mining_subfield(header: &BlockHeader) -> SubField {
    let hash = header.merged_mining_hash();
    let depth = 0;
    SubField::MergeMining(VarInt(depth), Hash::hash(&hash[..32]))
}

fn verify_header(header: &BlockHeader, monero_data: &MoneroData) -> Result<(), MergeMineError> {
    if !(monero_data
        .coinbase_tx
        .prefix
        .extra
        .0
        .contains(&merged_mining_subfield(header)))
    {
        return Err(MergeMineError::ValidationError(
            "Merge mining tag was not found in Monero coinbase transaction".to_string(),
        ));
    }
    verify_root(monero_data)?;
    // TODO: add seed check here.
    Ok(())
}

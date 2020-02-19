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

use crate::{blocks::BlockHeader, proof_of_work::Difficulty};
use bigint::uint::U256;
use derive_error::Error;
use monero::blockdata::{block::BlockHeader as MoneroBlockHeader, Transaction as MoneroTransaction};
use randomx_rs::{RandomXCache, RandomXError, RandomXFlag, RandomXVM};
use serde::{Deserialize, Serialize};
use tari_mmr::MerkleProof;

const MAX_TARGET: U256 = U256::MAX;

#[derive(Debug, Error, Clone)]
enum MergeMineError {
    // Error deserializing Monero data
    DeserializeError,
    // Hashing of Monero data failed
    HashingError,
    // Verification failed
    VerificationFailed,
    // RandomX Failure
    RandomXError(RandomXError),
}

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MoneroData {
    // Monero header fields
    // #[serde(with = "HashMoneroHeader")]
    header: MoneroBlockHeader,
    // randomX vm key
    key: String,
    // transaction count
    count: u16,
    // transaction root
    transaction_root: [u8; 32],
    // Transaction proof of work.
    merkle_proof: MerkleProof,
    // Coinbase tx from Monero
    coinbase_tx: MoneroTransaction,
}

impl MoneroData {
    fn new(tari_header: &BlockHeader) -> Result<MoneroData, MergeMineError> {
        bincode::deserialize(&tari_header.pow.pow_data).map_err(|_| MergeMineError::DeserializeError)
    }
}

/// Calculate the difficulty attained for the given block deserialized the Monero header from the provided header
pub fn monero_difficulty(header: &BlockHeader) -> Difficulty {
    match monero_difficulty_calculation(header) {
        Ok(v) => v,
        Err(_) => 1.into(), // todo this needs to change to 0 when merge mine is implemented
    }
}

/// Internal function to calculate the difficulty attained for the given block Deserialized the Monero header from the
/// provided header
fn monero_difficulty_calculation(header: &BlockHeader) -> Result<Difficulty, MergeMineError> {
    let monero = MoneroData::new(header)?;
    verify_header(&header, &monero)?;
    let flags = RandomXFlag::FLAG_DEFAULT;
    let key = monero.key.clone();
    let input = create_input_blob(&monero)?;
    let cache = RandomXCache::new(flags, &key)?;
    let vm = RandomXVM::new(flags, &cache, None)?;
    let hash = vm.calculate_hash(&input)?;

    let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
    let result = MAX_TARGET / scalar;
    let difficulty = u64::from(result).into();
    Ok(difficulty)
}

fn create_input_blob(_data: &MoneroData) -> Result<String, MergeMineError> {
    // Todo deserialize monero data to create string for  randomX vm
    // Returning an error here so that difficulty can return 0 as this is not yet implemented.
    Err(MergeMineError::HashingError)
}

fn verify_header(_header: &BlockHeader, _monero_data: &MoneroData) -> Result<(), MergeMineError> {
    // todo
    // verify that our header is in coinbase
    // todo
    // verify that coinbase is in root.
    Ok(())
}

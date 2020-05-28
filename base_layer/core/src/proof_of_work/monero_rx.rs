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

use crate::{
    blocks::BlockHeader,
    proof_of_work::{monero_merkle_proof::MoneroMerkleProof, monero_merkle_tree::MoneroMerkleTree, Difficulty},
    tari_utilities::ByteArray,
    U256,
};
use derive_error::Error;
use monero::{
    blockdata::{
        block::BlockHeader as MoneroBlockHeader,
        transaction::SubField,
        Transaction as MoneroTransaction,
        TransactionPrefix,
    },
    consensus::encode::VarInt,
    cryptonote::hash::{Hash, Hashable},
};
#[cfg(feature = "monero_merge_mining")]
use randomx_rs::{RandomXCache, RandomXDataset, RandomXError, RandomXFlag, RandomXVM};
use serde::{Deserialize, Serialize};

const MAX_TARGET: U256 = U256::MAX;
const SEEDHASH_EPOCH: u64 = 2048;
const SEEDHASH_LAG: u64 = 64;

#[derive(Debug, Error, Clone)]
enum MergeMineError {
    // Error deserializing Monero data
    DeserializeError,
    // Error serializing Monero data
    SerializeError,
    // Hashing of Monero data failed
    HashingError,
    // Validation Failure
    ValidationError,
    // RandomX Failure
    #[cfg(feature = "monero_merge_mining")]
    RandomXError(RandomXError),
}

/// This is a struct to deserialize the data from he pow field into data required for the randomX Monero merged mine
/// pow.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MoneroData {
    // Monero header fields
    pub header: MoneroBlockHeader,
    // randomX vm key
    pub key: Vec<u8>,
    // Monero block height
    pub height: u64,
    // Initial seed height
    pub initial_use: u64,
    // Monero block target time
    pub block_time: u64,
    // transaction count
    pub count: u16,
    // transaction root
    pub transaction_root: Vec<u8>,
    // Transaction proof of work.
    pub merkle_proof: MoneroMerkleProof,
    // Coinbase tx from Monero
    pub coinbase_tx: MoneroTransaction,
}

fn check_randomx_key(data: &MoneroData) -> Result<(), MergeMineError> {
    let age_limit = 4 * data.height * (data.block_time / 120); // get `120` from consensus constants instead
    if data.initial_use > (data.height - age_limit) {
        return Ok(());
    }
    Err(MergeMineError::ValidationError)
}

fn verify_merkle_root(
    transaction_root: Vec<u8>,
    coinbase_tx: Vec<u8>,
    merkle_proof: MoneroMerkleProof,
) -> Result<(), MergeMineError>
{
    println!("{:?}", merkle_proof);
    if !merkle_proof.validate(&transaction_root) {
        return Err(MergeMineError::ValidationError);
    }

    if !merkle_proof.validate_value(&coinbase_tx) {
        return Err(MergeMineError::ValidationError);
    }

    Ok(())
}

impl MoneroData {
    fn new(tari_header: &BlockHeader) -> Result<MoneroData, MergeMineError> {
        bincode::deserialize(&tari_header.pow.pow_data).map_err(|_| MergeMineError::DeserializeError)
    }

    fn generate(tari_header: &mut BlockHeader) {
        let prev_data = MoneroData::new(&tari_header);
        let mut prev_id = Hash::null_hash();
        match prev_data {
            Ok(data) => {
                prev_id = data.header.prev_id;
            },
            Err(_) => {},
        }

        let monero_header = MoneroBlockHeader {
            major_version: VarInt(3),
            minor_version: VarInt(5),
            timestamp: VarInt(tari_header.timestamp.as_u64()),
            prev_id,
            nonce: 0,
        };

        let monero_tx = MoneroTransaction {
            prefix: TransactionPrefix {
                version: Default::default(),
                unlock_time: Default::default(),
                inputs: vec![],
                outputs: vec![],
                extra: Default::default(),
            },
            signatures: vec![],
            rct_signatures: Default::default(),
        };

        let tx_hash = monero_tx.hash().0;

        let mut tree = MoneroMerkleTree::new();
        tree.push(tx_hash.to_vec());
        let proof = tree.get_proof(tx_hash.to_vec());
        let root_hash = tree.root_hash().unwrap();

        let mut monero_data = MoneroData {
            header: monero_header,
            key: Hash::null_hash().0.to_vec(),
            height: 0,
            initial_use: 0,
            block_time: 0,
            count: 1,
            transaction_root: root_hash.to_vec(),
            merkle_proof: proof,
            coinbase_tx: monero_tx,
        };

        monero_data.adjust_mm_tag(&tari_header);
        tari_header.pow.pow_data = bincode::serialize(&monero_data).unwrap();
        tari_header.pow.target_difficulty = Difficulty::from(1);
    }

    pub fn adjust_mm_tag(&mut self, tari_header: &BlockHeader) {
        self.coinbase_tx.prefix.extra.0.clear();
        self.coinbase_tx
            .prefix
            .extra
            .0
            .push(merged_mining_subfield(tari_header));
    }
}

/// Calculate the difficulty attained for the given block deserialized the Monero header from the provided header
pub fn monero_difficulty(header: &BlockHeader) -> Difficulty {
    monero_difficulty_with_hash(header).0
}

pub fn monero_difficulty_with_hash(header: &BlockHeader) -> (Difficulty, Vec<u8>) {
    match monero_difficulty_calculation(header) {
        Ok(d) => (d.0, d.1),
        Err(_) => (1.into(), Hash::null_hash().0.to_vec()),
    }
}

pub fn merged_mining_subfield(header: &BlockHeader) -> SubField {
    let tag = merged_mining_tag_hash(header);
    let depth = 0;
    SubField::MergeMining(VarInt(depth), tag)
}

fn merged_mining_transaction_hash(header: &BlockHeader) -> Hash {
    let mut merge_mining_prehash: Vec<u8> = header.version.to_le_bytes().to_vec();
    merge_mining_prehash.append(&mut header.height.to_le_bytes().to_vec());
    merge_mining_prehash.append(&mut header.prev_hash.as_bytes().to_vec());
    merge_mining_prehash.append(&mut header.timestamp.as_u64().to_le_bytes().to_vec());
    merge_mining_prehash.append(&mut header.output_mr.as_bytes().to_vec());
    merge_mining_prehash.append(&mut header.range_proof_mr.as_bytes().to_vec());
    merge_mining_prehash.append(&mut header.kernel_mr.as_bytes().to_vec());
    merge_mining_prehash.append(&mut header.total_kernel_offset.as_bytes().to_vec());
    Hash::hash(merge_mining_prehash.as_slice())
}

fn merged_mining_tag_hash(header: &BlockHeader) -> Hash {
    merged_mining_transaction_hash(header)
}

/// Internal function to calculate the difficulty attained for the given block Deserialized the Monero header from the
/// provided header
fn monero_difficulty_calculation(header: &BlockHeader) -> Result<(Difficulty, Vec<u8>), MergeMineError> {
    #[cfg(feature = "monero_merge_mining")]
    {
        println!("Deserializing monero header");
        let monero = MoneroData::new(header)?;
        println!("Verifying header");
        verify_header(&header, &monero)?;
        let flags = RandomXFlag::get_recommended_flags();
        check_randomx_key(&monero)?;
        println!("Creating input blob");
        let input = create_input_blob(&monero)?;
        println!("Creating cache");
        let cache = RandomXCache::new(flags, &monero.key.as_bytes())?;
        println!("Creating dataset");
        let dataset = RandomXDataset::new(flags, &cache, 0)?;
        println!("Creating VM");
        let vm = RandomXVM::new(flags, Some(&cache), Some(&dataset))?;
        println!("Calculating hash");
        let hash = vm.calculate_hash(&input.as_bytes())?;
        let scalar = U256::from_big_endian(&hash); // Big endian so the hash has leading zeroes
        let result = MAX_TARGET / scalar;
        let difficulty = result.low_u64().into();
        println!(
            "Checking difficulty {:?} against target {:?}",
            difficulty, header.pow.target_difficulty
        );
        if difficulty >= header.pow.target_difficulty {
            return Err(MergeMineError::ValidationError);
        }
        Ok((difficulty, hash))
    }
    #[cfg(not(feature = "monero_merge_mining"))]
    {
        Err(MergeMineError::HashingError)
    }
}

fn create_input_blob(data: &MoneroData) -> Result<Vec<u8>, MergeMineError> {
    let serialized_header = bincode::serialize(&data.header);
    println!("Serializing header");
    if serialized_header.is_err() {
        return Err(MergeMineError::SerializeError);
    }

    let serialized_root_hash = bincode::serialize(&data.transaction_root);
    println!("Serializing root");
    if serialized_root_hash.is_err() {
        return Err(MergeMineError::SerializeError);
    }

    let serialized_transaction_count = bincode::serialize(&data.count);
    println!("Serializing transaction count");
    if serialized_transaction_count.is_err() {
        return Err(MergeMineError::SerializeError);
    }

    let mut pre_hash_blob = serialized_header.unwrap();
    pre_hash_blob.append(&mut serialized_root_hash.unwrap());
    pre_hash_blob.append(&mut serialized_transaction_count.unwrap());
    let hash_blob = Hash::hash(pre_hash_blob.as_slice());
    println!("{:?}", hash_blob);
    Ok(hash_blob.0.to_vec())
}

fn verify_header(header: &BlockHeader, monero_data: &MoneroData) -> Result<(), MergeMineError> {
    println!("Verifying Merge Mining Tag");
    println!("Tag {:?}", monero_data.coinbase_tx.prefix.extra.0);
    println!("Compare {:?}", merged_mining_subfield(header));
    if !(monero_data
        .coinbase_tx
        .prefix
        .extra
        .0
        .contains(&merged_mining_subfield(header)))
    {
        return Err(MergeMineError::ValidationError);
    }
    println!("Verifying proof of work");
    if verify_merkle_root(
        monero_data.transaction_root.to_vec(),
        monero_data.coinbase_tx.hash().0.to_vec(),
        monero_data.merkle_proof.clone(),
    )
    .is_err()
    {
        return Err(MergeMineError::ValidationError);
    }
    println!("Proof of work verified");
    Ok(())
}

#[cfg(test)]
pub mod test {
    use crate::{
        blocks::BlockHeader,
        proof_of_work::{
            monero_difficulty,
            monero_rx::monero_difficulty_with_hash,
            Difficulty,
            MoneroData,
            PowAlgorithm,
        },
    };
    use chrono::{DateTime, NaiveDate, Utc};
    use monero::consensus::encode::VarInt;
    use std::convert::TryFrom;
    use tari_utilities::hex::Hex;

    #[cfg(feature = "monero_merge_mining")]
    pub fn get_header() -> BlockHeader {
        let mut header = BlockHeader::new(0);
        header.timestamp = DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2000, 1, 1).and_hms(1, 1, 1), Utc).into();
        MoneroData::generate(&mut header);
        let mut monero_data = MoneroData::new(&header).unwrap();
        monero_data.header.timestamp = VarInt(header.timestamp.as_u64());
        monero_data.header.nonce = u32::try_from(header.nonce).unwrap();
        header.pow.pow_algo = PowAlgorithm::Monero;
        monero_data.adjust_mm_tag(&header);
        header.pow.pow_data = bincode::serialize(&monero_data).unwrap();
        header
    }

    #[cfg(feature = "monero_merge_mining")]
    #[test]
    fn difficulty_check() {
        let mut header = get_header();
        header.nonce = 2606;
        MoneroData::generate(&mut header);
        let mut monero_data = MoneroData::new(&header).unwrap();
        monero_data.header.nonce = u32::try_from(header.nonce).unwrap();
        monero_data.adjust_mm_tag(&header);
        header.pow.pow_data = bincode::serialize(&monero_data).unwrap();
        let (diff, hash) = monero_difficulty_with_hash(&header);
        assert_eq!(diff, Difficulty::from(1));
        assert_eq!(
            hash.to_hex(),
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
    }
}
